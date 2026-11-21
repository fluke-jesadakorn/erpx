//! Inventory-owned Count inputs for generic initial-stock Finance History.
//! This seam is not a Count marker, valid source lineage, purchase or IAM
//! authorization. Workflow, Commerce, Finance and other owners retain their
//! own facts and the coordinator re-evaluates the joined decision after effects.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=((String,i32,String,String,String),String,Vec<i64>);

// Simple SQL projections retain original PostgreSQL types and collations. No
// selected values or receipt JSON are read into an application-side list.
const INITIAL_COUNT_HISTORY_CTES:&str=r#"initial_count_history_counts AS (SELECT id,company_id,branch_id,warehouse_id,status,version,journal_id,created_by,net_value_minor,quality_hold_id FROM public.inventory_counts),
initial_count_history_lines AS (SELECT id,count_id,product_id,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor FROM public.inventory_count_lines)"#;

/// Inventory utility locks only. Acquire before the first snapshot with the
/// other owner sources in a reviewed order; retry the whole unit on 55P03.
pub async fn lock_initial_count_history_sources(c:&mut PgConnection)->Result<(),Error>{
 if !c.is_in_transaction(){return Err(PersistenceError::new("INITIAL_COUNT_HISTORY_TRANSACTION_REQUIRED").into());}
 for sql in ["LOCK TABLE public.inventory_counts IN SHARE MODE NOWAIT","LOCK TABLE public.inventory_count_lines IN SHARE MODE NOWAIT"] {
  sqlx::query(sql).execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
 } Ok(())
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
 if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("INITIAL_COUNT_HISTORY_SERIALIZABLE_REQUIRED").into());}
 let (held,visible,path):(bool,bool,String)=sqlx::query_as("SELECT (SELECT count(*)=2 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.inventory_counts'::regclass,'public.inventory_count_lines'::regclass)),NOT (row_security_active('public.inventory_counts'::regclass) OR row_security_active('public.inventory_count_lines'::regclass)),current_setting('search_path')").fetch_one(&mut *c).await?;
 if !held{return Err(PersistenceError::new("INITIAL_COUNT_HISTORY_LOCKS_REQUIRED").into());}if !visible{return Err(PersistenceError::new("INITIAL_COUNT_HISTORY_RLS_UNSUPPORTED").into());}
 let oids:Vec<i64>=sqlx::query_scalar("SELECT unnest(ARRAY['public.inventory_counts'::regclass::oid::bigint,'public.inventory_count_lines'::regclass::oid::bigint])").fetch_all(&mut *c).await?;let transaction=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;Ok((transaction,path,oids))
}
/// Same-unit owner source only. `require_current` verifies the fences, not the
/// row content or the result of an initial-marker join. A consumer must retain
/// its full decision and re-run the joins after all late effects.
pub struct CurrentInitialCountHistorySource{unit:Unit}
impl CurrentInitialCountHistorySource{
 /// Fixed trusted CTEs only; never interpolate client-supplied SQL.
 pub fn cte_sql(&self)->&'static str{INITIAL_COUNT_HISTORY_CTES}
 pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if unit(c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"INITIAL_COUNT_HISTORY_UNIT_CHANGED","The Inventory Count History source changed.").into());}Ok(())}
}
pub async fn inspect_initial_count_history_source(c:&mut PgConnection)->Result<CurrentInitialCountHistorySource,Error>{Ok(CurrentInitialCountHistorySource{unit:unit(c).await?})}
