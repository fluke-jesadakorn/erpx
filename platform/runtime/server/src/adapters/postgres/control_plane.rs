//! Owner-local native read port for the runtime control plane.
//!
//! Mirrors the retained TypeScript authority in
//! `adapters/runtime/postgres.ts` and `adapters/runtime/migrations.ts`:
//! migration receipt/history reads, runtime health (session settings and
//! catalog presence) and the advisory lock/unlock wrappers. This is a read
//! port only; it is not wired into the composition crate and no TypeScript
//! consumer is switched or deleted by this module.

use super::PersistenceError;
use serde::Serialize;
use sqlx::{PgConnection, query_scalar};

/// Session-scoped migration lock key retained from `runtime/migrations.ts`.
pub const MIGRATION_LOCK_KEY: i64 = 714_928_001;

/// Verbatim PostgreSQL text of the retained TypeScript statements.
///
/// `postgres.ts` numbers positional parameters with `?` → `$n`; the constants
/// below are already in the executed `$n` form. The PG parity test copies the
/// TypeScript text independently and requires both SQL and results to agree.
pub mod sql {
    pub const MIGRATION_RECEIPTS: &str = "SELECT name FROM erpx_migration_receipts() ORDER BY name";
    pub const RECEIPT_TABLE_PRESENT: &str =
        "SELECT to_regclass('public.erpx_migrations') IS NOT NULL present";
    pub const RECEIPT_CHECKSUM: &str = "SELECT checksum FROM erpx_migrations WHERE name=$1";
    pub const COVERAGE_PROCEDURE_PRESENT: &str =
        "SELECT to_regprocedure('ensure_work_cache_coverage()') IS NOT NULL present";
    pub const RUNTIME_HEALTH: &str = "SELECT current_setting('lock_timeout') AS value";
    pub const LOCK_TIMEOUT_SET: &str = "SELECT set_config('lock_timeout','10s',false)";
    pub const LOCK_TIMEOUT_RESTORE: &str = "SELECT set_config('lock_timeout',$1,false)";
    pub const MIGRATION_LOCK: &str = "SELECT pg_advisory_lock(714928001)";
    pub const MIGRATION_UNLOCK: &str = "SELECT pg_advisory_unlock(714928001)";
    pub const OBJECT_LOCK: &str = "SELECT pg_advisory_lock(hashtextextended($1,0))";
    pub const OBJECT_UNLOCK: &str = "SELECT pg_advisory_unlock(hashtextextended($1,0)) AS released";
    pub const OBJECT_XACT_LOCK: &str = "SELECT pg_advisory_xact_lock(hashtextextended($1,0))";
    pub const PAIR_LOCK: &str = "SELECT pg_advisory_lock(hashtext($1),hashtext($2))";
    pub const PAIR_UNLOCK: &str = "SELECT pg_advisory_unlock(hashtext($1),hashtext($2)) released";
}

/// Live runtime session health projected by the retained `withObjectLocks` read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeHealth {
    pub lock_timeout: String,
}

/// Read applied receipt names. A missing narrow reader is not proof that no
/// migration was applied; the catalogue check preserves that failure mode.
pub async fn migration_receipt_names(
    conn: &mut PgConnection,
) -> Result<Vec<String>, PersistenceError> {
    match query_scalar::<_, String>(sql::MIGRATION_RECEIPTS)
        .fetch_all(&mut *conn)
        .await
    {
        Ok(names) => Ok(names),
        Err(error) => {
            let error = PersistenceError::from(error);
            if !matches!(error.sql_state(), Some("42P01" | "42883")) {
                return Err(error);
            }
            if receipt_table_present(&mut *conn).await? {
                return Err(PersistenceError::new("MIGRATION_RECEIPTS_UNAVAILABLE"));
            }
            Ok(Vec::new())
        }
    }
}

pub async fn receipt_table_present(conn: &mut PgConnection) -> Result<bool, PersistenceError> {
    Ok(query_scalar::<_, bool>(sql::RECEIPT_TABLE_PRESENT)
        .fetch_one(&mut *conn)
        .await?)
}

pub async fn migration_receipt_checksum(
    conn: &mut PgConnection,
    name: &str,
) -> Result<Option<String>, PersistenceError> {
    Ok(query_scalar::<_, String>(sql::RECEIPT_CHECKSUM)
        .bind(name)
        .fetch_optional(&mut *conn)
        .await?)
}

pub async fn coverage_procedure_present(conn: &mut PgConnection) -> Result<bool, PersistenceError> {
    Ok(query_scalar::<_, bool>(sql::COVERAGE_PROCEDURE_PRESENT)
        .fetch_one(&mut *conn)
        .await?)
}

pub async fn runtime_health(conn: &mut PgConnection) -> Result<RuntimeHealth, PersistenceError> {
    Ok(RuntimeHealth {
        lock_timeout: query_scalar::<_, String>(sql::RUNTIME_HEALTH)
            .fetch_one(&mut *conn)
            .await?,
    })
}

pub async fn migration_advisory_lock(conn: &mut PgConnection) -> Result<(), PersistenceError> {
    sqlx::query(sql::MIGRATION_LOCK).execute(&mut *conn).await?;
    Ok(())
}

pub async fn migration_advisory_unlock(conn: &mut PgConnection) -> Result<bool, PersistenceError> {
    Ok(query_scalar::<_, bool>(sql::MIGRATION_UNLOCK)
        .fetch_one(&mut *conn)
        .await?)
}

pub async fn object_advisory_lock(
    conn: &mut PgConnection,
    key: &str,
) -> Result<(), PersistenceError> {
    sqlx::query(sql::OBJECT_LOCK)
        .bind(key)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn object_advisory_unlock(
    conn: &mut PgConnection,
    key: &str,
) -> Result<bool, PersistenceError> {
    Ok(query_scalar::<_, bool>(sql::OBJECT_UNLOCK)
        .bind(key)
        .fetch_one(&mut *conn)
        .await?)
}

pub async fn object_advisory_xact_lock(
    conn: &mut PgConnection,
    key: &str,
) -> Result<(), PersistenceError> {
    sqlx::query(sql::OBJECT_XACT_LOCK)
        .bind(key)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn pair_advisory_lock(
    conn: &mut PgConnection,
    namespace: &str,
    key: &str,
) -> Result<(), PersistenceError> {
    sqlx::query(sql::PAIR_LOCK)
        .bind(namespace)
        .bind(key)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn pair_advisory_unlock(
    conn: &mut PgConnection,
    namespace: &str,
    key: &str,
) -> Result<bool, PersistenceError> {
    Ok(query_scalar::<_, bool>(sql::PAIR_UNLOCK)
        .bind(namespace)
        .bind(key)
        .fetch_one(&mut *conn)
        .await?)
}
