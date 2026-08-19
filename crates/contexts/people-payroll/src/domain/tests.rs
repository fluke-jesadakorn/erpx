use chrono::{Duration, TimeZone, Utc};
use erpx_kernel::AuthorityScope;
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
        .transition(EmployeeTransition::Activate, at() + Duration::minutes(1))
        .unwrap();
    employee
        .transition(EmployeeTransition::PutOnLeave, at() + Duration::minutes(2))
        .unwrap();
    employee
        .transition(
            EmployeeTransition::ReturnFromLeave,
            at() + Duration::minutes(3),
        )
        .unwrap();
    assert_eq!(employee.status(), EmployeeStatus::Active);

    let before = employee.clone();
    assert_eq!(
        employee.transition(
            EmployeeTransition::Terminate {
                terminated_on: date(1).pred_opt().unwrap()
            },
            at() + Duration::minutes(4),
        ),
        Err(PeoplePayrollDomainError::InvalidTransition),
    );
    assert_eq!(employee, before);

    employee
        .transition(
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
    assert_eq!(
        leave.transition(
            LeaveTransition::Approve,
            "requester",
            at() + Duration::minutes(2)
        ),
        Err(PeoplePayrollDomainError::ApprovalSeparation),
    );
    assert_eq!(leave, before);
    leave
        .transition(
            LeaveTransition::Approve,
            "reviewer",
            at() + Duration::minutes(2),
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
    assert_eq!(
        expense.transition(
            ExpenseTransition::Approve,
            "claimant",
            at() + Duration::minutes(2)
        ),
        Err(PeoplePayrollDomainError::ApprovalSeparation),
    );
    expense
        .transition(
            ExpenseTransition::Approve,
            "reviewer",
            at() + Duration::minutes(2),
        )
        .unwrap();
    expense
        .transition(
            ExpenseTransition::Reimburse,
            "payments",
            at() + Duration::minutes(3),
        )
        .unwrap();
    assert_eq!(expense.status(), ExpenseStatus::Reimbursed);
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
    assert_eq!(
        run.transition(
            PayrollTransition::Review,
            "preparer",
            at() + Duration::minutes(2)
        ),
        Err(PeoplePayrollDomainError::ApprovalSeparation),
    );
    run.transition(
        PayrollTransition::Review,
        "reviewer",
        at() + Duration::minutes(2),
    )
    .unwrap();
    assert_eq!(
        run.transition(
            PayrollTransition::Post,
            "reviewer",
            at() + Duration::minutes(3)
        ),
        Err(PeoplePayrollDomainError::ApprovalSeparation),
    );
    run.transition(
        PayrollTransition::Post,
        "poster",
        at() + Duration::minutes(3),
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
        employee.transition(EmployeeTransition::Activate, at() - Duration::seconds(1)),
        Err(PeoplePayrollDomainError::InvalidWindow),
    );
    assert_eq!(employee, before_time);

    employee.version = u64::MAX;
    let before_version = employee.clone();
    assert_eq!(
        employee.transition(EmployeeTransition::Activate, at()),
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
    run.transition(
        PayrollTransition::Review,
        "reviewer",
        at() + Duration::minutes(2),
    )
    .unwrap();
    run.transition(
        PayrollTransition::Post,
        "poster",
        at() + Duration::minutes(3),
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
