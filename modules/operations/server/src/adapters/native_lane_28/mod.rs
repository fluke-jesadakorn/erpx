//! Native migration lane 28 for operations.
//!
//! The owner-local staff-level acceptance facts read family is served by an
//! axum router backed by sqlx. TypeScript consumers become strict transports;
//! the retired `SELECT` literals are deleted from the owner adapter files.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{StaffLevelHandler, StaffLevelRead, StaffLevelReadFuture};
pub use domain::{
    ACCEPTANCE_COUNT_SQL, ACCEPTANCE_COUNT_SQL_SHA256, ACTIVE_RANKS_SQL, ACTIVE_RANKS_SQL_SHA256,
    LEVEL_HISTORY_SQL, LEVEL_HISTORY_SQL_SHA256, LevelHistoryRow, MAX_COMPANY_BYTES, StaffLevel,
    StaffLevelFacts, sql_oracle, validate_company, validate_engagement,
};
pub use http::{Lane28Target, router, router_for_holdings, router_with_reader};
pub use postgres::PgStaffLevelRead;
