//! Native migration lane 36 for planning.
//!
//! Owner-local successor of the three scoped workspace list reads in
//! `adapters/production-command-sources-read.ts`. The axum router owns the SQL,
//! the projection and the Company/branch scope; the retained TS consumer only
//! transports bounded scope facts. There is no TypeScript SQL fallback.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{LaneRead, MAX_ROWS};
pub use domain::{
    CalendarListItem, ComparisonRow, KINDS, LaneData, LaneScope, RecommendationListItem,
    ResourceListItem, ScenarioRow, ScopedRecommendationRow, is_row_kind,
};
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::{PgLaneRead, sql_for};
