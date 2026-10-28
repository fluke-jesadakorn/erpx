//! Existing Support source for a Service order. No mutation or claim is implied.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExistingServiceTicketFacts {
    pub id: String,
    pub company_id: CompanyId,
    pub branch_id: RecordId,
    pub department_id: Option<RecordId>,
    pub customer_id: RecordId,
    pub version: i64,
    pub status: String,
    pub source_kind: String,
    pub sales_order_id: Option<String>,
    pub service_agreement_id: Option<String>,
    pub service_order_id: Option<String>,
}
pub struct CurrentExistingServiceTicket {
    facts: ExistingServiceTicketFacts,
    unit: Unit,
    image: String,
}
impl CurrentExistingServiceTicket {
    pub fn facts(&self) -> &ExistingServiceTicketFacts {
        &self.facts
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let value = capture(c, &self.facts.company_id, &self.facts.id)
            .await?
            .ok_or_else(changed)?;
        if value != (self.facts.clone(), self.image.clone()) {
            return Err(changed());
        }
        Ok(())
    }
    pub fn require_new_work(&self, expected_version: i64) -> Result<(), Error> {
        if self.facts.version != expected_version
            || !matches!(
                self.facts.status.as_str(),
                "open" | "in_progress" | "waiting_customer"
            )
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_existing_service_ticket(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<CurrentExistingServiceTicket>, Error> {
    let unit = unit(c).await?;
    Ok(capture(c, company, id)
        .await?
        .map(|(facts, image)| CurrentExistingServiceTicket { facts, unit, image }))
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<(ExistingServiceTicketFacts, String)>, Error> {
    let row=sqlx::query("SELECT b.bounded,CASE WHEN b.bounded THEN t.branch_id END branch_id,CASE WHEN b.bounded THEN t.department_id END department_id,CASE WHEN b.bounded THEN t.customer_id END customer_id,CASE WHEN b.bounded THEN t.status END status,CASE WHEN b.bounded THEN t.source_kind END source_kind,CASE WHEN b.bounded THEN t.order_id END sales_order_id,CASE WHEN b.bounded THEN t.service_agreement_id END service_agreement_id,CASE WHEN b.bounded THEN t.service_order_id END service_order_id,t.version::bigint version,encode(sha256(convert_to(to_jsonb(t)::text,'UTF8')),'hex') image FROM support_tickets t CROSS JOIN LATERAL (SELECT octet_length(t.branch_id)<=200 AND coalesce(octet_length(t.department_id),0)<=200 AND octet_length(t.customer_id)<=200 AND octet_length(t.status)<=32 AND octet_length(t.source_kind)<=32 AND octet_length(t.branch_id)::bigint+coalesce(octet_length(t.department_id),0)::bigint+octet_length(t.customer_id)::bigint+octet_length(t.status)::bigint+octet_length(t.source_kind)::bigint+coalesce(octet_length(t.order_id),0)::bigint+coalesce(octet_length(t.service_agreement_id),0)::bigint+coalesce(octet_length(t.service_order_id),0)::bigint<=2097152 AS bounded) b WHERE t.id=$1 AND t.company_id=$2 AND t.branch_id IS NOT NULL AND t.customer_id IS NOT NULL FOR SHARE OF t").bind(id).bind(company.as_str()).fetch_optional(c).await?;
    let Some(r) = row else { return Ok(None) };
    if !r.try_get::<bool, _>("bounded")? {
        return Err(limit());
    }
    let required = |name| -> Result<String, Error> {
        r.try_get::<Option<String>, _>(name)?.ok_or_else(changed)
    };
    let facts = ExistingServiceTicketFacts {
        id: id.into(),
        company_id: company.clone(),
        branch_id: RecordId::new(required("branch_id")?)?,
        department_id: r
            .try_get::<Option<String>, _>("department_id")?
            .map(RecordId::new)
            .transpose()?,
        customer_id: RecordId::new(required("customer_id")?)?,
        version: r.try_get("version")?,
        status: required("status")?,
        source_kind: required("source_kind")?,
        sales_order_id: r.try_get("sales_order_id")?,
        service_agreement_id: r.try_get("service_agreement_id")?,
        service_order_id: r.try_get("service_order_id")?,
    };
    if !(1..=9_007_199_254_740_991).contains(&facts.version) {
        return Err(changed());
    }
    Ok(Some((facts, r.try_get("image")?)))
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("SUPPORT_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_SOURCE_CHANGED",
        "The original Support ticket changed.",
    )
    .into()
}
fn limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_ORDER_SOURCE_LIMIT",
        "The Service ticket exceeds this bounded read.",
    )
    .into()
}
