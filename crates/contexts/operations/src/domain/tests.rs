use chrono::{Duration, TimeZone, Utc};
use erpx_integration_contracts::{
    AggregateKind, CurrencyCode, IntegrationEventEnvelope, MoneyAmount, ReferenceCode, customer,
    identity_access, marketing, operations, sales,
};
use erpx_kernel::{AggregateId, CorrelationId, EventId, TenantScope};
use uuid::Uuid;

use super::*;
use crate::{SourceDatabase, SourcePosition};

fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}

fn source_message() -> SourceFactMessage {
    let currency = CurrencyCode::try_new("THB").unwrap();
    let envelope = IntegrationEventEnvelope::try_new(
        EventId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
        TenantScope::Company {
            holding_id: Uuid::from_u128(2),
            company_id: Uuid::from_u128(3),
        },
        AggregateKind::try_new("sales_order").unwrap(),
        AggregateId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
        1,
        CorrelationId::try_from_uuid(Uuid::from_u128(5)).unwrap(),
        None,
        at(),
        sales::Event::SalesOrderChanged(sales::SalesOrderFact {
            sales_order: sales::SalesOrderRef::try_from_uuid(Uuid::from_u128(4)).unwrap(),
            order_code: ReferenceCode::try_new("SO-4").unwrap(),
            customer: customer::CustomerRef::try_from_uuid(Uuid::from_u128(6)).unwrap(),
            status: sales::SalesOrderStatus::Confirmed,
            total: MoneyAmount::new(10_000, currency),
            lines: vec![sales::SalesOrderLineFact {
                line: sales::SalesOrderLineRef::try_from_uuid(Uuid::from_u128(7)).unwrap(),
                item_code: ReferenceCode::try_new("SKU-7").unwrap(),
                quantity: erpx_integration_contracts::Quantity::try_new(1, 0).unwrap(),
                line_value: MoneyAmount::new(10_000, currency),
            }],
        }),
    )
    .unwrap();
    let fact = match envelope.payload() {
        sales::Event::SalesOrderChanged(fact) => operations::SourceFact::Sales(fact.clone()),
        sales::Event::RevenueArrangementActivated(_)
        | sales::Event::FulfillmentAccepted(_)
        | sales::Event::BillingEligible(_) => unreachable!("fixture carries a sales order fact"),
    };
    SourceFactMessage::from_envelope(&envelope, fact).unwrap()
}

fn policy() -> DetectionPolicy {
    DetectionPolicy {
        policy_ref: "sales-delivery-policy".into(),
        allowed_sources: vec![BoundedContext::Sales],
        category: "delivery_breach".into(),
        severity: "high".into(),
        due_after_minutes: 30,
        escalation_after_minutes: 60,
    }
}

fn principal(value: u128) -> identity_access::PrincipalRef {
    identity_access::PrincipalRef::try_from_uuid(Uuid::from_u128(value)).unwrap()
}

fn assignment() -> PrincipalAssignment {
    PrincipalAssignment {
        assignee: principal(10),
        manager: principal(11),
        valid_from: at() - Duration::minutes(1),
        valid_until: at() + Duration::hours(8),
        authority_ref: "identity-assignment-chain:11".into(),
    }
}

fn exception() -> OperationsException {
    OperationsException::detect(
        ExceptionId::try_from_uuid(Uuid::from_u128(20)).unwrap(),
        source_message(),
        policy(),
        assignment(),
        at(),
    )
    .unwrap()
}

fn approval_source_message() -> SourceFactMessage {
    let budget_id = Uuid::from_u128(70);
    let fact = marketing::CampaignBudgetApprovalRequestedFact {
        budget: marketing::CampaignBudgetRef::try_from_uuid(budget_id).unwrap(),
        campaign: marketing::CampaignRef::try_from_uuid(Uuid::from_u128(71)).unwrap(),
        approval: operations::ApprovalRequestFact {
            request: operations::ApprovalRequestRef::try_from_uuid(Uuid::from_u128(72)).unwrap(),
            subject: erpx_integration_contracts::RecordRef::try_new(
                BoundedContext::Marketing,
                "campaign_budget",
                budget_id,
                1,
            )
            .unwrap(),
            policy_code: ReferenceCode::try_new("marketing-budget").unwrap(),
            amount: Some(MoneyAmount::new(
                100_000,
                CurrencyCode::try_new("THB").unwrap(),
            )),
            requested_by: ReferenceCode::try_new("budget-maker").unwrap(),
            requested_at: at(),
        },
    };
    let envelope = IntegrationEventEnvelope::try_new(
        EventId::try_from_uuid(Uuid::from_u128(73)).unwrap(),
        TenantScope::Company {
            holding_id: Uuid::from_u128(2),
            company_id: Uuid::from_u128(3),
        },
        AggregateKind::try_new("campaign_budget").unwrap(),
        AggregateId::try_from_uuid(budget_id).unwrap(),
        2,
        CorrelationId::try_from_uuid(Uuid::from_u128(74)).unwrap(),
        None,
        at(),
        marketing::Event::CampaignBudgetApprovalRequested(fact.clone()),
    )
    .unwrap();
    SourceFactMessage::from_envelope(
        &envelope,
        operations::SourceFact::MarketingBudgetApproval(fact),
    )
    .unwrap()
}

fn approval_exception() -> OperationsException {
    OperationsException::detect(
        ExceptionId::try_from_uuid(Uuid::from_u128(75)).unwrap(),
        approval_source_message(),
        DetectionPolicy {
            policy_ref: "marketing-budget-approval".into(),
            allowed_sources: vec![BoundedContext::Marketing],
            category: "approval".into(),
            severity: "review".into(),
            due_after_minutes: 30,
            escalation_after_minutes: 60,
        },
        assignment(),
        at(),
    )
    .unwrap()
}

#[test]
fn detection_keeps_typed_source_provenance_and_starts_assigned() {
    let mut value = exception();
    assert_eq!(value.version(), 1);
    assert_eq!(value.status(), ExceptionStatus::Assigned);
    assert_eq!(value.source().context, BoundedContext::Sales);
    assert_eq!(value.source().event_id.as_uuid(), Uuid::from_u128(1));
    assert!(matches!(
        value.source_fact(),
        operations::SourceFact::Sales(_)
    ));
    assert_eq!(value.due_at(), at() + Duration::minutes(30));
    assert_eq!(value.take_uncommitted_events().len(), 1);
}

#[test]
fn detection_rejects_future_source_and_unapproved_context() {
    assert_eq!(
        OperationsException::detect(
            ExceptionId::try_from_uuid(Uuid::from_u128(20)).unwrap(),
            source_message(),
            policy(),
            assignment(),
            at() - Duration::seconds(1),
        ),
        Err(OperationsDomainError::SourceFactFromFuture)
    );
    let mut denied = policy();
    denied.allowed_sources = vec![BoundedContext::Inventory];
    assert_eq!(
        OperationsException::detect(
            ExceptionId::try_from_uuid(Uuid::from_u128(20)).unwrap(),
            source_message(),
            denied,
            assignment(),
            at(),
        ),
        Err(OperationsDomainError::SourceContextDenied)
    );
}

#[test]
fn assignment_requires_distinct_active_principals() {
    let mut invalid = assignment();
    invalid.manager = invalid.assignee;
    assert_eq!(
        OperationsException::detect(
            ExceptionId::try_from_uuid(Uuid::from_u128(20)).unwrap(),
            source_message(),
            policy(),
            invalid,
            at(),
        ),
        Err(OperationsDomainError::AssignmentSeparation)
    );
    let mut expired = assignment();
    expired.valid_until = at();
    assert_eq!(
        OperationsException::detect(
            ExceptionId::try_from_uuid(Uuid::from_u128(20)).unwrap(),
            source_message(),
            policy(),
            expired,
            at(),
        ),
        Err(OperationsDomainError::InvalidAssignment)
    );
}

#[test]
fn lifecycle_transitions_are_versioned_and_fail_atomically() {
    let mut value = exception();
    value.take_uncommitted_events();
    value.acknowledge(at() + Duration::minutes(5)).unwrap();
    assert_eq!(value.version(), 2);
    assert_eq!(value.status(), ExceptionStatus::Acknowledged);

    let before = value.clone();
    assert_eq!(
        value.close(at() + Duration::minutes(6)),
        Err(OperationsDomainError::InvalidTransition)
    );
    assert_eq!(value, before);

    value.resolve(at() + Duration::minutes(7)).unwrap();
    value.close(at() + Duration::minutes(8)).unwrap();
    assert_eq!(value.version(), 4);
    assert_eq!(value.status(), ExceptionStatus::Closed);
}

#[test]
fn expired_assignment_blocks_action_until_governed_reassignment() {
    let mut value = exception();
    value.take_uncommitted_events();
    let before = value.clone();
    assert_eq!(
        value.acknowledge(at() + Duration::hours(8)),
        Err(OperationsDomainError::InvalidAssignment)
    );
    assert_eq!(value, before);

    let replacement = PrincipalAssignment {
        assignee: principal(10),
        manager: principal(12),
        valid_from: at() + Duration::hours(8),
        valid_until: at() + Duration::hours(16),
        authority_ref: "identity-assignment-chain:replacement".into(),
    };
    value
        .reassign(replacement, at() + Duration::hours(8))
        .unwrap();
    value
        .acknowledge(at() + Duration::hours(8) + Duration::minutes(1))
        .unwrap();
    assert_eq!(value.status(), ExceptionStatus::Acknowledged);
}

#[test]
fn exception_exposes_stable_first_class_record_identity() {
    let exception_id = Uuid::new_v4();
    let value = OperationsException::detect(
        ExceptionId::try_from_uuid(exception_id).unwrap(),
        source_message(),
        policy(),
        assignment(),
        at(),
    )
    .unwrap();
    let reference = value.record_ref().unwrap();
    assert_eq!(reference.id().as_uuid(), exception_id);
    assert_eq!(
        reference.canonical_detail_route(),
        format!("/operations/exception/{exception_id}")
    );
}

#[test]
fn approval_requires_typed_decision_and_enforces_maker_checker() {
    let mut value = approval_exception();
    value.take_uncommitted_events();
    assert_eq!(
        value.resolve(at() + Duration::minutes(1)),
        Err(OperationsDomainError::InvalidApprovalDecision)
    );
    assert_eq!(
        value.decide_approval(
            operations::ApprovalDecision::Approved,
            "budget-maker".into(),
            "within-ceiling".into(),
            at() + Duration::minutes(1),
        ),
        Err(OperationsDomainError::InvalidApprovalDecision)
    );

    value
        .decide_approval(
            operations::ApprovalDecision::Approved,
            "finance-controller".into(),
            "within-ceiling".into(),
            at() + Duration::minutes(1),
        )
        .unwrap();
    assert_eq!(value.status(), ExceptionStatus::Resolved);
    let events = crate::integration::published_events(&value.take_uncommitted_events()).unwrap();
    assert!(matches!(
        events.as_slice(),
        [
            operations::Event::OperationalStateChanged(_),
            operations::Event::ApprovalDecided(decision)
        ] if decision.decided_by.as_str() == "finance-controller"
    ));
}

#[test]
fn escalation_requires_due_sla_and_current_manager_chain_evidence() {
    let mut value = exception();
    let replacement = PrincipalAssignment {
        assignee: value.assignment().assignee,
        manager: principal(12),
        valid_from: at(),
        valid_until: at() + Duration::hours(8),
        authority_ref: "identity-assignment-chain:12".into(),
    };
    assert_eq!(
        value.escalate(replacement.clone(), at() + Duration::minutes(29)),
        Err(OperationsDomainError::EscalationNotDue)
    );
    assert_eq!(value.version(), 1);
    value
        .escalate(replacement, at() + Duration::minutes(30))
        .unwrap();
    assert_eq!(value.status(), ExceptionStatus::Escalated);
    assert_eq!(value.assignment().manager, principal(12));
}

#[test]
fn policy_rejects_duplicate_sources_and_operations_recursion() {
    let mut duplicate = policy();
    duplicate.allowed_sources.push(BoundedContext::Sales);
    assert_eq!(
        duplicate.validate(),
        Err(OperationsDomainError::InvalidDetectionPolicy)
    );
    let mut recursive = policy();
    recursive.allowed_sources = vec![BoundedContext::Operations];
    assert_eq!(
        recursive.validate(),
        Err(OperationsDomainError::InvalidDetectionPolicy)
    );
}

#[test]
fn work_queue_policy_is_holding_owned_bounded_and_versioned() {
    let mut value = WorkQueuePolicy::publish(
        WorkQueuePolicyId::try_from_uuid(Uuid::from_u128(30)).unwrap(),
        TenantScope::Holding {
            holding_id: Uuid::from_u128(2),
        },
        "priority queue".into(),
        vec![BoundedContext::Sales, BoundedContext::Inventory],
        60,
        at(),
    )
    .unwrap();
    assert_eq!(value.version(), 1);
    assert_eq!(value.status(), WorkQueuePolicyStatus::Active);
    assert_eq!(value.take_uncommitted_events().len(), 1);
    value
        .republish(
            "priority queue v2".into(),
            vec![BoundedContext::Sales, BoundedContext::Support],
            90,
            at() + Duration::minutes(1),
        )
        .unwrap();
    assert_eq!(value.version(), 2);
    assert_eq!(value.escalation_after_minutes(), 90);
}

#[test]
fn work_queue_policy_rejects_non_holding_and_unsupported_sources() {
    assert_eq!(
        WorkQueuePolicy::publish(
            WorkQueuePolicyId::try_from_uuid(Uuid::from_u128(30)).unwrap(),
            TenantScope::Company {
                holding_id: Uuid::from_u128(2),
                company_id: Uuid::from_u128(3),
            },
            "invalid scope".into(),
            vec![BoundedContext::Sales],
            60,
            at(),
        ),
        Err(OperationsDomainError::InvalidDetectionPolicy)
    );
    assert_eq!(
        WorkQueuePolicy::publish(
            WorkQueuePolicyId::try_from_uuid(Uuid::from_u128(30)).unwrap(),
            TenantScope::Holding {
                holding_id: Uuid::from_u128(2),
            },
            "recursive".into(),
            vec![BoundedContext::Operations],
            60,
            at(),
        ),
        Err(OperationsDomainError::InvalidDetectionPolicy)
    );
    assert_eq!(
        WorkQueuePolicy::publish(
            WorkQueuePolicyId::try_from_uuid(Uuid::from_u128(30)).unwrap(),
            TenantScope::Holding {
                holding_id: Uuid::from_u128(2),
            },
            "unpublished legal source".into(),
            vec![BoundedContext::Legal],
            60,
            at(),
        ),
        Err(OperationsDomainError::InvalidDetectionPolicy)
    );
}

#[test]
fn source_position_is_database_qualified_and_monotonic() {
    let source = source_message();
    let tenant = SourcePosition {
        database: SourceDatabase::Tenant,
        source_key: "tenant-write-outbox".into(),
        partition: "sales-order:4".into(),
        sequence: 7,
    };
    assert_eq!(tenant.validate(&source), Ok(()));

    let mut invalid = tenant.clone();
    invalid.database = SourceDatabase::Control;
    assert_eq!(
        invalid.validate(&source),
        Err(crate::PortError::SourcePositionInvalid)
    );
    invalid = tenant;
    invalid.sequence = 0;
    assert_eq!(
        invalid.validate(&source),
        Err(crate::PortError::SourcePositionInvalid)
    );
}
