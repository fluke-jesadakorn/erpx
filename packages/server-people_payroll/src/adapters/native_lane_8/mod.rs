//! Native migration lane 8 for people_payroll.
//!
//! Owner-local app-entrypoint reads: the axum router owns isolation and error
//! mapping, the application port owns dispatch, and PostgreSQL execution owns
//! the queries. No TypeScript fallback SQL remains for this family.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{AppEntrypointRead, LaneReadFuture};
pub use domain::{ApproverName, PayGroup, PersonOption, ReadKind, RosterShift};
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::{PgAppEntrypointRead, oracle, rendered_sql};
