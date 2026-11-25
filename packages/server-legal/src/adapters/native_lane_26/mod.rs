//! Native migration lane 26 for legal.
//!
//! Owner-local executed-copy reads: the axum router owns the target/Company
//! isolation, the application port owns the query boundary and projection, and
//! the Postgres adapter owns all five SQL statements. TypeScript keeps only a
//! strict transport client and fails closed when this service is unavailable.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{ExecutedCopySource, LaneFuture, read_executed_copies};
pub use domain::{
    ApprovalCycle, COPY_LIMIT, CopyAuthors, CopyHead, ExecutedCopyRead, ExecutedCopyRow,
    LaneTarget, ReviewRow, project, valid_id, valid_slug,
};
pub use http::router;
pub use postgres::{
    PgExecutedCopySource, SQL_APPROVAL, SQL_AUTHORS, SQL_LIST, SQL_MEMBER, SQL_REVIEWS,
};
