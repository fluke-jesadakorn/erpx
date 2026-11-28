//! Native migration lane 46 for identity_access.
//!
//! The owner now serves the directory "connection by id" read that previously
//! ran as inline TypeScript SQL in `adapters/directory-lookup.ts`
//! (`iamConnectionByIdSql('company_id=? AND id=?')`, via
//! `SELECT * FROM iam_connections`). The axum router owns the Holding scope and
//! response envelope; the sqlx adapter executes the real Control/Holding
//! statement and projects every column of the retired `SELECT *` row. There is
//! no TypeScript SQL fallback: the retired adapter is a strict transport.
//!
//! Composition and route deletion are lead-owned; see the handoff packet.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{ConnectionRead, ConnectionReadFuture, ConnectionReadRequest, execute};
pub use domain::{
    ConnectionFields, ConnectionRow, invalid, invalid_result, not_found, unavailable, valid_id,
    valid_token,
};
pub use http::{
    LaneTarget, READ_PATH, router, router_for_holdings, router_with_readers, router_with_reader,
};
pub use postgres::{
    PgConnectionRead, SQL_CONNECTION_BY_ID, TS_ORACLE_SQL, connection_by_id_sql,
};
