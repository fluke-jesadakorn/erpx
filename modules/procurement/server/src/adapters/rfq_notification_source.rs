//! RFQ-owned notification source; no IAM authority or notification write.
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

fn invalid() -> Error {
    PersistenceError::new("RFQ_NOTICE_UNIT_REQUIRED").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "RFQ_NOTICE_SOURCE_CHANGED",
        "The quotation request changed during notification.",
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
    let locked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_class c JOIN pg_locks l ON l.relation=c.oid WHERE c.oid=to_regclass('public.procurement_rfqs') AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND NOT row_security_active(c.oid) AND l.pid=pg_backend_pid() AND l.granted AND l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(&mut *c).await?;
    if !locked {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RfqNotificationFacts {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub version: i64,
    pub updated_at: i64,
}
async fn read(
    c: &mut PgConnection,
    id: &str,
    company: &str,
) -> Result<Option<RfqNotificationFacts>, Error> {
    let row: Option<(String,String,String,Option<String>,i64,i64)> = sqlx::query_as("SELECT id,company_id,branch_id,department_id,version,updated_at FROM public.procurement_rfqs WHERE id=$1 AND company_id=$2").bind(id).bind(company).fetch_optional(c).await?;
    Ok(row.map(
        |(id, company_id, branch_id, department_id, version, updated_at)| RfqNotificationFacts {
            id,
            company_id,
            branch_id,
            department_id,
            version,
            updated_at,
        },
    ))
}
pub struct RfqNotificationSource {
    unit: Unit,
    facts: RfqNotificationFacts,
}
impl RfqNotificationSource {
    pub fn facts(&self) -> &RfqNotificationFacts {
        &self.facts
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || read(c, &self.facts.id, &self.facts.company_id)
                .await?
                .as_ref()
                != Some(&self.facts)
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// The caller supplies its already-authorized Company and expected version.
/// This witness is bound to the current transaction and source lock.
pub async fn bind_rfq_notification_source(
    c: &mut PgConnection,
    company: &str,
    id: &str,
    version: i64,
) -> Result<RfqNotificationSource, Error> {
    let unit = unit(c).await?;
    let facts = read(c, id, company).await?.ok_or_else(changed)?;
    if facts.version != version {
        return Err(changed());
    }
    Ok(RfqNotificationSource { unit, facts })
}
