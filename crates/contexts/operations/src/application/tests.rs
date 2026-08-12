use chrono::{Duration, TimeZone, Utc};
use erpx_integration_contracts::identity_access;
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
