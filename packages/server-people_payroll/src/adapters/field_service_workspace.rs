//! People-owned names and read-predicate eligibility for Service workspace.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceWorkspacePerson {
    pub user_id: String,
    pub name: String,
    pub eligible: bool,
}
fn limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_WORKSPACE_LIMIT",
        "The Service workspace exceeds this bounded read.",
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
        return Err(PersistenceError::new("PEOPLE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_service_workspace_people(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PEOPLE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE hr_people,hr_engagements IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
async fn image(
    c: &mut PgConnection,
    company: &CompanyId,
    ids: &[String],
    include_picker: bool,
) -> Result<(Vec<ServiceWorkspacePerson>, String), Error> {
    let row=sqlx::query("WITH selected AS MATERIALIZED (SELECT p.*,EXISTS(SELECT 1 FROM hr_engagements e WHERE e.person_id=p.id AND e.company_id=p.company_id AND e.status='active') eligible FROM hr_people p WHERE p.company_id=$1 AND p.user_id IS NOT NULL AND p.merged_into_id IS NULL AND (p.user_id=ANY($2::text[]) OR ($3::boolean AND p.status='active' AND EXISTS(SELECT 1 FROM hr_engagements candidate WHERE candidate.person_id=p.id AND candidate.company_id=p.company_id AND candidate.status='active')))), payload AS (SELECT coalesce(jsonb_agg(jsonb_build_object('user_id',user_id,'name',legal_name,'eligible',status='active' AND eligible) ORDER BY legal_name,user_id,id),'[]'::jsonb) value,coalesce(jsonb_agg(to_jsonb(selected) ORDER BY id),'[]'::jsonb) image FROM selected), measured AS (SELECT value::text payload,encode(sha256(convert_to(image::text,'UTF8')),'hex') digest FROM payload) SELECT CASE WHEN octet_length(payload)<=2097152 THEN payload END payload,digest FROM measured").bind(company.as_str()).bind(ids).bind(include_picker).fetch_one(c).await?;
    let raw: Option<String> = row.try_get("payload")?;
    let data: Vec<ServiceWorkspacePerson> =
        serde_json::from_str(&raw.ok_or_else(limit)?).map_err(|_| limit())?;
    if data.len() > 10000 {
        return Err(limit());
    }
    Ok((data, row.try_get("digest")?))
}
pub struct CurrentServiceWorkspacePeople {
    company: CompanyId,
    ids: Vec<String>,
    include_picker: bool,
    unit: Unit,
    data: Vec<ServiceWorkspacePerson>,
    digest: String,
}
impl CurrentServiceWorkspacePeople {
    pub fn data(&self) -> &[ServiceWorkspacePerson] {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || image(c, &self.company, &self.ids, self.include_picker)
                .await?
                .1
                != self.digest
        {
            return Err(limit());
        }
        Ok(())
    }
}
pub async fn read_service_workspace_people(
    c: &mut PgConnection,
    company: &CompanyId,
    ids: Vec<String>,
    include_picker: bool,
) -> Result<CurrentServiceWorkspacePeople, Error> {
    let unit = unit(c).await?;
    let (data, digest) = image(c, company, &ids, include_picker).await?;
    Ok(CurrentServiceWorkspacePeople {
        company: company.clone(),
        ids,
        include_picker,
        unit,
        data,
        digest,
    })
}
