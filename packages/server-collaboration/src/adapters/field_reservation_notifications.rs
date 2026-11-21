//! Collaboration-owned idempotent notices. Field source retention is mandatory.
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, PartialEq, Eq)]
pub struct FieldReservationNotice {
    pub identity: String,
    pub company_id: String,
    pub user_id: String,
    pub order_id: String,
    pub version: i64,
    pub phase: &'static str,
    pub message: &'static str,
    pub now: i64,
}
pub trait PgFieldReservationNoticeSource: Sync {
    fn notices(&self) -> &[FieldReservationNotice];
    fn require_current(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_RESERVATION_NOTIFICATION_CHANGED",
        "The original reservation notification changed.",
    )
    .into()
}
async fn image(c: &mut PgConnection, id: &str) -> Result<Value, Error> {
    sqlx::query_scalar("SELECT jsonb_build_array(to_jsonb(n),to_jsonb(e),to_jsonb(d)) FROM staff_notifications n JOIN inbox_events e ON e.id='legacy:'||n.id AND e.company_id=n.company_id JOIN inbox_deliveries d ON d.id=n.id AND d.event_id=e.id WHERE n.id=$1 FOR SHARE OF n,e,d").bind(id).fetch_optional(c).await?.ok_or_else(changed)
}
async fn unit(c: &mut PgConnection) -> Result<(String, i32, String, String), Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(changed());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub struct RetainedFieldReservationNotices {
    unit: (String, i32, String, String),
    pub inserted: u64,
    images: Vec<(String, Value)>,
}
impl RetainedFieldReservationNotices {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        for (id, expected) in &self.images {
            if image(c, id).await? != *expected {
                return Err(changed());
            }
        }
        Ok(())
    }
}
pub async fn write_field_reservation_notices<S: PgFieldReservationNoticeSource>(
    c: &mut PgConnection,
    source: &S,
) -> Result<RetainedFieldReservationNotices, Error> {
    source.require_current(c).await?;
    if !c.is_in_transaction() || source.notices().len() > 100 {
        return Err(changed());
    }
    sqlx::query("SELECT set_config('erpx.notification_runtime','native-rust',true)")
        .execute(&mut *c)
        .await?;
    let unit = unit(c).await?;
    let mut inserted = 0;
    let mut images = Vec::new();
    for n in source.notices() {
        if !matches!(n.phase, "expiring" | "expired")
            || !(1..=9_007_199_254_740_991).contains(&n.version)
            || !(0..=9_007_199_254_740_991).contains(&n.now)
            || !n.identity.starts_with("field:")
        {
            return Err(changed());
        }
        let id: String = sqlx::query_scalar("SELECT 'inventory-reservation-'||$1||'-'||md5($2)")
            .bind(n.phase)
            .bind(&n.identity)
            .fetch_one(&mut *c)
            .await?;
        let added=sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)VALUES($1,$2,$3,'field_service_order',$4,$5,$6,$7)ON CONFLICT DO NOTHING").bind(&id).bind(&n.company_id).bind(&n.user_id).bind(&n.order_id).bind(n.version).bind(n.message).bind(n.now).execute(&mut *c).await?.rows_affected();
        inserted += added;
        // Existing read/archive state and the original creation clock are retained.
        // A conflicting immutable identity must never count as successful delivery.
        let actual:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(n)-'read_at'-'created_at' FROM staff_notifications n WHERE id=$1 FOR SHARE").bind(&id).fetch_optional(&mut *c).await?;
        let expected = json!({"id":id,"company_id":n.company_id,"user_id":n.user_id,"record_type":"field_service_order","record_id":n.order_id,"record_version":n.version,"message":n.message});
        // An earlier alert is immutable even if the current source version advanced;
        // compare its original envelope to its own bridged event, then its stable IDs.
        let Some(actual) = actual else {
            return Err(changed());
        };
        if actual["id"] != expected["id"]
            || actual["company_id"] != expected["company_id"]
            || actual["user_id"] != expected["user_id"]
            || actual["record_type"] != expected["record_type"]
            || actual["record_id"] != expected["record_id"]
            || actual["message"] != expected["message"]
        {
            return Err(changed());
        }
        let intact:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM staff_notifications n JOIN inbox_events e ON e.id='legacy:'||n.id AND e.company_id=n.company_id JOIN inbox_deliveries d ON d.id=n.id AND d.event_id=e.id WHERE n.id=$1 AND e.event_key=e.id AND e.source_type=n.record_type AND e.source_id=n.record_id AND e.source_version=n.record_version AND e.actor_user_id IS NULL AND e.category='work' AND e.preview=n.message AND e.created_at=n.created_at AND d.company_id=n.company_id AND d.user_id=n.user_id AND d.delivered_at=n.created_at )").bind(&id).fetch_one(&mut *c).await?;
        if !intact {
            return Err(changed());
        }
        let full = image(c, &id).await?;
        if added == 1
            && (actual != expected
                || full[0]["created_at"] != n.now
                || !full[0]["read_at"].is_null()
                || !full[2]["read_at"].is_null()
                || !full[2]["archived_at"].is_null())
        {
            return Err(changed());
        }
        if !actual["record_version"]
            .as_i64()
            .is_some_and(|v| v >= 1 && v <= n.version)
        {
            return Err(changed());
        }
        images.push((id, full));
    }
    source.require_current(c).await?;
    Ok(RetainedFieldReservationNotices {
        unit,
        inserted,
        images,
    })
}
