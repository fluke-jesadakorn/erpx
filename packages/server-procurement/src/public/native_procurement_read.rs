//! Public surface for the native Procurement caller-unit read-plan registry.
//! The command-plan router lives in `native_command_http`; this facade exposes
//! the read kinds and resolver so composition can mount or verify the plan
//! route without importing adapter internals.
pub use crate::adapters::native_read_sql::{
    is_read, read_statement, NativeReadStatement, RD_ACTIVE_QUOTATION_COUNT,
    RD_BILL_MATCH_RECEIPTS, RD_BILL_MATCH_VARIANCE, RD_DOCUMENT_POSTING_MATCHED_BILL,
    RD_INVOICE_MATCH_FOR_FINANCE, RD_OPERATIONAL_ATTACHMENT_BILL_MATCH,
    RD_REQUEST_ALLOCATIONS, RD_REQUEST_ORDER_LINK, RD_REQUEST_QUOTATION,
};
