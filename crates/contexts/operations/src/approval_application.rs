use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_integration_contracts::RecordRef;
use erpx_kernel::{ActorAttribution, AuthorityScope, HumanDecisionProof};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    ApprovalGateKind, ApprovalGateSpec, ApprovalGateStatus, ApprovalSession, ApprovalSessionError,
    ApprovalSessionId, approval_session_decision_ref,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalSessionCommand {
    Create {
        session_id: ApprovalSessionId,
        subject: RecordRef,
        policy_version: u32,
        requested_by: String,
        gates: Vec<ApprovalGateSpec>,
    },
    Decide {
        session_id: ApprovalSessionId,
        subject_version: u64,
        gate: ApprovalGateKind,
        decision: ApprovalGateStatus,
        reason_code: String,
    },
    Cancel {
        session_id: ApprovalSessionId,
    },
    SupersedeSource {
        session_id: ApprovalSessionId,
        current_subject_version: u64,
    },
}

impl ApprovalSessionCommand {
    pub const fn session_id(&self) -> ApprovalSessionId {
        match self {
            Self::Create { session_id, .. }
            | Self::Decide { session_id, .. }
            | Self::Cancel { session_id }
            | Self::SupersedeSource { session_id, .. } => *session_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalSessionCommandRequest {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub scope: AuthorityScope,
    pub actor: ActorAttribution,
    pub expected_version: u64,
    pub command: ApprovalSessionCommand,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalSessionCommit {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub expected_version: u64,
    pub session: ApprovalSession,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalSessionReceipt {
    pub session: ApprovalSession,
    pub replayed: bool,
}

#[async_trait]
pub trait ApprovalSessionAuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        scope: &AuthorityScope,
        actor: &ActorAttribution,
        command: &ApprovalSessionCommand,
    ) -> Result<bool, ApprovalSessionApplicationError>;
}

#[async_trait]
pub trait ApprovalSessionRepository: Send + Sync {
    async fn load(
        &self,
        scope: &AuthorityScope,
        id: ApprovalSessionId,
    ) -> Result<Option<ApprovalSession>, ApprovalSessionApplicationError>;

    async fn commit(
        &self,
        scope: &AuthorityScope,
        commit: ApprovalSessionCommit,
    ) -> Result<ApprovalSessionReceipt, ApprovalSessionApplicationError>;

    async fn my_work(
        &self,
        scope: &AuthorityScope,
        principal_ref: &str,
        limit: u16,
    ) -> Result<Vec<ApprovalSession>, ApprovalSessionApplicationError>;
}

#[derive(Debug)]
pub struct ApprovalSessionService<R, A> {
    repository: R,
    authorization: A,
}

impl<R, A> ApprovalSessionService<R, A>
where
    R: ApprovalSessionRepository,
    A: ApprovalSessionAuthorizationPort,
{
    pub fn new(repository: R, authorization: A) -> Self {
        Self {
            repository,
            authorization,
        }
    }

    pub async fn execute(
        &self,
        request: ApprovalSessionCommandRequest,
    ) -> Result<ApprovalSessionReceipt, ApprovalSessionApplicationError> {
        validate_request(&request)?;
        let human_decision = approval_session_human_decision(&request)?;
        if !self
            .authorization
            .authorize(&request.scope, &request.actor, &request.command)
            .await?
        {
            return Err(ApprovalSessionApplicationError::Denied);
        }
        let mut session = match &request.command {
            ApprovalSessionCommand::Create {
                session_id,
                subject,
                policy_version,
                requested_by,
                gates,
            } => {
                if request.expected_version != 0 {
                    return Err(ApprovalSessionApplicationError::VersionConflict);
                }
                ApprovalSession::create(
                    *session_id,
                    request.scope.clone(),
                    subject.clone(),
                    *policy_version,
                    requested_by.clone(),
                    gates.clone(),
                    request.at,
                )?
            }
            _ => {
                let value = self
                    .repository
                    .load(&request.scope, request.command.session_id())
                    .await?
                    .ok_or(ApprovalSessionApplicationError::NotFound)?;
                if value.version() != request.expected_version {
                    return Err(ApprovalSessionApplicationError::VersionConflict);
                }
                value
            }
        };
        match request.command {
            ApprovalSessionCommand::Create { .. } => {}
            ApprovalSessionCommand::Decide {
                subject_version,
                gate,
                decision,
                reason_code,
                ..
            } => {
                let human_decision =
                    human_decision.ok_or(ApprovalSessionApplicationError::HumanDecisionRequired)?;
                session.decide(
                    request.command_id,
                    gate,
                    subject_version,
                    decision,
                    human_decision,
                    reason_code,
                )?;
            }
            ApprovalSessionCommand::Cancel { .. } => {
                session.cancel(&request.actor.principal_ref, request.at)?
            }
            ApprovalSessionCommand::SupersedeSource {
                current_subject_version,
                ..
            } => {
                session.supersede_if_source_changed(current_subject_version, request.at)?;
            }
        }
        self.repository
            .commit(
                &request.scope,
                ApprovalSessionCommit {
                    command_id: request.command_id,
                    idempotency_key: request.idempotency_key,
                    expected_version: request.expected_version,
                    session,
                },
            )
            .await
    }

    pub async fn get(
        &self,
        scope: &AuthorityScope,
        id: ApprovalSessionId,
    ) -> Result<Option<ApprovalSession>, ApprovalSessionApplicationError> {
        self.repository.load(scope, id).await
    }

    pub async fn my_work(
        &self,
        scope: &AuthorityScope,
        principal_ref: &str,
        limit: u16,
    ) -> Result<Vec<ApprovalSession>, ApprovalSessionApplicationError> {
        if principal_ref.trim().is_empty() || limit == 0 || limit > 100 {
            return Err(ApprovalSessionApplicationError::InvalidRequest);
        }
        self.repository.my_work(scope, principal_ref, limit).await
    }
}

fn approval_session_human_decision(
    request: &ApprovalSessionCommandRequest,
) -> Result<Option<HumanDecisionProof>, ApprovalSessionApplicationError> {
    let ApprovalSessionCommand::Decide {
        session_id,
        subject_version,
        gate,
        decision,
        ..
    } = &request.command
    else {
        return Ok(None);
    };
    let proof = HumanDecisionProof::try_new(
        request.actor.clone(),
        request.scope.clone(),
        approval_session_decision_ref(
            request.command_id,
            *session_id,
            *subject_version,
            *gate,
            *decision,
        ),
        request.at,
    )
    .map_err(|_| ApprovalSessionApplicationError::HumanDecisionRequired)?;
    validate_approval_session_human_decision(request, &proof)?;
    Ok(Some(proof))
}

fn validate_approval_session_human_decision(
    request: &ApprovalSessionCommandRequest,
    proof: &HumanDecisionProof,
) -> Result<(), ApprovalSessionApplicationError> {
    let ApprovalSessionCommand::Decide {
        session_id,
        subject_version,
        gate,
        decision,
        ..
    } = &request.command
    else {
        return Err(ApprovalSessionApplicationError::InvalidRequest);
    };
    if proof.actor() != &request.actor
        || proof.authority_scope() != &request.scope
        || proof.decided_at() != request.at
        || proof.decision_ref()
            != approval_session_decision_ref(
                request.command_id,
                *session_id,
                *subject_version,
                *gate,
                *decision,
            )
    {
        return Err(ApprovalSessionApplicationError::HumanDecisionRequired);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApprovalSessionApplicationError {
    #[error("approval session request is invalid")]
    InvalidRequest,
    #[error("approval session access denied")]
    Denied,
    #[error("approval session not found")]
    NotFound,
    #[error("approval session version conflict")]
    VersionConflict,
    #[error("approval session storage is unavailable")]
    Unavailable,
    #[error("an authenticated human decision is required")]
    HumanDecisionRequired,
    #[error(transparent)]
    Domain(#[from] ApprovalSessionError),
}

fn validate_request(
    request: &ApprovalSessionCommandRequest,
) -> Result<(), ApprovalSessionApplicationError> {
    request
        .scope
        .validate()
        .map_err(|_| ApprovalSessionApplicationError::InvalidRequest)?;
    if request.command_id.is_nil()
        || request.idempotency_key.trim().is_empty()
        || request.idempotency_key.len() > 256
        || request.actor.validate().is_err()
    {
        return Err(ApprovalSessionApplicationError::InvalidRequest);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use chrono::TimeZone;
    use erpx_kernel::{ActorKind, SessionId};

    use super::*;

    #[derive(Clone, Default)]
    struct Repository {
        loads: Arc<AtomicUsize>,
        commits: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ApprovalSessionRepository for Repository {
        async fn load(
            &self,
            _scope: &AuthorityScope,
            _id: ApprovalSessionId,
        ) -> Result<Option<ApprovalSession>, ApprovalSessionApplicationError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn commit(
            &self,
            _scope: &AuthorityScope,
            _commit: ApprovalSessionCommit,
        ) -> Result<ApprovalSessionReceipt, ApprovalSessionApplicationError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            Err(ApprovalSessionApplicationError::Unavailable)
        }

        async fn my_work(
            &self,
            _scope: &AuthorityScope,
            _principal_ref: &str,
            _limit: u16,
        ) -> Result<Vec<ApprovalSession>, ApprovalSessionApplicationError> {
            Ok(Vec::new())
        }
    }

    #[derive(Clone, Default)]
    struct Authorization {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ApprovalSessionAuthorizationPort for Authorization {
        async fn authorize(
            &self,
            _scope: &AuthorityScope,
            _actor: &ActorAttribution,
            _command: &ApprovalSessionCommand,
        ) -> Result<bool, ApprovalSessionApplicationError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        }
    }

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 21, 9, 0, 0).single().unwrap()
    }

    fn request(kind: ActorKind) -> ApprovalSessionCommandRequest {
        ApprovalSessionCommandRequest {
            command_id: Uuid::from_u128(100),
            idempotency_key: "approval-session-decision-100".into(),
            scope: AuthorityScope::Holding {
                holding_id: Uuid::from_u128(1),
            },
            actor: ActorAttribution {
                principal_ref: "approval-reviewer".into(),
                kind,
                session_id: Some(SessionId::try_from_uuid(Uuid::from_u128(2)).unwrap()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            expected_version: 1,
            command: ApprovalSessionCommand::Decide {
                session_id: ApprovalSessionId::try_from_uuid(Uuid::from_u128(3)).unwrap(),
                subject_version: 4,
                gate: ApprovalGateKind::SalesCommercial,
                decision: ApprovalGateStatus::Approved,
                reason_code: "commercial-ok".into(),
            },
            at: at(),
        }
    }

    #[tokio::test]
    async fn agent_and_service_replay_attempts_are_denied_before_authorization_or_repository() {
        for kind in [ActorKind::Agent, ActorKind::Service] {
            let repository = Repository::default();
            let authorization = Authorization::default();
            let service = ApprovalSessionService::new(repository.clone(), authorization.clone());
            assert_eq!(
                service.execute(request(kind)).await,
                Err(ApprovalSessionApplicationError::HumanDecisionRequired)
            );
            assert_eq!(authorization.calls.load(Ordering::SeqCst), 0);
            assert_eq!(repository.loads.load(Ordering::SeqCst), 0);
            assert_eq!(repository.commits.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn proof_reference_cannot_be_reused_for_another_command_or_gate() {
        let first = request(ActorKind::HoldingUser);
        let proof = approval_session_human_decision(&first)
            .unwrap()
            .expect("decision proof");
        assert!(validate_approval_session_human_decision(&first, &proof).is_ok());

        let mut another_command = first.clone();
        another_command.command_id = Uuid::from_u128(101);
        assert_eq!(
            validate_approval_session_human_decision(&another_command, &proof),
            Err(ApprovalSessionApplicationError::HumanDecisionRequired)
        );

        let mut another_gate = first;
        if let ApprovalSessionCommand::Decide { gate, .. } = &mut another_gate.command {
            *gate = ApprovalGateKind::FinanceAccountingPlan;
        }
        assert_eq!(
            validate_approval_session_human_decision(&another_gate, &proof),
            Err(ApprovalSessionApplicationError::HumanDecisionRequired)
        );
    }
}
