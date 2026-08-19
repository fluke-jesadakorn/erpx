use erpx_kernel::Clock;
use thiserror::Error;

use crate::{
    AccessCertification, BreakGlassSession, HighRiskAuthority, IdentityAccessDomainError,
    IdentityGovernanceRecord,
    ports::{
        IdentityGovernanceAuthorizationPort, IdentityGovernanceAuthorizationRequest,
        IdentityGovernanceCommand, IdentityGovernanceCommandRequest, IdentityGovernanceCommit,
        IdentityGovernanceUnitOfWork, IdentityGovernanceWriteRepository, PortError,
        StoredIdentityGovernanceCommit,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IdentityGovernanceApplicationError {
    #[error(transparent)]
    Domain(#[from] IdentityAccessDomainError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("identity governance record was not found")]
    NotFound,
    #[error("governance create cannot declare an existing version")]
    UnexpectedCreateVersion,
    #[error("governance mutation requires the exact expected version")]
    MissingExpectedVersion,
    #[error("identity governance record version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("governance command does not match the stored record kind")]
    RecordKindMismatch,
    #[error("command actor does not match the governed evidence principal")]
    ActorMismatch,
    #[error("command authority does not contain the record scope")]
    ScopeOutsideAuthority,
}

pub struct IdentityGovernanceCommandHandler<W, U, A, C> {
    repository: W,
    unit_of_work: U,
    authorization: A,
    clock: C,
}

impl<W, U, A, C> IdentityGovernanceCommandHandler<W, U, A, C> {
    pub const fn new(repository: W, unit_of_work: U, authorization: A, clock: C) -> Self {
        Self {
            repository,
            unit_of_work,
            authorization,
            clock,
        }
    }
}

impl<W, U, A, C> IdentityGovernanceCommandHandler<W, U, A, C>
where
    W: IdentityGovernanceWriteRepository,
    U: IdentityGovernanceUnitOfWork,
    A: IdentityGovernanceAuthorizationPort,
    C: Clock,
{
    pub async fn execute(
        &self,
        request: IdentityGovernanceCommandRequest,
    ) -> Result<StoredIdentityGovernanceCommit, IdentityGovernanceApplicationError> {
        if request.identity.record_id.is_nil() {
            return Err(IdentityAccessDomainError::NilIdentifier {
                field: "identity_governance_record_id",
            }
            .into());
        }
        if let Some(receipt) = self
            .unit_of_work
            .find_governance_receipt(&request.identity)
            .await?
        {
            self.authorize(&request, &receipt.record).await?;
            return Ok(receipt);
        }

        let at = self.clock.now();
        let record = if request.identity.command.is_create() {
            if request.expected_version.is_some() {
                return Err(IdentityGovernanceApplicationError::UnexpectedCreateVersion);
            }
            match &request.identity.command {
                IdentityGovernanceCommand::StartAccessReview {
                    reviewer_ref,
                    due_at,
                    items,
                } => IdentityGovernanceRecord::AccessReview(AccessCertification::start(
                    request.identity.record_id,
                    request.authority_scope.clone(),
                    reviewer_ref.clone(),
                    *due_at,
                    items.clone(),
                )?),
                IdentityGovernanceCommand::RequestBreakGlass {
                    principal_id,
                    reason,
                    valid_until,
                } => IdentityGovernanceRecord::BreakGlass(BreakGlassSession::request(
                    request.identity.record_id,
                    *principal_id,
                    request.authority_scope.clone(),
                    reason.clone(),
                    request.actor.principal_ref.clone(),
                    at,
                    *valid_until,
                )?),
                _ => unreachable!("create commands are matched above"),
            }
        } else {
            let expected = request
                .expected_version
                .ok_or(IdentityGovernanceApplicationError::MissingExpectedVersion)?;
            let mut record = self
                .repository
                .load_governance_record(&request.authority_scope, request.identity.record_id)
                .await?
                .ok_or(IdentityGovernanceApplicationError::NotFound)?;
            if record.version() != expected {
                return Err(IdentityGovernanceApplicationError::VersionConflict {
                    expected,
                    actual: record.version(),
                });
            }
            match (&request.identity.command, &mut record) {
                (
                    IdentityGovernanceCommand::DecideAccessReview {
                        assignment_id,
                        decision,
                    },
                    IdentityGovernanceRecord::AccessReview(review),
                ) => {
                    require_actor(&request, review.reviewer_ref())?;
                    review.decide(expected, *assignment_id, *decision, at)?;
                }
                (
                    IdentityGovernanceCommand::ActivateBreakGlass {
                        requester_authenticated_at,
                        requester_mfa_at,
                        approver_ref,
                        approver_authenticated_at,
                        approver_mfa_at,
                        monitoring_available,
                    },
                    IdentityGovernanceRecord::BreakGlass(session),
                ) => {
                    require_actor(&request, approver_ref)?;
                    let requester_ref = session.requester_ref().to_owned();
                    session.activate(
                        expected,
                        &HighRiskAuthority {
                            requester_ref: &requester_ref,
                            approver_ref,
                            requester_authenticated_at: *requester_authenticated_at,
                            requester_mfa_at: *requester_mfa_at,
                            approver_authenticated_at: *approver_authenticated_at,
                            approver_mfa_at: *approver_mfa_at,
                            evaluated_at: at,
                        },
                        *monitoring_available,
                    )?;
                }
                (
                    IdentityGovernanceCommand::TerminateBreakGlass,
                    IdentityGovernanceRecord::BreakGlass(session),
                ) => session.terminate(expected, at)?,
                (
                    IdentityGovernanceCommand::ExpireBreakGlass,
                    IdentityGovernanceRecord::BreakGlass(session),
                ) => session.expire(expected, at)?,
                (
                    IdentityGovernanceCommand::RecordBreakGlassReview { reviewer_ref },
                    IdentityGovernanceRecord::BreakGlass(session),
                ) => {
                    require_actor(&request, reviewer_ref)?;
                    session.record_after_review(expected, reviewer_ref, at)?;
                }
                _ => return Err(IdentityGovernanceApplicationError::RecordKindMismatch),
            }
            record
        };
        record.validate_rehydrated()?;
        self.authorize(&request, &record).await?;
        self.unit_of_work
            .commit_governance(IdentityGovernanceCommit {
                request,
                record,
                changed_at: at,
            })
            .await
            .map_err(Into::into)
    }

    async fn authorize(
        &self,
        request: &IdentityGovernanceCommandRequest,
        record: &IdentityGovernanceRecord,
    ) -> Result<(), IdentityGovernanceApplicationError> {
        if !request.authority_scope.contains(record.scope()) {
            return Err(IdentityGovernanceApplicationError::ScopeOutsideAuthority);
        }
        self.authorization
            .authorize_governance(IdentityGovernanceAuthorizationRequest {
                actor: &request.actor,
                authority_scope: &request.authority_scope,
                target_scope: record.scope(),
                action: request.identity.command.action(),
            })
            .await?;
        Ok(())
    }
}

fn require_actor(
    request: &IdentityGovernanceCommandRequest,
    expected: &str,
) -> Result<(), IdentityGovernanceApplicationError> {
    if request.actor.principal_ref != expected {
        Err(IdentityGovernanceApplicationError::ActorMismatch)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Poll, Wake, Waker},
    };

    use async_trait::async_trait;
    use chrono::{Duration, TimeZone, Utc};
    use erpx_cqrs::IdempotencyKey;
    use erpx_kernel::{
        ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
        FixedClock,
    };
    use uuid::Uuid;

    use super::*;
    use crate::{
        IdentityGovernanceAction, IdentityGovernanceAuthorizationRequest,
        IdentityGovernanceCommandIdentity, PrincipalId, StoredIdentityGovernanceCommit,
    };

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn at() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 16, 10, 0, 0)
            .single()
            .unwrap()
    }

    fn scope() -> AuthorityScope {
        AuthorityScope::Holding { holding_id: id(1) }
    }

    fn request(sequence: u128) -> IdentityGovernanceCommandRequest {
        IdentityGovernanceCommandRequest {
            identity: IdentityGovernanceCommandIdentity {
                command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
                idempotency_key: IdempotencyKey::try_new(format!("iam-governance-{sequence}"))
                    .unwrap(),
                record_id: id(200 + sequence),
                command: IdentityGovernanceCommand::RequestBreakGlass {
                    principal_id: PrincipalId::try_from_uuid(id(300 + sequence)).unwrap(),
                    reason: "restore payroll availability".into(),
                    valid_until: at() + Duration::minutes(30),
                },
            },
            correlation_id: CorrelationId::try_from_uuid(id(400 + sequence)).unwrap(),
            actor: ActorAttribution {
                principal_ref: "requester:security".into(),
                kind: ActorKind::HoldingUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            authority_scope: scope(),
            audit: AuditContext {
                purpose: "iam.break-glass.test".into(),
                customer_visible: false,
                module: Some("identity_access".into()),
            },
            expected_version: None,
        }
    }

    #[derive(Clone, Default)]
    struct FakeRepository {
        record: Arc<Mutex<Option<IdentityGovernanceRecord>>>,
    }

    #[async_trait]
    impl IdentityGovernanceWriteRepository for FakeRepository {
        async fn load_governance_record(
            &self,
            _authority_scope: &AuthorityScope,
            _record_id: Uuid,
        ) -> Result<Option<IdentityGovernanceRecord>, PortError> {
            Ok(self.record.lock().unwrap().clone())
        }
    }

    #[derive(Clone, Default)]
    struct FakeUnitOfWork {
        receipt: Arc<
            Mutex<
                Option<(
                    IdentityGovernanceCommandIdentity,
                    StoredIdentityGovernanceCommit,
                )>,
            >,
        >,
        commits: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl IdentityGovernanceUnitOfWork for FakeUnitOfWork {
        async fn find_governance_receipt(
            &self,
            identity: &IdentityGovernanceCommandIdentity,
        ) -> Result<Option<StoredIdentityGovernanceCommit>, PortError> {
            let receipt = self.receipt.lock().unwrap();
            match receipt.as_ref() {
                Some((stored, value)) if stored == identity => {
                    let mut replay = value.clone();
                    replay.replayed = true;
                    Ok(Some(replay))
                }
                Some((stored, _))
                    if stored.command_id == identity.command_id
                        || stored.idempotency_key == identity.idempotency_key =>
                {
                    Err(PortError::IdempotencyConflict)
                }
                _ => Ok(None),
            }
        }

        async fn commit_governance(
            &self,
            request: IdentityGovernanceCommit,
        ) -> Result<StoredIdentityGovernanceCommit, PortError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            let value = StoredIdentityGovernanceCommit {
                command_id: request.request.identity.command_id,
                correlation_id: request.request.correlation_id,
                record: request.record,
                committed_at: request.changed_at,
                replayed: false,
            };
            *self.receipt.lock().unwrap() = Some((request.request.identity, value.clone()));
            Ok(value)
        }
    }

    #[derive(Clone)]
    struct FakeAuthorization(bool);

    #[async_trait]
    impl IdentityGovernanceAuthorizationPort for FakeAuthorization {
        async fn authorize_governance(
            &self,
            request: IdentityGovernanceAuthorizationRequest<'_>,
        ) -> Result<(), PortError> {
            assert_eq!(request.action, IdentityGovernanceAction::RequestBreakGlass);
            if self.0 {
                Ok(())
            } else {
                Err(PortError::Denied)
            }
        }
    }

    #[test]
    fn exact_retry_replays_without_second_governance_commit() {
        let unit = FakeUnitOfWork::default();
        let handler = IdentityGovernanceCommandHandler::new(
            FakeRepository::default(),
            unit.clone(),
            FakeAuthorization(true),
            FixedClock::new(at()),
        );
        let request = request(1);
        let first = block_on(handler.execute(request.clone())).unwrap();
        let replay = block_on(handler.execute(request)).unwrap();
        assert!(!first.replayed);
        assert!(replay.replayed);
        assert_eq!(first.record, replay.record);
        assert_eq!(unit.commits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn authorization_denial_never_reaches_the_governance_uow() {
        let unit = FakeUnitOfWork::default();
        let handler = IdentityGovernanceCommandHandler::new(
            FakeRepository::default(),
            unit.clone(),
            FakeAuthorization(false),
            FixedClock::new(at()),
        );
        assert!(matches!(
            block_on(handler.execute(request(2))),
            Err(IdentityGovernanceApplicationError::Port(PortError::Denied))
        ));
        assert_eq!(unit.commits.load(Ordering::SeqCst), 0);
    }

    struct ThreadWaker;

    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {}
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(ThreadWaker));
        let mut context = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }
}
