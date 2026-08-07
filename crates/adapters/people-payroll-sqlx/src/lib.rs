#![doc = "PostgreSQL command, read, and projection adapters for People & Payroll."]
#![forbid(unsafe_code)]

mod command;
mod projection;
mod read;

pub use command::{PeoplePayrollCommandAdapter, PeoplePayrollWriteRepository};
pub use projection::{
    EmployeeProjector, ExpenseClaimProjector, LeaveRequestProjector, PayrollRunProjector,
    PeoplePayrollOutboxPayload, PeoplePayrollSnapshot,
};
pub use read::{PeoplePayrollReadRepository, decode_cursor, encode_cursor};

pub const CONTEXT_NAME: &str = "people_payroll";
pub const EMPLOYEE_AGGREGATE_TYPE: &str = "employee";
pub const LEAVE_AGGREGATE_TYPE: &str = "leave_request";
pub const EXPENSE_AGGREGATE_TYPE: &str = "expense_claim";
pub const PAYROLL_AGGREGATE_TYPE: &str = "payroll_run";
pub const EMPLOYEE_PROJECTION: &str = "people_payroll.employees";
pub const LEAVE_PROJECTION: &str = "people_payroll.leave_requests";
pub const EXPENSE_PROJECTION: &str = "people_payroll.expense_claims";
pub const PAYROLL_PROJECTION: &str = "people_payroll.payroll_runs";
pub const EMPLOYEE_SNAPSHOT_EVENT: &str = "people_payroll.employee_snapshot";
pub const LEAVE_SNAPSHOT_EVENT: &str = "people_payroll.leave_request_snapshot";
pub const EXPENSE_SNAPSHOT_EVENT: &str = "people_payroll.expense_claim_snapshot";
pub const PAYROLL_SNAPSHOT_EVENT: &str = "people_payroll.payroll_run_snapshot";
pub const EVENT_VERSION: u32 = 1;
#[cfg(test)]
const WRITE_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0061_wave7_transactional_context_guards.sql"
);
#[cfg(test)]
const READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0053_wave7_transactional_context_projections.sql"
);

pub fn employee_partition(id: erpx_people_payroll::EmployeeId) -> String {
    format!("people_payroll.employee:{}", id.as_uuid().simple())
}
pub fn leave_partition(id: erpx_people_payroll::LeaveRequestId) -> String {
    format!("people_payroll.leave_request:{}", id.as_uuid().simple())
}
pub fn expense_partition(id: erpx_people_payroll::ExpenseClaimId) -> String {
    format!("people_payroll.expense_claim:{}", id.as_uuid().simple())
}
pub fn payroll_partition(id: erpx_people_payroll::PayrollRunId) -> String {
    format!("people_payroll.payroll_run:{}", id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn partitions_and_frozen_schema_are_stable() {
        let employee = erpx_people_payroll::EmployeeId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let payroll = erpx_people_payroll::PayrollRunId::try_from_uuid(Uuid::from_u128(2)).unwrap();
        assert!(employee_partition(employee).starts_with("people_payroll.employee:"));
        assert!(payroll_partition(payroll).starts_with("people_payroll.payroll_run:"));
        assert!(WRITE_DDL.contains("payroll_status <> 'approved'"));
        assert!(READ_DDL.contains(PAYROLL_PROJECTION));
    }

    #[test]
    fn reads_have_no_write_snapshot_fallback() {
        let source = include_str!("read.rs");
        assert!(!source.contains(concat!("Write", "Pool")));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
    }
}
