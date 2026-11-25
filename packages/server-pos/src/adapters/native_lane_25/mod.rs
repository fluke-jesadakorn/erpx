//! Native migration lane 25 for pos: executive transaction-evidence search.
//!
//! The Rust owner executes the SQL via sqlx, owns the projection and validates
//! the Company scope. TypeScript is a strict transport that fails closed.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{EvidenceFuture, PosEvidenceReader};
pub use domain::{
    EvidenceCursor, EvidenceQuery, EvidenceRow, METRIC_NET_POS_SALES, METRIC_POS_PAYMENT_MIX,
    METRIC_POS_SALES, METRICS, ROW_KEYS,
};
pub use http::{
    LaneTarget, router, router_for_holdings, router_with_reader, router_with_readers,
};
pub use postgres::{PgEvidenceReader, render};
