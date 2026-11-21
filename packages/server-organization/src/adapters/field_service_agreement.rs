//! Actual Organization nodes used to create a Service Agreement; no grant.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Eq, PartialEq)]
struct Image {
    branch: String,
    department: Option<String>,
}
pub struct CurrentAgreementOrganization {
    company: CompanyId,
    branch: String,
    department: Option<String>,
    image: Image,
    unit: Unit,
}
impl CurrentAgreementOrganization {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(c, &self.company, &self.branch, self.department.as_deref())
                .await?
                .as_ref()
                != Some(&self.image)
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_agreement_organization_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ORGANIZATION_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE organization_nodes IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_agreement_organization(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: &str,
    department: Option<&str>,
) -> Result<Option<CurrentAgreementOrganization>, Error> {
    let unit = unit(c).await?;
    Ok(capture(c, company, branch, department)
        .await?
        .map(|image| CurrentAgreementOrganization {
            company: company.clone(),
            branch: branch.into(),
            department: department.map(str::to_owned),
            image,
            unit,
        }))
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: &str,
    department: Option<&str>,
) -> Result<Option<Image>, Error> {
    let branch_image:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(n)::text,'UTF8')),'hex') FROM organization_nodes n WHERE n.id=$1 AND n.company_id=$2 AND n.kind IN ('branch','headquarters') AND n.status='active' FOR SHARE OF n").bind(branch).bind(company.as_str()).fetch_optional(&mut *c).await?;
    let Some(branch_image) = branch_image else {
        return Ok(None);
    };
    let department_image = if let Some(id) = department {
        let image:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(n)::text,'UTF8')),'hex') FROM organization_nodes n WHERE n.id=$1 AND n.company_id=$2 AND n.kind='department' AND n.branch_id=$3 AND n.status='active' FOR SHARE OF n").bind(id).bind(company.as_str()).bind(branch).fetch_optional(c).await?;
        let Some(image) = image else { return Ok(None) };
        Some(image)
    } else {
        None
    };
    Ok(Some(Image {
        branch: branch_image,
        department: department_image,
    }))
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
        "FIELD_SERVICE_ORGANIZATION_CHANGED",
        "The original Service Organization source changed.",
    )
    .into()
}
