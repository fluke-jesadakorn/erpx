use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use chrono::{Duration, TimeZone, Utc};
use erpx_cqrs::{
    CONTRACT_VERSION, CommandEnvelope, CommandHandler, FreshnessState, IdempotencyKey,
    ProjectionMetadata, ProjectionNotReady, ProjectionTarget, QueryConsistency, QueryEnvelope,
    QueryHandler, ReadYourWrites,
};
use erpx_integration_contracts::people_payroll;
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    FixedClock, QueryId, TenantScope,
};
use uuid::Uuid;

use super::*;
use crate::{
    domain::{EmployeeStatus, PayrollStatus},
    ports::{
        CommandIdentity, EmployeeListFilter, ExpenseClaimListFilter, LeaveRequestListFilter,
        PayrollRunListFilter, PeoplePayrollCommit, ReadProjection,
    },
};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}
fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, day).unwrap()
}
fn scope() -> TenantScope {
    TenantScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}
fn employee_id(value: u128) -> EmployeeId {
    EmployeeId::try_from_uuid(id(value)).unwrap()
}
fn actor(name: &str) -> ActorAttribution {
    ActorAttribution {
        principal_ref: name.into(),
        kind: ActorKind::TenantUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}
fn envelope(
    command: PeoplePayrollCommand,
    expected: Option<u64>,
    principal: &str,
) -> CommandEnvelope<PeoplePayrollCommand> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(100)).unwrap(),
        idempotency_key: IdempotencyKey::try_new("people-payroll-stable-key").unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(101)).unwrap(),
        causation_id: None,
        actor: actor(principal),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "people_payroll.test".into(),
            customer_visible: false,
            module: Some("people_payroll".into()),
        },
        expected_aggregate_version: expected,
        command,
    }
}

fn employee() -> Employee {
    Employee::hire(
        employee_id(10),
        scope(),
        "EMP-010".into(),
        "Ari".into(),
        date(1),
        at(),
    )
    .unwrap()
}

#[derive(Clone)]
struct FakeAuthorization(bool);
#[async_trait]
impl AuthorizationPort for FakeAuthorization {
    async fn authorize(&self, _request: AuthorizationRequest<'_>) -> Result<(), PortError> {
        if self.0 {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}

#[derive(Clone)]
struct FakeEntitlement(bool);
#[async_trait]
impl EntitlementPort for FakeEntitlement {
    async fn ensure_people_payroll_enabled(
        &self,
        _scope: &TenantScope,
        _write: bool,
    ) -> Result<(), PortError> {
        if self.0 {
            Ok(())
        } else {
            Err(PortError::EntitlementDenied)
        }
    }
}

#[derive(Clone, Default)]
struct FakeWrite {
    employee: Option<Employee>,
    leave: Option<LeaveRequest>,
    expense: Option<ExpenseClaim>,
    payroll: Option<PayrollRun>,
}

#[async_trait]
impl WriteRepository for FakeWrite {
    async fn load_employee(
        &self,
        _scope: &TenantScope,
        _id: EmployeeId,
    ) -> Result<Option<Employee>, PortError> {
        Ok(self.employee.clone())
    }
    async fn load_leave_request(
        &self,
        _scope: &TenantScope,
        _id: LeaveRequestId,
    ) -> Result<Option<LeaveRequest>, PortError> {
        Ok(self.leave.clone())
    }
    async fn load_expense_claim(
        &self,
        _scope: &TenantScope,
        _id: ExpenseClaimId,
    ) -> Result<Option<ExpenseClaim>, PortError> {
        Ok(self.expense.clone())
    }
    async fn load_payroll_run(
        &self,
        _scope: &TenantScope,
        _id: PayrollRunId,
    ) -> Result<Option<PayrollRun>, PortError> {
        Ok(self.payroll.clone())
    }
}

#[derive(Clone, Default)]
struct FakeUnitOfWork {
    commits: Arc<AtomicUsize>,
    request: Arc<Mutex<Option<PeoplePayrollCommit>>>,
}

#[async_trait]
impl CommandUnitOfWork for FakeUnitOfWork {
    async fn find_receipt(
        &self,
        _identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        Ok(None)
    }

    async fn commit(&self, request: PeoplePayrollCommit) -> Result<StoredCommit, PortError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        *self.request.lock().unwrap() = Some(request.clone());
        Ok(StoredCommit {
            command_id: request.identity.command_id,
            correlation_id: request.correlation_id,
            aggregate: request.aggregate,
            committed_events: Vec::new(),
            committed_at: at(),
            replayed: false,
        })
    }
}

fn handler(
    write: FakeWrite,
    uow: FakeUnitOfWork,
    allowed: bool,
) -> PeoplePayrollCommandHandler<
    FakeWrite,
    FakeUnitOfWork,
    FakeAuthorization,
    FakeEntitlement,
    FixedClock,
> {
    PeoplePayrollCommandHandler::new(
        write,
        uow,
        FakeAuthorization(allowed),
        FakeEntitlement(true),
        FixedClock::new(at()),
    )
}

#[tokio::test]
async fn mutation_requires_expected_version_before_atomic_commit() {
    let uow = FakeUnitOfWork::default();
    let error = handler(
        FakeWrite {
            employee: Some(employee()),
            ..FakeWrite::default()
        },
        uow.clone(),
        true,
    )
    .handle(envelope(
        PeoplePayrollCommand::TransitionEmployee(TransitionEmployee {
            employee_id: employee_id(10),
            transition: EmployeeTransition::Activate,
        }),
        None,
        "people-admin",
    ))
    .await
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn authorization_denial_prevents_any_commit() {
    let uow = FakeUnitOfWork::default();
    let error = handler(FakeWrite::default(), uow.clone(), false)
        .handle(envelope(
            PeoplePayrollCommand::HireEmployee(HireEmployee {
                employee_id: employee_id(10),
                employee_code: "EMP-010".into(),
                display_name: "Ari".into(),
                hired_on: date(1),
            }),
            None,
            "people-admin",
        ))
        .await
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Denied);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn hire_commits_snapshot_receipt_audit_and_employee_fact_as_one_request() {
    let uow = FakeUnitOfWork::default();
    let receipt = handler(FakeWrite::default(), uow.clone(), true)
        .handle(envelope(
            PeoplePayrollCommand::HireEmployee(HireEmployee {
                employee_id: employee_id(10),
                employee_code: "EMP-010".into(),
                display_name: "Ari".into(),
                hired_on: date(1),
            }),
            None,
            "people-admin",
        ))
        .await
        .unwrap();
    assert_eq!(receipt.aggregate.aggregate_type, "employee");
    let request = uow.request.lock().unwrap().clone().unwrap();
    assert_eq!(request.audit.purpose, "people_payroll.test");
    assert!(matches!(
        request.integration_events.as_slice(),
        [people_payroll::Event::EmployeeStatusChanged(fact)] if !fact.active
    ));
}

#[tokio::test]
async fn posting_payroll_emits_the_only_finance_facing_payroll_fact() {
    let mut payroll = PayrollRun::create(
        PayrollRunId::try_from_uuid(id(40)).unwrap(),
        scope(),
        "2026-08".into(),
        date(1),
        date(31),
        Currency::try_new("THB").unwrap(),
        vec![PayrollLine::try_new(employee_id(10), 10_000, 1_000).unwrap()],
        "preparer".into(),
        at() - Duration::minutes(3),
    )
    .unwrap();
    payroll
        .pin_calculation_evidence(
            PayrollCalculationEvidence {
                policy_version: "TH-PAYROLL-2026.08".into(),
                rate_table_version: "TH-RATES-2026.08".into(),
                calculation_sha256: format!("sha256:{}", "a".repeat(64)),
                source_inputs: Vec::new(),
                captured_at: at() - Duration::minutes(2),
            },
            at() - Duration::minutes(2),
        )
        .unwrap();
    payroll.take_uncommitted_events();
    payroll
        .transition(
            PayrollTransition::SubmitForReview,
            "preparer",
            at() - Duration::minutes(2),
        )
        .unwrap();
    payroll.take_uncommitted_events();
    payroll
        .transition(
            PayrollTransition::Review,
            "reviewer",
            at() - Duration::minutes(1),
        )
        .unwrap();
    payroll.take_uncommitted_events();
    assert_eq!(payroll.status(), PayrollStatus::Approved);

    let expected = payroll.version();
    let uow = FakeUnitOfWork::default();
    handler(
        FakeWrite {
            payroll: Some(payroll),
            ..FakeWrite::default()
        },
        uow.clone(),
        true,
    )
    .handle(envelope(
        PeoplePayrollCommand::TransitionPayrollRun(TransitionPayrollRun {
            payroll_run_id: PayrollRunId::try_from_uuid(id(40)).unwrap(),
            transition: PayrollTransition::Post,
        }),
        Some(expected),
        "poster",
    ))
    .await
    .unwrap();
    let request = uow.request.lock().unwrap().clone().unwrap();
    assert!(matches!(
        request.integration_events.as_slice(),
        [people_payroll::Event::PayrollFinalized(fact)]
            if fact.net_amount.minor_units() == 9_000
    ));
}

#[tokio::test]
async fn payroll_evidence_pin_requires_version_and_commits_immutable_snapshot() {
    let payroll_id = PayrollRunId::try_from_uuid(id(41)).unwrap();
    let payroll = PayrollRun::create(
        payroll_id,
        scope(),
        "2026-08".into(),
        date(1),
        date(31),
        Currency::try_new("THB").unwrap(),
        vec![PayrollLine::try_new(employee_id(10), 10_000, 1_000).unwrap()],
        "preparer".into(),
        at() - Duration::minutes(1),
    )
    .unwrap();
    let evidence = PayrollCalculationEvidence {
        policy_version: "TH-PAYROLL-2026.08".into(),
        rate_table_version: "TH-RATES-2026.08".into(),
        calculation_sha256: format!("sha256:{}", "a".repeat(64)),
        source_inputs: Vec::new(),
        captured_at: at(),
    };
    let command = PinPayrollCalculationEvidence {
        payroll_run_id: payroll_id,
        evidence: evidence.clone(),
    };
    let pin_envelope = |expected| CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(110)).unwrap(),
        idempotency_key: IdempotencyKey::try_new("payroll-evidence-stable-key").unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(111)).unwrap(),
        causation_id: None,
        actor: actor("payroll-preparer"),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "people_payroll.pin_evidence.test".into(),
            customer_visible: false,
            module: Some("people_payroll".into()),
        },
        expected_aggregate_version: expected,
        command: command.clone(),
    };

    let denied_uow = FakeUnitOfWork::default();
    let error = handler(
        FakeWrite {
            payroll: Some(payroll.clone()),
            ..FakeWrite::default()
        },
        denied_uow.clone(),
        true,
    )
    .handle(pin_envelope(None))
    .await
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(denied_uow.commits.load(Ordering::SeqCst), 0);

    let uow = FakeUnitOfWork::default();
    handler(
        FakeWrite {
            payroll: Some(payroll.clone()),
            ..FakeWrite::default()
        },
        uow.clone(),
        true,
    )
    .handle(pin_envelope(Some(payroll.version())))
    .await
    .unwrap();
    let request = uow.request.lock().unwrap().clone().unwrap();
    assert!(matches!(
        request.identity.fingerprint,
        CommandFingerprint::PinPayrollCalculationEvidence { .. }
    ));
    let Aggregate::PayrollRun(committed) = request.aggregate else {
        panic!("expected payroll run")
    };
    assert_eq!(committed.calculation_evidence(), Some(&evidence));
}

#[test]
fn stable_fingerprint_excludes_retry_metadata() {
    let command = PeoplePayrollCommand::TransitionEmployee(TransitionEmployee {
        employee_id: employee_id(10),
        transition: EmployeeTransition::Activate,
    });
    let first = command.fingerprint();
    let second = command.fingerprint();
    assert_eq!(first, second);
}

struct NotReadyRead;

#[async_trait]
impl ReadRepository for NotReadyRead {
    async fn get_employee(
        &self,
        _scope: &TenantScope,
        _id: EmployeeId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<EmployeeView>>, PortError> {
        let QueryConsistency::ReadYourWrites(requirement) = consistency else {
            return Err(PortError::Internal);
        };
        Err(PortError::NotReady(ProjectionNotReady {
            projection: requirement.projection.clone(),
            partition: requirement.partition.clone(),
            target: requirement.target,
            observed: None,
        }))
    }
    async fn list_employees(
        &self,
        _scope: &TenantScope,
        _filter: &EmployeeListFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<EmployeeList>, PortError> {
        Ok(ReadProjection {
            value: EmployeeList {
                employees: Vec::new(),
                next_cursor: None,
            },
            metadata: current_projection(),
        })
    }
    async fn get_leave_request(
        &self,
        _scope: &TenantScope,
        _id: LeaveRequestId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<LeaveRequestView>>, PortError> {
        Err(PortError::Internal)
    }
    async fn list_leave_requests(
        &self,
        _scope: &TenantScope,
        _filter: &LeaveRequestListFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<LeaveRequestList>, PortError> {
        Err(PortError::Internal)
    }
    async fn get_expense_claim(
        &self,
        _scope: &TenantScope,
        _id: ExpenseClaimId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ExpenseClaimView>>, PortError> {
        Err(PortError::Internal)
    }
    async fn list_expense_claims(
        &self,
        _scope: &TenantScope,
        _filter: &ExpenseClaimListFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<ExpenseClaimList>, PortError> {
        Err(PortError::Internal)
    }
    async fn get_payroll_run(
        &self,
        _scope: &TenantScope,
        _id: PayrollRunId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PayrollRunView>>, PortError> {
        Err(PortError::Internal)
    }
    async fn list_payroll_runs(
        &self,
        _scope: &TenantScope,
        _filter: &PayrollRunListFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<PayrollRunList>, PortError> {
        Err(PortError::Internal)
    }
}

fn current_projection() -> ProjectionMetadata {
    ProjectionMetadata {
        checkpoint: None,
        freshness: FreshnessState::Current,
        lag_millis: None,
    }
}

#[tokio::test]
async fn read_your_writes_not_ready_never_falls_back_to_write_state() {
    let target = ProjectionTarget {
        event_id: EventId::try_from_uuid(id(200)).unwrap(),
        global_sequence: 8,
    };
    let query = QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(id(201)).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(202)).unwrap(),
        actor: actor("people-reader"),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "people_payroll.test".into(),
            customer_visible: false,
            module: Some("people_payroll".into()),
        },
        consistency: QueryConsistency::ReadYourWrites(ReadYourWrites {
            projection: "people_payroll.employees".into(),
            partition: "tenant-1".into(),
            target,
            timeout_millis: 100,
        }),
        query: PeoplePayrollQuery::GetEmployee(GetEmployee {
            employee_id: employee_id(10),
        }),
    };
    let error = PeoplePayrollQueryHandler::new(
        NotReadyRead,
        FakeAuthorization(true),
        FakeEntitlement(true),
    )
    .handle(query)
    .await
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotReady);
}

#[tokio::test]
async fn list_queries_reject_unbounded_page_sizes_before_read_access() {
    let query = QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(id(210)).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(211)).unwrap(),
        actor: actor("people-reader"),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "people_payroll.test".into(),
            customer_visible: false,
            module: Some("people_payroll".into()),
        },
        consistency: QueryConsistency::Eventual,
        query: PeoplePayrollQuery::ListEmployees(ListEmployees {
            filter: EmployeeListFilter {
                status: Some(EmployeeStatus::Active),
                page_size: 0,
                cursor: None,
            },
        }),
    };
    let error = PeoplePayrollQueryHandler::new(
        NotReadyRead,
        FakeAuthorization(true),
        FakeEntitlement(true),
    )
    .handle(query)
    .await
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Invalid);
}
