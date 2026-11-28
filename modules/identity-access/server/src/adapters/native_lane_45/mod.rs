//! Native execution of the IAM "sessions by user" read.
//!
//! The owner crate owns the actor bound, the request validation, the exact
//! `SessionRow` projection and the axum envelope. The sqlx adapter executes the
//! real Control `session` statement. Composition mounts one Control-backed
//! reader per registered Holding; there is no TypeScript SQL, no plan fragment
//! and no fallback.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{SessionListFuture, SessionListRead, SessionListRequest, execute};
pub use domain::{
    KIND_SESSION_LIST, SessionFields, SessionRow, invalid, invalid_result, unavailable, valid_id,
    valid_timestamp,
};
pub use http::{
    LaneTarget, READ_PATH, router, router_for_holdings, router_with_reader, router_with_readers,
};
pub use postgres::{PgSessionListRead, SQL_SESSION_LIST, TS_ORACLE_SQL};
