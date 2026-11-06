//! Same-unit Records employee-request purchase-source witness. No
//! authorization, purchase or storage access is implied.
pub use crate::adapters::employee_request_source::{
    PgEmployeeRequestSource, lock_employee_request_sources,
};
pub use crate::application::employee_request_source::{
    EmployeeRequestSourcePort, PreparedEmployeeRequestSource, prepare_employee_request_source,
    prepare_order_handoff_source, prepare_quotation_source,
};
pub use crate::domain::employee_request_source::{
    CONTRACT_VERSION, EmployeeRequestGuard, EmployeeRequestRow, EmployeeRequestScope,
    approval_decision, guard_for, require_approved_purchase, require_current,
    require_quotation_source, validate_id, validate_row,
};
