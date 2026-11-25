//! Native migration lane 32 for Workflow (failed inbox metrics).
//!
//! The owner executes the dead-letter count with sqlx behind an owner-local
//! axum router. The retained TS adapter is a thin multi-Holding client with no
//! SQL and no fallback.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{FailedMetricsRead, FailedMetricsReadFuture, ReadFailedMetrics};
pub use domain::{FailedMetrics, LaneTarget};
pub use http::{router, router_for_holdings, router_with_reader};
pub use postgres::{EXECUTED_SQL, PgFailedMetricsRead, SOURCE_SQL};
