//! Exact original Service customer reference, not Finance management access.
//! Current source identity does not invent an active-status/new-work gate.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Eq, PartialEq)]
pub struct FieldServiceCustomerFacts {
    pub company_id: CompanyId,
    pub id: RecordId,
    pub kind: String,
    pub status: String,
    pub currency: String,
}
pub struct CurrentFieldServiceCustomer {
    facts: FieldServiceCustomerFacts,
    unit: Unit,
}
impl CurrentFieldServiceCustomer {
    pub fn facts(&self) -> &FieldServiceCustomerFacts {
        &self.facts
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || read(c, &self.facts.company_id, &self.facts.id)
                .await?
                .as_ref()
                != Some(&self.facts)
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_field_service_customer_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE finance_parties IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_current_field_service_customer(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &RecordId,
) -> Result<Option<CurrentFieldServiceCustomer>, Error> {
    let unit = unit(c).await?;
    Ok(read(c, company, id)
        .await?
        .map(|facts| CurrentFieldServiceCustomer { facts, unit }))
}
async fn read(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &RecordId,
) -> Result<Option<FieldServiceCustomerFacts>, Error> {
    let row=sqlx::query("SELECT CASE WHEN octet_length(kind)<=32 THEN kind END kind,CASE WHEN octet_length(status)<=32 THEN status END status,CASE WHEN octet_length(currency)<=16 THEN currency END currency FROM finance_parties WHERE id=$1 AND company_id=$2 AND kind='customer' FOR SHARE")
        .bind(id.as_str()).bind(company.as_str()).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    Ok(Some(FieldServiceCustomerFacts {
        company_id: company.clone(),
        id: id.clone(),
        kind: row
            .try_get::<Option<String>, _>("kind")?
            .ok_or_else(invalid)?,
        status: row
            .try_get::<Option<String>, _>("status")?
            .ok_or_else(invalid)?,
        currency: row
            .try_get::<Option<String>, _>("currency")?
            .ok_or_else(invalid)?,
    }))
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn invalid() -> Error {
    PersistenceError::new("FINANCE_FIELD_SERVICE_CUSTOMER_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FINANCE_FIELD_SERVICE_CUSTOMER_CHANGED",
        "The original Service customer reference changed.",
    )
    .into()
}
