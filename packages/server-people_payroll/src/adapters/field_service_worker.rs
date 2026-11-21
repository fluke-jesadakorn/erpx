//! Exact undated original Field Service worker reference. Maintenance's dated
//! workforce reference remains separate. No Person/assignment access is granted.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
pub struct CurrentFieldServicePeopleReference {
    company: CompanyId,
    user: String,
    positive: bool,
    unit: Unit,
}
impl CurrentFieldServicePeopleReference {
    pub fn is_eligible(&self) -> bool {
        self.positive
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || read(c, &self.company, &self.user).await? != self.positive
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub use super::identity_facts::lock_people_authority_sources;
pub async fn inspect_field_service_people_reference(
    c: &mut PgConnection,
    company: &CompanyId,
    user: &str,
) -> Result<CurrentFieldServicePeopleReference, Error> {
    let unit = unit(c).await?;
    let positive = read(c, company, user).await?;
    Ok(CurrentFieldServicePeopleReference {
        company: company.clone(),
        user: user.to_owned(),
        positive,
        unit,
    })
}
async fn read(c: &mut PgConnection, company: &CompanyId, user: &str) -> Result<bool, Error> {
    Ok(sqlx::query_scalar::<_,bool>("SELECT true FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=$1 AND p.user_id=$2 AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' ORDER BY p.id,e.id LIMIT 1 FOR SHARE OF p,e")
        .bind(company.as_str()).bind(user).fetch_optional(c).await?.unwrap_or(false))
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
    PersistenceError::new("PEOPLE_FIELD_SERVICE_REFERENCE_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "PEOPLE_FIELD_SERVICE_REFERENCE_CHANGED",
        "The original active Field Service worker reference changed.",
    )
    .into()
}
