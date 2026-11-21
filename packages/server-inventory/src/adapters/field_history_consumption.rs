//! Inventory-owned SQL source for the generic Field parts journal History join.
//! The consuming coordinator retains actual Finance, Workflow, Field, Money,
//! Commercial and Identity evidence; this source is not authority or lineage.
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,FailureKind};
use sqlx::{Connection,PgConnection};
type Error=PgTransactionError<BusinessError>;
type Unit=((String,i32,String,String,String),String,i64);

// Exact Inventory-owned WHERE predicates from Field's partsJournal source.
// Other source conditions (event, receipt, journal lines, Money, Company) stay
// in their owners and must be joined in SQL using the actual database collation.
const FIELD_HISTORY_CONSUMPTIONS_CTE:&str="field_history_consumptions AS (SELECT id,company_id,branch_id,department_id,source_kind,source_id,source_version,warehouse_id,product_id,quantity_milli,value_minor,currency,journal_id,created_by FROM public.operational_inventory_consumptions WHERE source_kind='field_service' AND quantity_milli>0 AND value_minor>0)";

/// Acquire before the first snapshot along with other owner fences in a
/// reviewed global order; 55P03 requires retrying the complete Holding unit.
pub async fn lock_field_history_consumption_join_sources(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("FIELD_HISTORY_CONSUMPTION_TRANSACTION_REQUIRED").into());}
    sqlx::query("LOCK TABLE public.operational_inventory_consumptions IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;Ok(())
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable" {return Err(PersistenceError::new("FIELD_HISTORY_CONSUMPTION_SERIALIZABLE_REQUIRED").into());}
    let (held,visible,path,oid):(bool,bool,String,i64)=sqlx::query_as("SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')),NOT row_security_active(c.oid),current_setting('search_path'),c.oid::bigint FROM pg_class c WHERE c.oid='public.operational_inventory_consumptions'::regclass").fetch_one(&mut *c).await?;
    if !held{return Err(PersistenceError::new("FIELD_HISTORY_CONSUMPTION_LOCK_REQUIRED").into());}
    if !visible{return Err(PersistenceError::new("FIELD_HISTORY_CONSUMPTION_RLS_UNSUPPORTED").into());}
    let transaction=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;Ok((transaction,path,oid))
}
/// Only a canonical same-unit join source. `require_current` checks its
/// identity and fences, not row contents or the consuming classification.
/// Consumers must recompute the complete joined positive/negative result after
/// late effects and compare it with their own retained outcome.
pub struct CurrentFieldHistoryConsumptionJoin{unit:Unit}
impl CurrentFieldHistoryConsumptionJoin{
    /// Fixed trusted SQL fragment; never interpolate request-supplied SQL.
    pub fn cte_sql(&self)->&'static str{FIELD_HISTORY_CONSUMPTIONS_CTE}
    /// Raw global source for the installed 1097 journal marker. Unlike the
    /// History CTE this intentionally has no Company, quantity, value, size or
    /// row-count filter. The caller joins journal_id/company_id in PostgreSQL
    /// and reruns its complete marker classification after later effects.
    pub fn global_journal_marker_cte_sql(&self)->&'static str {
        "field_global_journal_consumptions AS (SELECT journal_id,company_id FROM public.operational_inventory_consumptions WHERE source_kind='field_service')"
    }
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if unit(c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"FIELD_HISTORY_CONSUMPTION_UNIT_CHANGED","The Inventory History join source changed.").into());}Ok(())}
}
pub async fn inspect_field_history_consumption_join(c:&mut PgConnection)->Result<CurrentFieldHistoryConsumptionJoin,Error>{Ok(CurrentFieldHistoryConsumptionJoin{unit:unit(c).await?})}
