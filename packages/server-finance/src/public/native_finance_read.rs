//! Public surface for the native Finance caller-unit read-plan registry.
//! The command-plan router lives in `finance_owner_write_http`; this facade
//! exposes the read kinds and resolver so composition can mount or verify the
//! plan route without importing adapter internals.
pub use crate::adapters::native_finance_read_sql::{
    NativeReadStatement, RD_ALL_PERIODS, RD_ASSET_POLICY_ROW, RD_ASSET_POLICY_RUNS,
    RD_BUSINESS_BILL_ROW, RD_COLLECTION_CASE_ROW, RD_INSTRUMENT_EDITION, RD_INSTRUMENT_RUNS,
    RD_PERIOD_END_DATES,
    RD_SUPPLIER_BATCH_ACCOUNTS, RD_SUPPLIER_BATCH_BANK, RD_SUPPLIER_BATCH_INVOICE,
    RD_SUPPLIER_BATCH_INVOICES, RD_SUPPLIER_BATCH_ITEMS, RD_SUPPLIER_BATCH_LIST,
    RD_SUPPLIER_BATCH_PERIODS, RD_SUPPLIER_BATCH_ROW, RD_SUPPLIER_BATCH_WORK_LIST,
    RD_SUPPLIER_INSTRUCTION_ROW, RD_TAX_CHART, RD_TAX_CLOSE_PROFILE, RD_TAX_CORRECTION,
    RD_TAX_CURRENT_ENTRY, RD_TAX_ENTRY, RD_TAX_ORIGINAL_ENTRY, RD_TAX_PACKAGE,
    RD_TAX_PENDING_CORRECTION_COUNT, RD_TAX_PENDING_FAMILY, RD_TAX_PREVIOUS_ENTRY,
    RD_TAX_PROFILE, RD_TAX_PROFILES, is_read, read_statement,
};
