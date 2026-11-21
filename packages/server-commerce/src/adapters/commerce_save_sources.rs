//! Fixed Commerce-owned SQL sources for the save-order coordinator. This is
//! relation/unit evidence only, not selected rows, authorization, pricing,
//! Finance settings, purchase permission, or a retained decision. The caller
//! must rerun its exact joined decisions after its own and deferred writes.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String,i32,String,String,String,String,String,Vec<i64>);

// Settings and orders use the closed projections consumed by Finance's actual
// TS join; warehouse/product readers and order-line readback use their native
// table columns. SQL is fixed, no caller-supplied SQL or data crosses this API.
const CTE: &str = r#"commerce_save_settings AS (SELECT company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id FROM public.commerce_settings),
commerce_save_warehouses AS (SELECT * FROM public.commerce_warehouses),
commerce_save_products AS (SELECT * FROM public.commerce_products),
commerce_save_orders AS (SELECT id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,created_at,updated_at,price_list_id,price_list_version,discount_reason,discount_approval_required,discount_approved_by,discount_approved_at FROM public.commerce_orders),
commerce_save_order_lines AS (SELECT * FROM public.commerce_order_lines)"#;

/// Acquire all owner table locks before the first snapshot. Writers must retry
/// the entire unit on contention. Cross-owner lock ordering is the caller's.
pub async fn lock_commerce_save_sources(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("COMMERCE_SAVE_TRANSACTION_REQUIRED").into());}
    for sql in [
        "LOCK TABLE public.commerce_settings IN SHARE MODE NOWAIT",
        "LOCK TABLE public.commerce_warehouses IN SHARE MODE NOWAIT",
        "LOCK TABLE public.commerce_products IN SHARE MODE NOWAIT",
        "LOCK TABLE public.commerce_orders IN SHARE ROW EXCLUSIVE MODE NOWAIT",
        "LOCK TABLE public.commerce_order_lines IN SHARE ROW EXCLUSIVE MODE NOWAIT",
    ] { sqlx::query(sql).execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?; }
    Ok(())
}
async fn unit(c:&mut PgConnection)->Result<Unit,Error>{
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable" {return Err(PersistenceError::new("COMMERCE_SAVE_SERIALIZABLE_REQUIRED").into());}
    let valid:bool=sqlx::query_scalar("SELECT count(*)=5 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND NOT row_security_active(c.oid) AND to_regclass(c.relname::text)=c.oid AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.relation=c.oid AND CASE WHEN c.oid IN ('public.commerce_orders'::regclass,'public.commerce_order_lines'::regclass) THEN l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') ELSE l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') END)) FROM pg_class c WHERE c.oid IN ('public.commerce_settings'::regclass,'public.commerce_warehouses'::regclass,'public.commerce_products'::regclass,'public.commerce_orders'::regclass,'public.commerce_order_lines'::regclass)").fetch_one(&mut *c).await?;
    if !valid{return Err(PersistenceError::new("COMMERCE_SAVE_SOURCE_FENCE_REQUIRED").into());}
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path'),ARRAY['public.commerce_settings'::regclass::oid::bigint,'public.commerce_warehouses'::regclass::oid::bigint,'public.commerce_products'::regclass::oid::bigint,'public.commerce_orders'::regclass::oid::bigint,'public.commerce_order_lines'::regclass::oid::bigint]").fetch_one(c).await?)
}
pub struct CurrentCommerceSaveSources{unit:Unit}
impl CurrentCommerceSaveSources{
    /// Fixed SQL fragments for inclusion in a trusted, parameterized `WITH`.
    /// They carry no row or content witness. This API transfers no full tables;
    /// consumer queries must bound selected results and PostgreSQL work.
    pub fn cte_sql(&self)->&'static str{CTE}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if unit(c).await?!=self.unit{return Err(changed().into());}Ok(())}
}
/// The presence of a lock cannot prove it was acquired before a snapshot.
pub async fn inspect_commerce_save_sources(c:&mut PgConnection)->Result<CurrentCommerceSaveSources,Error>{let identity=unit(&mut *c).await?;if unit(c).await?!=identity{return Err(changed().into());}Ok(CurrentCommerceSaveSources{unit:identity})}
fn changed()->BusinessError{BusinessError::new(FailureKind::Conflict,"COMMERCE_SAVE_SOURCE_CHANGED","The Commerce save source changed.")}
