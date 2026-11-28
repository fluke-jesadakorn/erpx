//! Native migration lane 2 for finance: owner-local overview counts and invoice reads.
//!
//! The TypeScript adapter is a thin fail-closed client; the SQL, projection and
//! Company isolation live here. Composition mounts `router` on the private
//! native API and supplies the registered Holding targets.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{
    AgingFuture, CompanyToolsFuture, CompanyToolsQuery, InvoicePageFuture, InvoiceReadQuery,
    OverviewCountsFuture, OverviewCountsQuery, load, load_aging, load_page, load_tools,
};
pub use domain::{
    AgingRequest, CompanyTool, InvoicePageRequest, InvoicePageWire, OverviewCounts,
    validate_aging_request, validate_page_request,
};
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::{
    COMPANY_TOOLS_SQL, OVERVIEW_COUNTS_SQL, PgCompanyToolsQuery, PgInvoiceReadQuery,
    PgOverviewCountsQuery,
};
