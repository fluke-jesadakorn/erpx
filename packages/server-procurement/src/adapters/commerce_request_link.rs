//! Procurement source only. This grants no request visibility or command authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String, String, String, String, i64);
const CTE: &str = "procurement_commerce_request_links AS (SELECT request_id,order_id FROM public.procurement_request_orders WHERE order_id=$1)";

/// Call before the first snapshot. This source lock is not a global lock order.
pub async fn lock_commerce_request_links(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() { return Err(PersistenceError::new("PROCUREMENT_LINK_TRANSACTION_REQUIRED").into()); }
    sqlx::query("LOCK TABLE public.procurement_request_orders IN SHARE ROW EXCLUSIVE MODE NOWAIT")
        .execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_, String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("PROCUREMENT_LINK_SERIALIZABLE_REQUIRED").into());
    }
    let (held, visible, oid): (bool, bool, i64) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM pg_class c JOIN pg_locks l ON l.relation=c.oid WHERE c.oid='public.procurement_request_orders'::regclass AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')), NOT row_security_active('public.procurement_request_orders'::regclass), 'public.procurement_request_orders'::regclass::oid::bigint").fetch_one(&mut *c).await?;
    if !held || !visible { return Err(PersistenceError::new("PROCUREMENT_LINK_FENCE_REQUIRED").into()); }
    let (db,pid,xid,start,role,session,path): (String,i32,String,String,String,String,String) = sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path')").fetch_one(c).await?;
    Ok((db,pid,xid,start,role,session,path,oid))
}
async fn image(c: &mut PgConnection, order: &str) -> Result<Option<String>, Error> {
    // Unique order_id bounds rows; byte bound refuses oversized persisted IDs.
    let result: Option<Option<String>> = sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(l)::text)<=2097152 THEN to_jsonb(l)::text END FROM public.procurement_request_orders l WHERE order_id=$1").bind(order).fetch_optional(c).await?;
    match result { Some(None) => Err(PersistenceError::new("PROCUREMENT_LINK_LIMIT_EXCEEDED").into()), Some(Some(s)) => Ok(Some(s)), None => Ok(None) }
}
/// Retains a selected order's link or absence, within the fenced transaction.
/// The fixed CTE preserves PostgreSQL join collation and must be combined with
/// the Records direct-link predicate, visibility and authority by the caller.
pub struct CurrentCommerceRequestLinks { unit: Unit, order: String, image: Option<String> }
impl CurrentCommerceRequestLinks {
    pub fn cte_sql(&self) -> &'static str { CTE }
    pub fn order_id(&self) -> &str { &self.order }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || image(c, &self.order).await? != self.image {
            return Err(BusinessError::new(FailureKind::Conflict,"PROCUREMENT_LINK_CHANGED","The request link changed.").into());
        }
        Ok(())
    }
}
pub async fn inspect_commerce_request_links(c: &mut PgConnection, order: &str) -> Result<CurrentCommerceRequestLinks, Error> {
    if order.len() > 2_097_152 { return Err(PersistenceError::new("PROCUREMENT_LINK_LIMIT_EXCEEDED").into()); }
    let unit = unit(c).await?;
    let image = image(c, order).await?;
    Ok(CurrentCommerceRequestLinks { unit, order: order.to_owned(), image })
}
