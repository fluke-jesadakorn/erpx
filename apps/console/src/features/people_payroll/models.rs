//! Target-neutral People & Payroll UI models.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmployeeItem {
    pub employee_id: String,
    pub version: u64,
    pub scope: String,
    pub employee_code: String,
    pub display_name: String,
    pub hired_on: String,
    pub terminated_on: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmployeeItems {
    pub employees: Vec<EmployeeItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaveRequestItem {
    pub leave_request_id: String,
    pub version: u64,
    pub scope: String,
    pub employee_id: String,
    pub starts_on: String,
    pub ends_on: String,
    pub reason: String,
    pub status: String,
    pub requested_by: String,
    pub approved_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaveRequestItems {
    pub leave_requests: Vec<LeaveRequestItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpenseClaimItem {
    pub expense_claim_id: String,
    pub version: u64,
    pub scope: String,
    pub employee_id: String,
    pub currency: String,
    pub amount_minor: i64,
    pub reason: String,
    pub status: String,
    pub claimed_by: String,
    pub approved_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpenseClaimItems {
    pub expense_claims: Vec<ExpenseClaimItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayrollLineItem {
    pub employee_id: String,
    pub gross_minor: i64,
    pub deductions_minor: i64,
    pub net_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayrollRunItem {
    pub payroll_run_id: String,
    pub version: u64,
    pub scope: String,
    pub period_code: String,
    pub starts_on: String,
    pub ends_on: String,
    pub currency: String,
    pub lines: Vec<PayrollLineItem>,
    pub status: String,
    pub prepared_by: String,
    pub reviewed_by: Option<String>,
    pub approved_by: Option<String>,
    pub total_gross_minor: i64,
    pub total_deductions_minor: i64,
    pub total_net_minor: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayrollRunItems {
    pub payroll_runs: Vec<PayrollRunItem>,
    pub next_cursor: Option<String>,
}
