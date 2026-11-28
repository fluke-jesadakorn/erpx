//! Inventory-owned posted Count identity only. The caller must separately
//! retain the Quality parent, reciprocal parent Count ID, disposition event,
//! child arithmetic, source authority and purchase decisions.
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
type Image = (String, String, String, String, String, String);

pub struct QualityDispositionCountRequest {
    pub company_id: CompanyId,
    pub branch_id: String,
    pub warehouse_id: String,
    pub count_id: String,
    pub hold_id: String,
}
pub struct CurrentQualityDispositionCount {
    request: QualityDispositionCountRequest,
    image: Image,
    unit: Unit,
}
impl CurrentQualityDispositionCount {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || read(c, &self.request).await?.as_ref() != Some(&self.image)
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// Acquire all participating owner locks before the first transaction snapshot.
pub async fn lock_quality_disposition_count_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query(
        "LOCK TABLE inventory_counts,inventory_count_lines,inventory_count_lot_lines IN SHARE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}
/// None means the actual Count is absent, unposted, differently scoped, or
/// does not point back to the requested Quality hold; it cannot attest lineage.
pub async fn inspect_quality_disposition_count(
    c: &mut PgConnection,
    request: QualityDispositionCountRequest,
) -> Result<Option<CurrentQualityDispositionCount>, Error> {
    let unit = unit(c).await?;
    Ok(read(c, &request)
        .await?
        .map(|image| CurrentQualityDispositionCount {
            request,
            image,
            unit,
        }))
}
async fn read(
    c: &mut PgConnection,
    r: &QualityDispositionCountRequest,
) -> Result<Option<Image>, Error> {
    Ok(sqlx::query_as("SELECT id,company_id,branch_id,warehouse_id,quality_hold_id,status FROM inventory_counts WHERE id=$1 AND company_id=$2 AND branch_id=$3 AND warehouse_id=$4 AND quality_hold_id=$5 AND status='posted' FOR SHARE")
        .bind(&r.count_id).bind(r.company_id.as_str()).bind(&r.branch_id).bind(&r.warehouse_id).bind(&r.hold_id).fetch_optional(c).await?)
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(invalid());
    }
    let held: bool = sqlx::query_scalar("SELECT count(*)=3 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND to_regclass(c.relname::text)=c.oid AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.granted AND l.relation=c.oid AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.inventory_counts'::regclass,'public.inventory_count_lines'::regclass,'public.inventory_count_lot_lines'::regclass)").fetch_one(&mut *c).await?;
    if !held {
        return Err(PersistenceError::new("INVENTORY_QUALITY_COUNT_SOURCE_FENCE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn invalid() -> Error {
    PersistenceError::new("INVENTORY_QUALITY_COUNT_TRANSACTION_REQUIRED").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "INVENTORY_QUALITY_COUNT_CHANGED",
        "The original Quality disposition Count changed.",
    )
    .into()
}
