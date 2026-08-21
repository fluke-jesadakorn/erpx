use chrono::{NaiveDate, TimeZone, Utc};
use erpx_integration_contracts::{
    CurrencyCode, MoneyAmount, RecordVersionRef, StableRecordRef, catalog::ContextId,
    reference::RecordTypeId,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuthorityScope, CommandId, CorrelationId, HumanDecisionProof,
    RecordId, SessionId,
};
use uuid::Uuid;

use super::*;
fn uuid(value: u128) -> Uuid {
    let mut bytes = value.to_be_bytes();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
fn at(hour: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 15, hour, 0, 0)
        .single()
        .unwrap()
}
fn day(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, day).unwrap()
}
fn scope() -> AuthorityScope {
    AuthorityScope::Branch {
        holding_id: uuid(1),
        company_id: uuid(2),
        branch_id: uuid(3),
    }
}
fn record(id: u128, owner: ContextId, kind: &str) -> StableRecordRef {
    StableRecordRef::try_new(
        RecordId::try_from_uuid(uuid(id)).unwrap(),
        owner,
        RecordTypeId::try_new(kind).unwrap(),
    )
    .unwrap()
}
fn pin(id: u128, owner: ContextId, kind: &str, version: u64) -> RecordVersionRef {
    RecordVersionRef::try_new(record(id, owner, kind), version).unwrap()
}
fn money(value: i64) -> MoneyAmount {
    MoneyAmount::new(value, CurrencyCode::try_new("THB").unwrap())
}
fn human_decision(reference: String, decided_at: chrono::DateTime<Utc>) -> HumanDecisionProof {
    HumanDecisionProof::try_new(
        ActorAttribution {
            principal_ref: uuid(8).to_string(),
            kind: ActorKind::HoldingUser,
            session_id: Some(SessionId::try_from_uuid(uuid(9)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope(),
        reference,
        decided_at,
    )
    .unwrap()
}
fn plan() -> BenefitPlan {
    let command_id = CommandId::try_from_uuid(uuid(7)).unwrap();
    let mut value = BenefitPlan::draft(
        BenefitPlanId::try_from_uuid(uuid(10)).unwrap(),
        scope(),
        "Health plan",
        CurrencyCode::try_new("THB").unwrap(),
        "full_time",
        90,
        ContributionRule {
            employee_basis_points: 200,
            employer_basis_points: 400,
            employee_flat: money(100),
            employer_flat: money(200),
            employee_cap: Some(money(2_000)),
        },
        at(1),
    )
    .unwrap();
    value
        .publish(
            &human_decision(
                benefit_plan_publish_decision_ref(command_id, value.id()),
                at(2),
            ),
            command_id,
            at(2),
        )
        .unwrap();
    value
}

#[test]
fn plan_publication_requires_exact_authenticated_human_evidence() {
    let command_id = CommandId::try_from_uuid(uuid(70)).unwrap();
    let mut value = BenefitPlan::draft(
        BenefitPlanId::try_from_uuid(uuid(71)).unwrap(),
        scope(),
        "Human guarded plan",
        CurrencyCode::try_new("THB").unwrap(),
        "full_time",
        30,
        ContributionRule {
            employee_basis_points: 100,
            employer_basis_points: 100,
            employee_flat: money(0),
            employer_flat: money(0),
            employee_cap: None,
        },
        at(1),
    )
    .unwrap();
    let wrong = human_decision("benefits.plan.publish:wrong".into(), at(2));
    assert_eq!(
        value.publish(&wrong, command_id, at(2)),
        Err(BenefitsDomainError::HumanDecisionRequired)
    );
    assert!(
        HumanDecisionProof::try_new(
            ActorAttribution {
                principal_ref: uuid(72).to_string(),
                kind: ActorKind::Agent,
                session_id: Some(SessionId::try_from_uuid(uuid(73)).unwrap()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope(),
            benefit_plan_publish_decision_ref(command_id, value.id()),
            at(2),
        )
        .is_err()
    );
}
fn window(plan: &BenefitPlan) -> EnrollmentWindow {
    EnrollmentWindow {
        id: EnrollmentWindowId::try_from_uuid(uuid(11)).unwrap(),
        plan: BenefitPlanRef {
            plan_id: plan.id(),
            version: plan.version(),
        },
        opens_on: day(10),
        closes_on: day(20),
        coverage_starts_on: NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
    }
}
fn eligibility(employee: RecordVersionRef) -> EligibilityDecision {
    EligibilityDecision {
        employee,
        employment_class: "full_time".into(),
        tenure_days: 365,
        eligible: true,
        source: pin(13, ContextId::PeoplePayroll, "people.employment", 4),
        evaluated_at: at(3),
    }
}

#[test]
fn window_and_eligibility_are_pinned_and_life_event_is_explicit() {
    let plan = plan();
    let window = window(&plan);
    let employee = pin(12, ContextId::PeoplePayroll, "people.employee", 3);
    assert_eq!(
        BenefitEnrollment::open(
            BenefitEnrollmentId::try_from_uuid(uuid(20)).unwrap(),
            scope(),
            &plan,
            &window,
            employee.clone(),
            eligibility(employee.clone()),
            None,
            day(25),
            at(3)
        ),
        Err(BenefitsDomainError::WindowClosed)
    );
    let life_event = LifeEvent {
        id: LifeEventId::try_from_uuid(uuid(21)).unwrap(),
        kind: LifeEventKind::Marriage,
        occurred_on: day(24),
        evidence: pin(22, ContextId::Legal, "legal.contract", 1),
    };
    BenefitEnrollment::open(
        BenefitEnrollmentId::try_from_uuid(uuid(23)).unwrap(),
        scope(),
        &plan,
        &window,
        employee.clone(),
        eligibility(employee),
        Some(life_event),
        day(25),
        at(3),
    )
    .unwrap();
}

#[test]
fn contribution_is_fixed_precision_and_requires_provider_ack_before_payroll() {
    let plan = plan();
    let window = window(&plan);
    let employee = pin(30, ContextId::PeoplePayroll, "people.employee", 3);
    let mut enrollment = BenefitEnrollment::open(
        BenefitEnrollmentId::try_from_uuid(uuid(31)).unwrap(),
        scope(),
        &plan,
        &window,
        employee.clone(),
        eligibility(employee),
        None,
        day(15),
        at(3),
    )
    .unwrap();
    enrollment
        .elect(
            &plan,
            ElectionId::try_from_uuid(uuid(32)).unwrap(),
            CoverageTier::Employee,
            vec![],
            money(50_000),
            pin(33, ContextId::PeoplePayroll, "payroll.payslip", 2),
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
            at(4),
        )
        .unwrap();
    let handoff_id = PayrollHandoffId::try_from_uuid(uuid(34)).unwrap();
    assert_eq!(
        enrollment.payroll_handoff(handoff_id, CorrelationId::try_from_uuid(uuid(35)).unwrap()),
        Err(BenefitsDomainError::ProviderNotAcknowledged)
    );
    enrollment.submit_provider(at(5)).unwrap();
    enrollment
        .acknowledge_provider(ProviderAcknowledgement {
            id: ProviderAcknowledgementId::try_from_uuid(uuid(36)).unwrap(),
            request_key: "request-1".into(),
            accepted: true,
            provider_reference: "ack-1".into(),
            reason: None,
            received_at: at(6),
        })
        .unwrap();
    let handoff = enrollment
        .payroll_handoff(handoff_id, CorrelationId::try_from_uuid(uuid(35)).unwrap())
        .unwrap();
    assert_eq!(handoff.employee_contribution.minor_units(), 1_100);
    assert_eq!(handoff.employer_contribution.minor_units(), 2_200);
}

#[test]
fn provider_rejection_is_reasoned_and_can_be_resubmitted() {
    let plan = plan();
    let window = window(&plan);
    let employee = pin(40, ContextId::PeoplePayroll, "people.employee", 3);
    let mut enrollment = BenefitEnrollment::open(
        BenefitEnrollmentId::try_from_uuid(uuid(41)).unwrap(),
        scope(),
        &plan,
        &window,
        employee.clone(),
        eligibility(employee),
        None,
        day(15),
        at(3),
    )
    .unwrap();
    enrollment
        .elect(
            &plan,
            ElectionId::try_from_uuid(uuid(42)).unwrap(),
            CoverageTier::Employee,
            vec![],
            money(50_000),
            pin(43, ContextId::PeoplePayroll, "payroll.payslip", 2),
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
            at(4),
        )
        .unwrap();
    enrollment.submit_provider(at(5)).unwrap();
    enrollment
        .acknowledge_provider(ProviderAcknowledgement {
            id: ProviderAcknowledgementId::try_from_uuid(uuid(44)).unwrap(),
            request_key: "request-reject".into(),
            accepted: false,
            provider_reference: "reject-1".into(),
            reason: Some("Dependent evidence incomplete".into()),
            received_at: at(6),
        })
        .unwrap();
    assert_eq!(
        enrollment.status(),
        BenefitEnrollmentStatus::ProviderRejected
    );
    enrollment.submit_provider(at(7)).unwrap();
    assert_eq!(
        enrollment.status(),
        BenefitEnrollmentStatus::PendingProvider
    );
}
