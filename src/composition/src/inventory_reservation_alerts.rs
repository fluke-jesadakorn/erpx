//! Native scheduled inventory reservation alerts for Sales orders and
//! Manufacturing orders. Field reservations retain their own source-qualified
//! alert job; this worker covers the remaining reservation sources and writes
//! the same deduplicated staff notifications.
use erpx_server_inventory::public::reservation_alerts::ReservationAlertPhase;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgPool;

type Error = PgTransactionError<BusinessError>;

const SALES_ALERT: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT 'inventory-reservation-'||$1||'-'||md5('sales:'||r.id||':'||r.expires_at::text||':'||recipient.user_id),o.company_id,recipient.user_id,'commerce_order',o.id,o.version,$2,$3 FROM inventory_order_reservations r JOIN commerce_order_lines l ON l.id=r.order_line_id JOIN commerce_orders o ON o.id=l.order_id CROSS JOIN LATERAL(SELECT o.created_by user_id UNION SELECT r.created_by) recipient WHERE r.quantity_milli>r.consumed_milli+r.released_milli AND o.state IN ('confirmed','partially_fulfilled') AND ($6::boolean IS FALSE OR EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=o.company_id AND f.module_id='inventory' AND f.rollout_state IN ('pilot','available'))) AND (($1='expiring' AND r.expires_at>$4 AND r.expires_at<=$5) OR ($1='expired' AND r.expires_at<=$4)) ON CONFLICT DO NOTHING";

const MANUFACTURING_ALERT: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT 'inventory-reservation-'||$1||'-'||md5('manufacturing:'||r.id||':'||r.expires_at::text||':'||recipient.user_id),o.company_id,recipient.user_id,'manufacturing_order',o.id,o.version,$2,$3 FROM manufacturing_material_reservations r JOIN manufacturing_orders o ON o.id=r.order_id CROSS JOIN LATERAL(SELECT o.created_by user_id UNION SELECT r.created_by) recipient WHERE r.quantity_milli>r.consumed_milli+r.released_milli AND o.status='reserved' AND ($6::boolean IS FALSE OR EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=o.company_id AND f.module_id='inventory' AND f.rollout_state IN ('pilot','available'))) AND (($1='expiring' AND r.expires_at>$4 AND r.expires_at<=$5) OR ($1='expired' AND r.expires_at<=$4)) ON CONFLICT DO NOTHING";

fn clock(now: i64) -> Result<i64, Error> {
    now.checked_add(86_400_000)
        .filter(|value| *value <= 9_007_199_254_740_991)
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::Unavailable,
                "INVENTORY_RESERVATION_ALERT_CLOCK_INVALID",
                "The scheduled clock is unavailable.",
            )
            .into()
        })
}

/// One serializable sweep for the Sales and Manufacturing reservation sources.
/// `production` selects the existing inventory rollout gate; development
/// matches the owner contract's `TRUE` predicate.
pub async fn run_once(write: &PgPool, now: i64, production: bool) -> Result<(u64, u64), Error> {
    let horizon = clock(now)?;
    let mut counts = [0_u64, 0_u64];
    for (index, phase) in [
        ReservationAlertPhase::Expiring,
        ReservationAlertPhase::Expired,
    ]
    .into_iter()
    .enumerate()
    {
        let mut tx = write.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await?;
        sqlx::query("LOCK TABLE inventory_order_reservations,commerce_order_lines,commerce_orders,manufacturing_material_reservations,manufacturing_orders,company_feature_rollouts,staff_notifications IN SHARE MODE")
            .execute(&mut *tx)
            .await?;
        let phase_name = match phase {
            ReservationAlertPhase::Expiring => "expiring",
            ReservationAlertPhase::Expired => "expired",
        };
        for statement in [SALES_ALERT, MANUFACTURING_ALERT] {
            let result = sqlx::query(statement)
                .bind(phase_name)
                .bind(phase.message())
                .bind(now)
                .bind(now)
                .bind(horizon)
                .bind(production)
                .execute(&mut *tx)
                .await?;
            counts[index] += result.rows_affected();
        }
        sqlx::raw_sql("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }
    Ok((counts[0], counts[1]))
}

#[cfg(feature = "private-field-api")]
pub(crate) async fn run(writes: Vec<PgPool>) {
    loop {
        for write in &writes {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
                .unwrap_or(0);
            let production = std::env::var("ERPX_ENVIRONMENT").as_deref() == Ok("production")
                || std::env::var("NODE_ENV").as_deref() == Ok("production");
            if run_once(write, now, production).await.is_err() {
                eprintln!("INVENTORY_RESERVATION_ALERTS_UNAVAILABLE");
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;
    }
}
