//! Collaboration-owned Quality notices. The enclosing composition must attest
//! the saved Quality hold and complete current Identity recipient set.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
const MESSAGE: &str = "Quality handoff: review the source record";
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_NOTIFICATION_CHANGED",
        "The Quality notification source changed.",
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
        return Err(PersistenceError::new("QUALITY_NOTIFICATION_UNIT_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// Utility-only write intent; acquire along with source locks before snapshot.
pub async fn lock_quality_notifications(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(changed());
    }
    sqlx::query(
        "LOCK TABLE staff_notifications,inbox_events,inbox_deliveries IN ROW EXCLUSIVE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityNotificationFacts {
    pub command_key: String,
    pub company_id: String,
    pub hold_id: String,
    pub version: i64,
    pub updated_at: i64,
    pub recipients: Vec<String>,
}
/// Concrete composition implementation must retain and recheck the saved
/// Quality after-image and exact complete Identity recipient witness in this
/// unit. Supplying these facts alone never proves either.
pub trait PgQualityNotificationSource: Sync {
    fn facts(&self) -> &QualityNotificationFacts;
    fn require_current(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
async fn conflicts(
    c: &mut PgConnection,
    f: &QualityNotificationFacts,
    recipient: &str,
) -> Result<Vec<Value>, Error> {
    let id = format!("{}-{}", f.command_key, recipient);
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(n) FROM staff_notifications n WHERE n.id=$1 OR (n.user_id=$2 AND n.record_type='quality_hold' AND n.record_id=$3 AND n.record_version=$4 AND n.message=$5) ORDER BY n.id FOR SHARE")
        .bind(id).bind(recipient).bind(&f.hold_id).bind(f.version).bind(MESSAGE).fetch_all(c).await?;
    Ok(rows)
}
async fn bridge(c: &mut PgConnection, id: &str) -> Result<(Option<Value>, Option<Value>), Error> {
    let event = sqlx::query_scalar("SELECT to_jsonb(e) FROM inbox_events e WHERE id=$1 FOR SHARE")
        .bind(format!("legacy:{id}"))
        .fetch_optional(&mut *c)
        .await?;
    let delivery =
        sqlx::query_scalar("SELECT to_jsonb(d) FROM inbox_deliveries d WHERE id=$1 FOR SHARE")
            .bind(id)
            .fetch_optional(c)
            .await?;
    Ok((event, delivery))
}
/// Run only after the actual Quality hold effect, inside the caller's retained
/// transaction. Existing ID or unique-tuple conflicts remain no-ops as in TS.
pub async fn stage_quality_notifications<S: PgQualityNotificationSource>(
    c: &mut PgConnection,
    source: &S,
) -> Result<RetainedQualityNotifications, Error> {
    source.require_current(c).await?;
    let u = unit(c).await?;
    let f = source.facts();
    if f.command_key.is_empty()
        || f.company_id.is_empty()
        || f.hold_id.is_empty()
        || f.version < 1
        || f.updated_at < 0
        || f.recipients.iter().any(|r| r.is_empty())
    {
        return Err(changed());
    }
    let mut seen = std::collections::BTreeSet::new();
    if f.recipients.iter().any(|r| !seen.insert(r)) {
        return Err(changed());
    }
    let mut retained = Vec::with_capacity(f.recipients.len());
    for recipient in &f.recipients {
        let id = format!("{}-{}", f.command_key, recipient);
        let before = conflicts(c, f, recipient).await?;
        let bridge_before = bridge(c, &id).await?;
        let inserted=sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES($1,$2,$3,'quality_hold',$4,$5,$6,$7) ON CONFLICT DO NOTHING")
            .bind(&id).bind(&f.company_id).bind(recipient).bind(&f.hold_id).bind(f.version).bind(MESSAGE).bind(f.updated_at).execute(&mut *c).await?.rows_affected();
        if inserted != u64::from(before.is_empty()) {
            return Err(changed());
        }
        let after = conflicts(c, f, recipient).await?;
        let bridge_after = bridge(c, &id).await?;
        if inserted == 0 && (after != before || bridge_after != bridge_before) {
            return Err(changed());
        }
        if inserted == 1 {
            let expected = json!({"id":id,"company_id":f.company_id,"user_id":recipient,"record_type":"quality_hold","record_id":f.hold_id,"record_version":f.version,"message":MESSAGE,"created_at":f.updated_at,"read_at":null});
            if after != vec![expected] {
                return Err(changed());
            }
            let expected_event = json!({"id":format!("legacy:{id}"),"company_id":f.company_id,"event_key":format!("legacy:{id}"),"source_type":"quality_hold","source_id":f.hold_id,"source_version":f.version,"actor_user_id":null,"category":"work","preview":MESSAGE,"created_at":f.updated_at});
            let expected_delivery = json!({"id":id,"event_id":format!("legacy:{id}"),"company_id":f.company_id,"user_id":recipient,"delivered_at":f.updated_at,"read_at":null,"archived_at":null});
            if bridge_before.0.is_none() && bridge_after.0.as_ref() != Some(&expected_event)
                || bridge_before.1.is_none() && bridge_after.1.as_ref() != Some(&expected_delivery)
            {
                return Err(changed());
            }
            if bridge_before.0.is_some() && bridge_after.0 != bridge_before.0
                || bridge_before.1.is_some() && bridge_after.1 != bridge_before.1
            {
                return Err(changed());
            }
        }
        retained.push((recipient.clone(), after, bridge_after));
    }
    source.require_current(c).await?;
    Ok(RetainedQualityNotifications {
        unit: u,
        facts: f.clone(),
        retained,
    })
}
pub struct RetainedQualityNotifications {
    unit: Unit,
    facts: QualityNotificationFacts,
    retained: Vec<(String, Vec<Value>, (Option<Value>, Option<Value>))>,
}
impl RetainedQualityNotifications {
    pub async fn require_current<S: PgQualityNotificationSource>(
        &self,
        c: &mut PgConnection,
        source: &S,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit || source.facts() != &self.facts {
            return Err(changed());
        }
        source.require_current(c).await?;
        for (recipient, expected, expected_bridge) in &self.retained {
            if conflicts(c, &self.facts, recipient).await? != *expected
                || bridge(c, &format!("{}-{}", self.facts.command_key, recipient)).await?
                    != *expected_bridge
            {
                return Err(changed());
            }
        }
        Ok(())
    }
}
