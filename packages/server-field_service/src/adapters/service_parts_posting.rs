//! Field-owned billing history for the original Service posting setup image.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServicePartsFieldHistory {
    pub service_json: String,
    pub has_history: bool,
}
pub struct CurrentServicePartsFieldHistory {
    company: CompanyId,
    unit: Unit,
    data: ServicePartsFieldHistory,
}
impl CurrentServicePartsFieldHistory {
    pub fn data(&self) -> &ServicePartsFieldHistory {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || capture(c, &self.company).await? != self.data {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_service_parts_field_history(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE field_service_agreements,field_service_agreement_events,field_service_orders,field_service_order_events,field_service_part_reservations,field_service_billing_releases IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_service_parts_field_history(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<CurrentServicePartsFieldHistory, Error> {
    let unit = unit(c).await?;
    let data = capture(c, company).await?;
    Ok(CurrentServicePartsFieldHistory {
        company: company.clone(),
        unit,
        data,
    })
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<ServicePartsFieldHistory, Error> {
    let row=sqlx::query("WITH image AS (SELECT (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM field_service_billing_releases r JOIN field_service_orders o ON o.id=r.order_id WHERE o.company_id=$1)::text value) SELECT CASE WHEN octet_length(coalesce(value,'null'))<=2097152 THEN coalesce(value,'null') END value, value IS NOT NULL history FROM image").bind(company.as_str()).fetch_one(c).await?;
    Ok(ServicePartsFieldHistory {
        service_json: row
            .try_get::<Option<String>, _>("value")?
            .ok_or_else(invalid)?,
        has_history: row.try_get("history")?,
    })
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
fn invalid() -> Error {
    PersistenceError::new("FIELD_SERVICE_POSTING_HISTORY_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_PARTS_POSTING_SOURCE_CHANGED",
        "The original Service billing history changed.",
    )
    .into()
}
