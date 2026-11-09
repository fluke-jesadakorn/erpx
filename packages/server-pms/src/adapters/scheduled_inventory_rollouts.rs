//! PMS rollout facts for Inventory scheduled source decisions.
//! Release acceptance, engineering attestation, subject identity and the
//! scheduled worker's decision remain with their respective owners.
//! The CTE pins `public.company_feature_rollouts`; the retained TS predicate
//! resolves its unqualified relation through search_path. A hostile temp shadow
//! therefore cannot inject a rollout into this source (stricter than TS).
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    transaction: (String, i32, String, String, String),
    path: String,
    oid: i64,
    image: String,
}
const CTE:&str="scheduled_inventory_rollouts AS (SELECT company_id,module_id,rollout_state FROM public.company_feature_rollouts)";
const IMAGE:&str="SELECT CASE WHEN octet_length(value)<=1048576 THEN value END FROM (SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY to_jsonb(r)::text),'[]'::jsonb)::text value FROM scheduled_inventory_rollouts r) image";
fn limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "SCHEDULED_INVENTORY_ROLLOUT_SOURCE_LIMIT",
        "The scheduled Inventory rollout source exceeds its bounded snapshot budget.",
    )
    .into()
}
pub async fn lock_scheduled_inventory_rollouts(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(
            PersistenceError::new("SCHEDULED_INVENTORY_ROLLOUT_TRANSACTION_REQUIRED").into(),
        );
    }
    sqlx::query("LOCK TABLE public.company_feature_rollouts IN SHARE MODE NOWAIT")
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
        return Err(
            PersistenceError::new("SCHEDULED_INVENTORY_ROLLOUT_SERIALIZABLE_REQUIRED").into(),
        );
    }
    let (held, visible, path, oid): (bool, bool, String, i64) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM pg_class c JOIN pg_locks l ON l.relation=c.oid WHERE c.oid='public.company_feature_rollouts'::regclass AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')),NOT row_security_active('public.company_feature_rollouts'::regclass),current_setting('search_path'),'public.company_feature_rollouts'::regclass::oid::bigint").fetch_one(&mut *c).await?;
    if !held {
        return Err(PersistenceError::new("SCHEDULED_INVENTORY_ROLLOUT_LOCK_REQUIRED").into());
    }
    if !visible {
        return Err(PersistenceError::new("SCHEDULED_INVENTORY_ROLLOUT_RLS_UNSUPPORTED").into());
    }
    let transaction = sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(&mut *c).await?;
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
/// Owner source identity and bounded row image only. A coordinator retains and
/// rechecks the complete decision including the actual release/engineering
/// owner evidence after effects.
pub struct CurrentScheduledInventoryRollouts {
    snapshot: Snapshot,
}
impl CurrentScheduledInventoryRollouts {
    /// Exact columns used by the retained rollout predicate.
    pub fn cte_sql(&self) -> &'static str {
        CTE
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if snapshot(c).await? != self.snapshot {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SCHEDULED_INVENTORY_ROLLOUT_UNIT_CHANGED",
                "The Inventory scheduled rollout source changed.",
            )
            .into());
        }
        Ok(())
    }
}
pub async fn inspect_scheduled_inventory_rollouts(
    c: &mut PgConnection,
) -> Result<CurrentScheduledInventoryRollouts, Error> {
    Ok(CurrentScheduledInventoryRollouts {
        snapshot: snapshot(c).await?,
    })
}
