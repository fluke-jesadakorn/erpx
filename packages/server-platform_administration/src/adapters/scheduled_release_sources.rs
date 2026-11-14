//! Platform-owned raw release rows for composed scheduled availability.
//! Other owners retain Company setup, reviewer, currency and engineering facts.
//! This source is not accepted-release evidence or an authorization decision.
//! It intentionally omits command version/evidence columns; use the production
//! release owner reader for a mutable release guard.
//! Pair it with `lock_scheduled_engineering_sources` before the fresh
//! transaction's first data query to cover the retained TS scheduled lease.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    transaction: (String, i32, String, String, String),
    path: String,
    oids: Vec<i64>,
    image: String,
}
const CTES:&str="scheduled_release_acceptances AS (SELECT id,company_id,scope_version,software_version,configuration_revision,setup_version,requested_by,revoked_at,created_at FROM public.company_release_acceptances), scheduled_release_revisions AS (SELECT company_id,revision FROM public.company_release_revisions), scheduled_legal_release_revisions AS (SELECT company_id,revision FROM public.company_legal_release_revisions), scheduled_release_gate_reviews AS (SELECT company_id,release_id,criterion_id,state,permission,assignment_id,reviewer_id FROM public.company_release_gate_reviews)";
const IMAGE:&str="SELECT CASE WHEN octet_length(a.value)<=262144 AND octet_length(b.value)<=262144 AND octet_length(c.value)<=262144 AND octet_length(d.value)<=262144 AND octet_length(a.value)+octet_length(b.value)+octet_length(c.value)+octet_length(d.value)<=1048576 THEN a.value||'|'||b.value||'|'||c.value||'|'||d.value END FROM (SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY to_jsonb(r)::text),'[]'::jsonb)::text value FROM scheduled_release_acceptances r) a,(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY to_jsonb(r)::text),'[]'::jsonb)::text value FROM scheduled_release_revisions r) b,(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY to_jsonb(r)::text),'[]'::jsonb)::text value FROM scheduled_legal_release_revisions r) c,(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY to_jsonb(r)::text),'[]'::jsonb)::text value FROM scheduled_release_gate_reviews r) d";
fn limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "SCHEDULED_RELEASE_SOURCE_LIMIT",
        "The scheduled release source exceeds its bounded snapshot budget.",
    )
    .into()
}
pub async fn lock_scheduled_release_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("SCHEDULED_RELEASE_SOURCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.company_release_acceptances,public.company_release_gate_reviews,public.company_release_revisions,public.company_legal_release_revisions IN SHARE MODE NOWAIT")
        .execute(&mut *c)
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
        return Err(PersistenceError::new("SCHEDULED_RELEASE_SOURCE_SERIALIZABLE_REQUIRED").into());
    }
    let (held, visible, path): (bool, bool, String) = sqlx::query_as("SELECT (SELECT count(*)=4 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.company_release_acceptances'::regclass,'public.company_release_revisions'::regclass,'public.company_legal_release_revisions'::regclass,'public.company_release_gate_reviews'::regclass)),NOT (row_security_active('public.company_release_acceptances'::regclass) OR row_security_active('public.company_release_revisions'::regclass) OR row_security_active('public.company_legal_release_revisions'::regclass) OR row_security_active('public.company_release_gate_reviews'::regclass)),current_setting('search_path')").fetch_one(&mut *c).await?;
    if !held {
        return Err(PersistenceError::new("SCHEDULED_RELEASE_SOURCE_LOCKS_REQUIRED").into());
    }
    if !visible {
        return Err(PersistenceError::new("SCHEDULED_RELEASE_SOURCE_RLS_UNSUPPORTED").into());
    }
    let oids = sqlx::query_scalar("SELECT unnest(ARRAY['public.company_release_acceptances'::regclass::oid::bigint,'public.company_release_revisions'::regclass::oid::bigint,'public.company_legal_release_revisions'::regclass::oid::bigint,'public.company_release_gate_reviews'::regclass::oid::bigint])").fetch_all(&mut *c).await?;
    let transaction = sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(&mut *c).await?;
    let image = sqlx::query_scalar::<_, Option<String>>(sqlx::AssertSqlSafe(format!(
        "WITH {CTES} {IMAGE}"
    )))
    .fetch_one(c)
    .await?
    .ok_or_else(limit)?;
    Ok(Snapshot {
        transaction,
        path,
        oids,
        image,
    })
}
/// Identity/fences and a bounded row image only; the coordinator retains and
/// recomputes the complete cross-owner release decision after effects and must
/// not infer authorization.
pub struct CurrentScheduledReleaseSources {
    snapshot: Snapshot,
}
impl CurrentScheduledReleaseSources {
    /// Exact columns used by the retained scheduled release predicate; evidence,
    /// reason and version are not part of this raw source.
    pub fn cte_sql(&self) -> &'static str {
        CTES
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if snapshot(c).await? != self.snapshot {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SCHEDULED_RELEASE_SOURCE_UNIT_CHANGED",
                "The scheduled release source changed.",
            )
            .into());
        }
        Ok(())
    }
}
pub async fn inspect_scheduled_release_sources(
    c: &mut PgConnection,
) -> Result<CurrentScheduledReleaseSources, Error> {
    Ok(CurrentScheduledReleaseSources {
        snapshot: snapshot(c).await?,
    })
}
