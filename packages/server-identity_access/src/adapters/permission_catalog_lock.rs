//! Permission catalog fence for selected owner sources. Acquire alongside all
//! owner locks before the first serializable snapshot.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;

/// Never wait while holding other Identity/owner locks: the catalog writer
/// may be waiting for those locks. Only this lock's 55P03 is marked for the
/// enclosing whole-unit retry; other PostgreSQL failures retain their meaning.
pub async fn lock_permission_catalog_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("IAM_PERMISSION_CATALOG_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE iam_permissions IN SHARE MODE NOWAIT")
        .execute(c)
        .await
        .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}

/// Require an already held source fence without acquiring or upgrading it.
/// This neither proves the lock preceded the snapshot nor grants authority.
pub async fn require_permission_catalog_lock(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("IAM_PERMISSION_CATALOG_TRANSACTION_REQUIRED").into());
    }
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation='iam_permissions'::regclass AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(c).await?;
    if !held {return Err(PersistenceError::new("IAM_PERMISSION_CATALOG_LOCK_REQUIRED").into());}
    Ok(())
}
