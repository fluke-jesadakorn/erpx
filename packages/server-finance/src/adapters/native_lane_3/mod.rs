//! Native migration lane 3 for finance.
//!
//! Owner-local successor of the live invoice-line read in
//! `adapters/invoice-line-read.rs`. The axum router owns the SQL, the projection,
//! the company scope and the response shape; the retained TypeScript adapter is
//! a strict, fail-closed transport with no SQL fallback.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;
pub mod setup_counts;
pub mod starter_probes;

pub use application::{current_invoice_lines, current_invoice_lines_arc, InvoiceLineRead, InvoiceLineReadFuture};
pub use domain::{
    invalid, result_oracle, sql_oracle, unavailable, valid_id, InvoiceLine, InvoiceLineRequest,
    InvoiceLinesData, InvoiceLinesResponse, INVOICE_LINES_SQL,
};
pub use http::{router, router_for_holdings, router_with_reader, LaneTarget};
pub use postgres::{PgInvoiceLineRead, BIND_ORDER};
pub use setup_counts::{
    read_setup, result_oracle as setup_result_oracle, setup_router, setup_router_for_holdings,
    setup_router_with_reader, sql_oracles as setup_sql_oracles, PgSetupCountsRead, SetupCounts,
    SetupCountsRead, SetupData, SetupResponse, SQL_ACCOUNT_COUNT, SQL_BANK_COUNT,
    SQL_FIRST_CURRENCY, SQL_PERIOD_COUNT, SQL_TAX_COUNT,
};
pub use starter_probes::{
    read_probes, result_oracle as starter_result_oracle, sql_oracles as starter_sql_oracles,
    starter_router, starter_router_for_holdings, starter_router_with_reader, PgStarterProbeRead,
    ProbeRequest, ProbesData, ProbesResponse, StarterProbeRead, StarterProbes, SQL_EXISTING_CHART,
    SQL_OVERLAPPING_PERIOD,
};
