//! Existential marker projection over matching Workflow receipts. This is not
//! a complete receipt image. A consumer must apply its own join semantics;
//! notably the Manufacturing source-image join also compares Company.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CountOriginMarkers {
    pub initial: bool,
    pub service: bool,
    pub pos: bool,
    pub manufacturing: bool,
}
pub struct CurrentCountOriginMarkers { count: RecordId, unit: Unit, markers: CountOriginMarkers }

/// Acquire before the first data snapshot, in a coordinator-reviewed global
/// order. No actor, Count existence, or Company authority is implied.
pub async fn lock_count_origin_marker_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() { return Err(PersistenceError::new("COUNT_MARKERS_TRANSACTION_REQUIRED").into()); }
    sqlx::query("LOCK TABLE command_receipts IN SHARE MODE").execute(c).await?;
    Ok(())
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("COUNT_MARKERS_SERIALIZABLE_REQUIRED").into());
    }
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation='command_receipts'::regclass AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(&mut *c).await?;
    if !held { return Err(PersistenceError::new("COUNT_MARKERS_LOCK_REQUIRED").into()); }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn capture(c: &mut PgConnection, count: &RecordId) -> Result<CountOriginMarkers, Error> {
    // Do not narrow by Company or infer the command key: the legacy marker
    // join considers every create receipt with this actual record_id. Native
    // JSONB casting deliberately propagates malformed JSON errors.
    let (initial,service,pos,manufacturing):(bool,bool,bool,bool)=sqlx::query_as("SELECT coalesce(bool_or(jsonb_exists(result_json::jsonb,'_initialPhysicalStockCount')),false),coalesce(bool_or(jsonb_exists(result_json::jsonb,'_serviceInitialPhysicalStockCount')),false),coalesce(bool_or(jsonb_exists(result_json::jsonb,'_posInitialPhysicalStockCount')),false),coalesce(bool_or(jsonb_exists(result_json::jsonb,'_manufacturingInitialPhysicalStockCount')),false) FROM command_receipts WHERE operation='inventory.count.create' AND record_id=$1").bind(count.as_str()).fetch_one(c).await?;
    Ok(CountOriginMarkers { initial, service, pos, manufacturing })
}
pub async fn inspect_count_origin_markers(c: &mut PgConnection, count: &RecordId) -> Result<CurrentCountOriginMarkers, Error> {
    let unit=unit(c).await?;let markers=capture(c,count).await?;
    Ok(CurrentCountOriginMarkers { count:count.clone(),unit,markers })
}
impl CurrentCountOriginMarkers {
    pub fn count_id(&self) -> &RecordId { &self.count }
    pub fn markers(&self) -> CountOriginMarkers { self.markers }
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error> {
        if unit(c).await? != self.unit || capture(c,&self.count).await? != self.markers {
            return Err(BusinessError::new(FailureKind::Conflict,"COUNT_ORIGIN_MARKERS_CHANGED","The Count receipt markers changed.").into());
        }
        Ok(())
    }
}
