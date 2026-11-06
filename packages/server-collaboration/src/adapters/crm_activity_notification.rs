//! Collaboration-owned notification effect. The CRM owner must attest actual
//! persisted source facts, selected authority and recipient in this same unit.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
use std::future::Future;

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn invalid() -> Error {
    PersistenceError::new("CRM_ACTIVITY_NOTIFICATION_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "CRM_ACTIVITY_NOTIFICATION_CHANGED",
        "The original CRM activity notification changed.",
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
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// Acquire before the enclosing transaction's first snapshot.
pub async fn lock_crm_activity_notifications(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE staff_notifications IN ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrmActivityNotificationFacts {
    pub command_key: String,
    pub company_id: String,
    pub recipient: String,
    pub activity_id: String,
    pub version: i64,
    pub status: String,
    pub updated_at: i64,
}
impl CrmActivityNotificationFacts {
    fn id(&self) -> String {
        format!("crm-activity-notice-{}", self.command_key)
    }
    fn message(&self) -> &'static str {
        if self.status == "open" {
            "CRM activity assigned: review the due date and next action"
        } else {
            "CRM activity outcome recorded"
        }
    }
}
/// Composition supplies facts from the real persisted CRM activity and checks
/// them before/after the effect; these values alone grant no permission.
pub trait PgCrmActivityNotificationSource: Sync {
    fn facts(&self) -> &CrmActivityNotificationFacts;
    fn require_before(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_after(&self, c: &mut PgConnection)
    -> impl Future<Output = Result<(), Error>> + Send;
}
pub struct PreparedCrmActivityNotification<'a, S: PgCrmActivityNotificationSource> {
    source: &'a S,
    unit: Unit,
    expected: serde_json::Value,
    prior: Vec<serde_json::Value>,
}
pub async fn prepare_crm_activity_notification<'a, S: PgCrmActivityNotificationSource>(
    c: &mut PgConnection,
    source: &'a S,
) -> Result<PreparedCrmActivityNotification<'a, S>, Error> {
    source.require_before(c).await?;
    let f = source.facts();
    if f.command_key.is_empty()
        || f.company_id.is_empty()
        || f.recipient.is_empty()
        || f.activity_id.is_empty()
        || f.version <= 0
        || !matches!(f.status.as_str(), "open" | "completed" | "cancelled")
    {
        return Err(invalid());
    }
    let expected = serde_json::json!({"id":f.id(),"company_id":f.company_id,"user_id":f.recipient,"record_type":"crm_activity","record_id":f.activity_id,"record_version":f.version,"message":f.message(),"created_at":f.updated_at,"read_at":null});
    let unit = unit(c).await?;
    let prior = conflicts(c, f).await?;
    Ok(PreparedCrmActivityNotification {
        source,
        unit,
        expected,
        prior,
    })
}
impl<S: PgCrmActivityNotificationSource> PreparedCrmActivityNotification<'_, S> {
    pub async fn persist(
        self,
        c: &mut PgConnection,
    ) -> Result<RetainedCrmActivityNotification, Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        self.source.require_before(c).await?;
        let f = self.source.facts();
        if conflicts(c, f).await? != self.prior {
            return Err(changed());
        }
        let inserted = sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES($1,$2,$3,'crm_activity',$4,$5,$6,$7) ON CONFLICT DO NOTHING")
            .bind(f.id()).bind(&f.company_id).bind(&f.recipient).bind(&f.activity_id).bind(f.version).bind(f.message()).bind(f.updated_at).execute(&mut *c).await?.rows_affected();
        if inserted != u64::from(self.prior.is_empty()) {
            return Err(changed());
        }
        self.source.require_after(c).await?;
        let retained = RetainedCrmActivityNotification {
            unit: self.unit,
            facts: f.clone(),
            expected: if self.prior.is_empty() {
                vec![self.expected]
            } else {
                self.prior
            },
        };
        retained.require_current(c).await?;
        Ok(retained)
    }
}
pub struct RetainedCrmActivityNotification {
    unit: Unit,
    facts: CrmActivityNotificationFacts,
    expected: Vec<serde_json::Value>,
}
impl RetainedCrmActivityNotification {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || conflicts(c, &self.facts).await? != self.expected {
            return Err(changed());
        }
        Ok(())
    }
}
async fn conflicts(
    c: &mut PgConnection,
    f: &CrmActivityNotificationFacts,
) -> Result<Vec<serde_json::Value>, Error> {
    let rows: Vec<Option<serde_json::Value>> = sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(n)::text)<=2097152 THEN to_jsonb(n) END FROM staff_notifications n WHERE id=$1 OR (user_id=$2 AND record_type='crm_activity' AND record_id=$3 AND record_version=$4 AND message=$5) ORDER BY id LIMIT 3")
        .bind(f.id()).bind(&f.recipient).bind(&f.activity_id).bind(f.version).bind(f.message()).fetch_all(c).await?;
    if rows.len() > 2 {
        return Err(invalid());
    }
    rows.into_iter()
        .map(|row| row.ok_or_else(invalid))
        .collect()
}
