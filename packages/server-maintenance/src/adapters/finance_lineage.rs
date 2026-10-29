//! Actual Work/physical-Asset lineage for independent Finance settlement. No
//! records.write, operational worker, People engagement or Work-state grant.
use erpx_server_assets::public::maintenance::{
    CurrentMaintenanceAsset, lock_maintenance_asset_sources, read_current_maintenance_asset,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId, RecordId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

/// Data supplied by service from its actual immutable Inventory source. It is
/// not a caller permission, source approval or replacement for that proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceFinanceLineageRequest {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub work_id: RecordId,
    pub branch_id: RecordId,
    pub department_id: Option<RecordId>,
    pub minimum_consumption_version: i64,
}
#[derive(Eq, PartialEq)]
struct Work {
    asset: RecordId,
    version: i64,
}
pub struct CurrentMaintenanceFinanceLineage {
    request: MaintenanceFinanceLineageRequest,
    work: Work,
    asset: CurrentMaintenanceAsset,
    unit: Unit,
}
impl CurrentMaintenanceFinanceLineage {
    pub fn request(&self) -> &MaintenanceFinanceLineageRequest {
        &self.request
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        if read_work(c, &self.request).await?.as_ref() != Some(&self.work) {
            return Err(changed());
        }
        self.asset.require_current(c).await
    }
}
pub async fn lock_maintenance_finance_lineage_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    lock_maintenance_asset_sources(c).await?;
    sqlx::query("LOCK TABLE maintenance_work_orders IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_maintenance_finance_lineage(
    c: &mut PgConnection,
    request: &MaintenanceFinanceLineageRequest,
) -> Result<Option<CurrentMaintenanceFinanceLineage>, Error> {
    let unit = unit(c).await?;
    if !(1..=9_007_199_254_740_991).contains(&request.minimum_consumption_version) {
        return Err(invalid());
    }
    let Some(work) = read_work(c, request).await? else {
        return Ok(None);
    };
    let Some(asset) =
        read_current_maintenance_asset(c, &request.holding_id, &request.company_id, &work.asset)
            .await?
    else {
        return Ok(None);
    };
    if asset.facts().branch_id != request.branch_id
        || asset.facts().department_id != request.department_id
    {
        return Ok(None);
    }
    Ok(Some(CurrentMaintenanceFinanceLineage {
        request: request.clone(),
        work,
        asset,
        unit,
    }))
}
async fn read_work(
    c: &mut PgConnection,
    r: &MaintenanceFinanceLineageRequest,
) -> Result<Option<Work>, Error> {
    let row=sqlx::query("SELECT CASE WHEN octet_length(asset_id)<=200 THEN asset_id END asset_id,version FROM maintenance_work_orders WHERE id=$1 AND company_id=$2 AND branch_id=$3 AND department_id IS NOT DISTINCT FROM $4 AND version>=$5 FOR SHARE")
        .bind(r.work_id.as_str()).bind(r.company_id.as_str()).bind(r.branch_id.as_str()).bind(r.department_id.as_ref().map(RecordId::as_str)).bind(r.minimum_consumption_version).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    let asset = row
        .try_get::<Option<String>, _>("asset_id")?
        .ok_or_else(invalid)?;
    let version: i64 = row.try_get("version")?;
    if !(1..=9_007_199_254_740_991).contains(&version) {
        return Err(invalid());
    }
    Ok(Some(Work {
        asset: RecordId::new(asset)?,
        version,
    }))
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn invalid() -> Error {
    PersistenceError::new("MAINTENANCE_FINANCE_LINEAGE_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "MAINTENANCE_FINANCE_LINEAGE_CHANGED",
        "The original Work or physical Asset lineage changed.",
    )
    .into()
}
