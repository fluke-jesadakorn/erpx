//! Bounded Field-owned reservation candidates for the native scheduled worker.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct FieldReservationAlert {
    pub reservation_id: String,
    pub company_id: String,
    pub user_id: String,
    pub order_id: String,
    pub order_version: i64,
    pub expires_at: i64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FieldReservationAlertPhase {
    Expiring,
    Expired,
}
impl FieldReservationAlertPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Expiring => "expiring",
            Self::Expired => "expired",
        }
    }
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_RESERVATION_ALERT_SOURCE_CHANGED",
        "The original reservation notification source changed.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FIELD_RESERVATION_ALERT_UNIT_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_field_reservation_alerts(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(changed());
    }
    // Intent precedes the first snapshot; capture validates actual current facts.
    sqlx::query("LOCK TABLE field_service_orders,field_service_part_reservations IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
async fn capture(
    c: &mut PgConnection,
    phase: FieldReservationAlertPhase,
    now: i64,
    horizon: i64,
    after: &(String, String),
) -> Result<Vec<(FieldReservationAlert, String)>, Error> {
    let rows=sqlx::query("SELECT r.id reservation_id,o.company_id,recipient.user_id,o.id order_id,o.version order_version,r.expires_at,jsonb_build_array(r.order_id,r.created_by,r.quantity_milli,r.consumed_milli,r.released_milli,r.expires_at,o.company_id,o.status,o.version,o.technician_id,o.created_by)::text image FROM field_service_part_reservations r JOIN field_service_orders o ON o.id=r.order_id CROSS JOIN LATERAL(SELECT o.technician_id user_id UNION SELECT o.created_by UNION SELECT r.created_by) recipient WHERE r.quantity_milli>r.consumed_milli+r.released_milli AND (($1='expiring' AND r.expires_at>$2 AND r.expires_at<=$3) OR ($1='expired' AND r.expires_at<=$2)) AND o.status IN('open','scheduled','dispatched','in_visit','exception') AND (r.id,recipient.user_id)>($4,$5) ORDER BY r.id,recipient.user_id LIMIT 100").bind(phase.as_str()).bind(now).bind(horizon).bind(&after.0).bind(&after.1).fetch_all(c).await?;
    rows.into_iter()
        .map(|r| {
            let facts = FieldReservationAlert {
                reservation_id: r.try_get("reservation_id")?,
                company_id: r.try_get("company_id")?,
                user_id: r.try_get("user_id")?,
                order_id: r.try_get("order_id")?,
                order_version: r.try_get("order_version")?,
                expires_at: r.try_get("expires_at")?,
            };
            let image: String = r.try_get("image")?;
            if image.len() > 65536
                || facts.order_version < 1
                || facts.order_version > 9_007_199_254_740_991
                || [
                    &facts.reservation_id,
                    &facts.company_id,
                    &facts.user_id,
                    &facts.order_id,
                ]
                .iter()
                .any(|s| s.is_empty() || s.len() > 1024)
            {
                return Err(changed());
            }
            Ok((facts, image))
        })
        .collect()
}
pub struct CurrentFieldReservationAlerts {
    unit: Unit,
    phase: FieldReservationAlertPhase,
    now: i64,
    horizon: i64,
    after: (String, String),
    rows: Vec<(FieldReservationAlert, String)>,
}
impl CurrentFieldReservationAlerts {
    pub fn facts(&self) -> impl Iterator<Item = &FieldReservationAlert> {
        self.rows.iter().map(|(f, _)| f)
    }
    pub fn next(&self) -> Option<(String, String)> {
        self.rows
            .last()
            .map(|(r, _)| (r.reservation_id.clone(), r.user_id.clone()))
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(c, self.phase, self.now, self.horizon, &self.after).await? != self.rows
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_field_reservation_alerts(
    c: &mut PgConnection,
    phase: FieldReservationAlertPhase,
    now: i64,
    horizon: i64,
    after: (String, String),
) -> Result<CurrentFieldReservationAlerts, Error> {
    if !(0..=9_007_199_254_740_991).contains(&now)
        || horizon < now
        || horizon > 9_007_199_254_740_991
    {
        return Err(changed());
    }
    let unit = unit(c).await?;
    let rows = capture(c, phase, now, horizon, &after).await?;
    Ok(CurrentFieldReservationAlerts {
        unit,
        phase,
        now,
        horizon,
        after,
        rows,
    })
}
