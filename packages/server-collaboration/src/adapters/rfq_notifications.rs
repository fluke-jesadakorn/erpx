//! Same-unit Collaboration effect; the source must retain Procurement and IAM.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn invalid() -> Error {
    PersistenceError::new("RFQ_NOTIFICATION_UNIT_REQUIRED").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "RFQ_NOTIFICATION_SOURCE_CHANGED",
        "The quotation notification source changed.",
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
pub async fn lock_rfq_notifications(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE public.staff_notifications IN ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
async fn fence(c: &mut PgConnection) -> Result<(), Error> {
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_class c JOIN pg_locks l ON l.relation=c.oid WHERE c.oid=to_regclass('public.staff_notifications') AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND NOT row_security_active(c.oid) AND l.pid=pg_backend_pid() AND l.granted AND l.mode IN ('RowExclusiveLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(c).await?;
    if !held {
        return Err(invalid());
    }
    Ok(())
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RfqNoticeFacts {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub version: i64,
    pub updated_at: i64,
}
/// Production implementation must read facts from Procurement's current
/// transaction-bound witness and recipients from Identity on this connection.
pub trait RfqNoticeSource: Sync {
    fn facts(&self) -> &RfqNoticeFacts;
    fn require_current(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn recipients(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<Vec<String>, Error>> + Send;
}
pub async fn notify_rfq_staff<S: RfqNoticeSource>(
    c: &mut PgConnection,
    key: &str,
    source: &S,
) -> Result<(), Error> {
    let id = unit(c).await?;
    fence(c).await?;
    let facts = source.facts().clone();
    source.require_current(c).await?;
    if source.facts() != &facts {
        return Err(changed());
    }
    let recipients = source.recipients(c).await?;
    sqlx::query("INSERT INTO public.staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT DISTINCT $1||'-'||recipient,$2,recipient,'procurement_rfq',$3,$4,'Supplier quotation review changed: review the next action',$5 FROM unnest($6::text[]) AS recipient ON CONFLICT DO NOTHING")
        .bind(key).bind(&facts.company_id).bind(&facts.id).bind(facts.version).bind(facts.updated_at).bind(&recipients).execute(&mut *c).await?;
    if unit(c).await? != id || source.facts() != &facts {
        return Err(changed());
    }
    fence(c).await?;
    source.require_current(c).await?;
    if source.facts() != &facts {
        return Err(changed());
    }
    Ok(())
}
