//! Native migration lane 7 for finance: the invoice list consumed by the
//! feature-finance API route and the shared finance catch-all.
//!
//! The owner executes the single Finance-owned statement and returns every
//! column the retired TypeScript read returned. The TypeScript side is a
//! strict fail-closed transport with no SQL, no fallback and no payload
//! shaping; the inert plan-kind bridge is deleted with this slice.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{InvoiceListFuture, InvoiceListRead, handle};
pub use domain::{
    PG_INVOICE_LIST_SQL, TS_INVOICE_LIST_SQL, bound_limit, invoice_list_wire, valid_company_id,
    valid_host, valid_slug,
};
pub use http::{LaneTarget, router_for_holdings, router_with_reader};
pub use postgres::PgInvoiceListRead;
