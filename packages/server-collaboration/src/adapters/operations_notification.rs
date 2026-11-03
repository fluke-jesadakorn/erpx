//! Collaboration owns operational notifications. The enclosing Operations unit
//! must retain its source, recipient, lifecycle and Workflow witnesses.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("OPERATIONS_NOTIFICATION_UNIT_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "OPERATIONS_NOTIFICATION_CHANGED",
        "The operational notification changed.",
    )
    .into()
}
async fn image(c: &mut PgConnection, id: &str) -> Result<Option<Value>, Error> {
    Ok(
        sqlx::query_scalar("SELECT to_jsonb(n) FROM staff_notifications n WHERE id=$1 FOR SHARE")
            .bind(id)
            .fetch_optional(c)
            .await?,
    )
}
pub struct RetainedOperationsNotification {
    unit: Unit,
    id: String,
    expected: Value,
}
impl RetainedOperationsNotification {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || image(c, &self.id).await?.as_ref() != Some(&self.expected)
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// Only own notification rows are written. This function does not establish
/// source visibility or recipient eligibility and never opens a transaction.
pub async fn stage_operations_notification(
    c: &mut PgConnection,
    key: &CommandKey,
    company: &CompanyId,
    recipient: &str,
    record: &str,
    version: i64,
    now: i64,
) -> Result<RetainedOperationsNotification, Error> {
    let unit = unit(c).await?;
    if recipient.is_empty() || record.is_empty() || !(1..=9_007_199_254_740_991).contains(&version) {
        return Err(changed());
    }
    let id = format!("operations-notice-{}", key.as_str());
    let message = if version == 1 {
        "An operational exception needs acknowledgement"
    } else {
        "Operational exception updated; review its current next step"
    };
    let expected = json!({"id":id,"company_id":company.as_str(),"user_id":recipient,"record_type":"operational_exception","record_id":record,"record_version":version,"message":message,"created_at":now,"read_at":null});
    sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)VALUES($1,$2,$3,'operational_exception',$4,$5,$6,$7)").bind(&id).bind(company.as_str()).bind(recipient).bind(record).bind(version).bind(message).bind(now).execute(&mut *c).await?;
    let retained = RetainedOperationsNotification { unit, id, expected };
    retained.require_current(c).await?;
    Ok(retained)
}
