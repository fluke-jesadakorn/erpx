use erpx_kernel::Clock;
use thiserror::Error;

use crate::{
    CrmDomainError, CrmRecord,
    ports::{
        CrmRecordAuthorizationPort, CrmRecordAuthorizationRequest, CrmRecordCommand,
        CrmRecordCommandRequest, CrmRecordCommit, CrmRecordUnitOfWork, CrmRecordWriteRepository,
        PortError, StoredCrmRecordCommit,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CrmRecordApplicationError {
    #[error(transparent)]
    Domain(#[from] CrmDomainError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("Crm record was not found")]
    NotFound,
    #[error("Crm record create cannot declare a current version")]
    UnexpectedCreateVersion,
    #[error("Crm record transition requires the exact current version")]
    MissingExpectedVersion,
    #[error("Crm record authority does not contain its scope")]
    ScopeOutsideAuthority,
}
pub struct CrmRecordCommandHandler<W, U, A, C> {
    repository: W,
    unit_of_work: U,
    authorization: A,
    clock: C,
}

impl<W, U, A, C> CrmRecordCommandHandler<W, U, A, C> {
    pub const fn new(repository: W, unit_of_work: U, authorization: A, clock: C) -> Self {
        Self {
            repository,
            unit_of_work,
            authorization,
            clock,
        }
    }
}

impl<W, U, A, C> CrmRecordCommandHandler<W, U, A, C>
where
    W: CrmRecordWriteRepository,
    U: CrmRecordUnitOfWork,
    A: CrmRecordAuthorizationPort,
    C: Clock,
{
    pub async fn execute(
        &self,
        request: CrmRecordCommandRequest,
    ) -> Result<StoredCrmRecordCommit, CrmRecordApplicationError> {
        if let Some(receipt) = self
            .unit_of_work
            .find_crm_record_receipt(&request.identity)
            .await?
        {
            self.authorize(&request, &receipt.record).await?;
            return Ok(receipt);
        }
        let at = self.clock.now();
        let record = match &request.identity.command {
            CrmRecordCommand::Create { kind, scope, title } => {
                if request.expected_version.is_some() {
                    return Err(CrmRecordApplicationError::UnexpectedCreateVersion);
                }
                CrmRecord::create(
                    request.identity.record_id,
                    *kind,
                    scope.clone(),
                    title.clone(),
                    at,
                )?
            }
            CrmRecordCommand::Transition { status } => {
                let expected = request
                    .expected_version
                    .ok_or(CrmRecordApplicationError::MissingExpectedVersion)?;
                let mut record = self
                    .repository
                    .load_crm_record(&request.authority_scope, request.identity.record_id)
                    .await?
                    .ok_or(CrmRecordApplicationError::NotFound)?;
                record.transition(expected, *status, at)?;
                record
            }
        };
        self.authorize(&request, &record).await?;
        self.unit_of_work
            .commit_crm_record(CrmRecordCommit {
                request,
                record,
                changed_at: at,
            })
            .await
            .map_err(Into::into)
    }

    async fn authorize(
        &self,
        request: &CrmRecordCommandRequest,
        record: &CrmRecord,
    ) -> Result<(), CrmRecordApplicationError> {
        if !request.authority_scope.contains(record.scope()) {
            return Err(CrmRecordApplicationError::ScopeOutsideAuthority);
        }
        self.authorization
            .authorize_crm_record(CrmRecordAuthorizationRequest {
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
    use crate::{CrmRecordAction, CrmRecordCommandIdentity, CrmRecordKind, StoredCrmRecordCommit};

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
    fn request() -> CrmRecordCommandRequest {
        CrmRecordCommandRequest {
            identity: CrmRecordCommandIdentity {
                command_id: CommandId::try_from_uuid(Uuid::from_u128(10)).unwrap(),
                idempotency_key: IdempotencyKey::try_new("crm-record-test").unwrap(),
                record_id: Uuid::from_u128(11),
                command: CrmRecordCommand::Create {
                    kind: CrmRecordKind::Activity,
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
                purpose: "crm.record.test".into(),
                customer_visible: false,
                module: Some("crm".into()),
            },
            expected_version: None,
        }
    }

    #[derive(Clone, Default)]
    struct Repo;
    #[async_trait]
    impl CrmRecordWriteRepository for Repo {
        async fn load_crm_record(
            &self,
            _: &TenantScope,
            _: Uuid,
        ) -> Result<Option<CrmRecord>, PortError> {
            Ok(None)
        }
    }

    #[derive(Clone, Default)]
    struct Uow {
        receipt: Arc<Mutex<Option<(CrmRecordCommandIdentity, StoredCrmRecordCommit)>>>,
        commits: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl CrmRecordUnitOfWork for Uow {
        async fn find_crm_record_receipt(
            &self,
            identity: &CrmRecordCommandIdentity,
        ) -> Result<Option<StoredCrmRecordCommit>, PortError> {
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
        async fn commit_crm_record(
            &self,
            commit: CrmRecordCommit,
        ) -> Result<StoredCrmRecordCommit, PortError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            let value = StoredCrmRecordCommit {
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
    impl CrmRecordAuthorizationPort for Auth {
        async fn authorize_crm_record(
            &self,
            request: CrmRecordAuthorizationRequest<'_>,
        ) -> Result<(), PortError> {
            assert_eq!(request.action, CrmRecordAction::Create);
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
            CrmRecordCommandHandler::new(Repo, uow.clone(), Auth(true), FixedClock::new(at()));
        let first = handler.execute(request()).await.unwrap();
        let retry = handler.execute(request()).await.unwrap();
        assert!(!first.replayed && retry.replayed);
        assert_eq!(uow.commits.load(Ordering::SeqCst), 1);

        let denied = Uow::default();
        let handler =
            CrmRecordCommandHandler::new(Repo, denied.clone(), Auth(false), FixedClock::new(at()));
        assert!(matches!(
            handler.execute(request()).await,
            Err(CrmRecordApplicationError::Port(PortError::Denied))
        ));
        assert_eq!(denied.commits.load(Ordering::SeqCst), 0);
    }
}
