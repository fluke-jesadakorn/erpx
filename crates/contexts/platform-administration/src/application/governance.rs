use erpx_kernel::Clock;
use thiserror::Error;

use crate::{
    domain::{PlatformGovernanceRecord, PlatformGovernanceRecordError},
    ports::{
        PlatformGovernanceRecordAuthorizationPort, PlatformGovernanceRecordAuthorizationRequest,
        PlatformGovernanceRecordCommand, PlatformGovernanceRecordCommandRequest,
        PlatformGovernanceRecordCommit, PlatformGovernanceRecordQuery,
        PlatformGovernanceRecordReadRepository, PlatformGovernanceRecordUnitOfWork,
        PlatformGovernanceRecordWriteRepository, PortError, StoredPlatformGovernanceRecordCommit,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlatformGovernanceRecordApplicationError {
    #[error(transparent)]
    Domain(#[from] PlatformGovernanceRecordError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("platform governance record was not found")]
    NotFound,
    #[error("platform governance create cannot declare a current version")]
    UnexpectedCreateVersion,
    #[error("platform governance transition requires the exact current version")]
    MissingExpectedVersion,
    #[error("platform governance authority does not contain its scope")]
    ScopeOutsideAuthority,
}

pub struct PlatformGovernanceRecordCommandHandler<W, U, A, C> {
    repository: W,
    unit_of_work: U,
    authorization: A,
    clock: C,
}

pub struct PlatformGovernanceRecordQueryHandler<R, A> {
    repository: R,
    authorization: A,
}

impl<R, A> PlatformGovernanceRecordQueryHandler<R, A> {
    pub const fn new(repository: R, authorization: A) -> Self {
        Self {
            repository,
            authorization,
        }
    }
}

impl<R, A> PlatformGovernanceRecordQueryHandler<R, A>
where
    R: PlatformGovernanceRecordReadRepository,
    A: PlatformGovernanceRecordAuthorizationPort,
{
    pub async fn execute(
        &self,
        request: PlatformGovernanceRecordQuery,
    ) -> Result<
        crate::GovernanceReadProjection<Option<PlatformGovernanceRecord>>,
        PlatformGovernanceRecordApplicationError,
    > {
        if !request.authority_scope.contains(&request.target_scope) {
            return Err(PlatformGovernanceRecordApplicationError::ScopeOutsideAuthority);
        }
        self.authorization
            .authorize_platform_governance_record(PlatformGovernanceRecordAuthorizationRequest {
                actor: &request.actor,
                authority_scope: &request.authority_scope,
                target_scope: &request.target_scope,
                action: crate::PlatformGovernanceRecordAction::View,
                kind: request.expected_kind,
            })
            .await?;
        let projection = self
            .repository
            .get_platform_governance_record(
                &request.authority_scope,
                request.record_id,
                &request.consistency,
            )
            .await?;
        if projection.value.as_ref().is_some_and(|record| {
            record.scope() != &request.target_scope || record.kind() != request.expected_kind
        }) {
            return Err(PlatformGovernanceRecordApplicationError::ScopeOutsideAuthority);
        }
        Ok(projection)
    }
}

impl<W, U, A, C> PlatformGovernanceRecordCommandHandler<W, U, A, C> {
    pub const fn new(repository: W, unit_of_work: U, authorization: A, clock: C) -> Self {
        Self {
            repository,
            unit_of_work,
            authorization,
            clock,
        }
    }
}

impl<W, U, A, C> PlatformGovernanceRecordCommandHandler<W, U, A, C>
where
    W: PlatformGovernanceRecordWriteRepository,
    U: PlatformGovernanceRecordUnitOfWork,
    A: PlatformGovernanceRecordAuthorizationPort,
    C: Clock,
{
    pub async fn execute(
        &self,
        request: PlatformGovernanceRecordCommandRequest,
    ) -> Result<StoredPlatformGovernanceRecordCommit, PlatformGovernanceRecordApplicationError>
    {
        if let Some(receipt) = self
            .unit_of_work
            .find_platform_governance_record_receipt(&request.identity)
            .await?
        {
            self.authorize(&request, &receipt.record).await?;
            return Ok(receipt);
        }
        let at = self.clock.now();
        let record = match &request.identity.command {
            PlatformGovernanceRecordCommand::Create {
                kind,
                scope,
                title,
                subject_ref,
            } => {
                if request.expected_version.is_some() {
                    return Err(PlatformGovernanceRecordApplicationError::UnexpectedCreateVersion);
                }
                PlatformGovernanceRecord::create(
                    request.identity.record_id,
                    *kind,
                    scope.clone(),
                    title.clone(),
                    subject_ref.clone(),
                    at,
                )?
            }
            PlatformGovernanceRecordCommand::Transition {
                status,
                evidence_ref,
            } => {
                let expected = request
                    .expected_version
                    .ok_or(PlatformGovernanceRecordApplicationError::MissingExpectedVersion)?;
                let mut record = self
                    .repository
                    .load_platform_governance_record(
                        &request.authority_scope,
                        request.identity.record_id,
                    )
                    .await?
                    .ok_or(PlatformGovernanceRecordApplicationError::NotFound)?;
                record.transition(expected, *status, evidence_ref.clone(), at)?;
                record
            }
        };
        self.authorize(&request, &record).await?;
        self.unit_of_work
            .commit_platform_governance_record(PlatformGovernanceRecordCommit {
                request,
                record,
                changed_at: at,
            })
            .await
            .map_err(Into::into)
    }

    async fn authorize(
        &self,
        request: &PlatformGovernanceRecordCommandRequest,
        record: &PlatformGovernanceRecord,
    ) -> Result<(), PlatformGovernanceRecordApplicationError> {
        if !request.authority_scope.contains(record.scope()) {
            return Err(PlatformGovernanceRecordApplicationError::ScopeOutsideAuthority);
        }
        self.authorization
            .authorize_platform_governance_record(PlatformGovernanceRecordAuthorizationRequest {
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
    use erpx_cqrs::{FreshnessState, IdempotencyKey, ProjectionMetadata, QueryConsistency};
    use erpx_kernel::{
        ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, FixedClock,
        TenantScope,
    };
    use uuid::Uuid;

    use super::*;
    use crate::{
        PlatformGovernanceRecordAction, PlatformGovernanceRecordCommandIdentity,
        PlatformGovernanceRecordKind,
    };

    fn at() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 16, 12, 0, 0)
            .single()
            .unwrap()
    }
    fn request() -> PlatformGovernanceRecordCommandRequest {
        PlatformGovernanceRecordCommandRequest {
            identity: PlatformGovernanceRecordCommandIdentity {
                command_id: CommandId::try_from_uuid(Uuid::from_u128(10)).unwrap(),
                idempotency_key: IdempotencyKey::try_new("platform-governance-test").unwrap(),
                record_id: Uuid::new_v4(),
                command: PlatformGovernanceRecordCommand::Create {
                    kind: PlatformGovernanceRecordKind::Release,
                    scope: TenantScope::Platform,
                    title: "Release 2026.08".into(),
                    subject_ref: "artifact:sha256:abc".into(),
                },
            },
            correlation_id: CorrelationId::try_from_uuid(Uuid::from_u128(12)).unwrap(),
            actor: ActorAttribution {
                principal_ref: "platform:user:1".into(),
                kind: ActorKind::PlatformUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            authority_scope: TenantScope::Platform,
            audit: AuditContext {
                purpose: "platform.governance.test".into(),
                customer_visible: false,
                module: Some("platform".into()),
            },
            expected_version: None,
        }
    }

    #[derive(Clone)]
    struct Repo;
    #[async_trait]
    impl PlatformGovernanceRecordWriteRepository for Repo {
        async fn load_platform_governance_record(
            &self,
            _: &TenantScope,
            _: Uuid,
        ) -> Result<Option<PlatformGovernanceRecord>, PortError> {
            Ok(None)
        }
    }

    #[derive(Clone, Default)]
    struct Uow {
        receipt: Arc<
            Mutex<
                Option<(
                    crate::PlatformGovernanceRecordCommandIdentity,
                    StoredPlatformGovernanceRecordCommit,
                )>,
            >,
        >,
        commits: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl PlatformGovernanceRecordUnitOfWork for Uow {
        async fn find_platform_governance_record_receipt(
            &self,
            identity: &crate::PlatformGovernanceRecordCommandIdentity,
        ) -> Result<Option<StoredPlatformGovernanceRecordCommit>, PortError> {
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
        async fn commit_platform_governance_record(
            &self,
            commit: PlatformGovernanceRecordCommit,
        ) -> Result<StoredPlatformGovernanceRecordCommit, PortError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            let value = StoredPlatformGovernanceRecordCommit {
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
    impl PlatformGovernanceRecordAuthorizationPort for Auth {
        async fn authorize_platform_governance_record(
            &self,
            request: PlatformGovernanceRecordAuthorizationRequest<'_>,
        ) -> Result<(), PortError> {
            assert_eq!(request.action, PlatformGovernanceRecordAction::Create);
            if self.0 {
                Ok(())
            } else {
                Err(PortError::Denied)
            }
        }
    }

    #[derive(Clone)]
    struct ReadRepo(Arc<AtomicUsize>);
    #[async_trait]
    impl PlatformGovernanceRecordReadRepository for ReadRepo {
        async fn get_platform_governance_record(
            &self,
            _: &TenantScope,
            _: Uuid,
            _: &QueryConsistency,
        ) -> Result<crate::GovernanceReadProjection<Option<PlatformGovernanceRecord>>, PortError>
        {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(crate::GovernanceReadProjection {
                value: None,
                metadata: ProjectionMetadata {
                    checkpoint: None,
                    freshness: FreshnessState::Current,
                    lag_millis: Some(0),
                },
            })
        }
    }
    #[derive(Clone)]
    struct DenyViewAuth;
    #[async_trait]
    impl PlatformGovernanceRecordAuthorizationPort for DenyViewAuth {
        async fn authorize_platform_governance_record(
            &self,
            request: PlatformGovernanceRecordAuthorizationRequest<'_>,
        ) -> Result<(), PortError> {
            assert_eq!(request.action, PlatformGovernanceRecordAction::View);
            Err(PortError::Denied)
        }
    }

    #[tokio::test]
    async fn exact_retry_replays_and_denial_never_commits() {
        let uow = Uow::default();
        let handler = PlatformGovernanceRecordCommandHandler::new(
            Repo,
            uow.clone(),
            Auth(true),
            FixedClock::new(at()),
        );
        let retryable = request();
        assert!(!handler.execute(retryable.clone()).await.unwrap().replayed);
        assert!(handler.execute(retryable).await.unwrap().replayed);
        assert_eq!(uow.commits.load(Ordering::SeqCst), 1);
        let denied = Uow::default();
        let handler = PlatformGovernanceRecordCommandHandler::new(
            Repo,
            denied.clone(),
            Auth(false),
            FixedClock::new(at()),
        );
        assert!(matches!(
            handler.execute(request()).await,
            Err(PlatformGovernanceRecordApplicationError::Port(
                PortError::Denied
            ))
        ));
        assert_eq!(denied.commits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn query_authorizes_kind_and_scope_before_storage_lookup() {
        let reads = Arc::new(AtomicUsize::new(0));
        let handler =
            PlatformGovernanceRecordQueryHandler::new(ReadRepo(reads.clone()), DenyViewAuth);
        let query = PlatformGovernanceRecordQuery {
            record_id: Uuid::new_v4(),
            expected_kind: PlatformGovernanceRecordKind::Migration,
            actor: request().actor,
            authority_scope: TenantScope::Platform,
            target_scope: TenantScope::Platform,
            consistency: QueryConsistency::Eventual,
        };
        assert!(matches!(
            handler.execute(query).await,
            Err(PlatformGovernanceRecordApplicationError::Port(
                PortError::Denied
            ))
        ));
        assert_eq!(reads.load(Ordering::SeqCst), 0);
    }
}
