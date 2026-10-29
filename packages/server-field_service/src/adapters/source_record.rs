//! Field-owned single-order source projection for Records.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::Deserialize;
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct FieldRecordAgreement {
    pub branch_id: String,
    pub department_id: Option<String>,
    pub customer_id: String,
    pub asset_id: Option<String>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct FieldRecordSource {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub version: i64,
    pub title: String,
    pub status: String,
    pub ticket_id: String,
    pub asset_id: Option<String>,
    pub agreement: Option<FieldRecordAgreement>,
}
fn missing() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "SOURCE_RECORD_NOT_FOUND",
        "Source record is unavailable in this work scope.",
    )
    .into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SOURCE_RECORD_CHANGED",
        "The source version changed. Reload its owning workspace before handing it off.",
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
        return Err(PersistenceError::new("FIELD_SERVICE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_source_record(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FIELD_SERVICE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE field_service_orders,field_service_agreements IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
async fn image(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
    id: &str,
) -> Result<(FieldRecordSource, String), Error> {
    let r=sqlx::query("WITH source AS MATERIALIZED (SELECT o.*,to_jsonb(g) agreement FROM field_service_orders o LEFT JOIN field_service_agreements g ON g.id=o.agreement_id AND g.company_id=o.company_id WHERE o.id=$1 AND o.company_id=$2 AND ($3::text IS NULL OR o.branch_id=$3) AND ($4::text IS NULL OR o.department_id=$4)), valueset AS (SELECT jsonb_build_object('id',id,'company_id',company_id,'branch_id',branch_id,'department_id',department_id,'version',version,'title',issue,'status',status,'ticket_id',ticket_id,'asset_id',asset_id,'agreement',CASE WHEN agreement IS NULL THEN NULL ELSE jsonb_build_object('branch_id',agreement->>'branch_id','department_id',agreement->>'department_id','customer_id',agreement->>'customer_id','asset_id',agreement->>'asset_id') END) payload,to_jsonb(source) full_image FROM source) SELECT CASE WHEN octet_length(payload::text)+octet_length(full_image::text)<=2097152 THEN payload::text END payload,encode(sha256(convert_to(full_image::text,'UTF8')),'hex') digest FROM valueset").bind(id).bind(company.as_str()).bind(branch).bind(department).fetch_optional(c).await?.ok_or_else(missing)?;
    let value: Option<String> = r.try_get("payload")?;
    let data = serde_json::from_str(&value.ok_or_else(changed)?).map_err(|_| changed())?;
    Ok((data, r.try_get("digest")?))
}
pub struct CurrentFieldRecordSource {
    company: CompanyId,
    branch: Option<String>,
    department: Option<String>,
    unit: Unit,
    data: FieldRecordSource,
    digest: String,
}
impl CurrentFieldRecordSource {
    pub fn data(&self) -> &FieldRecordSource {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || image(
                c,
                &self.company,
                self.branch.as_deref(),
                self.department.as_deref(),
                &self.data.id,
            )
            .await?
                != (self.data.clone(), self.digest.clone())
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_source_record(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
    id: &str,
) -> Result<CurrentFieldRecordSource, Error> {
    let unit = unit(c).await?;
    let (data, digest) = image(c, company, branch, department, id).await?;
    Ok(CurrentFieldRecordSource {
        company: company.clone(),
        branch: branch.map(str::to_owned),
        department: department.map(str::to_owned),
        unit,
        data,
        digest,
    })
}
