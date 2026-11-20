//! Native scheduled access-certification alert sweep.
//!
//! The two idempotent due-soon/overdue notification INSERTs are owned here and
//! executed on caller-owned serializable units. This replaces the TS-only
//! `escalateAccessReviews` scheduled writer.
use erpx_server_identity_access::public::SystemAccountClock;
use erpx_server_runtime::public::postgres::{DatabasePool, PoolLimits};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use sqlx::{Connection, PgConnection, PgPool};
use std::time::Duration;

const DUE_SOON: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,\
 record_id,record_version,message,created_at) \
 SELECT 'iam-access-review-due-soon-'||md5(c.id||':'||c.due_at::text||':'||a.user_id),\
 c.company_id,a.user_id,'iam_access_review',c.id,c.version,$1,$2 \
 FROM iam_access_review_campaigns c JOIN assignments a ON a.company_id=c.company_id \
 WHERE c.status='open' AND EXISTS(SELECT 1 FROM iam_access_review_items i \
 WHERE i.campaign_id=c.id AND i.decision='pending') \
 AND c.due_at>$3 AND c.due_at<=$4 \
 AND a.scope_kind='holding' AND a.revoked_at IS NULL \
 AND iam_assignment_active(a.id,$5) AND iam_role_allows(a.role,a.company_id,'iam.review') \
 GROUP BY c.id,c.company_id,c.version,c.due_at,a.user_id ON CONFLICT DO NOTHING";

const OVERDUE: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,\
 record_id,record_version,message,created_at) \
 SELECT 'iam-access-review-overdue-'||md5(c.id||':'||c.due_at::text||':'||a.user_id),\
 c.company_id,a.user_id,'iam_access_review',c.id,c.version,$1,$2 \
 FROM iam_access_review_campaigns c JOIN assignments a ON a.company_id=c.company_id \
 WHERE c.status='open' AND EXISTS(SELECT 1 FROM iam_access_review_items i \
 WHERE i.campaign_id=c.id AND i.decision='pending') \
 AND c.due_at<=$3 \
 AND a.scope_kind='holding' AND a.revoked_at IS NULL \
 AND iam_assignment_active(a.id,$4) AND iam_role_allows(a.role,a.company_id,'iam.review') \
 GROUP BY c.id,c.company_id,c.version,c.due_at,a.user_id ON CONFLICT DO NOTHING";

const DUE_SOON_MESSAGE: &str = "Access certification is due within 7 days; review pending assignments or coordinate an independent reviewer";
const OVERDUE_MESSAGE: &str = "Access certification is overdue; complete the remaining independent decisions";
pub const WARNING_WINDOW_MS: i64 = 7 * 86_400_000;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ACCESS_REVIEW_ALERTS_UNAVAILABLE",
        "The scheduled access-certification sweep is unavailable.",
    )
}

async fn sweep(
    connection: &mut PgConnection,
    now: i64,
    warning_window_ms: i64,
) -> Result<(u64, u64), BusinessError> {
    if !connection.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| unavailable())?
            != "serializable"
    {
        return Err(unavailable());
    }
    let horizon = now
        .checked_add(warning_window_ms)
        .filter(|v| *v <= 9_007_199_254_740_991)
        .ok_or_else(unavailable)?;
    let due_soon = sqlx::query(DUE_SOON)
        .bind(DUE_SOON_MESSAGE)
        .bind(now)
        .bind(now)
        .bind(horizon)
        .bind(now)
        .execute(&mut *connection)
        .await
        .map_err(|_| unavailable())?
        .rows_affected();
    let overdue = sqlx::query(OVERDUE)
        .bind(OVERDUE_MESSAGE)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(&mut *connection)
        .await
        .map_err(|_| unavailable())?
        .rows_affected();
    Ok((due_soon, overdue))
}

/// One serializable sweep of the caller's Holding database.
pub async fn run_once(
    write: &PgPool,
    now: i64,
    warning_window_ms: i64,
) -> Result<(u64, u64), BusinessError> {
    let mut tx = write.begin().await.map_err(|_| unavailable())?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .map_err(|_| unavailable())?;
    let result = sweep(&mut tx, now, warning_window_ms).await?;
    tx.commit().await.map_err(|_| unavailable())?;
    Ok(result)
}

#[cfg(feature = "private-field-api")]
pub(crate) async fn run(
    databases: Vec<crate::runtime_database::RuntimeDatabase>,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    let limits = PoolLimits {
        max_connections: 2,
        acquire_timeout: Duration::from_secs(5),
    };
    let mut pools = Vec::new();
    for database in databases {
        match database.connect(limits).await {
            Ok(pool) => pools.push(pool),
            Err(_) => {
                eprintln!("ACCESS_REVIEW_ALERTS_UNAVAILABLE");
                return;
            }
        }
    }
    loop {
        if *stop.borrow() {
            return;
        }
        let now = SystemAccountClock.now().get();
        for pool in &pools {
            if run_once(pool.raw(), now, WARNING_WINDOW_MS).await.is_err() {
                eprintln!("ACCESS_REVIEW_ALERTS_UNAVAILABLE");
            }
        }
        tokio::select! {
            _ = stop.changed() => return,
            _ = tokio::time::sleep(Duration::from_secs(120)) => {}
        }
    }
}
