//! Fully-native Commerce order return-line read family.
//!
//! Owner-local successor of the TypeScript `readCommerceReturnLines1` statement
//! in `adapters/commerce-order-read.ts`. The axum router owns Holding target
//! isolation, Company scope and the response envelope; `postgres.rs` executes
//! the SQL; no TypeScript prepares or executes this read.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{CommerceReturnLineRead, ReturnLineFuture, read_lines};
pub use domain::{ReturnLine, invalid, project, valid_id};
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::{PgCommerceReturnLineRead, SQL_RETURN_LINES_TS, statement};
