//! Native migration lane 2 for finance: owner-local overview company counts.
//!
//! The TypeScript adapter is a thin fail-closed client; the SQL, projection and
//! Company isolation live here. Composition mounts `router` on the private
//! native API and supplies the registered Holding targets.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{OverviewCountsFuture, OverviewCountsQuery};
pub use domain::OverviewCounts;
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::{OVERVIEW_COUNTS_SQL, PgOverviewCountsQuery};
