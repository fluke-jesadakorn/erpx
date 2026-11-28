//! Native Legal package-obligation read contract. The caller executes the
//! returned statement on its own open unit; the owner keeps the SQL and binds.
pub use crate::adapters::native_read_plan::{
    LegalReadStatement, PACKAGE_OBLIGATION_PREFIX, PACKAGE_OBLIGATION_SOURCES_JSON,
    RD_PACKAGE_OBLIGATIONS, package_obligation_plan_sql, plan_legal_read,
};
