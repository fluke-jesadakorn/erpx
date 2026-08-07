use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_people_payroll::{
    EmployeeStatus, ExpenseStatus, LeaveStatus, PayrollStatus, PeoplePayrollCommand,
    PeoplePayrollMutationResult, PeoplePayrollQuery, PeoplePayrollQueryResponse,
    ports::{
        EmployeeList, EmployeeView, ExpenseClaimList, ExpenseClaimView, LeaveRequestList,
        LeaveRequestView, PayrollRunList, PayrollRunView,
    },
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

use super::{
    models::{
        EmployeeItem, EmployeeItems, ExpenseClaimItem, ExpenseClaimItems, LeaveRequestItem,
        LeaveRequestItems, PayrollLineItem, PayrollRunItem, PayrollRunItems,
    },
    presenter::PeoplePayrollContent,
};

pub fn present_mutation(value: &PeoplePayrollMutationResult) -> PeoplePayrollContent {
    match value {
        PeoplePayrollMutationResult::Employee(value) => {
            PeoplePayrollContent::Employee(value.clone().into())
        }
        PeoplePayrollMutationResult::LeaveRequest(value) => {
            PeoplePayrollContent::LeaveRequest(value.clone().into())
        }
        PeoplePayrollMutationResult::ExpenseClaim(value) => {
            PeoplePayrollContent::ExpenseClaim(value.clone().into())
        }
        PeoplePayrollMutationResult::PayrollRun(value) => {
            PeoplePayrollContent::PayrollRun(value.clone().into())
        }
    }
}

pub fn present_query(
    value: QueryResult<PeoplePayrollQueryResponse>,
) -> QueryResult<Option<PeoplePayrollContent>> {
    let metadata = value.metadata;
    let value = match value.value {
        PeoplePayrollQueryResponse::Employee(value) => value
            .map(EmployeeItem::from)
            .map(PeoplePayrollContent::Employee),
        PeoplePayrollQueryResponse::Employees(value) => {
            let items = EmployeeItems::from(value);
            (!items.employees.is_empty())
                .then_some(items)
                .map(PeoplePayrollContent::Employees)
        }
        PeoplePayrollQueryResponse::LeaveRequest(value) => value
            .map(LeaveRequestItem::from)
            .map(PeoplePayrollContent::LeaveRequest),
        PeoplePayrollQueryResponse::LeaveRequests(value) => {
            let items = LeaveRequestItems::from(value);
            (!items.leave_requests.is_empty())
                .then_some(items)
                .map(PeoplePayrollContent::LeaveRequests)
        }
        PeoplePayrollQueryResponse::ExpenseClaim(value) => value
            .map(ExpenseClaimItem::from)
            .map(PeoplePayrollContent::ExpenseClaim),
        PeoplePayrollQueryResponse::ExpenseClaims(value) => {
            let items = ExpenseClaimItems::from(value);
            (!items.expense_claims.is_empty())
                .then_some(items)
                .map(PeoplePayrollContent::ExpenseClaims)
        }
        PeoplePayrollQueryResponse::PayrollRun(value) => value
            .map(PayrollRunItem::from)
            .map(PeoplePayrollContent::PayrollRun),
        PeoplePayrollQueryResponse::PayrollRuns(value) => {
            let items = PayrollRunItems::from(value);
            (!items.payroll_runs.is_empty())
                .then_some(items)
                .map(PeoplePayrollContent::PayrollRuns)
        }
    };
    QueryResult { value, metadata }
}

impl From<EmployeeView> for EmployeeItem {
    fn from(value: EmployeeView) -> Self {
        Self {
            employee_id: value.employee_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            employee_code: value.employee_code,
            display_name: value.display_name,
            hired_on: value.hired_on.to_string(),
            terminated_on: value.terminated_on.map(|date| date.to_string()),
            status: employee_status(value.status).into(),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}
impl From<EmployeeList> for EmployeeItems {
    fn from(value: EmployeeList) -> Self {
        Self {
            employees: value
                .employees
                .into_iter()
                .map(EmployeeItem::from)
                .collect(),
            next_cursor: value.next_cursor,
        }
    }
}
impl From<LeaveRequestView> for LeaveRequestItem {
    fn from(value: LeaveRequestView) -> Self {
        Self {
            leave_request_id: value.leave_request_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            employee_id: value.employee_id.as_uuid().to_string(),
            starts_on: value.starts_on.to_string(),
            ends_on: value.ends_on.to_string(),
            reason: value.reason,
            status: leave_status(value.status).into(),
            requested_by: value.requested_by,
            approved_by: value.approved_by,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}
impl From<LeaveRequestList> for LeaveRequestItems {
    fn from(value: LeaveRequestList) -> Self {
        Self {
            leave_requests: value
                .leave_requests
                .into_iter()
                .map(LeaveRequestItem::from)
                .collect(),
            next_cursor: value.next_cursor,
        }
    }
}
impl From<ExpenseClaimView> for ExpenseClaimItem {
    fn from(value: ExpenseClaimView) -> Self {
        Self {
            expense_claim_id: value.expense_claim_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            employee_id: value.employee_id.as_uuid().to_string(),
            currency: value.currency.as_str().into(),
            amount_minor: value.amount_minor,
            reason: value.reason,
            status: expense_status(value.status).into(),
            claimed_by: value.claimed_by,
            approved_by: value.approved_by,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}
impl From<ExpenseClaimList> for ExpenseClaimItems {
    fn from(value: ExpenseClaimList) -> Self {
        Self {
            expense_claims: value
                .expense_claims
                .into_iter()
                .map(ExpenseClaimItem::from)
                .collect(),
            next_cursor: value.next_cursor,
        }
    }
}
impl From<PayrollRunView> for PayrollRunItem {
    fn from(value: PayrollRunView) -> Self {
        Self {
            payroll_run_id: value.payroll_run_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            period_code: value.period_code,
            starts_on: value.starts_on.to_string(),
            ends_on: value.ends_on.to_string(),
            currency: value.currency.as_str().into(),
            lines: value
                .lines
                .into_iter()
                .map(|line| PayrollLineItem {
                    employee_id: line.employee_id.as_uuid().to_string(),
                    gross_minor: line.gross_minor,
                    deductions_minor: line.deductions_minor,
                    net_minor: line.net_minor,
                })
                .collect(),
            status: payroll_status(value.status).into(),
            prepared_by: value.prepared_by,
            reviewed_by: value.reviewed_by,
            approved_by: value.approved_by,
            total_gross_minor: value.total_gross_minor,
            total_deductions_minor: value.total_deductions_minor,
            total_net_minor: value.total_net_minor,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}
impl From<PayrollRunList> for PayrollRunItems {
    fn from(value: PayrollRunList) -> Self {
        Self {
            payroll_runs: value
                .payroll_runs
                .into_iter()
                .map(PayrollRunItem::from)
                .collect(),
            next_cursor: value.next_cursor,
        }
    }
}

const fn employee_status(value: EmployeeStatus) -> &'static str {
    match value {
        EmployeeStatus::Draft => "draft",
        EmployeeStatus::Active => "active",
        EmployeeStatus::OnLeave => "on_leave",
        EmployeeStatus::Suspended => "suspended",
        EmployeeStatus::Terminated => "terminated",
    }
}
const fn leave_status(value: LeaveStatus) -> &'static str {
    match value {
        LeaveStatus::Draft => "draft",
        LeaveStatus::Submitted => "submitted",
        LeaveStatus::Approved => "approved",
        LeaveStatus::Rejected => "rejected",
        LeaveStatus::Cancelled => "cancelled",
        LeaveStatus::Taken => "taken",
    }
}
const fn expense_status(value: ExpenseStatus) -> &'static str {
    match value {
        ExpenseStatus::Draft => "draft",
        ExpenseStatus::Submitted => "submitted",
        ExpenseStatus::Approved => "approved",
        ExpenseStatus::Rejected => "rejected",
        ExpenseStatus::Reimbursed => "reimbursed",
    }
}
const fn payroll_status(value: PayrollStatus) -> &'static str {
    match value {
        PayrollStatus::Draft => "draft",
        PayrollStatus::Review => "review",
        PayrollStatus::Approved => "approved",
        PayrollStatus::Posted => "posted",
        PayrollStatus::Cancelled => "cancelled",
    }
}

/// Generated protobuf and platform-channel details stop behind this boundary.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait PeoplePayrollProtoTransport: Send + Sync {
    async fn command(
        &self,
        envelope: CommandEnvelope<PeoplePayrollCommand>,
    ) -> Result<CommandReceipt<PeoplePayrollMutationResult>, UiError>;
    async fn query(
        &self,
        envelope: QueryEnvelope<PeoplePayrollQuery>,
    ) -> Result<QueryResult<PeoplePayrollQueryResponse>, UiError>;
}

#[derive(Debug, Clone)]
pub struct PeoplePayrollGateway<T> {
    transport: T,
}
impl<T> PeoplePayrollGateway<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }
    pub const fn transport(&self) -> &T {
        &self.transport
    }
    pub fn into_transport(self) -> T {
        self.transport
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: PeoplePayrollProtoTransport> CommandGateway<PeoplePayrollCommand>
    for PeoplePayrollGateway<T>
{
    type Response = PeoplePayrollMutationResult;
    async fn execute(
        &self,
        envelope: CommandEnvelope<PeoplePayrollCommand>,
    ) -> Result<CommandReceipt<Self::Response>, UiError> {
        self.transport.command(envelope).await
    }
}
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: PeoplePayrollProtoTransport> QueryGateway<PeoplePayrollQuery> for PeoplePayrollGateway<T> {
    type Response = PeoplePayrollQueryResponse;
    async fn execute(
        &self,
        envelope: QueryEnvelope<PeoplePayrollQuery>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.query(envelope).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake;
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl PeoplePayrollProtoTransport for Fake {
        async fn command(
            &self,
            _: CommandEnvelope<PeoplePayrollCommand>,
        ) -> Result<CommandReceipt<PeoplePayrollMutationResult>, UiError> {
            unreachable!()
        }
        async fn query(
            &self,
            _: QueryEnvelope<PeoplePayrollQuery>,
        ) -> Result<QueryResult<PeoplePayrollQueryResponse>, UiError> {
            unreachable!()
        }
    }
    #[test]
    fn gateway_is_typed_for_both_directions() {
        fn command<
            G: CommandGateway<PeoplePayrollCommand, Response = PeoplePayrollMutationResult>,
        >() {
        }
        fn query<G: QueryGateway<PeoplePayrollQuery, Response = PeoplePayrollQueryResponse>>() {}
        command::<PeoplePayrollGateway<Fake>>();
        query::<PeoplePayrollGateway<Fake>>();
    }
    #[test]
    fn gateway_is_the_only_core_to_ui_mapping_boundary() {
        let source = include_str!("gateway.rs");
        assert!(source.contains(concat!("impl From<Employee", "View> for EmployeeItem")));
        assert!(!source.contains(concat!("erpx_", "proto")));
        assert!(!source.contains(concat!("tonic", "::")));
        for source in [
            include_str!("models.rs"),
            include_str!("presenter.rs"),
            include_str!("view.rs"),
        ] {
            assert!(!source.contains(concat!("erpx_people", "_payroll")));
            assert!(!source.contains(concat!("erpx_", "proto")));
        }
    }
}
