//! Inventory-only raw allocation source for the Quality origin SQL joins.
//! No Company scope or authority is implied. The caller retains Commerce,
//! Workflow and Quality witnesses and re-evaluates the complete decision.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String,String,String,String,i64);
const CTE:&str="quality_inventory_fulfillment_allocations AS (SELECT id,fulfillment_line_id,lot_id,quantity_milli,value_minor,disposition,quality_hold_id FROM public.inventory_fulfillment_lot_allocations)";
pub struct CurrentQualityOriginAllocationJoin{unit:Unit}
impl CurrentQualityOriginAllocationJoin{
 pub fn cte_sql(&self)->&'static str{CTE}
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if unit(c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"INVENTORY_QUALITY_ORIGIN_UNIT_CHANGED","Quality allocation source changed.").into())}bounded(c).await}
}
pub async fn lock_quality_origin_allocation_join(c:&mut PgConnection)->Result<(),Error>{if !c.is_in_transaction(){return Err(PersistenceError::new("INVENTORY_QUALITY_ORIGIN_TRANSACTION_REQUIRED").into())}sqlx::query("LOCK TABLE public.inventory_fulfillment_lot_allocations IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;Ok(())}
pub async fn inspect_quality_origin_allocation_join(c:&mut PgConnection)->Result<CurrentQualityOriginAllocationJoin,Error>{let identity=unit(c).await?;bounded(c).await?;if unit(c).await?!=identity{return Err(PersistenceError::new("INVENTORY_QUALITY_ORIGIN_UNIT_CHANGED").into())}Ok(CurrentQualityOriginAllocationJoin{unit:identity})}
async fn bounded(c:&mut PgConnection)->Result<(),Error>{
 // Bounded source rows and bytes, not a bound on PostgreSQL scan/work memory.
 let valid:bool=sqlx::query_scalar("SELECT count(*)<=10000 AND coalesce(sum(octet_length(to_jsonb(a)::text)::bigint),0)<=2097152 FROM public.inventory_fulfillment_lot_allocations a").fetch_one(c).await?;if !valid{return Err(BusinessError::new(FailureKind::Unavailable,"INVENTORY_QUALITY_ORIGIN_LIMIT","Quality allocation source exceeds the bounded read.").into())}Ok(())
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{if !c.is_in_transaction()||sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("INVENTORY_QUALITY_ORIGIN_SERIALIZABLE_REQUIRED").into())}let valid:bool=sqlx::query_scalar("SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND to_regclass('inventory_fulfillment_lot_allocations')=c.oid AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND l.relation=c.oid) FROM pg_class c WHERE c.oid=to_regclass('public.inventory_fulfillment_lot_allocations')").fetch_optional(&mut *c).await?.unwrap_or(false);if !valid{return Err(PersistenceError::new("INVENTORY_QUALITY_ORIGIN_FENCE_REQUIRED").into())}Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path'),'public.inventory_fulfillment_lot_allocations'::regclass::oid::bigint").fetch_one(c).await?)}
