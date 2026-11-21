//! Raw Commerce owner sources for the Quality return join; no authority grant.
//! Consumer supplies and retains actual Workflow receipt and Inventory facts,
//! recomputes its complete SQL decision after effects, and owns lock ordering.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (
    String,
    i32,
    String,
    String,
    String,
    String,
    String,
    Vec<i64>,
);
const CTE: &str = "quality_commerce_orders AS (SELECT * FROM public.commerce_orders), quality_commerce_order_lines AS (SELECT * FROM public.commerce_order_lines), quality_commerce_fulfillments AS (SELECT * FROM public.commerce_fulfillments), quality_commerce_fulfillment_lines AS (SELECT * FROM public.commerce_fulfillment_lines), quality_commerce_returns AS (SELECT * FROM public.commerce_returns), quality_commerce_return_lines AS (SELECT * FROM public.commerce_return_lines), quality_commerce_return_allocations AS (SELECT *,row_number() OVER (PARTITION BY return_line_id ORDER BY id)-1 lot_ordinal FROM public.commerce_return_lot_allocations)";
pub struct CurrentQualityReturnJoin {
    unit: Unit,
}
impl CurrentQualityReturnJoin {
    pub fn cte_sql(&self) -> &'static str {
        CTE
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "COMMERCE_QUALITY_RETURN_UNIT_CHANGED",
                "Quality return source changed.",
            )
            .into());
        }
        bounded(c).await
    }
}
pub async fn lock_quality_return_join(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("COMMERCE_QUALITY_RETURN_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.commerce_orders,public.commerce_order_lines,public.commerce_fulfillments,public.commerce_fulfillment_lines,public.commerce_returns,public.commerce_return_lines,public.commerce_return_lot_allocations IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}
pub async fn inspect_quality_return_join(
    c: &mut PgConnection,
) -> Result<CurrentQualityReturnJoin, Error> {
    let identity = unit(c).await?;
    bounded(c).await?;
    if unit(c).await? != identity {
        return Err(PersistenceError::new("COMMERCE_QUALITY_RETURN_UNIT_CHANGED").into());
    }
    Ok(CurrentQualityReturnJoin { unit: identity })
}
async fn bounded(c: &mut PgConnection) -> Result<(), Error> {
    // Bounds transferred/join source size, not PostgreSQL scan or intermediate work.
    let valid:bool=sqlx::query_scalar("SELECT count(*)<=10000 AND coalesce(sum(bytes),0)<=2097152 FROM (SELECT octet_length(to_jsonb(t)::text)::bigint bytes FROM public.commerce_orders t UNION ALL SELECT octet_length(to_jsonb(t)::text)::bigint FROM public.commerce_order_lines t UNION ALL SELECT octet_length(to_jsonb(t)::text)::bigint FROM public.commerce_fulfillments t UNION ALL SELECT octet_length(to_jsonb(t)::text)::bigint FROM public.commerce_fulfillment_lines t UNION ALL SELECT octet_length(to_jsonb(t)::text)::bigint FROM public.commerce_returns t UNION ALL SELECT octet_length(to_jsonb(t)::text)::bigint FROM public.commerce_return_lines t UNION ALL SELECT octet_length(to_jsonb(t)::text)::bigint FROM public.commerce_return_lot_allocations t) s").fetch_one(c).await?;
    if !valid {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "COMMERCE_QUALITY_RETURN_LIMIT",
            "Quality return source exceeds the bounded read.",
        )
        .into());
    }
    Ok(())
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("COMMERCE_QUALITY_RETURN_SERIALIZABLE_REQUIRED").into());
    }
    let valid:bool=sqlx::query_scalar("SELECT count(*)=7 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND to_regclass(c.relname::text)=c.oid AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND l.relation=c.oid)) FROM pg_class c WHERE c.oid IN ('public.commerce_orders'::regclass,'public.commerce_order_lines'::regclass,'public.commerce_fulfillments'::regclass,'public.commerce_fulfillment_lines'::regclass,'public.commerce_returns'::regclass,'public.commerce_return_lines'::regclass,'public.commerce_return_lot_allocations'::regclass)").fetch_one(&mut *c).await?;
    if !valid {
        return Err(PersistenceError::new("COMMERCE_QUALITY_RETURN_SOURCE_FENCE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path'),ARRAY['public.commerce_orders'::regclass::oid::bigint,'public.commerce_order_lines'::regclass::oid::bigint,'public.commerce_fulfillments'::regclass::oid::bigint,'public.commerce_fulfillment_lines'::regclass::oid::bigint,'public.commerce_returns'::regclass::oid::bigint,'public.commerce_return_lines'::regclass::oid::bigint,'public.commerce_return_lot_allocations'::regclass::oid::bigint]").fetch_one(c).await?)
}
