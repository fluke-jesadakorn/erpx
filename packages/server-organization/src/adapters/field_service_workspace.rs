//! Current Organization locations for the Service workspace; no editor grant.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceLocationReference {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    pub branch_id: String,
}
#[derive(Clone, PartialEq, Eq)]
struct Image {
    data: Vec<ServiceLocationReference>,
    digests: Vec<String>,
}
pub struct CurrentServiceWorkspaceLocations {
    company: CompanyId,
    branch: Option<String>,
    department: Option<String>,
    include_locations: bool,
    unit: Unit,
    image: Image,
}
impl CurrentServiceWorkspaceLocations {
    pub fn data(&self) -> &Vec<ServiceLocationReference> {
        &self.image.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(
                c,
                &self.company,
                self.branch.as_deref(),
                self.department.as_deref(),
                self.include_locations,
            )
            .await?
                != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_service_workspace_locations(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ORGANIZATION_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE organization_nodes IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_service_workspace_locations(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
    include_locations: bool,
) -> Result<CurrentServiceWorkspaceLocations, Error> {
    let unit = unit(c).await?;
    let image = capture(c, company, branch, department, include_locations).await?;
    Ok(CurrentServiceWorkspaceLocations {
        company: company.clone(),
        branch: branch.map(str::to_owned),
        department: department.map(str::to_owned),
        include_locations,
        unit,
        image,
    })
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
    include_locations: bool,
) -> Result<Image, Error> {
    let rows=sqlx::query("WITH source AS MATERIALIZED (SELECT n.*,CASE WHEN n.kind='department' THEN n.branch_id ELSE n.id END service_branch FROM organization_nodes n WHERE $4 AND n.company_id=$1 AND n.status='active' AND n.kind IN ('branch','headquarters','department') AND (n.kind<>'department' OR EXISTS(SELECT 1 FROM organization_nodes b WHERE b.id=n.branch_id AND b.company_id=n.company_id AND b.kind IN ('branch','headquarters') AND b.status='active')) AND ($2::text IS NULL OR CASE WHEN n.kind='department' THEN n.branch_id ELSE n.id END=$2) AND ($3::text IS NULL OR n.kind='department' AND n.id=$3 OR n.kind IN ('branch','headquarters') AND EXISTS(SELECT 1 FROM organization_nodes d WHERE d.id=$3 AND d.company_id=n.company_id AND d.kind='department' AND d.status='active' AND d.branch_id=n.id))), measured AS (SELECT *,sum(octet_length(to_jsonb(source)::text)) OVER () bytes,count(*) OVER () count,row_number() OVER () ordinal FROM source) SELECT CASE WHEN bytes<=2097152 AND count<=10000 THEN id END id,CASE WHEN bytes<=2097152 AND count<=10000 THEN kind END kind,CASE WHEN bytes<=2097152 AND count<=10000 THEN display_name END display_name,CASE WHEN bytes<=2097152 AND count<=10000 THEN service_branch END service_branch,encode(sha256(convert_to((to_jsonb(measured)-'bytes'-'count'-'ordinal')::text,'UTF8')),'hex') digest FROM measured WHERE (bytes<=2097152 AND count<=10000) OR ordinal=1 ORDER BY kind,display_name,id").bind(company.as_str()).bind(branch).bind(department).bind(include_locations).fetch_all(c).await?;
    let mut data = Vec::new();
    let mut digests = Vec::new();
    for r in rows {
        data.push(ServiceLocationReference {
            id: r.try_get::<Option<String>, _>("id")?.ok_or_else(limited)?,
            kind: r
                .try_get::<Option<String>, _>("kind")?
                .ok_or_else(limited)?,
            display_name: r
                .try_get::<Option<String>, _>("display_name")?
                .ok_or_else(limited)?,
            branch_id: r
                .try_get::<Option<String>, _>("service_branch")?
                .ok_or_else(limited)?,
        });
        digests.push(r.try_get("digest")?);
    }
    Ok(Image { data, digests })
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("ORGANIZATION_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_LOCATIONS_CHANGED",
        "The Service Organization source changed.",
    )
    .into()
}
fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_LOCATIONS_LIMIT",
        "The Service Organization workspace exceeds its bounded read.",
    )
    .into()
}
