//! Collaboration-owned Count Flow notification effect; source authority and
//! the actual Workflow recipient are required from the same owner unit.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn invalid() -> Error {
    PersistenceError::new("COUNT_FLOW_NOTIFICATION_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "COUNT_FLOW_NOTIFICATION_CHANGED",
        "The original Count Flow notification changed.",
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
pub async fn lock_count_flow_notifications(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE staff_notifications IN ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountFlowNotificationFacts {
    pub id: String,
    pub company_id: String,
    pub recipient: String,
    pub count_id: String,
    pub version: i64,
    pub message: String,
    pub created_at: i64,
}
pub trait PgCountFlowNotificationSource: Sync {
    fn facts(&self) -> &CountFlowNotificationFacts;
    fn require_before(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_after(&self, c: &mut PgConnection)
    -> impl Future<Output = Result<(), Error>> + Send;
}
pub struct PreparedCountFlowNotification<'a, S: PgCountFlowNotificationSource> {
    source: &'a S,
    unit: Unit,
    expected: String,
    prior: Vec<String>,
}
pub async fn prepare_count_flow_notification<'a, S: PgCountFlowNotificationSource>(
    c: &mut PgConnection,
    source: &'a S,
) -> Result<PreparedCountFlowNotification<'a, S>, Error> {
    source.require_before(c).await?;
    let f = source.facts();
    if f.id.is_empty()
        || f.company_id.is_empty()
        || f.recipient.is_empty()
        || f.count_id.is_empty()
        || f.version <= 0
        || f.message.is_empty()
    {
        return Err(invalid());
    }
    let expected=serde_json::json!({"id":f.id,"company_id":f.company_id,"user_id":f.recipient,"record_type":"stock_count","record_id":f.count_id,"record_version":f.version,"message":f.message,"created_at":f.created_at,"read_at":null}).to_string();
    let transaction = unit(c).await?;
    let prior = conflicts(c, f).await?;
    Ok(PreparedCountFlowNotification {
        source,
        unit: transaction,
        expected,
        prior,
    })
}
impl<S: PgCountFlowNotificationSource> PreparedCountFlowNotification<'_, S> {
    pub async fn persist(
        self,
        c: &mut PgConnection,
    ) -> Result<RetainedCountFlowNotification, Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        self.source.require_before(c).await?;
        let f = self.source.facts();
        if conflicts(c, f).await? != self.prior {
            return Err(changed());
        }
        let inserted=sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES($1,$2,$3,'stock_count',$4,$5,$6,$7) ON CONFLICT DO NOTHING").bind(&f.id).bind(&f.company_id).bind(&f.recipient).bind(&f.count_id).bind(f.version).bind(&f.message).bind(f.created_at).execute(&mut *c).await?.rows_affected();
        if inserted != u64::from(self.prior.is_empty()) {
            return Err(changed());
        }
        self.source.require_after(c).await?;
        let result = RetainedCountFlowNotification {
            unit: self.unit,
            facts: f.clone(),
            expected: if self.prior.is_empty() {
                vec![self.expected]
            } else {
                self.prior
            },
        };
        result.require_current(c).await?;
        Ok(result)
    }
}
pub struct RetainedCountFlowNotification {
    unit: Unit,
    facts: CountFlowNotificationFacts,
    expected: Vec<String>,
}
impl RetainedCountFlowNotification {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let actual = conflicts(c, &self.facts).await?;
        let parse = |rows: Vec<String>| -> Result<Vec<serde_json::Value>, Error> {
            rows.into_iter()
                .map(|v| serde_json::from_str(&v).map_err(|_| changed()))
                .collect()
        };
        if parse(actual)? != parse(self.expected.clone())? {
            return Err(changed());
        }
        Ok(())
    }
}
async fn conflicts(
    c: &mut PgConnection,
    f: &CountFlowNotificationFacts,
) -> Result<Vec<String>, Error> {
    let rows:Vec<Option<String>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(n)::text)<=2097152 THEN to_jsonb(n)::text END FROM staff_notifications n WHERE id=$1 OR (user_id=$2 AND record_type='stock_count' AND record_id=$3 AND record_version=$4 AND message=$5) ORDER BY id LIMIT 3").bind(&f.id).bind(&f.recipient).bind(&f.count_id).bind(f.version).bind(&f.message).fetch_all(c).await?;
    if rows.len() > 2 {
        return Err(invalid());
    }
    rows.into_iter().map(|v| v.ok_or_else(invalid)).collect()
}
