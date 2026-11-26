//! Native PostgreSQL execution for the Quality escalation queue read.
//!
//! This is the only layer that prepares and executes SQL. The statement is the
//! retained workspace escalation read: Quality alert/hold rows joined to the
//! current Commercial product and warehouse labels, with the live Identity
//! recipient existence probe and bounded recipient count. Cross-owner purchase
//! visibility remains a composition port and is never inferred from the wire.
use crate::application::quality_alert_queue::{
    QualityAlertQueueFuture, QualityAlertQueuePort, QualityAlertQueueQuery,
};
use crate::domain::quality_alert_queue::{AlertPhase, QualityAlertRow};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Explicit column projections shared with the retained source. Every join is
/// on a closed, fixed column list; no caller supplies SQL or a relation name.
const ASSIGNMENTS: &str = "SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments";
const PRODUCTS: &str = "SELECT \"id\",\"company_id\",\"sku\",\"name\",\"unit\",\"price_minor\",\"tax_bps\",\"status\",\"version\",\"tracking_mode\" FROM commerce_products";
const WAREHOUSES: &str = "SELECT \"id\",\"company_id\",\"branch_id\",\"code\",\"name\",\"status\",\"version\" FROM commerce_warehouses";
const RECIPIENT: &str = "m.company_id=h.company_id \
  AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
  AND iam_role_allows(m.role,m.company_id,CASE WHEN h.status IN ('awaiting_release','cancel_pending') THEN 'quality.release' ELSE 'quality.inspect' END) \
  AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=h.branch_id)) \
  AND NOT iam_same_person(m.user_id,h.created_by,h.company_id) \
  AND (h.status<>'cancel_pending' OR NOT iam_same_person(m.user_id,h.cancel_requested_by,h.company_id)) \
  AND (h.status<>'awaiting_release' OR NOT iam_same_person(m.user_id,h.last_inspector,h.company_id))";

/// `$1` actor, `$2` company, `$3` branch. The bounded `LIMIT 1001` lets the
/// application distinguish an unbounded snapshot from a complete queue. The
/// statement is assembled only from these closed literals; no caller text is
/// ever concatenated into it.
pub fn quality_alert_queue_sql() -> String {
    format!(
        "SELECT a.id,a.hold_id,a.phase,a.source_version,h.version hold_version,\
a.due_date::text due_date,a.detected_at,a.version,a.acknowledged_by,a.acknowledged_at,a.reason,\
h.company_id,h.branch_id,h.status,p.name product_name,w.name warehouse_name,\
CASE WHEN EXISTS(SELECT 1 FROM ({}) m WHERE {} AND m.user_id=$1) THEN 1 ELSE 0 END can_ack,\
(SELECT COUNT(DISTINCT m.user_id) FROM ({}) m WHERE {}) recipients\
 FROM inventory_quality_alerts a\
 JOIN inventory_quality_holds h ON h.id=a.hold_id\
 JOIN ({}) p ON p.id=h.product_id AND p.company_id=h.company_id\
 JOIN ({}) w ON w.id=h.warehouse_id AND w.company_id=h.company_id\
 WHERE h.company_id=$2 AND ($3::text IS NULL OR h.branch_id=$3)\
 ORDER BY a.due_date,CASE WHEN a.phase='overdue' THEN 0 ELSE 1 END,a.id LIMIT 1001",
        ASSIGNMENTS,
        RECIPIENT,
        ASSIGNMENTS,
        RECIPIENT,
        PRODUCTS,
        WAREHOUSES,
    )
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "QUALITY_ALERT_QUEUE_INVALID",
        "The Quality escalation source is invalid.",
    )
}

fn map(row: &sqlx::postgres::PgRow) -> Result<QualityAlertRow, Error> {
    let raw_phase: String = row.try_get("phase")?;
    let phase = AlertPhase::parse(&raw_phase).ok_or_else(invalid)?;
    Ok(QualityAlertRow {
        id: row.try_get("id")?,
        hold_id: row.try_get("hold_id")?,
        phase,
        source_version: row.try_get("source_version")?,
        hold_version: row.try_get("hold_version")?,
        due_date: row.try_get("due_date")?,
        detected_at: row.try_get("detected_at")?,
        version: row.try_get("version")?,
        acknowledged_by: row.try_get("acknowledged_by")?,
        acknowledged_at: row.try_get("acknowledged_at")?,
        reason: row.try_get("reason")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        status: row.try_get("status")?,
        product_name: row.try_get("product_name")?,
        warehouse_name: row.try_get("warehouse_name")?,
        can_ack: row.try_get("can_ack")?,
        recipients: row.try_get("recipients")?,
    })
}

/// Execute the queue read on the caller's already-open Holding unit. The
/// identity and commercial source locks belong to the surrounding composition
/// transaction, exactly as they do for the retained TypeScript read.
pub async fn read_quality_alert_queue(
    c: &mut PgConnection,
    query: &QualityAlertQueueQuery,
) -> Result<Vec<QualityAlertRow>, Error> {
    let sql = quality_alert_queue_sql();
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(query.actor.as_str())
        .bind(query.company.as_str())
        .bind(query.branch.as_ref().map(|branch| branch.as_str()))
        .fetch_all(&mut *c)
        .await?;
    rows.iter().map(map).collect()
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "QUALITY_ALERT_QUEUE_UNAVAILABLE",
            "The Quality escalation queue is unavailable.",
        ),
    }
}

/// Pooled entry for a composition backend that calls the owner read directly.
/// The query is cloned into the retryable unit; no value borrows the connection.
pub async fn read_quality_alert_queue_pooled(
    pool: DatabasePool,
    query: QualityAlertQueueQuery,
) -> Result<Vec<QualityAlertRow>, BusinessError> {
    PgUnitOfWork::new(pool)
        .run(TransactionOptions::default(), move |c| {
            let query = query.clone();
            Box::pin(async move { read_quality_alert_queue(c, &query).await })
        })
        .await
        .map_err(business)
}

/// Adapter implementing the owner port on a composition-owned transaction.
pub struct PgQualityAlertQueue<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgQualityAlertQueue<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
}

impl QualityAlertQueuePort for PgQualityAlertQueue<'_> {
    fn alerts<'a>(
        &'a mut self,
        query: &'a QualityAlertQueueQuery,
    ) -> QualityAlertQueueFuture<'a, Vec<QualityAlertRow>> {
        Box::pin(async move {
            read_quality_alert_queue(&mut *self.connection, query)
                .await
                .map_err(business)
        })
    }
}
