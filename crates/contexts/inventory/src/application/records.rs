use erpx_kernel::Clock;
use thiserror::Error;

use crate::{
    InventoryLifecycleRecord, InventoryLifecycleRecordError,
    ports::{
        InventoryLifecycleRecordAuthorizationPort, InventoryLifecycleRecordAuthorizationRequest,
        InventoryLifecycleRecordCommand, InventoryLifecycleRecordCommandRequest,
        InventoryLifecycleRecordCommit, InventoryLifecycleRecordUnitOfWork,
        InventoryLifecycleRecordWriteRepository, PortError, StoredInventoryLifecycleRecordCommit,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InventoryLifecycleRecordApplicationError {
    #[error(transparent)]
    Domain(#[from] InventoryLifecycleRecordError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("Inventory lifecycle record was not found")]
    NotFound,
    #[error("Inventory lifecycle record create cannot declare a current version")]
    UnexpectedCreateVersion,
    #[error("Inventory lifecycle record transition requires the exact current version")]
    MissingExpectedVersion,
    #[error("Inventory lifecycle record authority does not contain its scope")]
    ScopeOutsideAuthority,
}
pub struct InventoryLifecycleRecordCommandHandler<W, U, A, C> {
    repository: W,
    unit_of_work: U,
    authorization: A,
    clock: C,
}

impl<W, U, A, C> InventoryLifecycleRecordCommandHandler<W, U, A, C> {
    pub const fn new(repository: W, unit_of_work: U, authorization: A, clock: C) -> Self {
        Self {
            repository,
            unit_of_work,
            authorization,
            clock,
        }
    }
}

impl<W, U, A, C> InventoryLifecycleRecordCommandHandler<W, U, A, C>
where
    W: InventoryLifecycleRecordWriteRepository,
    U: InventoryLifecycleRecordUnitOfWork,
    A: InventoryLifecycleRecordAuthorizationPort,
    C: Clock,
{
    pub async fn execute(
        &self,
        request: InventoryLifecycleRecordCommandRequest,
    ) -> Result<StoredInventoryLifecycleRecordCommit, InventoryLifecycleRecordApplicationError>
    {
        if let Some(receipt) = self
            .unit_of_work
            .find_inventory_lifecycle_record_receipt(&request.identity)
            .await?
        {
            self.authorize(&request, &receipt.record).await?;
            return Ok(receipt);
        }
        let at = self.clock.now();
        let record = match &request.identity.command {
            InventoryLifecycleRecordCommand::Create { kind, scope, title } => {
                if request.expected_version.is_some() {
                    return Err(InventoryLifecycleRecordApplicationError::UnexpectedCreateVersion);
                }
                InventoryLifecycleRecord::create(
                    request.identity.record_id,
                    *kind,
                    scope.clone(),
                    title.clone(),
                    at,
                )?
            }
            InventoryLifecycleRecordCommand::Transition { status } => {
                let expected = request
                    .expected_version
                    .ok_or(InventoryLifecycleRecordApplicationError::MissingExpectedVersion)?;
                let mut record = self
                    .repository
                    .load_inventory_lifecycle_record(
                        &request.authority_scope,
                        request.identity.record_id,
                    )
                    .await?
                    .ok_or(InventoryLifecycleRecordApplicationError::NotFound)?;
                record.transition(expected, *status, at)?;
                record
            }
        };
        self.authorize(&request, &record).await?;
        self.unit_of_work
            .commit_inventory_lifecycle_record(InventoryLifecycleRecordCommit {
                request,
                record,
                changed_at: at,
            })
            .await
            .map_err(Into::into)
    }

    async fn authorize(
        &self,
        request: &InventoryLifecycleRecordCommandRequest,
        record: &InventoryLifecycleRecord,
    ) -> Result<(), InventoryLifecycleRecordApplicationError> {
        if !request.authority_scope.contains(record.scope()) {
            return Err(InventoryLifecycleRecordApplicationError::ScopeOutsideAuthority);
        }
        self.authorization
            .authorize_inventory_lifecycle_record(InventoryLifecycleRecordAuthorizationRequest {
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
        InventoryLifecycleRecordAction, InventoryLifecycleRecordCommandIdentity,
        InventoryLifecycleRecordKind, StoredInventoryLifecycleRecordCommit,
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
    fn request() -> InventoryLifecycleRecordCommandRequest {
        InventoryLifecycleRecordCommandRequest {
            identity: InventoryLifecycleRecordCommandIdentity {
                command_id: CommandId::try_from_uuid(Uuid::from_u128(10)).unwrap(),
                idempotency_key: IdempotencyKey::try_new("inventory-lifecycle-record-test")
                    .unwrap(),
                record_id: Uuid::from_u128(11),
                command: InventoryLifecycleRecordCommand::Create {
                    kind: InventoryLifecycleRecordKind::Document,
                    scope: scope(),
                    title: "Governed customer activity".into(),
                },
            },
            correlation_id: CorrelationId::try_from_uuid(Uuid::from_u128(12)).unwrap(),
            actor: ActorAttribution {
                principal_ref: "inventory:user:1".into(),
                kind: ActorKind::HoldingUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            authority_scope: scope(),
            audit: AuditContext {
                purpose: "inventory.lifecycle-record.test".into(),
                customer_visible: false,
                module: Some("inventory".into()),
            },
            expected_version: None,
        }
    }

    #[derive(Clone, Default)]
    struct Repo;
    #[async_trait]
    impl InventoryLifecycleRecordWriteRepository for Repo {
        async fn load_inventory_lifecycle_record(
            &self,
            _: &AuthorityScope,
            _: Uuid,
        ) -> Result<Option<InventoryLifecycleRecord>, PortError> {
            Ok(None)
        }
    }

    #[derive(Clone, Default)]
    struct Uow {
        receipt: Arc<
            Mutex<
                Option<(
                    InventoryLifecycleRecordCommandIdentity,
                    StoredInventoryLifecycleRecordCommit,
                )>,
            >,
        >,
        commits: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl InventoryLifecycleRecordUnitOfWork for Uow {
        async fn find_inventory_lifecycle_record_receipt(
            &self,
            identity: &InventoryLifecycleRecordCommandIdentity,
        ) -> Result<Option<StoredInventoryLifecycleRecordCommit>, PortError> {
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
        async fn commit_inventory_lifecycle_record(
            &self,
            commit: InventoryLifecycleRecordCommit,
        ) -> Result<StoredInventoryLifecycleRecordCommit, PortError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            let value = StoredInventoryLifecycleRecordCommit {
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
    impl InventoryLifecycleRecordAuthorizationPort for Auth {
        async fn authorize_inventory_lifecycle_record(
            &self,
            request: InventoryLifecycleRecordAuthorizationRequest<'_>,
        ) -> Result<(), PortError> {
            assert_eq!(request.action, InventoryLifecycleRecordAction::Create);
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
        let handler = InventoryLifecycleRecordCommandHandler::new(
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
        let handler = InventoryLifecycleRecordCommandHandler::new(
            Repo,
            denied.clone(),
            Auth(false),
            FixedClock::new(at()),
        );
        assert!(matches!(
            handler.execute(request()).await,
            Err(InventoryLifecycleRecordApplicationError::Port(
                PortError::Denied
            ))
        ));
        assert_eq!(denied.commits.load(Ordering::SeqCst), 0);
    }
}
