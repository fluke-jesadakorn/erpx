//! Native migration lane 10 for identity_access.
//!
//! Owner-local account invitation reads: the axum router owns the Company
//! scope and response shape, the sqlx reader owns the three statements retired
//! from `adapters/account-access.ts`, and the TypeScript adapter is a strict
//! fail-closed transport with no SQL fallback.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{AccountAccessRead, AccountAccessReadFuture, AccountAccessReadRequest};
pub use domain::{AccountAccessReadKind, AccountAccessReadResult, Invitation};
pub use http::{
    LaneTarget, READ_PATH, router, router_for_holdings, router_with_reader, router_with_readers,
};
pub use postgres::{PgAccountAccessRead, SQL_EMAIL_EXISTS, SQL_INVITATION_BY_TOKEN, SQL_INVITATION_STATUS, TS_ORACLE_SQL};
