//! Native migration lane 14 for commerce.
//!
//! Owner-local Commerce workflow order reads: the axum router owns transport,
//! Company isolation and response shape; `postgres.rs` owns the SQL and its
//! execution; TypeScript is a thin, fail-closed transport client.

pub(crate) mod application;
pub(crate) mod domain;
pub(crate) mod http;
pub(crate) mod postgres;

pub use application::{CommerceLane14Read, LaneReadFuture};
pub use domain::{COLUMNS, Cell, Column, ColumnKind, LaneKind, valid_id};
pub use http::{LaneTarget, router, router_with_reader};
pub use postgres::{
    SQL_DOCUMENT_PAGE_TS, SQL_DOCUMENT_TS, SQL_REFERENCE_ROWS_TS, render, statement_for,
};
