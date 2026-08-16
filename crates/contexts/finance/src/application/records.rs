use erpx_kernel::Clock;
use thiserror::Error;

use crate::{
    FinanceRecord, FinanceRecordError,
    ports::{
        FinanceRecordAuthorizationPort, FinanceRecordAuthorizationRequest, FinanceRecordCommand,
        FinanceRecordCommandRequest, FinanceRecordCommit, FinanceRecordUnitOfWork,
        FinanceRecordWriteRepository, PortError, StoredFinanceRecordCommit,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FinanceRecordApplicationError {
    #[error(transparent)]
    Domain(#[from] FinanceRecordError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("Finance record was not found")]
    NotFound,
    #[error("Finance record create cannot declare a current version")]
    UnexpectedCreateVersion,
    #[error("Finance record transition requires the exact current version")]
    MissingExpectedVersion,
    #[error("Finance record authority does not contain its scope")]
    ScopeOutsideAuthority,
}
pub struct FinanceRecordCommandHandler<W, U, A, C> {
    repository: W,
    unit_of_work: U,
    authorization: A,
    clock: C,
}

impl<W, U, A, C> FinanceRecordCommandHandler<W, U, A, C> {
    pub const fn new(repository: W, unit_of_work: U, authorization: A, clock: C) -> Self {
        Self {
            repository,
            unit_of_work,
            authorization,
            clock,
        }
    }
}

impl<W, U, A, C> FinanceRecordCommandHandler<W, U, A, C>
where
    W: FinanceRecordWriteRepository,
    U: FinanceRecordUnitOfWork,
    A: FinanceRecordAuthorizationPort,
    C: Clock,
{
    pub async fn execute(
        &self,
        request: FinanceRecordCommandRequest,
    ) -> Result<StoredFinanceRecordCommit, FinanceRecordApplicationError> {
        if let Some(receipt) = self
            .unit_of_work
            .find_finance_record_receipt(&request.identity)
            .await?
        {
            self.authorize(&request, &receipt.record).await?;
            return Ok(receipt);
        }
        let at = self.clock.now();
        let record = match &request.identity.command {
            FinanceRecordCommand::Create { kind, scope, title } => {
                if request.expected_version.is_some() {
                    return Err(FinanceRecordApplicationError::UnexpectedCreateVersion);
                }
                FinanceRecord::create(
                    request.identity.record_id,
                    *kind,
                    scope.clone(),
                    title.clone(),
                    at,
                )?
            }
            FinanceRecordCommand::Transition { status } => {
                let expected = request
                    .expected_version
                    .ok_or(FinanceRecordApplicationError::MissingExpectedVersion)?;
                let mut record = self
                    .repository
                    .load_finance_record(&request.authority_scope, request.identity.record_id)
                    .await?
                    .ok_or(FinanceRecordApplicationError::NotFound)?;
                record.transition(expected, *status, at)?;
                record
            }
        };
        self.authorize(&request, &record).await?;
        self.unit_of_work
            .commit_finance_record(FinanceRecordCommit {
                request,
                record,
                changed_at: at,
            })
            .await
            .map_err(Into::into)
    }

    async fn authorize(
        &self,
        request: &FinanceRecordCommandRequest,
        record: &FinanceRecord,
    ) -> Result<(), FinanceRecordApplicationError> {
        if !request.authority_scope.contains(record.scope()) {
            return Err(FinanceRecordApplicationError::ScopeOutsideAuthority);
        }
        self.authorization
            .authorize_finance_record(FinanceRecordAuthorizationRequest {
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
        FinanceRecordAction, FinanceRecordCommandIdentity, FinanceRecordKind,
        StoredFinanceRecordCommit,
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
    fn request() -> FinanceRecordCommandRequest {
        FinanceRecordCommandRequest {
            identity: FinanceRecordCommandIdentity {
                command_id: CommandId::try_from_uuid(Uuid::from_u128(10)).unwrap(),
                idempotency_key: IdempotencyKey::try_new("finance-record-test").unwrap(),
                record_id: Uuid::from_u128(11),
                command: FinanceRecordCommand::Create {
                    kind: FinanceRecordKind::Payment,
                    scope: scope(),
                    title: "Governed customer activity".into(),
                },
            },
            correlation_id: CorrelationId::try_from_uuid(Uuid::from_u128(12)).unwrap(),
            actor: ActorAttribution {
                principal_ref: "sales:user:1".into(),
                kind: ActorKind::TenantUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            authority_scope: scope(),
            audit: AuditContext {
                purpose: "finance.record.test".into(),
                customer_visible: false,
                module: Some("finance".into()),
            },
            expected_version: None,
        }
    }

    #[derive(Clone, Default)]
    struct Repo;
    #[async_trait]
    impl FinanceRecordWriteRepository for Repo {
        async fn load_finance_record(
            &self,
            _: &TenantScope,
            _: Uuid,
        ) -> Result<Option<FinanceRecord>, PortError> {
            Ok(None)
        }
    }

    #[derive(Clone, Default)]
    struct Uow {
        receipt: Arc<Mutex<Option<(FinanceRecordCommandIdentity, StoredFinanceRecordCommit)>>>,
        commits: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl FinanceRecordUnitOfWork for Uow {
        async fn find_finance_record_receipt(
            &self,
            identity: &FinanceRecordCommandIdentity,
        ) -> Result<Option<StoredFinanceRecordCommit>, PortError> {
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
        async fn commit_finance_record(
            &self,
            commit: FinanceRecordCommit,
        ) -> Result<StoredFinanceRecordCommit, PortError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            let value = StoredFinanceRecordCommit {
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
    impl FinanceRecordAuthorizationPort for Auth {
        async fn authorize_finance_record(
            &self,
            request: FinanceRecordAuthorizationRequest<'_>,
        ) -> Result<(), PortError> {
            assert_eq!(request.action, FinanceRecordAction::Create);
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
        let handler =
            FinanceRecordCommandHandler::new(Repo, uow.clone(), Auth(true), FixedClock::new(at()));
        let first = handler.execute(request()).await.unwrap();
        let retry = handler.execute(request()).await.unwrap();
        assert!(!first.replayed && retry.replayed);
        assert_eq!(uow.commits.load(Ordering::SeqCst), 1);

        let denied = Uow::default();
        let handler = FinanceRecordCommandHandler::new(
            Repo,
            denied.clone(),
            Auth(false),
            FixedClock::new(at()),
        );
        assert!(matches!(
            handler.execute(request()).await,
            Err(FinanceRecordApplicationError::Port(PortError::Denied))
        ));
        assert_eq!(denied.commits.load(Ordering::SeqCst), 0);
    }
}
