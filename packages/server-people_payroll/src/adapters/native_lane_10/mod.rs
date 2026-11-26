//! Native migration lane 10 for people_payroll.
//!
//! Owner-local migration of the live `activeLeavePolicySettings` read: the axum
//! router executes the Company-scoped statement with sqlx, validates and shapes
//! the response in Rust, and a strict TS transport replaces the deleted TS SQL.
//! No TypeScript fallback remains in this slice.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{LeavePolicyFuture, LeavePolicyRead, project, validate};
pub use domain::{ActiveLeavePolicies, LeavePolicySetting, valid_id, valid_slug};
pub use http::{
    router, router_for_holdings, router_with_reader, router_with_readers,
};
pub use postgres::{PgLeavePolicyRead, SQL_ACTIVE_LEAVE_POLICIES};
