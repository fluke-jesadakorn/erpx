use erpx_kernel::{Clock, HumanDecisionError, HumanDecisionProof};
use thiserror::Error;

use crate::{
    OrganizationDomainError, OrganizationSharingAgreement, SharingApprovalSide,
    ports::{
        PortError, SharingAction, SharingAuthorizationPort, SharingAuthorizationRequest,
        SharingCommandPayload, SharingCommandRequest, SharingCommandUnitOfWork, SharingCommit,
        SharingWriteRepository, StoredSharingCommit,
    },
    sharing_approval_decision_ref,
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SharingApplicationError {
    #[error(transparent)]
    Domain(#[from] OrganizationDomainError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("sharing agreement was not found")]
    NotFound,
    #[error("sharing create cannot declare an existing version")]
    UnexpectedCreateVersion,
    #[error("sharing mutation requires the exact expected version")]
    MissingExpectedVersion,
    #[error("sharing authority does not cover the required agreement side")]
    ScopeOutsideAuthority,
    #[error("sharing payload actor does not match the authenticated actor")]
    ActorMismatch,
    #[error(transparent)]
    HumanDecision(#[from] HumanDecisionError),
    #[error("sharing human decision proof is not bound to the exact command")]
    HumanDecisionBinding,
}

pub struct OrganizationSharingCommandHandler<W, U, A, C> {
    repository: W,
    unit_of_work: U,
    authorization: A,
    clock: C,
}

impl<W, U, A, C> OrganizationSharingCommandHandler<W, U, A, C> {
    pub const fn new(repository: W, unit_of_work: U, authorization: A, clock: C) -> Self {
        Self {
            repository,
            unit_of_work,
            authorization,
            clock,
        }
    }
}

impl<W, U, A, C> OrganizationSharingCommandHandler<W, U, A, C>
where
    W: SharingWriteRepository,
    U: SharingCommandUnitOfWork,
    A: SharingAuthorizationPort,
    C: Clock,
{
    pub async fn execute(
        &self,
        request: SharingCommandRequest,
    ) -> Result<StoredSharingCommit, SharingApplicationError> {
        let human_decision = match request.identity.payload.action() {
            SharingAction::ApproveSource | SharingAction::ApproveTarget => {
                Some(sharing_human_decision(&request, self.clock.now())?)
            }
            _ => None,
        };
        if let Some(receipt) = self
            .unit_of_work
            .find_sharing_receipt(&request.identity)
            .await?
        {
            self.authorize(&request, &receipt.aggregate).await?;
            return Ok(receipt);
        }

        let now = human_decision
            .as_ref()
            .map_or_else(|| self.clock.now(), HumanDecisionProof::decided_at);
        let aggregate = match &request.identity.payload {
            SharingCommandPayload::Propose(command) => {
                if request.expected_version.is_some() {
                    return Err(SharingApplicationError::UnexpectedCreateVersion);
                }
                if command.id != request.identity.sharing_agreement_id {
                    return Err(SharingApplicationError::ActorMismatch);
                }
                ensure_actor(&request, &command.proposed_by)?;
                let aggregate = OrganizationSharingAgreement::propose(command.as_ref().clone())?;
                self.authorize(&request, &aggregate).await?;
                aggregate
            }
            _ => {
                let expected = request
                    .expected_version
                    .ok_or(SharingApplicationError::MissingExpectedVersion)?;
                let mut aggregate = self
                    .repository
                    .load_sharing_agreement(
                        &request.authority_scope,
                        request.identity.sharing_agreement_id,
                    )
                    .await?
                    .ok_or(SharingApplicationError::NotFound)?;
                self.authorize(&request, &aggregate).await?;
                match &request.identity.payload {
                    SharingCommandPayload::ApproveSource { approver_ref } => {
                        ensure_actor(&request, approver_ref)?;
                        let decision = human_decision
                            .clone()
                            .ok_or(SharingApplicationError::HumanDecisionBinding)?;
                        aggregate.approve_source(
                            expected,
                            request.identity.command_id,
                            decision,
                        )?;
                    }
                    SharingCommandPayload::ApproveTarget { approver_ref } => {
                        ensure_actor(&request, approver_ref)?;
                        let decision = human_decision
                            .clone()
                            .ok_or(SharingApplicationError::HumanDecisionBinding)?;
                        aggregate.approve_target(
                            expected,
                            request.identity.command_id,
                            decision,
                        )?;
                    }
                    SharingCommandPayload::Reject { actor_ref } => {
                        ensure_actor(&request, actor_ref)?;
                        aggregate.reject(expected, actor_ref, now)?;
                    }
                    SharingCommandPayload::Revoke { actor_ref } => {
                        ensure_actor(&request, actor_ref)?;
                        aggregate.revoke(expected, actor_ref, now)?;
                    }
                    SharingCommandPayload::Expire => {
                        aggregate.expire(expected, now)?;
                    }
                    SharingCommandPayload::Propose(_) => unreachable!("handled above"),
                }
                aggregate
            }
        };
        // Revalidates same-holding and lifecycle invariants before persistence.
        if aggregate.id() != request.identity.sharing_agreement_id {
            return Err(SharingApplicationError::ActorMismatch);
        }
        self.unit_of_work
            .commit_sharing(SharingCommit {
                request,
                aggregate,
                changed_at: now,
            })
            .await
            .map_err(Into::into)
    }

    async fn authorize(
        &self,
        request: &SharingCommandRequest,
        aggregate: &OrganizationSharingAgreement,
    ) -> Result<(), SharingApplicationError> {
        let required = match request.identity.payload.action() {
            SharingAction::Propose | SharingAction::ApproveSource | SharingAction::Revoke => {
                aggregate.source_scope()
            }
            SharingAction::ApproveTarget => aggregate.target_scope(),
            SharingAction::Reject | SharingAction::Expire => {
                if request.authority_scope.contains(aggregate.source_scope()) {
                    aggregate.source_scope()
                } else {
                    aggregate.target_scope()
                }
            }
        };
        if !request.authority_scope.contains(required) {
            return Err(SharingApplicationError::ScopeOutsideAuthority);
        }
        self.authorization
            .authorize(SharingAuthorizationRequest {
                actor: &request.actor,
                authority_scope: &request.authority_scope,
                source_scope: aggregate.source_scope(),
                target_scope: aggregate.target_scope(),
                action: request.identity.payload.action(),
            })
            .await?;
        Ok(())
    }
}

fn sharing_human_decision(
    request: &SharingCommandRequest,
    at: chrono::DateTime<chrono::Utc>,
) -> Result<HumanDecisionProof, SharingApplicationError> {
    let (approver_ref, side) = match &request.identity.payload {
        SharingCommandPayload::ApproveSource { approver_ref } => {
            (approver_ref, SharingApprovalSide::Source)
        }
        SharingCommandPayload::ApproveTarget { approver_ref } => {
            (approver_ref, SharingApprovalSide::Target)
        }
        _ => return Err(SharingApplicationError::HumanDecisionBinding),
    };
    ensure_actor(request, approver_ref)?;
    let proof = HumanDecisionProof::try_new(
        request.actor.clone(),
        request.authority_scope.clone(),
        sharing_approval_decision_ref(
            request.identity.command_id,
            request.identity.sharing_agreement_id,
            side,
        ),
        at,
    )?;
    validate_sharing_human_decision(request, side, at, &proof)?;
    Ok(proof)
}

fn validate_sharing_human_decision(
    request: &SharingCommandRequest,
    side: SharingApprovalSide,
    at: chrono::DateTime<chrono::Utc>,
    proof: &HumanDecisionProof,
) -> Result<(), SharingApplicationError> {
    if proof.actor() != &request.actor
        || proof.authority_scope() != &request.authority_scope
        || proof.decided_at() != at
        || proof.decision_ref()
            != sharing_approval_decision_ref(
                request.identity.command_id,
                request.identity.sharing_agreement_id,
                side,
            )
    {
        return Err(SharingApplicationError::HumanDecisionBinding);
    }
    Ok(())
}

fn ensure_actor(
    request: &SharingCommandRequest,
    actor_ref: &str,
) -> Result<(), SharingApplicationError> {
    if actor_ref != request.actor.principal_ref {
        return Err(SharingApplicationError::ActorMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Poll, Wake, Waker},
    };

    use chrono::{TimeZone, Utc};
    use erpx_cqrs::IdempotencyKey;
    use erpx_kernel::{
        ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
        FixedClock, SessionId,
    };
    use uuid::Uuid;

    use super::*;
    use crate::{SharingAgreementId, ports::SharingCommandIdentity};

    #[derive(Clone, Default)]
    struct Probe {
        receipt_calls: Arc<AtomicUsize>,
        repository_calls: Arc<AtomicUsize>,
        authorization_calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl SharingWriteRepository for Probe {
        async fn load_sharing_agreement(
            &self,
            _authority_scope: &AuthorityScope,
            _sharing_agreement_id: SharingAgreementId,
        ) -> Result<Option<OrganizationSharingAgreement>, PortError> {
            self.repository_calls.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }
    }

    #[async_trait::async_trait]
    impl SharingCommandUnitOfWork for Probe {
        async fn find_sharing_receipt(
            &self,
            _identity: &SharingCommandIdentity,
        ) -> Result<Option<StoredSharingCommit>, PortError> {
            self.receipt_calls.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn commit_sharing(
            &self,
            _request: SharingCommit,
        ) -> Result<StoredSharingCommit, PortError> {
            Err(PortError::Internal)
        }
    }

    #[async_trait::async_trait]
    impl SharingAuthorizationPort for Probe {
        async fn authorize(
            &self,
            _request: SharingAuthorizationRequest<'_>,
        ) -> Result<(), PortError> {
            self.authorization_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn at() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 21, 10, 0, 0)
            .single()
            .unwrap()
    }

    fn request(kind: ActorKind) -> SharingCommandRequest {
        SharingCommandRequest {
            identity: SharingCommandIdentity {
                command_id: CommandId::try_from_uuid(Uuid::from_u128(200)).unwrap(),
                idempotency_key: IdempotencyKey::try_new("sharing-approval-200").unwrap(),
                sharing_agreement_id: SharingAgreementId::try_from_uuid(Uuid::from_u128(201))
                    .unwrap(),
                payload: SharingCommandPayload::ApproveSource {
                    approver_ref: "sharing-reviewer".into(),
                },
            },
            correlation_id: CorrelationId::try_from_uuid(Uuid::from_u128(202)).unwrap(),
            actor: ActorAttribution {
                principal_ref: "sharing-reviewer".into(),
                kind,
                session_id: Some(SessionId::try_from_uuid(Uuid::from_u128(203)).unwrap()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            authority_scope: AuthorityScope::Holding {
                holding_id: Uuid::from_u128(204),
            },
            audit: AuditContext {
                purpose: "organization sharing approval test".into(),
                customer_visible: false,
                module: Some("organization".into()),
            },
            expected_version: Some(1),
        }
    }

    #[test]
    fn agent_and_service_replay_attempts_are_denied_before_receipt_or_repository_access() {
        for kind in [ActorKind::Agent, ActorKind::Service] {
            let probe = Probe::default();
            let handler = OrganizationSharingCommandHandler::new(
                probe.clone(),
                probe.clone(),
                probe.clone(),
                FixedClock::new(at()),
            );
            assert!(matches!(
                block_on(handler.execute(request(kind))),
                Err(SharingApplicationError::HumanDecision(_))
            ));
            assert_eq!(probe.receipt_calls.load(Ordering::SeqCst), 0);
            assert_eq!(probe.repository_calls.load(Ordering::SeqCst), 0);
            assert_eq!(probe.authorization_calls.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn proof_reference_cannot_be_reused_for_another_command_or_side() {
        let first = request(ActorKind::HoldingUser);
        let proof = sharing_human_decision(&first, at()).unwrap();
        assert!(
            validate_sharing_human_decision(&first, SharingApprovalSide::Source, at(), &proof,)
                .is_ok()
        );

        let mut another_command = first.clone();
        another_command.identity.command_id =
            CommandId::try_from_uuid(Uuid::from_u128(205)).unwrap();
        assert_eq!(
            validate_sharing_human_decision(
                &another_command,
                SharingApprovalSide::Source,
                at(),
                &proof,
            ),
            Err(SharingApplicationError::HumanDecisionBinding)
        );

        let mut another_side = first;
        another_side.identity.payload = SharingCommandPayload::ApproveTarget {
            approver_ref: "sharing-reviewer".into(),
        };
        assert_eq!(
            validate_sharing_human_decision(
                &another_side,
                SharingApprovalSide::Target,
                at(),
                &proof,
            ),
            Err(SharingApplicationError::HumanDecisionBinding)
        );
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
