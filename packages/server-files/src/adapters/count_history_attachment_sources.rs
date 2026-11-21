//! Files-owned raw attachment history sources for Count Finance History.
//! The caller combines these fixed sources with Workflow receipts and the
//! Inventory terminal decision. This is not a binding proof or authorization.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = ((String, i32, String, String, String), String, Vec<i64>);
// The original Files query returns to_jsonb(source/event); retaining every
// physical column preserves those images as well as native SQL collation.
const CTES: &str = "count_history_attachment_sets AS (SELECT * FROM public.attachment_sets), count_history_attachment_events AS (SELECT * FROM public.attachment_events)";
pub async fn lock_count_history_attachment_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("COUNT_HISTORY_FILES_TRANSACTION_REQUIRED").into());
    }
    for sql in [
        "LOCK TABLE public.attachment_sets IN SHARE MODE NOWAIT",
        "LOCK TABLE public.attachment_events IN SHARE MODE NOWAIT",
    ] {
        sqlx::query(sql)
            .execute(&mut *c)
            .await
            .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
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
        return Err(PersistenceError::new("COUNT_HISTORY_FILES_SERIALIZABLE_REQUIRED").into());
    }
    let (held,visible,path):(bool,bool,String)=sqlx::query_as("SELECT (SELECT count(*)=2 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.attachment_sets'::regclass,'public.attachment_events'::regclass)),NOT (row_security_active('public.attachment_sets'::regclass) OR row_security_active('public.attachment_events'::regclass)),current_setting('search_path')").fetch_one(&mut *c).await?;
    if !held {
        return Err(PersistenceError::new("COUNT_HISTORY_FILES_LOCKS_REQUIRED").into());
    }
    if !visible {
        return Err(PersistenceError::new("COUNT_HISTORY_FILES_RLS_UNSUPPORTED").into());
    }
    let oids=sqlx::query_scalar("SELECT unnest(ARRAY['public.attachment_sets'::regclass::oid::bigint,'public.attachment_events'::regclass::oid::bigint])").fetch_all(&mut *c).await?;
    let transaction=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;
    Ok((transaction, path, oids))
}
/// Fences source identity only; the caller must re-run the complete joined
/// classification after late effects and compare the separately retained result.
pub struct CurrentCountHistoryAttachmentSources {
    unit: Unit,
}
impl CurrentCountHistoryAttachmentSources {
    pub fn cte_sql(&self) -> &'static str {
        CTES
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "COUNT_HISTORY_FILES_UNIT_CHANGED",
                "The Files Count History source changed.",
            )
            .into());
        }
        Ok(())
    }
}
pub async fn inspect_count_history_attachment_sources(
    c: &mut PgConnection,
) -> Result<CurrentCountHistoryAttachmentSources, Error> {
    Ok(CurrentCountHistoryAttachmentSources {
        unit: unit(c).await?,
    })
}
