//! Native migration lane 6 for finance: the chart reference list consumed by
//! the feature-finance API route and the shared finance catch-all.
//!
//! The owner executes the single Finance-owned statement and returns every
//! `finance_charts` column. The TypeScript side is a strict fail-closed
//! transport with no SQL, no fallback and no payload shaping.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{ChartListFuture, ChartListRead, handle};
pub use domain::{
    PG_CHART_LIST_SQL, TS_CHART_LIST_SQL, chart_list_wire, valid_company_id, valid_host, valid_slug,
};
pub use http::{LaneTarget, router_for_holdings, router_with_reader};
pub use postgres::PgChartListRead;
