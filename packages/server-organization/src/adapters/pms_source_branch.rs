//! Exact active branch predicate used by PMS source disclosure; no new kind rule.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("PMS_BRANCH_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn image(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: &str,
) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(n)::text,'UTF8')),'hex') FROM organization_nodes n WHERE id=$1 AND company_id=$2 AND status='active' FOR SHARE").bind(branch).bind(company.as_str()).fetch_optional(c).await?)
}
pub struct CurrentPmsSourceBranch {
    company: CompanyId,
    branch: String,
    unit: Unit,
    image: String,
}
impl CurrentPmsSourceBranch {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || image(c, &self.company, &self.branch).await?.as_deref() != Some(self.image.as_str())
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SOURCE_RECORD_CHANGED",
                "The source branch changed.",
            )
            .into());
        }
        Ok(())
    }
}
pub async fn read_pms_source_branch(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: &str,
) -> Result<Option<CurrentPmsSourceBranch>, Error> {
    let unit = unit(c).await?;
    Ok(image(c, company, branch)
        .await?
        .map(|image| CurrentPmsSourceBranch {
            company: company.clone(),
            branch: branch.into(),
            unit,
            image,
        }))
}
