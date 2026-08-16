#![doc = "PostgreSQL command, read, and projection adapters for People & Payroll."]
#![forbid(unsafe_code)]

mod command;
mod projection;
mod read;
mod time_record;

pub use command::{PeoplePayrollCommandAdapter, PeoplePayrollWriteRepository};
pub use projection::{
    EmployeeProjector, ExpenseClaimProjector, LeaveRequestProjector, PayrollRunProjector,
    PeoplePayrollOutboxPayload, PeoplePayrollSnapshot,
};
pub use read::{PeoplePayrollReadRepository, decode_cursor, encode_cursor};
pub use time_record::{TimeRecordProjector, TimeRecordReadAdapter, TimeRecordSqlxAdapter};

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
#[cfg(test)]
const COMPLETION_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0080_current_record_completion.sql");
#[cfg(test)]
const COMPLETION_READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0071_current_record_completion_projection.sql"
);
#[cfg(test)]
const NORMALIZED_AUTHORITY_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0081_current_normalized_authority.sql"
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

    #[test]
    fn completion_records_are_normalized_tenant_bound_and_rebuildable() {
        for table in ["employment_records", "payslip_records"] {
            assert!(COMPLETION_WRITE_DDL.contains(table));
        }
        for table in ["employment_projection", "payslip_projection"] {
            assert!(COMPLETION_READ_DDL.contains(table));
        }
        assert!(COMPLETION_WRITE_DDL.contains("FORCE ROW LEVEL SECURITY"));
        assert!(COMPLETION_READ_DDL.contains("FORCE ROW LEVEL SECURITY"));
        assert!(COMPLETION_READ_DDL.contains("(tenant_id,projection_name,generation)"));
        assert!(!COMPLETION_WRITE_DDL.contains("employment_snapshot"));
        assert!(!COMPLETION_WRITE_DDL.contains("payslip_snapshot"));
    }

    #[test]
    fn command_authority_is_typed_relational_and_snapshot_free() {
        let source = include_str!("command.rs");
        for table in [
            "employee_authority",
            "leave_request_authority",
            "expense_claim_authority",
            "payroll_run_authority",
            "payroll_line_authority",
            "payroll_calculation_evidence_authority",
            "payroll_input_evidence_authority",
        ] {
            assert!(source.contains(table));
            assert!(NORMALIZED_AUTHORITY_DDL.contains(table));
        }
        assert!(source.contains("commit_normalized"));
        assert!(source.contains("authoritative_version"));
        assert!(!source.contains("load_snapshot"));
        assert!(!source.contains("commit_guarded"));
        assert!(!NORMALIZED_AUTHORITY_DDL.contains("aggregate_components"));
    }
}
