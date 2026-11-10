//! Read-only worker process/database attestation.
//!
//! This observation never grants actor or business authority. It verifies the
//! exact database identity, primary/recovery state and reviewed migration
//! inventory before a worker is admitted.
use super::PersistenceError;
use super::development_schema::verify_current_migration_inventory;
use super::migrations::{MigrationFile, MigrationTrack};
use super::pool::DatabasePool;
use sqlx::Row;

pub struct WorkerAttestationRequest<'a> {
    /// Exact durable database identity; `None` is only for an already-leased disposable database.
    pub expected_database: Option<&'a str>,
    pub track: MigrationTrack,
    pub files: &'a [MigrationFile],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerDatabaseAttestation {
    pub database: String,
    pub role: String,
    pub primary: bool,
    pub track: MigrationTrack,
    pub migration_count: usize,
    pub migration_sha256: String,
}

fn invalid(code: &'static str) -> PersistenceError {
    PersistenceError::new(code)
}

/// Attest one worker database before it is admitted to a drain loop.
///
/// The migration inventory uses the same reviewed file set and canonical lock
/// as `migrate_database`. No schema repair, migration or write is performed.
pub async fn attest_worker_database(
    pool: &DatabasePool,
    request: WorkerAttestationRequest<'_>,
) -> Result<WorkerDatabaseAttestation, PersistenceError> {
    if request
        .expected_database
        .is_some_and(|value| value.trim().is_empty() || value.len() > 128)
    {
        return Err(invalid("WORKER_DATABASE_IDENTITY_INVALID"));
    }
    let row = sqlx::query(
        "SELECT current_database()::text database,current_user::text role,pg_is_in_recovery() recovery",
    )
    .fetch_one(pool.raw())
    .await?;
    let database: String = row.try_get("database")?;
    let role: String = row.try_get("role")?;
    let recovery: bool = row.try_get("recovery")?;
    if request
        .expected_database
        .is_some_and(|expected| database != expected)
    {
        return Err(invalid("WORKER_DATABASE_IDENTITY_MISMATCH"));
    }
    if role.trim().is_empty() {
        return Err(invalid("WORKER_DATABASE_ROLE_INVALID"));
    }
    if recovery {
        return Err(invalid("WORKER_DATABASE_NOT_PRIMARY"));
    }
    let inventory = verify_current_migration_inventory(pool, request.track, request.files).await?;
    Ok(WorkerDatabaseAttestation {
        database,
        role,
        primary: !recovery,
        track: inventory.track(),
        migration_count: inventory.count(),
        migration_sha256: inventory.sha256().to_owned(),
    })
}
