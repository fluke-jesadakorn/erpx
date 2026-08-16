use erpx_kernel::Clock;
use thiserror::Error;

use crate::{
    MarketingDomainError, MarketingRecord,
    ports::{
        MarketingRecordAuthorizationPort, MarketingRecordAuthorizationRequest,
        MarketingRecordCommand, MarketingRecordCommandRequest, MarketingRecordCommit,
        MarketingRecordUnitOfWork, MarketingRecordWriteRepository, PortError,
        StoredMarketingRecordCommit,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MarketingRecordApplicationError {
    #[error(transparent)]
    Domain(#[from] MarketingDomainError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("Marketing record was not found")]
    NotFound,
    #[error("Marketing record create cannot declare a current version")]
    UnexpectedCreateVersion,
    #[error("Marketing record transition requires the exact current version")]
    MissingExpectedVersion,
    #[error("Marketing record authority does not contain its scope")]
    ScopeOutsideAuthority,
}
pub struct MarketingRecordCommandHandler<W, U, A, C> {
    repository: W,
    unit_of_work: U,
    authorization: A,
    clock: C,
}

impl<W, U, A, C> MarketingRecordCommandHandler<W, U, A, C> {
    pub const fn new(repository: W, unit_of_work: U, authorization: A, clock: C) -> Self {
        Self {
            repository,
            unit_of_work,
            authorization,
            clock,
        }
    }
}

impl<W, U, A, C> MarketingRecordCommandHandler<W, U, A, C>
where
    W: MarketingRecordWriteRepository,
    U: MarketingRecordUnitOfWork,
    A: MarketingRecordAuthorizationPort,
    C: Clock,
{
    pub async fn execute(
        &self,
        request: MarketingRecordCommandRequest,
    ) -> Result<StoredMarketingRecordCommit, MarketingRecordApplicationError> {
        if let Some(receipt) = self
            .unit_of_work
            .find_marketing_record_receipt(&request.identity)
            .await?
        {
            self.authorize(&request, &receipt.record).await?;
            return Ok(receipt);
        }
        let at = self.clock.now();
        let record = match &request.identity.command {
            MarketingRecordCommand::Create { kind, scope, title } => {
                if request.expected_version.is_some() {
                    return Err(MarketingRecordApplicationError::UnexpectedCreateVersion);
                }
                MarketingRecord::create(
                    request.identity.record_id,
                    *kind,
                    scope.clone(),
                    title.clone(),
                    at,
                )?
            }
            MarketingRecordCommand::Transition { status } => {
                let expected = request
                    .expected_version
                    .ok_or(MarketingRecordApplicationError::MissingExpectedVersion)?;
                let mut record = self
                    .repository
                    .load_marketing_record(&request.authority_scope, request.identity.record_id)
                    .await?
                    .ok_or(MarketingRecordApplicationError::NotFound)?;
                record.transition(expected, *status, at)?;
                record
            }
        };
        self.authorize(&request, &record).await?;
        self.unit_of_work
            .commit_marketing_record(MarketingRecordCommit {
                request,
                record,
                changed_at: at,
            })
            .await
            .map_err(Into::into)
    }

    async fn authorize(
        &self,
        request: &MarketingRecordCommandRequest,
        record: &MarketingRecord,
    ) -> Result<(), MarketingRecordApplicationError> {
        if !request.authority_scope.contains(record.scope()) {
            return Err(MarketingRecordApplicationError::ScopeOutsideAuthority);
        }
        self.authorization
            .authorize_marketing_record(MarketingRecordAuthorizationRequest {
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
        ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, FixedClock,
        TenantScope,
    };
    use uuid::Uuid;

    use super::*;
    use crate::{
        MarketingRecordAction, MarketingRecordCommandIdentity, MarketingRecordKind,
        StoredMarketingRecordCommit,
    };

    fn at() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 16, 12, 0, 0)
            .single()
            .unwrap()
    }
    fn scope() -> TenantScope {
        TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }
    fn request() -> MarketingRecordCommandRequest {
        MarketingRecordCommandRequest {
            identity: MarketingRecordCommandIdentity {
                command_id: CommandId::try_from_uuid(Uuid::from_u128(10)).unwrap(),
                idempotency_key: IdempotencyKey::try_new("marketing-record-test").unwrap(),
                record_id: Uuid::from_u128(11),
                command: MarketingRecordCommand::Create {
                    kind: MarketingRecordKind::Campaign,
                    scope: scope(),
                    title: "Governed launch".into(),
                },
            },
            correlation_id: CorrelationId::try_from_uuid(Uuid::from_u128(12)).unwrap(),
            actor: ActorAttribution {
                principal_ref: "marketer:1".into(),
                kind: ActorKind::TenantUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            authority_scope: scope(),
            audit: AuditContext {
                purpose: "marketing.record.test".into(),
                customer_visible: false,
                module: Some("marketing".into()),
            },
            expected_version: None,
        }
    }

    #[derive(Clone, Default)]
    struct Repo;
    #[async_trait]
    impl MarketingRecordWriteRepository for Repo {
        async fn load_marketing_record(
            &self,
            _: &TenantScope,
            _: Uuid,
        ) -> Result<Option<MarketingRecord>, PortError> {
            Ok(None)
        }
    }

    #[derive(Clone, Default)]
    struct Uow {
        receipt: Arc<Mutex<Option<(MarketingRecordCommandIdentity, StoredMarketingRecordCommit)>>>,
        commits: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl MarketingRecordUnitOfWork for Uow {
        async fn find_marketing_record_receipt(
            &self,
            identity: &MarketingRecordCommandIdentity,
        ) -> Result<Option<StoredMarketingRecordCommit>, PortError> {
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
        async fn commit_marketing_record(
            &self,
            commit: MarketingRecordCommit,
        ) -> Result<StoredMarketingRecordCommit, PortError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            let value = StoredMarketingRecordCommit {
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
    impl MarketingRecordAuthorizationPort for Auth {
        async fn authorize_marketing_record(
            &self,
            request: MarketingRecordAuthorizationRequest<'_>,
        ) -> Result<(), PortError> {
            assert_eq!(request.action, MarketingRecordAction::Create);
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
        let handler = MarketingRecordCommandHandler::new(
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
        let handler = MarketingRecordCommandHandler::new(
            Repo,
            denied.clone(),
            Auth(false),
            FixedClock::new(at()),
        );
        assert!(matches!(
            handler.execute(request()).await,
            Err(MarketingRecordApplicationError::Port(PortError::Denied))
        ));
        assert_eq!(denied.commits.load(Ordering::SeqCst), 0);
    }
}
