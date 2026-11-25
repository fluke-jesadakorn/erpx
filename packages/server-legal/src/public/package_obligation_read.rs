//! Native Legal package-obligation read plans. The caller executes the returned
//! statement on its own open unit; the owner keeps the SQL and the bind order.
pub use crate::adapters::native_read_plan::{
    LegalReadStatement, PACKAGE_OBLIGATION_PREFIX, RD_PACKAGE_OBLIGATIONS, plan_legal_read,
};
