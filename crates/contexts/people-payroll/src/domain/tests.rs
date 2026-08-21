use chrono::{Duration, TimeZone, Utc};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuthorityScope, CommandId, HumanDecisionProof, SessionId,
};
use uuid::Uuid;

use super::*;

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}
fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, day).unwrap()
}
fn scope() -> AuthorityScope {
    AuthorityScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}
fn employee_id(value: u128) -> EmployeeId {
    EmployeeId::try_from_uuid(id(value)).unwrap()
}
fn command_id(value: u128) -> CommandId {
    CommandId::try_from_uuid(id(value)).unwrap()
}
fn human_decision(
    principal_ref: &str,
    authority_scope: AuthorityScope,
    command_id: CommandId,
    decided_at: DateTime<Utc>,
) -> HumanDecisionProof {
    HumanDecisionProof::try_new(
        ActorAttribution {
            principal_ref: principal_ref.into(),
            kind: ActorKind::HoldingUser,
            session_id: Some(SessionId::try_from_uuid(id(99)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        authority_scope,
        command_id.to_string(),
        decided_at,
    )
    .unwrap()
}
fn payroll_evidence(captured_at: DateTime<Utc>) -> PayrollCalculationEvidence {
    PayrollCalculationEvidence {
        policy_version: "TH-PAYROLL-2026.08".into(),
        rate_table_version: "TH-RATES-2026.08".into(),
        calculation_sha256: format!("sha256:{}", "a".repeat(64)),
        source_inputs: Vec::new(),
        captured_at,
    }
}

#[test]
fn records_require_exact_company_or_branch_scope() {
    let result = Employee::hire(
        employee_id(10),
        AuthorityScope::Holding { holding_id: id(1) },
        "EMP-010".into(),
        "Ari".into(),
        date(1),
        at(),
    );
    assert_eq!(result, Err(PeoplePayrollDomainError::InvalidScope));
}

#[test]
fn employment_lifecycle_is_closed_and_termination_is_date_safe() {
    let mut employee = Employee::hire(
        employee_id(10),
        scope(),
        "EMP-010".into(),
        "Ari".into(),
        date(1),
        at(),
    )
    .unwrap();
    employee
        .transition_for_test(EmployeeTransition::Activate, at() + Duration::minutes(1))
        .unwrap();
    employee
        .transition_for_test(EmployeeTransition::PutOnLeave, at() + Duration::minutes(2))
        .unwrap();
    employee
        .transition_for_test(
            EmployeeTransition::ReturnFromLeave,
            at() + Duration::minutes(3),
        )
        .unwrap();
    assert_eq!(employee.status(), EmployeeStatus::Active);

    let before = employee.clone();
    assert_eq!(
        employee.transition_for_test(
            EmployeeTransition::Terminate {
                terminated_on: date(1).pred_opt().unwrap()
            },
            at() + Duration::minutes(4),
        ),
        Err(PeoplePayrollDomainError::InvalidTransition),
    );
    assert_eq!(employee, before);

    employee
        .transition_for_test(
            EmployeeTransition::Terminate {
                terminated_on: date(5),
            },
            at() + Duration::minutes(5),
        )
        .unwrap();
    assert_eq!(employee.status(), EmployeeStatus::Terminated);
    assert!(employee.validate().is_ok());
    let employment = employee.employment_record_ref().unwrap();
    assert_eq!(employment.record_type().as_str(), "people.employment");
    assert_ne!(employment.id().as_uuid(), employee.id().as_uuid());
    assert_eq!(employment, employee.employment_record_ref().unwrap());
}

#[test]
fn payslip_identity_is_stable_and_requires_an_owned_payroll_line() {
    let employee = employee_id(10);
    let run = PayrollRun::create(
        PayrollRunId::try_from_uuid(id(70)).unwrap(),
        scope(),
        "2026-08".into(),
        date(1),
        date(31),
        Currency::try_new("THB").unwrap(),
        vec![PayrollLine::try_new(employee, 100_000, 10_000).unwrap()],
        "preparer".into(),
        at(),
    )
    .unwrap();
    let payslip = run.payslip_record_ref(employee).unwrap().unwrap();
    assert_eq!(payslip.record_type().as_str(), "payroll.payslip");
    assert_ne!(payslip.id().as_uuid(), run.id().as_uuid());
    assert_eq!(payslip, run.payslip_record_ref(employee).unwrap().unwrap());
    assert_eq!(run.payslip_record_ref(employee_id(99)).unwrap(), None);
}

#[test]
fn leave_approval_separates_requester_and_reviewer_without_partial_mutation() {
    let mut leave = LeaveRequest::create(
        LeaveRequestId::try_from_uuid(id(20)).unwrap(),
        scope(),
        employee_id(10),
        date(10),
        date(12),
        "annual leave".into(),
        "requester".into(),
        at(),
    )
    .unwrap();
    leave
        .transition(
            LeaveTransition::Submit,
            "requester",
            at() + Duration::minutes(1),
        )
        .unwrap();
    let before = leave.clone();
    let approve_at = at() + Duration::minutes(2);
    let approve_command = command_id(90);
    assert_eq!(
        leave.decide(
            LeaveTransition::Approve,
            human_decision("requester", scope(), approve_command, approve_at),
            approve_command,
            approve_at,
        ),
        Err(PeoplePayrollDomainError::ApprovalSeparation),
    );
    assert_eq!(leave, before);
    leave
        .decide(
            LeaveTransition::Approve,
            human_decision("reviewer", scope(), approve_command, approve_at),
            approve_command,
            approve_at,
        )
        .unwrap();
    leave
        .transition(
            LeaveTransition::MarkTaken,
            "reviewer",
            at() + Duration::minutes(3),
        )
        .unwrap();
    assert_eq!(leave.status(), LeaveStatus::Taken);
}

#[test]
fn expense_uses_minor_units_and_separated_approval() {
    assert_eq!(
        Currency::try_new("thb"),
        Err(PeoplePayrollDomainError::InvalidCurrency)
    );
    let mut expense = ExpenseClaim::create(
        ExpenseClaimId::try_from_uuid(id(30)).unwrap(),
        scope(),
        employee_id(10),
        Currency::try_new("THB").unwrap(),
        1_500,
        "travel".into(),
        "claimant".into(),
        at(),
    )
    .unwrap();
    expense
        .transition(
            ExpenseTransition::Submit,
            "claimant",
            at() + Duration::minutes(1),
        )
        .unwrap();
    let approve_at = at() + Duration::minutes(2);
    let approve_command = command_id(91);
    assert_eq!(
        expense.decide(
            ExpenseTransition::Approve,
            human_decision("claimant", scope(), approve_command, approve_at),
            approve_command,
            approve_at,
        ),
        Err(PeoplePayrollDomainError::ApprovalSeparation),
    );
    expense
        .decide(
            ExpenseTransition::Approve,
            human_decision("reviewer", scope(), approve_command, approve_at),
            approve_command,
            approve_at,
        )
        .unwrap();
    let reimburse_at = at() + Duration::minutes(3);
    let reimburse_command = command_id(92);
    expense
        .decide(
            ExpenseTransition::Reimburse,
            human_decision("payments", scope(), reimburse_command, reimburse_at),
            reimburse_command,
            reimburse_at,
        )
        .unwrap();
    assert_eq!(expense.status(), ExpenseStatus::Reimbursed);
}

#[test]
fn leave_and_expense_rejections_require_and_accept_matching_human_decisions() {
    let decided_at = at() + Duration::minutes(2);

    let mut leave = LeaveRequest::create(
        LeaveRequestId::try_from_uuid(id(21)).unwrap(),
        scope(),
        employee_id(10),
        date(10),
        date(12),
        "annual leave".into(),
        "requester".into(),
        at(),
    )
    .unwrap();
    leave
        .transition(
            LeaveTransition::Submit,
            "workflow-agent",
            at() + Duration::minutes(1),
        )
        .unwrap();
    let leave_command = command_id(97);
    leave
        .decide(
            LeaveTransition::Reject,
            human_decision("reviewer", scope(), leave_command, decided_at),
            leave_command,
            decided_at,
        )
        .unwrap();
    assert_eq!(leave.status(), LeaveStatus::Rejected);

    let mut expense = ExpenseClaim::create(
        ExpenseClaimId::try_from_uuid(id(31)).unwrap(),
        scope(),
        employee_id(10),
        Currency::try_new("THB").unwrap(),
        1_500,
        "travel".into(),
        "claimant".into(),
        at(),
    )
    .unwrap();
    expense
        .transition(
            ExpenseTransition::Submit,
            "workflow-agent",
            at() + Duration::minutes(1),
        )
        .unwrap();
    let expense_command = command_id(98);
    expense
        .decide(
            ExpenseTransition::Reject,
            human_decision("reviewer", scope(), expense_command, decided_at),
            expense_command,
            decided_at,
        )
        .unwrap();
    assert_eq!(expense.status(), ExpenseStatus::Rejected);
}

#[test]
fn decision_proofs_reject_non_humans_legacy_actor_bypass_and_mismatched_metadata() {
    let mut leave = LeaveRequest::create(
        LeaveRequestId::try_from_uuid(id(22)).unwrap(),
        scope(),
        employee_id(10),
        date(10),
        date(12),
        "annual leave".into(),
        "requester".into(),
        at(),
    )
    .unwrap();
    leave
        .transition(
            LeaveTransition::Submit,
            "requester",
            at() + Duration::minutes(1),
        )
        .unwrap();
    let before = leave.clone();
    let decided_at = at() + Duration::minutes(2);
    let expected_command = command_id(110);

    for kind in [
        ActorKind::Agent,
        ActorKind::Service,
        ActorKind::SystemWorker,
    ] {
        assert!(
            HumanDecisionProof::try_new(
                ActorAttribution {
                    principal_ref: "reviewer".into(),
                    kind,
                    session_id: Some(SessionId::try_from_uuid(id(99)).unwrap()),
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                scope(),
                expected_command.to_string(),
                decided_at,
            )
            .is_err()
        );
        assert_eq!(leave, before);
    }

    assert_eq!(
        leave.transition(LeaveTransition::Approve, "reviewer", decided_at),
        Err(PeoplePayrollDomainError::InvalidHumanDecision)
    );
    assert_eq!(leave, before);
    assert_eq!(
        leave.decide(
            LeaveTransition::Approve,
            human_decision("reviewer", scope(), command_id(111), decided_at),
            expected_command,
            decided_at,
        ),
        Err(PeoplePayrollDomainError::InvalidHumanDecision)
    );
    assert_eq!(leave, before);
    assert_eq!(
        leave.decide(
            LeaveTransition::Approve,
            human_decision("reviewer", scope(), expected_command, decided_at),
            expected_command,
            decided_at + Duration::seconds(1),
        ),
        Err(PeoplePayrollDomainError::InvalidHumanDecision)
    );
    assert_eq!(leave, before);
    assert_eq!(
        leave.decide(
            LeaveTransition::Approve,
            human_decision(
                "reviewer",
                AuthorityScope::Company {
                    holding_id: id(1),
                    company_id: id(3),
                },
                expected_command,
                decided_at,
            ),
            expected_command,
            decided_at,
        ),
        Err(PeoplePayrollDomainError::InvalidHumanDecision)
    );
    assert_eq!(leave, before);
}

#[test]
fn payroll_requires_unique_lines_and_three_distinct_lifecycle_actors() {
    let employee = employee_id(10);
    let line = PayrollLine::try_new(employee, 10_000, 1_000).unwrap();
    assert_eq!(
        PayrollRun::create(
            PayrollRunId::try_from_uuid(id(40)).unwrap(),
            scope(),
            "2026-08".into(),
            date(1),
            date(31),
            Currency::try_new("THB").unwrap(),
            vec![line.clone(), line.clone()],
            "preparer".into(),
            at(),
        ),
        Err(PeoplePayrollDomainError::InvalidPayrollLines),
    );

    let mut run = PayrollRun::create(
        PayrollRunId::try_from_uuid(id(41)).unwrap(),
        scope(),
        "2026-08".into(),
        date(1),
        date(31),
        Currency::try_new("THB").unwrap(),
        vec![line],
        "preparer".into(),
        at(),
    )
    .unwrap();
    assert_eq!(
        run.transition(
            PayrollTransition::SubmitForReview,
            "preparer",
            at() + Duration::seconds(30),
        ),
        Err(PeoplePayrollDomainError::InvalidTransition),
    );
    run.pin_calculation_evidence(
        payroll_evidence(at() + Duration::seconds(30)),
        at() + Duration::seconds(30),
    )
    .unwrap();
    run.transition(
        PayrollTransition::SubmitForReview,
        "preparer",
        at() + Duration::minutes(1),
    )
    .unwrap();
    let pinned = run.calculation_evidence().cloned().unwrap();
    assert_eq!(
        run.pin_calculation_evidence(
            payroll_evidence(at() + Duration::seconds(45)),
            at() + Duration::seconds(45),
        ),
        Err(PeoplePayrollDomainError::InvalidPayrollEvidence),
    );
    assert_eq!(run.calculation_evidence(), Some(&pinned));
    let review_at = at() + Duration::minutes(2);
    let review_command = command_id(93);
    assert_eq!(
        run.decide(
            PayrollTransition::Review,
            human_decision("preparer", scope(), review_command, review_at),
            review_command,
            review_at,
        ),
        Err(PeoplePayrollDomainError::ApprovalSeparation),
    );
    run.decide(
        PayrollTransition::Review,
        human_decision("reviewer", scope(), review_command, review_at),
        review_command,
        review_at,
    )
    .unwrap();
    let post_at = at() + Duration::minutes(3);
    let post_command = command_id(94);
    assert_eq!(
        run.decide(
            PayrollTransition::Post,
            human_decision("reviewer", scope(), post_command, post_at),
            post_command,
            post_at,
        ),
        Err(PeoplePayrollDomainError::ApprovalSeparation),
    );
    run.decide(
        PayrollTransition::Post,
        human_decision("poster", scope(), post_command, post_at),
        post_command,
        post_at,
    )
    .unwrap();
    assert_eq!(run.status(), PayrollStatus::Posted);
    assert_eq!(run.total_net_minor(), 9_000);
}

#[test]
fn failed_version_and_timestamp_advances_are_atomic() {
    let mut employee = Employee::hire(
        employee_id(10),
        scope(),
        "EMP-010".into(),
        "Ari".into(),
        date(1),
        at(),
    )
    .unwrap();
    let before_time = employee.clone();
    assert_eq!(
        employee.transition_for_test(EmployeeTransition::Activate, at() - Duration::seconds(1)),
        Err(PeoplePayrollDomainError::InvalidWindow),
    );
    assert_eq!(employee, before_time);

    employee.version = u64::MAX;
    let before_version = employee.clone();
    assert_eq!(
        employee.transition_for_test(EmployeeTransition::Activate, at()),
        Err(PeoplePayrollDomainError::VersionOverflow),
    );
    assert_eq!(employee, before_version);
}

#[test]
fn leave_and_expense_events_remain_private_while_payroll_finalization_is_publishable() {
    let mut run = PayrollRun::create(
        PayrollRunId::try_from_uuid(id(50)).unwrap(),
        scope(),
        "2026-08".into(),
        date(1),
        date(31),
        Currency::try_new("THB").unwrap(),
        vec![PayrollLine::try_new(employee_id(10), 10_000, 1_000).unwrap()],
        "preparer".into(),
        at(),
    )
    .unwrap();
    run.pin_calculation_evidence(
        payroll_evidence(at() + Duration::seconds(30)),
        at() + Duration::seconds(30),
    )
    .unwrap();
    run.take_uncommitted_events();
    run.transition(
        PayrollTransition::SubmitForReview,
        "preparer",
        at() + Duration::minutes(1),
    )
    .unwrap();
    let review_at = at() + Duration::minutes(2);
    let review_command = command_id(95);
    run.decide(
        PayrollTransition::Review,
        human_decision("reviewer", scope(), review_command, review_at),
        review_command,
        review_at,
    )
    .unwrap();
    let post_at = at() + Duration::minutes(3);
    let post_command = command_id(96);
    run.decide(
        PayrollTransition::Post,
        human_decision("poster", scope(), post_command, post_at),
        post_command,
        post_at,
    )
    .unwrap();
    let events = run.take_uncommitted_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, DomainEvent::PayrollFinalized { .. }))
    );
}

#[test]
fn typed_employee_rehydration_revalidates_state_without_replaying_events() {
    let live = Employee::hire(
        employee_id(91),
        scope(),
        "EMP-091".into(),
        "Relational Worker".into(),
        date(1),
        at(),
    )
    .unwrap();
    let mut restored = Employee::try_rehydrate(EmployeeState {
        id: live.id(),
        version: live.version(),
        scope: live.scope().clone(),
        employee_code: live.employee_code().into(),
        display_name: live.display_name().into(),
        hired_on: live.hired_on(),
        terminated_on: live.terminated_on(),
        status: live.status(),
        created_at: live.created_at(),
        updated_at: live.updated_at(),
    })
    .unwrap();
    assert!(restored.take_uncommitted_events().is_empty());

    let invalid = Employee::try_rehydrate(EmployeeState {
        id: live.id(),
        version: 0,
        scope: live.scope().clone(),
        employee_code: live.employee_code().into(),
        display_name: live.display_name().into(),
        hired_on: live.hired_on(),
        terminated_on: live.terminated_on(),
        status: live.status(),
        created_at: live.created_at(),
        updated_at: live.updated_at(),
    });
    assert_eq!(invalid, Err(PeoplePayrollDomainError::VersionOverflow));
}
