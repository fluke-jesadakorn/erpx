//! Public surface for the native Finance caller-unit read-plan registry.
//! The command-plan router lives in `finance_owner_write_http`; this facade
//! exposes the read kinds and resolver so composition can mount or verify the
//! plan route without importing adapter internals.
pub use crate::adapters::native_finance_read_sql::{
    NativeReadStatement, RD_ASSET_POLICY_ROW, RD_ASSET_POLICY_RUNS, RD_BUSINESS_BILL_ROW,
    RD_COLLECTION_CASE_ROW, is_read, read_statement,
};
