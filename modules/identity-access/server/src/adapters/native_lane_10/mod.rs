//! Native migration lane 10 for identity_access.
//!
//! Owner-local Control reads: the axum router owns the Company scope and
//! response shape, the sqlx reader owns the three statements retired from
//! `adapters/account-access.ts` and the two Control account-recovery statements
//! retired from `adapters/account-recovery.ts`. The TypeScript adapter is a
//! strict fail-closed transport with no SQL fallback.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{AccountAccessRead, AccountAccessReadFuture, AccountAccessReadRequest};
pub use domain::{AccountAccessReadKind, AccountAccessReadResult, Invitation};
pub use http::{
    LaneTarget, READ_PATH, router, router_for_holdings, router_with_reader, router_with_readers,
};
pub use postgres::{
    PgAccountAccessRead, SQL_EMAIL_EXISTS, SQL_INVITATION_BY_TOKEN, SQL_INVITATION_STATUS,
    SQL_RECOVERY_ACCOUNT_EMAIL, SQL_RECOVERY_LATEST_JOB, TS_ORACLE_RECOVERY_SQL, TS_ORACLE_SQL,
};
