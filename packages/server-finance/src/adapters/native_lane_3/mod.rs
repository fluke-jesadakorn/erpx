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

pub use application::{current_invoice_lines, current_invoice_lines_arc, InvoiceLineRead, InvoiceLineReadFuture};
pub use domain::{
    invalid, result_oracle, sql_oracle, unavailable, valid_id, InvoiceLine, InvoiceLineRequest,
    InvoiceLinesData, InvoiceLinesResponse, INVOICE_LINES_SQL,
};
pub use http::{router, router_for_holdings, router_with_reader, LaneTarget};
pub use postgres::{PgInvoiceLineRead, BIND_ORDER};
