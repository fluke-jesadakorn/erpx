//! Quality-only raw sources for both legacy purchase lineage SQL paths.
//! The consumer retains Inventory Count, Commerce and Workflow sources,
//! evaluates the whole decision and repeats it after effects. No authority is
//! granted by these raw relations or by this source fence.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=(String,i32,String,String,String,String,String,Vec<i64>);
const CTE:&str="quality_purchase_holds AS (SELECT * FROM public.inventory_quality_holds), quality_purchase_dispositions AS (SELECT * FROM public.inventory_quality_disposition_events)";
pub struct CurrentQualityPurchaseLineageJoin{unit:Unit}
impl CurrentQualityPurchaseLineageJoin{
 pub fn cte_sql(&self)->&'static str{CTE}
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if unit(c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"QUALITY_PURCHASE_JOIN_UNIT_CHANGED","Quality purchase source changed.").into())}bounded(c).await}
}
pub async fn lock_quality_purchase_lineage_join(c:&mut PgConnection)->Result<(),Error>{if !c.is_in_transaction(){return Err(PersistenceError::new("QUALITY_PURCHASE_JOIN_TRANSACTION_REQUIRED").into())}sqlx::query("LOCK TABLE public.inventory_quality_holds,public.inventory_quality_disposition_events IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;Ok(())}
pub async fn inspect_quality_purchase_lineage_join(c:&mut PgConnection)->Result<CurrentQualityPurchaseLineageJoin,Error>{let identity=unit(c).await?;bounded(c).await?;if unit(c).await?!=identity{return Err(PersistenceError::new("QUALITY_PURCHASE_JOIN_UNIT_CHANGED").into())}Ok(CurrentQualityPurchaseLineageJoin{unit:identity})}
async fn bounded(c:&mut PgConnection)->Result<(),Error>{
 // This bounds accepted source rows and transferred source bytes, not the
 // database scan or intermediate PostgreSQL memory. Never truncate a result.
 let valid:bool=sqlx::query_scalar("SELECT count(*)<=10000 AND coalesce(sum(bytes),0)<=2097152 FROM (SELECT octet_length(to_jsonb(t)::text)::bigint bytes FROM public.inventory_quality_holds t UNION ALL SELECT octet_length(to_jsonb(t)::text)::bigint FROM public.inventory_quality_disposition_events t) s").fetch_one(c).await?;if !valid{return Err(BusinessError::new(FailureKind::Unavailable,"QUALITY_PURCHASE_JOIN_LIMIT","Quality purchase source exceeds the bounded read.").into())}Ok(())
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{if !c.is_in_transaction()||sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("QUALITY_PURCHASE_JOIN_SERIALIZABLE_REQUIRED").into())}let valid:bool=sqlx::query_scalar("SELECT count(*)=2 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND to_regclass(c.relname::text)=c.oid AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND l.relation=c.oid)) FROM pg_class c WHERE c.oid IN ('public.inventory_quality_holds'::regclass,'public.inventory_quality_disposition_events'::regclass)").fetch_one(&mut *c).await?;if !valid{return Err(PersistenceError::new("QUALITY_PURCHASE_JOIN_FENCE_REQUIRED").into())}Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path'),ARRAY['public.inventory_quality_holds'::regclass::oid::bigint,'public.inventory_quality_disposition_events'::regclass::oid::bigint]").fetch_one(c).await?)}
