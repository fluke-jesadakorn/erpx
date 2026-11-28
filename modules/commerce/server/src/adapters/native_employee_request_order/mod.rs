//! Fully-native Commerce employee-request purchase-order scope read family.
//!
//! Owner-local successor of `readActOnEmployeeRequestOrder` in
//! `adapters/records-employee-requests-owned-facts.ts`. The axum router owns
//! Holding target isolation, Company scope and the response envelope;
//! `postgres.rs` executes the SQL; no TypeScript prepares or executes this read.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{CommerceEmployeeOrderRead, EmployeeOrderFuture, read_order};
pub use domain::{EmployeeOrder, invalid, project, valid_id};
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::{PgCommerceEmployeeOrderRead, SQL_EMPLOYEE_ORDER_TS, statement};
