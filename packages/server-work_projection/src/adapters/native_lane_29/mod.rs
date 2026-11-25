//! Native migration lane 29 (work_projection).
//!
//! Owner-local calendar availability/occurrence reads are executed by axum and
//! sqlx here. The TypeScript adapters keep only a strict private transport.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{Lane29Future, Lane29Read, Lane29Service};
pub use domain::{
    AvailabilitySource, MovedOccurrence, OccurrenceException, ReadKind, ReadRequest, SeriesRow,
    valid_id,
};
pub use http::{LaneTarget, router, router_for_holdings};
pub use postgres::{
    PgLane29Read, SQL_AVAILABILITY_HEAD, SQL_MOVED_OCCURRENCES, SQL_OCCURRENCE_EXCEPTION,
    SQL_SERIES_BATCH, sql_availability_sources,
};
