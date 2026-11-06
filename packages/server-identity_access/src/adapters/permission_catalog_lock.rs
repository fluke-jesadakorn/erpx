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
