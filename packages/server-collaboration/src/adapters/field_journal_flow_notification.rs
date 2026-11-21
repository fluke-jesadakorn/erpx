//! Collaboration-owned immutable assignment notification image. The enclosing
//! native Field-cost journal command retains current source/recipient/Workflow authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_JOURNAL_FLOW_NOTICE_CHANGED",
        "The current journal responsibility notification changed.",
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
        return Err(PersistenceError::new("COLLABORATION_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub struct FieldJournalFlowNotice<'a> {
    pub id: &'a str,
    pub company: &'a str,
    pub recipient: &'a str,
    pub journal: &'a str,
    pub version: i64,
    pub message: &'a str,
    pub now: i64,
}
pub struct RetainedFieldJournalFlowNotice {
    unit: Unit,
    id: String,
    recipient: String,
    journal: String,
    version: i64,
    message: String,
    expected: Vec<Value>,
}
impl RetainedFieldJournalFlowNotice {
    async fn images(&self, c: &mut PgConnection) -> Result<Vec<Value>, Error> {
        Ok(sqlx::query_scalar("SELECT to_jsonb(n) FROM staff_notifications n WHERE id=$1 OR(user_id=$2 AND record_type='journal' AND record_id=$3 AND record_version=$4 AND message=$5) ORDER BY id LIMIT 3 FOR SHARE").bind(&self.id).bind(&self.recipient).bind(&self.journal).bind(self.version).bind(&self.message).fetch_all(c).await?)
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || self.images(c).await? != self.expected {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn stage_field_journal_flow_notice(
    c: &mut PgConnection,
    f: FieldJournalFlowNotice<'_>,
) -> Result<RetainedFieldJournalFlowNotice, Error> {
    if [f.id, f.company, f.recipient, f.journal, f.message]
        .iter()
        .any(|s| s.is_empty())
        || !(1..=9_007_199_254_740_991).contains(&f.version)
        || !(0..=9_007_199_254_740_991).contains(&f.now)
    {
        return Err(changed());
    }
    let mut retained = RetainedFieldJournalFlowNotice {
        unit: unit(c).await?,
        id: f.id.into(),
        recipient: f.recipient.into(),
        journal: f.journal.into(),
        version: f.version,
        message: f.message.into(),
        expected: Vec::new(),
    };
    let before = retained.images(c).await?;
    if before.len() > 1
        || before.iter().any(|n| {
            n["company_id"] != f.company
                || n["user_id"] != f.recipient
                || n["record_type"] != "journal"
                || n["record_id"] != f.journal
                || n["record_version"] != f.version
                || n["message"] != f.message
        })
    {
        return Err(changed());
    }
    let affected=sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)VALUES($1,$2,$3,'journal',$4,$5,$6,$7) ON CONFLICT DO NOTHING").bind(f.id).bind(f.company).bind(f.recipient).bind(f.journal).bind(f.version).bind(f.message).bind(f.now).execute(&mut *c).await?.rows_affected();
    if affected != u64::from(before.is_empty()) {
        return Err(changed());
    }
    retained.expected = if before.is_empty() {
        vec![
            json!({"id":f.id,"company_id":f.company,"user_id":f.recipient,"record_type":"journal","record_id":f.journal,"record_version":f.version,"message":f.message,"created_at":f.now,"read_at":null}),
        ]
    } else {
        before
    };
    if affected == 1 {
        let marker = json!({"database":retained.unit.0,"pid":retained.unit.1.to_string(),"xid":retained.unit.2,"started":retained.unit.3,"notice":retained.expected[0]});
        sqlx::query("SELECT set_config('erpx.field_journal_flow_notice',$1,true)")
            .bind(marker.to_string())
            .execute(&mut *c)
            .await?;
    }
    retained.require_current(c).await?;
    Ok(retained)
}

pub async fn lock_field_journal_flow_notices(c: &mut PgConnection) -> Result<(), Error> {
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
