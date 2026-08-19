use erpx_kernel::Clock;
use thiserror::Error;

use crate::{
    AssetLifecycleRecord, AssetLifecycleRecordError,
    ports::{
        AssetLifecycleRecordAuthorizationPort, AssetLifecycleRecordAuthorizationRequest,
        AssetLifecycleRecordCommand, AssetLifecycleRecordCommandRequest,
        AssetLifecycleRecordCommit, AssetLifecycleRecordUnitOfWork,
        AssetLifecycleRecordWriteRepository, PortError, StoredAssetLifecycleRecordCommit,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AssetLifecycleRecordApplicationError {
    #[error(transparent)]
    Domain(#[from] AssetLifecycleRecordError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("Asset lifecycle record was not found")]
    NotFound,
    #[error("Asset lifecycle record create cannot declare a current version")]
    UnexpectedCreateVersion,
    #[error("Asset lifecycle record transition requires the exact current version")]
    MissingExpectedVersion,
    #[error("Asset lifecycle record authority does not contain its scope")]
    ScopeOutsideAuthority,
}
pub struct AssetLifecycleRecordCommandHandler<W, U, A, C> {
    repository: W,
    unit_of_work: U,
    authorization: A,
    clock: C,
}

impl<W, U, A, C> AssetLifecycleRecordCommandHandler<W, U, A, C> {
    pub const fn new(repository: W, unit_of_work: U, authorization: A, clock: C) -> Self {
        Self {
            repository,
            unit_of_work,
            authorization,
            clock,
        }
    }
}

impl<W, U, A, C> AssetLifecycleRecordCommandHandler<W, U, A, C>
where
    W: AssetLifecycleRecordWriteRepository,
    U: AssetLifecycleRecordUnitOfWork,
    A: AssetLifecycleRecordAuthorizationPort,
    C: Clock,
{
    pub async fn execute(
        &self,
        request: AssetLifecycleRecordCommandRequest,
    ) -> Result<StoredAssetLifecycleRecordCommit, AssetLifecycleRecordApplicationError> {
        if let Some(receipt) = self
            .unit_of_work
            .find_asset_lifecycle_record_receipt(&request.identity)
            .await?
        {
            self.authorize(&request, &receipt.record).await?;
            return Ok(receipt);
        }
        let at = self.clock.now();
        let record = match &request.identity.command {
            AssetLifecycleRecordCommand::Create { kind, scope, title } => {
                if request.expected_version.is_some() {
                    return Err(AssetLifecycleRecordApplicationError::UnexpectedCreateVersion);
                }
                AssetLifecycleRecord::create(
                    request.identity.record_id,
                    *kind,
                    scope.clone(),
                    title.clone(),
                    at,
                )?
            }
            AssetLifecycleRecordCommand::Transition { status } => {
                let expected = request
                    .expected_version
                    .ok_or(AssetLifecycleRecordApplicationError::MissingExpectedVersion)?;
                let mut record = self
                    .repository
                    .load_asset_lifecycle_record(
                        &request.authority_scope,
                        request.identity.record_id,
                    )
                    .await?
                    .ok_or(AssetLifecycleRecordApplicationError::NotFound)?;
                record.transition(expected, *status, at)?;
                record
            }
        };
        self.authorize(&request, &record).await?;
        self.unit_of_work
            .commit_asset_lifecycle_record(AssetLifecycleRecordCommit {
                request,
                record,
                changed_at: at,
            })
            .await
            .map_err(Into::into)
    }

    async fn authorize(
        &self,
        request: &AssetLifecycleRecordCommandRequest,
        record: &AssetLifecycleRecord,
    ) -> Result<(), AssetLifecycleRecordApplicationError> {
        if !request.authority_scope.contains(record.scope()) {
            return Err(AssetLifecycleRecordApplicationError::ScopeOutsideAuthority);
        }
        self.authorization
            .authorize_asset_lifecycle_record(AssetLifecycleRecordAuthorizationRequest {
                actor: &request.actor,
                authority_scope: &request.authority_scope,
                target_scope: record.scope(),
                action: request.identity.command.action(),
                kind: record.kind(),
            })
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;
    use chrono::{TimeZone, Utc};
    use erpx_cqrs::IdempotencyKey;
    use erpx_kernel::{
        ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
        FixedClock,
    };
    use uuid::Uuid;

    use super::*;
    use crate::{
        AssetLifecycleRecordAction, AssetLifecycleRecordCommandIdentity, AssetLifecycleRecordKind,
        StoredAssetLifecycleRecordCommit,
    };

    fn at() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 16, 12, 0, 0)
            .single()
            .unwrap()
    }
    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }
    fn request() -> AssetLifecycleRecordCommandRequest {
        AssetLifecycleRecordCommandRequest {
            identity: AssetLifecycleRecordCommandIdentity {
                command_id: CommandId::try_from_uuid(Uuid::from_u128(10)).unwrap(),
                idempotency_key: IdempotencyKey::try_new("asset-lifecycle-record-test").unwrap(),
                record_id: Uuid::from_u128(11),
                command: AssetLifecycleRecordCommand::Create {
                    kind: AssetLifecycleRecordKind::Transfer,
                    scope: scope(),
                    title: "Governed customer activity".into(),
                },
            },
            correlation_id: CorrelationId::try_from_uuid(Uuid::from_u128(12)).unwrap(),
            actor: ActorAttribution {
                principal_ref: "sales:user:1".into(),
                kind: ActorKind::HoldingUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            authority_scope: scope(),
            audit: AuditContext {
                purpose: "assets.lifecycle-record.test".into(),
                customer_visible: false,
                module: Some("asset".into()),
            },
            expected_version: None,
        }
    }

    #[derive(Clone, Default)]
    struct Repo;
    #[async_trait]
    impl AssetLifecycleRecordWriteRepository for Repo {
        async fn load_asset_lifecycle_record(
            &self,
            _: &AuthorityScope,
            _: Uuid,
        ) -> Result<Option<AssetLifecycleRecord>, PortError> {
            Ok(None)
        }
    }

    #[derive(Clone, Default)]
    struct Uow {
        receipt: Arc<
            Mutex<
                Option<(
                    AssetLifecycleRecordCommandIdentity,
                    StoredAssetLifecycleRecordCommit,
                )>,
            >,
        >,
        commits: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl AssetLifecycleRecordUnitOfWork for Uow {
        async fn find_asset_lifecycle_record_receipt(
            &self,
            identity: &AssetLifecycleRecordCommandIdentity,
        ) -> Result<Option<StoredAssetLifecycleRecordCommit>, PortError> {
            Ok(self
                .receipt
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|(stored, value)| {
                    (stored == identity).then(|| {
                        let mut replay = value.clone();
                        replay.replayed = true;
                        replay
                    })
                }))
        }
        async fn commit_asset_lifecycle_record(
            &self,
            commit: AssetLifecycleRecordCommit,
        ) -> Result<StoredAssetLifecycleRecordCommit, PortError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            let value = StoredAssetLifecycleRecordCommit {
                command_id: commit.request.identity.command_id,
                correlation_id: commit.request.correlation_id,
                record: commit.record,
                committed_at: commit.changed_at,
                replayed: false,
            };
            *self.receipt.lock().unwrap() = Some((commit.request.identity, value.clone()));
            Ok(value)
        }
    }

    #[derive(Clone)]
    struct Auth(bool);
    #[async_trait]
    impl AssetLifecycleRecordAuthorizationPort for Auth {
        async fn authorize_asset_lifecycle_record(
            &self,
            request: AssetLifecycleRecordAuthorizationRequest<'_>,
        ) -> Result<(), PortError> {
            assert_eq!(request.action, AssetLifecycleRecordAction::Create);
            if self.0 {
                Ok(())
            } else {
                Err(PortError::Denied)
            }
        }
    }

    #[tokio::test]
    async fn exact_retry_replays_and_denial_never_commits() {
        let uow = Uow::default();
        let handler = AssetLifecycleRecordCommandHandler::new(
            Repo,
            uow.clone(),
            Auth(true),
            FixedClock::new(at()),
        );
        let first = handler.execute(request()).await.unwrap();
        let retry = handler.execute(request()).await.unwrap();
        assert!(!first.replayed && retry.replayed);
        assert_eq!(uow.commits.load(Ordering::SeqCst), 1);

        let denied = Uow::default();
        let handler = AssetLifecycleRecordCommandHandler::new(
            Repo,
            denied.clone(),
            Auth(false),
            FixedClock::new(at()),
        );
        assert!(matches!(
            handler.execute(request()).await,
            Err(AssetLifecycleRecordApplicationError::Port(
                PortError::Denied
            ))
        ));
        assert_eq!(denied.commits.load(Ordering::SeqCst), 0);
    }
}
