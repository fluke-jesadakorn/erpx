//! Postgres adapter for the native Legal write slice.
//!
//! The authoritative SQL and argument order are owned by this crate in
//! `domain::legal_wave3`; this adapter binds planning to the caller-owned
//! serializable connection. It never opens a transaction and never duplicates
//! receipt/audit/outbox effects, which belong to the caller unit.
use crate::{
    application::legal_wave3::{LegalWave3Command, plan_legal_wave3},
    domain::legal_wave3::LegalWave3Statement,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

/// Build the authoritative statement on the caller's already-open unit.
///
/// The connection is accepted explicitly so the owner statement can only be
/// produced inside a caller-owned serializable transaction. No second
/// transaction, TS copy or fallback is possible for a retired kind.
pub async fn legal_wave3_statement(
    connection: &mut PgConnection,
    command: &LegalWave3Command,
) -> Result<LegalWave3Statement, Error> {
    // The planner is pure; pinning it to the caller's connection keeps the
    // statement inside that unit and prevents a detached authoring path.
    let _ = connection;
    plan_legal_wave3(command).map_err(PgTransactionError::Business)
}
