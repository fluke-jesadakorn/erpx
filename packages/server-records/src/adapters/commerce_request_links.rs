//! Records-owned raw request relation for Commerce's linkage query. The
//! Procurement order-link source and full joined decision remain separate.
//! The caller must acquire `lock_commerce_request_links` before its first
//! protected snapshot and must run under a canonical search_path and
//! non-BYPASSRLS application role; this raw witness does not enforce either.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String, String, String, String, i64);
const CTE: &str = "commerce_request_links AS (SELECT id,company_id,purchase_order_id FROM public.employee_requests)";
pub struct CurrentCommerceRequestLinks {
    unit: Unit,
}
impl CurrentCommerceRequestLinks {
    pub fn cte_sql(&self) -> &'static str {
        CTE
    }
    /// This attests source identity and bounds, not rows or a decision. The
    /// coordinator must rerun the full Records+Procurement SQL after effects.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit {
            return Err(changed());
        }
        bounded(&mut *c).await?;
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        Ok(())
    }
}
/// Acquire before all protected snapshots; composition owns global lock order.
pub async fn lock_commerce_request_links(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("RECORDS_COMMERCE_LINK_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.employee_requests IN SHARE MODE NOWAIT")
        .execute(c)
        .await
        .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}
pub async fn inspect_commerce_request_links(
    c: &mut PgConnection,
) -> Result<CurrentCommerceRequestLinks, Error> {
    let identity = unit(&mut *c).await?;
    bounded(&mut *c).await?;
    if unit(c).await? != identity {
        return Err(changed());
    }
    Ok(CurrentCommerceRequestLinks { unit: identity })
}
async fn bounded(c: &mut PgConnection) -> Result<(), Error> {
    // Bounds projected source rows/bytes; PostgreSQL scan work and intermediate
    // memory are not bounded, and no source is silently truncated.
    let valid:bool=sqlx::query_scalar("SELECT count(*)<=10000 AND COALESCE(sum(octet_length(to_jsonb(s)::text)::bigint),0)<=2097152 FROM (SELECT id,company_id,purchase_order_id FROM public.employee_requests) s").fetch_one(c).await?;
    if !valid {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "RECORDS_COMMERCE_LINK_LIMIT",
            "The Records Commerce link source exceeds its bound.",
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
        return Err(PersistenceError::new("RECORDS_COMMERCE_LINK_SERIALIZABLE_REQUIRED").into());
    }
    let valid:bool=sqlx::query_scalar("SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND to_regclass('employee_requests')=c.oid AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND l.relation=c.oid) FROM pg_class c WHERE c.oid=to_regclass('public.employee_requests')").fetch_optional(&mut *c).await?.unwrap_or(false);
    if !valid {
        return Err(PersistenceError::new("RECORDS_COMMERCE_LINK_FENCE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path'),'public.employee_requests'::regclass::oid::bigint").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "RECORDS_COMMERCE_LINK_UNIT_CHANGED",
        "The Records Commerce link source changed.",
    )
    .into()
}
