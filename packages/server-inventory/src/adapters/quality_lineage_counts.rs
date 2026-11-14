//! Inventory Count relation for Quality's recursive purchase lineage. This is
//! only a same-unit source fence, not a retained lineage decision or grant.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String,String,String,String,i64);
const CTE:&str="quality_lineage_counts AS (SELECT id,quality_hold_id,company_id,branch_id,warehouse_id,status FROM public.inventory_counts)";
pub struct CurrentQualityLineageCounts{unit:Unit}
impl CurrentQualityLineageCounts{
    pub fn cte_sql(&self)->&'static str{CTE}
    /// This confirms source identity and bounds only. The consumer must rerun
    /// the full joined ancestry and compare its decision after late effects.
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if unit(&mut *c).await?!=self.unit{return Err(changed())}bounded(&mut *c).await?;if unit(c).await?!=self.unit{return Err(changed())}Ok(())}
}
/// Call before the first protected snapshot in the complete owner lock order.
pub async fn lock_quality_lineage_counts(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("QUALITY_LINEAGE_COUNT_TRANSACTION_REQUIRED").into())}
    sqlx::query("LOCK TABLE public.inventory_counts IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;Ok(())
}
pub async fn inspect_quality_lineage_counts(c:&mut PgConnection)->Result<CurrentQualityLineageCounts,Error>{let identity=unit(&mut *c).await?;bounded(&mut *c).await?;if unit(c).await?!=identity{return Err(changed())}Ok(CurrentQualityLineageCounts{unit:identity})}
async fn bounded(c:&mut PgConnection)->Result<(),Error>{
    // Bounds source rows and projected bytes transferred by a consumer; this
    // aggregate does not bound PostgreSQL scan cost or intermediate memory.
    let valid:bool=sqlx::query_scalar("SELECT count(*)<=10000 AND COALESCE(sum(octet_length(to_jsonb(s)::text)::bigint),0)<=2097152 FROM (SELECT id,quality_hold_id,company_id,branch_id,warehouse_id,status FROM public.inventory_counts) s").fetch_one(c).await?;
    if !valid{return Err(BusinessError::new(FailureKind::Unavailable,"QUALITY_LINEAGE_COUNT_LIMIT","The Quality Count source exceeds the bounded read.").into())}Ok(())
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
    if !c.is_in_transaction()||sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("QUALITY_LINEAGE_COUNT_SERIALIZABLE_REQUIRED").into())}
    let valid:bool=sqlx::query_scalar("SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND to_regclass('inventory_counts')=c.oid AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND l.relation=c.oid) FROM pg_class c WHERE c.oid=to_regclass('public.inventory_counts')").fetch_optional(&mut *c).await?.unwrap_or(false);
    if !valid{return Err(PersistenceError::new("QUALITY_LINEAGE_COUNT_FENCE_REQUIRED").into())}
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path'),'public.inventory_counts'::regclass::oid::bigint").fetch_one(c).await?)
}
fn changed()->Error{BusinessError::new(FailureKind::Conflict,"QUALITY_LINEAGE_COUNT_UNIT_CHANGED","The Quality Count source changed.").into()}
