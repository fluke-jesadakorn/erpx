use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::people_payroll;
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    Aggregate, Currency, Employee, EmployeeId, EmployeeStatus, EmployeeTransition, ExpenseClaim,
    ExpenseClaimId, ExpenseStatus, ExpenseTransition, LeaveRequest, LeaveRequestId, LeaveStatus,
    LeaveTransition, PayrollLine, PayrollRun, PayrollRunId, PayrollStatus, PayrollTransition,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeoplePayrollAction {
    HireEmployee,
    TransitionEmployee,
    CreateLeaveRequest,
    TransitionLeaveRequest,
    CreateExpenseClaim,
    TransitionExpenseClaim,
    CreatePayrollRun,
    TransitionPayrollRun,
    ViewEmployee,
    ListEmployees,
    ViewLeaveRequest,
    ListLeaveRequests,
    ViewExpenseClaim,
    ListExpenseClaims,
    ViewPayrollRun,
    ListPayrollRuns,
}

impl PeoplePayrollAction {
    pub const fn requires_write_entitlement(self) -> bool {
        matches!(
            self,
            Self::HireEmployee
                | Self::TransitionEmployee
                | Self::CreateLeaveRequest
                | Self::TransitionLeaveRequest
                | Self::CreateExpenseClaim
                | Self::TransitionExpenseClaim
                | Self::CreatePayrollRun
                | Self::TransitionPayrollRun
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a TenantScope,
    pub target_scope: &'a TenantScope,
    pub action: PeoplePayrollAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_people_payroll_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    HireEmployee {
        employee_id: EmployeeId,
        employee_code: String,
        display_name: String,
        hired_on: NaiveDate,
    },
    TransitionEmployee {
        employee_id: EmployeeId,
        transition: EmployeeTransition,
    },
    CreateLeaveRequest {
        leave_request_id: LeaveRequestId,
        employee_id: EmployeeId,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
        reason: String,
    },
    TransitionLeaveRequest {
        leave_request_id: LeaveRequestId,
        transition: LeaveTransition,
    },
    CreateExpenseClaim {
        expense_claim_id: ExpenseClaimId,
        employee_id: EmployeeId,
        currency: Currency,
        amount_minor: i64,
        reason: String,
    },
    TransitionExpenseClaim {
        expense_claim_id: ExpenseClaimId,
        transition: ExpenseTransition,
    },
    CreatePayrollRun {
        payroll_run_id: PayrollRunId,
        period_code: String,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
        currency: Currency,
        lines: Vec<PayrollLine>,
    },
    TransitionPayrollRun {
        payroll_run_id: PayrollRunId,
        transition: PayrollTransition,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: CommandFingerprint,
}

#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load_employee(
        &self,
        authority_scope: &TenantScope,
        employee_id: EmployeeId,
    ) -> Result<Option<Employee>, PortError>;

    async fn load_leave_request(
        &self,
        authority_scope: &TenantScope,
        leave_request_id: LeaveRequestId,
    ) -> Result<Option<LeaveRequest>, PortError>;

    async fn load_expense_claim(
        &self,
        authority_scope: &TenantScope,
        expense_claim_id: ExpenseClaimId,
    ) -> Result<Option<ExpenseClaim>, PortError>;

    async fn load_payroll_run(
        &self,
        authority_scope: &TenantScope,
        payroll_run_id: PayrollRunId,
    ) -> Result<Option<PayrollRun>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeoplePayrollCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: Aggregate,
    pub integration_events: Vec<people_payroll::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: Aggregate,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

impl StoredCommit {
    pub fn into_receipt<R>(self, response: R) -> CommandReceipt<R> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "people_payroll".into(),
                aggregate_type: self.aggregate.aggregate_type().into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id_uuid())
                    .expect("validated People & Payroll aggregate identifier"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError>;

    /// Atomically persists the aggregate snapshot, stable receipt, audit, and outbox.
    async fn commit(&self, request: PeoplePayrollCommit) -> Result<StoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmployeeView {
    pub employee_id: EmployeeId,
    pub version: u64,
    pub scope: TenantScope,
    pub employee_code: String,
    pub display_name: String,
    pub hired_on: NaiveDate,
    pub terminated_on: Option<NaiveDate>,
    pub status: EmployeeStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Employee> for EmployeeView {
    fn from(value: &Employee) -> Self {
        Self {
            employee_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            employee_code: value.employee_code().into(),
            display_name: value.display_name().into(),
            hired_on: value.hired_on(),
            terminated_on: value.terminated_on(),
            status: value.status(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaveRequestView {
    pub leave_request_id: LeaveRequestId,
    pub version: u64,
    pub scope: TenantScope,
    pub employee_id: EmployeeId,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub reason: String,
    pub status: LeaveStatus,
    pub requested_by: String,
    pub approved_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&LeaveRequest> for LeaveRequestView {
    fn from(value: &LeaveRequest) -> Self {
        Self {
            leave_request_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            employee_id: value.employee_id(),
            starts_on: value.starts_on(),
            ends_on: value.ends_on(),
            reason: value.reason().into(),
            status: value.status(),
            requested_by: value.requested_by().into(),
            approved_by: value.approved_by().map(str::to_owned),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpenseClaimView {
    pub expense_claim_id: ExpenseClaimId,
    pub version: u64,
    pub scope: TenantScope,
    pub employee_id: EmployeeId,
    pub currency: Currency,
    pub amount_minor: i64,
    pub reason: String,
    pub status: ExpenseStatus,
    pub claimed_by: String,
    pub approved_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&ExpenseClaim> for ExpenseClaimView {
    fn from(value: &ExpenseClaim) -> Self {
        Self {
            expense_claim_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            employee_id: value.employee_id(),
            currency: value.currency(),
            amount_minor: value.amount_minor(),
            reason: value.reason().into(),
            status: value.status(),
            claimed_by: value.claimed_by().into(),
            approved_by: value.approved_by().map(str::to_owned),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayrollRunView {
    pub payroll_run_id: PayrollRunId,
    pub version: u64,
    pub scope: TenantScope,
    pub period_code: String,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub currency: Currency,
    pub lines: Vec<PayrollLine>,
    pub status: PayrollStatus,
    pub prepared_by: String,
    pub reviewed_by: Option<String>,
    pub approved_by: Option<String>,
    pub total_gross_minor: i64,
    pub total_deductions_minor: i64,
    pub total_net_minor: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&PayrollRun> for PayrollRunView {
    fn from(value: &PayrollRun) -> Self {
        Self {
            payroll_run_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            period_code: value.period_code().into(),
            starts_on: value.starts_on(),
            ends_on: value.ends_on(),
            currency: value.currency(),
            lines: value.lines().to_vec(),
            status: value.status(),
            prepared_by: value.prepared_by().into(),
            reviewed_by: value.reviewed_by().map(str::to_owned),
            approved_by: value.approved_by().map(str::to_owned),
            total_gross_minor: value.total_gross_minor(),
            total_deductions_minor: value.total_deductions_minor(),
            total_net_minor: value.total_net_minor(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EmployeeListFilter {
    pub status: Option<EmployeeStatus>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmployeeList {
    pub employees: Vec<EmployeeView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LeaveRequestListFilter {
    pub employee_id: Option<EmployeeId>,
    pub status: Option<LeaveStatus>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaveRequestList {
    pub leave_requests: Vec<LeaveRequestView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExpenseClaimListFilter {
    pub employee_id: Option<EmployeeId>,
    pub status: Option<ExpenseStatus>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpenseClaimList {
    pub expense_claims: Vec<ExpenseClaimView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PayrollRunListFilter {
    pub status: Option<PayrollStatus>,
    pub starts_on_or_after: Option<NaiveDate>,
    pub ends_on_or_before: Option<NaiveDate>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayrollRunList {
    pub payroll_runs: Vec<PayrollRunView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Disposable projection access. This port cannot expose a write-store fallback.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get_employee(
        &self,
        authority_scope: &TenantScope,
        employee_id: EmployeeId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<EmployeeView>>, PortError>;
    async fn list_employees(
        &self,
        authority_scope: &TenantScope,
        filter: &EmployeeListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<EmployeeList>, PortError>;
    async fn get_leave_request(
        &self,
        authority_scope: &TenantScope,
        leave_request_id: LeaveRequestId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<LeaveRequestView>>, PortError>;
    async fn list_leave_requests(
        &self,
        authority_scope: &TenantScope,
        filter: &LeaveRequestListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<LeaveRequestList>, PortError>;
    async fn get_expense_claim(
        &self,
        authority_scope: &TenantScope,
        expense_claim_id: ExpenseClaimId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ExpenseClaimView>>, PortError>;
    async fn list_expense_claims(
        &self,
        authority_scope: &TenantScope,
        filter: &ExpenseClaimListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<ExpenseClaimList>, PortError>;
    async fn get_payroll_run(
        &self,
        authority_scope: &TenantScope,
        payroll_run_id: PayrollRunId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PayrollRunView>>, PortError>;
    async fn list_payroll_runs(
        &self,
        authority_scope: &TenantScope,
        filter: &PayrollRunListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<PayrollRunList>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("People & Payroll operation is denied")]
    Denied,
    #[error("People & Payroll entitlement is unavailable")]
    EntitlementDenied,
    #[error("People & Payroll record was not found")]
    NotFound,
    #[error("People & Payroll aggregate conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("People & Payroll idempotency identity conflicts")]
    IdempotencyConflict,
    #[error("People & Payroll projection is not ready")]
    NotReady(ProjectionNotReady),
    #[error("People & Payroll storage is unavailable")]
    Unavailable,
    #[error("People & Payroll adapter failed")]
    Internal,
}
