//! Maintenance-owned absence only. Asset admission, actor authority and
//! commercial purchase remain independent coordinator obligations.
use super::{Error, Transaction, changed, transaction};
use erpx_server_runtime::public::postgres::PersistenceError;
use erpx_server_shared::public::RecordId;
use sqlx::PgConnection;

pub struct CurrentMaintenanceAssetVacancy {
    asset: RecordId,
    unit: Transaction,
}

async fn require_lock(c: &mut PgConnection) -> Result<(), Error> {
    let held: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation='maintenance_work_orders'::regclass AND granted AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))")
        .fetch_one(c).await?;
    if !held { return Err(PersistenceError::new("MAINTENANCE_CREATION_LOCK_REQUIRED").into()); }
    Ok(())
}

async fn vacant(c: &mut PgConnection, asset: &RecordId) -> Result<bool, Error> {
    // Asset IDs are globally keyed. Legacy does not filter work by company.
    Ok(sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM maintenance_work_orders WHERE asset_id=$1 AND status NOT IN ('completed','cancelled'))")
        .bind(asset.as_str()).fetch_one(c).await?)
}

/// Caller must acquire MaintenanceWorkSourcePreparation before its first
/// data snapshot. This observation cannot attest that lock timing.
pub async fn inspect_maintenance_asset_vacancy(c: &mut PgConnection, asset: &RecordId) -> Result<Option<CurrentMaintenanceAssetVacancy>, Error> {
    let unit = transaction(c).await?;
    require_lock(c).await?;
    if !vacant(c, asset).await? { return Ok(None); }
    Ok(Some(CurrentMaintenanceAssetVacancy { asset: asset.clone(), unit }))
}

impl CurrentMaintenanceAssetVacancy {
    pub fn asset_id(&self) -> &RecordId { &self.asset }

    /// Valid only before the expected Work insertion; after insertion the
    /// Maintenance owner must retain and validate an exact after-image.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if transaction(c).await? != self.unit { return Err(changed().into()); }
        require_lock(c).await?;
        if !vacant(c, &self.asset).await? { return Err(changed().into()); }
        Ok(())
    }
}
