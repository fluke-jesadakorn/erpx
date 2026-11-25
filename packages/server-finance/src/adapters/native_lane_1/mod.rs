//! Native migration lane 1 for Finance: expense posting projection reads.
//!
//! The TypeScript adapter no longer executes these two SELECTs. It transports
//! the complete native row arrays and fails closed when the owner route is
//! unavailable. Composition mounts `router` on the private native API.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{ExpenseProjectionReader, ExpenseProjectionService, LaneReadFuture};
pub use domain::{
    CompanyScope, ExpenseProjection, JournalLineRow, PostingAccountRow, ProjectionKind,
    valid_identifier,
};
pub use http::{LaneTarget, router};
pub use postgres::{
    PgExpenseProjectionReader, SQL_JOURNAL_LINES, SQL_POSTING_ACCOUNTS, SQLX_JOURNAL_LINES,
    SQLX_POSTING_ACCOUNTS, hash, render_sql, sql_for,
};
