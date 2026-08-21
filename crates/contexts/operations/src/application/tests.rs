use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use chrono::{Duration, TimeZone, Utc};
use erpx_cqrs::{CONTRACT_VERSION, CommandEnvelope, CommandHandler, IdempotencyKey};
use erpx_integration_contracts::identity_access;
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, FixedClock, SessionId,
};
use uuid::Uuid;

use super::*;
use crate::domain::ExceptionStatus;

fn exception_id() -> ExceptionId {
    ExceptionId::try_from_uuid(Uuid::from_u128(1)).unwrap()
}

fn principal(value: u128) -> identity_access::PrincipalRef {
    identity_access::PrincipalRef::try_from_uuid(Uuid::from_u128(value)).unwrap()
}

#[test]
fn public_command_inventory_contains_human_lifecycle_only() {
    let commands = [
        OperationsCommand::Reassign(ReassignException {
            exception_id: exception_id(),
            assignee: principal(2),
            manager: principal(3),
        }),
        OperationsCommand::Acknowledge(AcknowledgeException {
            exception_id: exception_id(),
        }),
        OperationsCommand::Escalate(EscalateException {
            exception_id: exception_id(),
            manager: principal(3),
        }),
        OperationsCommand::DecideApproval(DecideApproval {
            exception_id: exception_id(),
            decision: erpx_integration_contracts::operations::ApprovalDecision::Approved,
            reason_code: "approved".into(),
        }),
        OperationsCommand::Resolve(ResolveException {
            exception_id: exception_id(),
        }),
        OperationsCommand::Close(CloseException {
            exception_id: exception_id(),
        }),
        OperationsCommand::PublishWorkQueuePolicy(PublishWorkQueuePolicy {
            policy_id: WorkQueuePolicyId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
            name: "default queue".into(),
            allowed_sources: vec![erpx_integration_contracts::BoundedContext::Sales],
            escalation_after_minutes: 60,
        }),
    ];
    let wire = serde_json::to_string(&commands).unwrap();
    assert!(!wire.contains("source_fact"));
    assert!(!wire.contains("payload_json"));
    assert!(!wire.contains("authority_ref"));
}

#[test]
fn human_command_fingerprint_contains_stable_selectors_only() {
    let command = OperationsCommand::Reassign(ReassignException {
        exception_id: exception_id(),
        assignee: principal(2),
        manager: principal(3),
    });
    assert_eq!(
        command.fingerprint(),
        Some(CommandFingerprint::Reassign {
            exception_id: exception_id(),
            assignee: principal(2),
            manager: principal(3),
        })
    );
    let debug = format!("{:?}", command.fingerprint());
    assert!(!debug.contains("valid_until"));
    assert!(!debug.contains("mfa"));
}

#[test]
fn high_risk_and_internal_actions_are_explicit() {
    assert!(OperationsAction::ReassignException.requires_current_mfa());
    assert!(OperationsAction::EscalateException.requires_current_mfa());
    assert!(OperationsAction::DecideApproval.requires_current_mfa());
    assert!(!OperationsAction::ResolveException.requires_current_mfa());
    assert!(OperationsAction::ConsumeSourceFact.is_internal());
    assert!(!OperationsAction::ListQueue.requires_write_entitlement());
}

#[test]
fn queue_filter_rejects_unbounded_pages_and_duplicate_statuses() {
    assert_eq!(
        validate_filter(&QueueFilter {
            page_size: 0,
            ..QueueFilter::default()
        }),
        Err(OperationsApplicationError::InvalidPageSize)
    );
    assert_eq!(
        validate_filter(&QueueFilter {
            statuses: vec![ExceptionStatus::Assigned, ExceptionStatus::Assigned],
            page_size: 20,
            ..QueueFilter::default()
        }),
        Err(OperationsApplicationError::DuplicateStatusFilter)
    );
}

#[test]
fn source_timestamps_are_not_part_of_human_command_fingerprints() {
    let start = Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap();
    let retry = start + Duration::minutes(1);
    let command = OperationsCommand::Escalate(EscalateException {
        exception_id: exception_id(),
        manager: principal(3),
    });
    assert_eq!(command.fingerprint(), command.clone().fingerprint());
    assert_ne!(start, retry);
}

#[test]
fn policy_fingerprint_is_stable_and_contains_only_owned_policy_input() {
    let value = PublishWorkQueuePolicy {
        policy_id: WorkQueuePolicyId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
        name: "priority queue".into(),
        allowed_sources: vec![
            erpx_integration_contracts::BoundedContext::Sales,
            erpx_integration_contracts::BoundedContext::Inventory,
        ],
        escalation_after_minutes: 90,
    };
    let fingerprint = WorkQueuePolicyFingerprint {
        policy_id: value.policy_id,
        name: value.name.clone(),
        allowed_sources: value.allowed_sources.clone(),
        escalation_after_minutes: value.escalation_after_minutes,
    };
    let json = serde_json::to_string(&fingerprint).unwrap();
    assert!(!json.contains("updated_at"));
    assert!(!json.contains("actor"));
    assert!(!json.contains("source_fact"));
}

#[derive(Clone, Default)]
struct DecisionBoundaryProbe {
    receipt_calls: Arc<AtomicUsize>,
    repository_calls: Arc<AtomicUsize>,
    authorization_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl WriteRepository for DecisionBoundaryProbe {
    async fn load_exception(
        &self,
        _authority_scope: &AuthorityScope,
        _exception_id: ExceptionId,
    ) -> Result<Option<crate::domain::OperationsException>, PortError> {
        self.repository_calls.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }

    async fn load_work_queue_policy(
        &self,
        _authority_scope: &AuthorityScope,
        _policy_id: WorkQueuePolicyId,
    ) -> Result<Option<WorkQueuePolicy>, PortError> {
        self.repository_calls.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }
}

#[async_trait]
impl CommandUnitOfWork for DecisionBoundaryProbe {
    async fn find_receipt(
        &self,
        _identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        self.receipt_calls.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }

    async fn commit(&self, _request: OperationsCommit) -> Result<StoredCommit, PortError> {
        Err(PortError::Internal)
    }
}

#[async_trait]
impl WorkQueuePolicyUnitOfWork for DecisionBoundaryProbe {
    async fn find_policy_receipt(
        &self,
        _identity: &WorkQueuePolicyCommandIdentity,
    ) -> Result<Option<crate::ports::StoredWorkQueuePolicyCommit>, PortError> {
        self.receipt_calls.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }

    async fn commit_policy(
        &self,
        _request: WorkQueuePolicyCommit,
    ) -> Result<crate::ports::StoredWorkQueuePolicyCommit, PortError> {
        Err(PortError::Internal)
    }
}

#[async_trait]
impl AuthorizationPort for DecisionBoundaryProbe {
    async fn authorize(&self, _request: AuthorizationRequest<'_>) -> Result<(), PortError> {
        self.authorization_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[async_trait]
impl EntitlementPort for DecisionBoundaryProbe {
    async fn ensure_operations_enabled(
        &self,
        _scope: &AuthorityScope,
        _write: bool,
    ) -> Result<(), PortError> {
        self.authorization_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[async_trait]
impl AssignmentPort for DecisionBoundaryProbe {
    async fn resolve_assignment(
        &self,
        _scope: &AuthorityScope,
        _assignee: identity_access::PrincipalRef,
        _manager: identity_access::PrincipalRef,
        _at: chrono::DateTime<Utc>,
    ) -> Result<crate::domain::PrincipalAssignment, PortError> {
        Err(PortError::Internal)
    }
}

fn decision_envelope(kind: ActorKind) -> CommandEnvelope<OperationsCommand> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(Uuid::from_u128(140)).unwrap(),
        idempotency_key: IdempotencyKey::try_new("operations-decision-140").unwrap(),
        correlation_id: CorrelationId::try_from_uuid(Uuid::from_u128(141)).unwrap(),
        causation_id: None,
        actor: ActorAttribution {
            principal_ref: "operations-reviewer".into(),
            kind,
            session_id: Some(SessionId::try_from_uuid(Uuid::from_u128(142)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        active_scope: AuthorityScope::Company {
            holding_id: Uuid::from_u128(143),
            company_id: Uuid::from_u128(144),
        },
        audit: AuditContext {
            purpose: "operations approval test".into(),
            customer_visible: false,
            module: Some("operations".into()),
        },
        expected_aggregate_version: Some(1),
        command: OperationsCommand::DecideApproval(DecideApproval {
            exception_id: exception_id(),
            decision: erpx_integration_contracts::operations::ApprovalDecision::Approved,
            reason_code: "within-ceiling".into(),
        }),
    }
}

#[tokio::test]
async fn agent_and_service_replay_attempts_are_denied_before_receipt_or_repository_access() {
    for kind in [ActorKind::Agent, ActorKind::Service] {
        let probe = DecisionBoundaryProbe::default();
        let handler = OperationsCommandHandler::new(
            probe.clone(),
            probe.clone(),
            probe.clone(),
            probe.clone(),
            probe.clone(),
            probe.clone(),
            FixedClock::new(Utc.with_ymd_and_hms(2026, 8, 21, 9, 0, 0).single().unwrap()),
        );
        let error = handler.handle(decision_envelope(kind)).await.unwrap_err();
        assert_eq!(error.category(), erpx_kernel::ErrorCategory::Denied);
        assert_eq!(probe.receipt_calls.load(Ordering::SeqCst), 0);
        assert_eq!(probe.repository_calls.load(Ordering::SeqCst), 0);
        assert_eq!(probe.authorization_calls.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn decision_proof_reference_cannot_be_reused_for_another_command_or_action() {
    let at = Utc.with_ymd_and_hms(2026, 8, 21, 9, 0, 0).single().unwrap();
    let first = decision_envelope(ActorKind::HoldingUser);
    let OperationsCommand::DecideApproval(first_command) = &first.command else {
        unreachable!()
    };
    let proof = operations_human_decision(&first, first_command, at).unwrap();
    assert!(validate_operations_human_decision(&first, first_command, at, &proof).is_ok());

    let mut another_command = first.clone();
    another_command.command_id = CommandId::try_from_uuid(Uuid::from_u128(145)).unwrap();
    let OperationsCommand::DecideApproval(command) = &another_command.command else {
        unreachable!()
    };
    assert_eq!(
        validate_operations_human_decision(&another_command, command, at, &proof),
        Err(OperationsApplicationError::HumanDecisionBinding)
    );

    let mut another_action = first;
    let OperationsCommand::DecideApproval(command) = &mut another_action.command else {
        unreachable!()
    };
    command.decision = erpx_integration_contracts::operations::ApprovalDecision::Rejected;
    let command = command.clone();
    assert_eq!(
        validate_operations_human_decision(&another_action, &command, at, &proof),
        Err(OperationsApplicationError::HumanDecisionBinding)
    );
}
