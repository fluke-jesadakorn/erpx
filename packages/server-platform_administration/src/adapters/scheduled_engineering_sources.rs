//! Platform persisted engineering contexts for scheduled availability.
//! A row is not an attestation, native roster, database identity or grant.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    transaction: (String, i32, String, String, String, String),
    path: String,
    oid: i64,
    image: String,
}
const CTE:&str="scheduled_engineering_contexts AS (SELECT company_id,run_id,source_fingerprint,software_version,release_manifest_sha256,enabled,expires_at FROM public.engineering_verification_contexts)";
const IMAGE:&str="SELECT CASE WHEN octet_length(value)<=1048576 THEN value END FROM (SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY to_jsonb(r)::text),'[]'::jsonb)::text value FROM scheduled_engineering_contexts r) image";
fn limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "SCHEDULED_ENGINEERING_SOURCE_LIMIT",
        "The scheduled engineering source exceeds its bounded snapshot budget.",
    )
    .into()
}
pub async fn lock_scheduled_engineering_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("SCHEDULED_ENGINEERING_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.engineering_verification_contexts IN SHARE MODE NOWAIT")
        .execute(c)
        .await
        .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}
async fn snapshot(c: &mut PgConnection) -> Result<Snapshot, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("SCHEDULED_ENGINEERING_SERIALIZABLE_REQUIRED").into());
    }
    let (held, visible, path, oid): (bool, bool, String, i64) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM pg_class c JOIN pg_locks l ON l.relation=c.oid WHERE c.oid='public.engineering_verification_contexts'::regclass AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')),NOT row_security_active('public.engineering_verification_contexts'::regclass),current_setting('search_path'),'public.engineering_verification_contexts'::regclass::oid::bigint").fetch_one(&mut *c).await?;
    if !held {
        return Err(PersistenceError::new("SCHEDULED_ENGINEERING_LOCK_REQUIRED").into());
    }
    if !visible {
        return Err(PersistenceError::new("SCHEDULED_ENGINEERING_RLS_UNSUPPORTED").into());
    }
    let transaction = sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text").fetch_one(&mut *c).await?;
    let image = sqlx::query_scalar::<_, Option<String>>(sqlx::AssertSqlSafe(format!(
        "WITH {CTE} {IMAGE}"
    )))
    .fetch_one(c)
    .await?
    .ok_or_else(limit)?;
    Ok(Snapshot {
        transaction,
        path,
        oid,
        image,
    })
}
/// Source fence and bounded row image only. Consumers retain and recheck the
/// full owner decision, including actual database-clock expiry and sealed
/// filesystem/roster facts.
pub struct CurrentScheduledEngineeringSources {
    snapshot: Snapshot,
}
impl CurrentScheduledEngineeringSources {
    /// Exact columns used by the retained engineering row predicate.
    pub fn cte_sql(&self) -> &'static str {
        CTE
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if snapshot(c).await? != self.snapshot {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SCHEDULED_ENGINEERING_UNIT_CHANGED",
                "The scheduled engineering source changed.",
            )
            .into());
        }
        Ok(())
    }
}
pub async fn inspect_scheduled_engineering_sources(
    c: &mut PgConnection,
) -> Result<CurrentScheduledEngineeringSources, Error> {
    Ok(CurrentScheduledEngineeringSources {
        snapshot: snapshot(c).await?,
    })
}
