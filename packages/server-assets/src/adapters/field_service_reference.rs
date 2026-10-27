//! Original minimal physical Asset reference for an existing Service source.
//! The caller's actual order/source scope and personal authority are separate.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId, RecordId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Eq, PartialEq)]
pub struct FieldServiceAssetFacts {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub asset_id: RecordId,
    pub branch_id: RecordId,
    pub department_id: Option<RecordId>,
    pub lifecycle_status: String,
    pub warranty_end: Option<String>,
    pub version: i64,
}
pub struct CurrentFieldServiceAssetReference {
    facts: FieldServiceAssetFacts,
    unit: Unit,
}
impl CurrentFieldServiceAssetReference {
    pub fn facts(&self) -> &FieldServiceAssetFacts {
        &self.facts
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || read(
                c,
                &self.facts.holding_id,
                &self.facts.company_id,
                &self.facts.asset_id,
            )
            .await?
            .as_ref()
                != Some(&self.facts)
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_field_service_asset_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE operational_assets IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_current_field_service_asset(
    c: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    id: &RecordId,
) -> Result<Option<CurrentFieldServiceAssetReference>, Error> {
    let unit = unit(c).await?;
    Ok(read(c, holding, company, id)
        .await?
        .map(|facts| CurrentFieldServiceAssetReference { facts, unit }))
}
async fn read(
    c: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    id: &RecordId,
) -> Result<Option<FieldServiceAssetFacts>, Error> {
    // Existing source joins by Company/branch only. Department and lifecycle
    // are retained facts, not a newly added existing-parts eligibility filter.
    let row=sqlx::query("SELECT CASE WHEN octet_length(branch_id)<=200 THEN branch_id END branch_id,CASE WHEN octet_length(department_id)<=200 THEN department_id END department_id,octet_length(department_id)::bigint department_bytes,CASE WHEN octet_length(lifecycle_status)<=32 THEN lifecycle_status END lifecycle_status,CASE WHEN octet_length(warranty_end::text)<=200 THEN warranty_end::text END warranty_end,octet_length(warranty_end::text)::bigint warranty_bytes,version::bigint version FROM operational_assets WHERE id=$1 AND company_id=$2 FOR SHARE")
        .bind(id.as_str()).bind(company.as_str()).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    for field in ["department_bytes", "warranty_bytes"] {
        if row
            .try_get::<Option<i64>, _>(field)?
            .is_some_and(|n| n > 200)
        {
            return Err(invalid());
        }
    }
    Ok(Some(FieldServiceAssetFacts {
        holding_id: holding.clone(),
        company_id: company.clone(),
        asset_id: id.clone(),
        branch_id: RecordId::new(
            row.try_get::<Option<String>, _>("branch_id")?
                .ok_or_else(invalid)?,
        )?,
        department_id: row
            .try_get::<Option<String>, _>("department_id")?
            .map(RecordId::new)
            .transpose()?,
        lifecycle_status: row
            .try_get::<Option<String>, _>("lifecycle_status")?
            .ok_or_else(invalid)?,
        warranty_end: row.try_get("warranty_end")?,
        version: row.try_get("version")?,
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
    PersistenceError::new("ASSETS_FIELD_SERVICE_REFERENCE_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "ASSETS_FIELD_SERVICE_REFERENCE_CHANGED",
        "The original Service physical Asset reference changed.",
    )
    .into()
}
