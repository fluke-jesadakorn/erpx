//! Read-only owner source facts, fenced to the caller's serializable unit.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::record_source_contract::SourceRecord;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SOURCE_RECORD_CHANGED",
        "The owning source changed.",
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
        return Err(PersistenceError::new("RECORD_SOURCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<(SourceRecord, String)>, Error> {
    let row = sqlx::query(SQL)
        .bind(id)
        .bind(company.as_str())
        .fetch_optional(c)
        .await?;
    let Some(row) = row else { return Ok(None) };
    let raw: Option<String> = row.try_get("payload")?;
    let source = serde_json::from_str(&raw.ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "RECORD_SOURCE_LIMIT",
            "The source exceeds its bounded read.",
        )
    })?)
    .map_err(|_| changed())?;
    Ok(Some((source, row.try_get("digest")?)))
}
pub struct CurrentRecordSource {
    company: CompanyId,
    id: String,
    unit: Unit,
    image: (SourceRecord, String),
}
impl CurrentRecordSource {
    pub fn data(&self) -> &SourceRecord {
        &self.image.0
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(c, &self.company, &self.id).await?.as_ref() != Some(&self.image)
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_record_source(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<CurrentRecordSource>, Error> {
    let unit = unit(c).await?;
    Ok(capture(c, company, id)
        .await?
        .map(|image| CurrentRecordSource {
            company: company.clone(),
            id: id.into(),
            unit,
            image,
        }))
}
pub async fn lock_record_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("RECORD_SOURCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE commerce_orders IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
const SQL: &str = "WITH s AS MATERIALIZED(SELECT * FROM commerce_orders WHERE id=$1 AND company_id=$2 FOR SHARE),payload AS(SELECT jsonb_build_object('kind','commerce_order','id',s.id,'company_id',s.company_id,'branch_id',s.branch_id,'department_id',s.department_id,'version',s.version,'title',s.number,'status',s.state)::text value,to_jsonb(s)::text image FROM s) SELECT CASE WHEN octet_length(value)<=2097152 AND octet_length(image)<=2097152 THEN value END payload,encode(sha256(convert_to(image,'UTF8')),'hex') digest FROM payload";
