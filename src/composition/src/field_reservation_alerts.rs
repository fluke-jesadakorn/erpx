//! Native Field scheduled alerts. Outer assembly supplies only enforced Holding
//! pools; Field retains its source and Collaboration is the sole notice writer.
use erpx_server_collaboration::public::field_reservation_notifications::{
    FieldReservationNotice, PgFieldReservationNoticeSource, write_field_reservation_notices,
};
use erpx_server_field_service::public::reservation_alerts::{
    CurrentFieldReservationAlerts, FieldReservationAlertPhase, lock_field_reservation_alerts,
    read_field_reservation_alerts,
};
use erpx_server_inventory::public::reservation_alerts::ReservationAlertPhase;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, PgPool};
type Error = PgTransactionError<BusinessError>;
struct AlertSource {
    field: CurrentFieldReservationAlerts,
    notices: Vec<FieldReservationNotice>,
}
impl PgFieldReservationNoticeSource for AlertSource {
    fn notices(&self) -> &[FieldReservationNotice] {
        &self.notices
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.field.require_current(c).await
    }
}
pub async fn run_once(write: &PgPool, now: i64) -> Result<(u64, u64), Error> {
    let horizon = now
        .checked_add(86_400_000)
        .filter(|v| *v <= 9_007_199_254_740_991)
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::Unavailable,
                "FIELD_RESERVATION_ALERT_CLOCK_INVALID",
                "The scheduled clock is unavailable.",
            )
        })?;
    let mut counts = [0, 0];
    for (index, phase, message) in [
        (
            0,
            FieldReservationAlertPhase::Expiring,
            ReservationAlertPhase::Expiring.message(),
        ),
        (
            1,
            FieldReservationAlertPhase::Expired,
            ReservationAlertPhase::Expired.message(),
        ),
    ] {
        let mut after = (String::new(), String::new());
        loop {
            let mut tx = write.begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut *tx)
                .await?;
            lock_field_reservation_alerts(&mut tx).await?;
            let field = read_field_reservation_alerts(&mut tx, phase, now, horizon, after).await?;
            let Some(next) = field.next() else {
                tx.commit().await?;
                break;
            };
            let notices = field
                .facts()
                .map(|f| FieldReservationNotice {
                    identity: format!("field:{}:{}:{}", f.reservation_id, f.expires_at, f.user_id),
                    company_id: f.company_id.clone(),
                    user_id: f.user_id.clone(),
                    order_id: f.order_id.clone(),
                    version: f.order_version,
                    phase: phase.as_str(),
                    message,
                    now,
                })
                .collect();
            let source = AlertSource { field, notices };
            let notices = write_field_reservation_notices(&mut tx, &source).await?;
            sqlx::raw_sql("SET CONSTRAINTS ALL IMMEDIATE")
                .execute(&mut *tx)
                .await?;
            source.field.require_current(&mut tx).await?;
            notices.require_current(&mut tx).await?;
            tx.commit().await?;
            counts[index] += notices.inserted;
            after = next;
        }
    }
    Ok((counts[0], counts[1]))
}
#[cfg(feature = "private-field-api")]
pub(crate) async fn run(writes: Vec<PgPool>) {
    loop {
        for write in &writes {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
                .unwrap_or(0);
            if run_once(write, now).await.is_err() {
                eprintln!("FIELD_RESERVATION_ALERTS_UNAVAILABLE");
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;
    }
}
