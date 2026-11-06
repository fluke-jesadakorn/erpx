//! Benefits employee selection and retained People source, not an access grant.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

/// Acquire before the first snapshot, in a coordinator-established lock order.
/// The held locks cannot establish that the caller had no earlier snapshot.
pub async fn lock_benefit_people_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() { return Err(PersistenceError::new("PEOPLE_BENEFIT_TRANSACTION_REQUIRED").into()); }
    sqlx::query("LOCK TABLE hr_people,hr_engagements IN SHARE MODE").execute(c).await?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenefitEmployee {
    pub person_id: String,
    pub engagement_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}
pub struct CurrentBenefitPerson {
    company: String,
    user: String,
    selected: Option<BenefitEmployee>,
    image: Option<String>,
    unit: Unit,
}
impl CurrentBenefitPerson {
    /// The actual result of the inner unordered LIMIT query, never a sorted substitute.
    pub fn selected(&self) -> Option<&BenefitEmployee> { self.selected.as_ref() }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || !holds_locks(c).await? { return Err(changed()); }
        match &self.selected {
            Some(selected) => {
                if image(c, &self.company, &selected.person_id).await? != self.image
                    || !member(c, &self.company, &self.user, selected).await? { return Err(changed()); }
            }
            None => if select(c, &self.company, &self.user).await?.is_some() { return Err(changed()); },
        }
        Ok(())
    }
}

/// Reproduces the outer source read and inner selection separately. No order is
/// imposed and multiple active engagements remain valid. The source image is
/// the same field projection retained by the TypeScript Benefits boundary.
pub async fn inspect_benefit_person(c: &mut PgConnection, company: &str, user: &str) -> Result<CurrentBenefitPerson, Error> {
    let unit = unit(c).await?;
    if !holds_locks(c).await? { return Err(PersistenceError::new("PEOPLE_BENEFIT_SOURCE_LOCK_REQUIRED").into()); }
    let outer = select(c, company, user).await?;
    let source = match &outer { Some(person) => image(c, company, &person.person_id).await?, None => None };
    let selected = select(c, company, user).await?;
    // user_id is unique in the migrated schema; only the engagement can differ.
    if outer.as_ref().map(|p| &p.person_id) != selected.as_ref().map(|p| &p.person_id) { return Err(changed()); }
    if let Some(person) = &selected {
        if source.is_none() || !member(c, company, user, person).await? { return Err(changed()); }
    }
    Ok(CurrentBenefitPerson { company: company.to_owned(), user: user.to_owned(), selected, image: source, unit })
}
async fn select(c: &mut PgConnection, company: &str, user: &str) -> Result<Option<BenefitEmployee>, Error> {
    let row = sqlx::query("SELECT p.id,e.id engagement_id,e.branch_id,e.department_id FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=$1 AND p.user_id=$2 AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' LIMIT 1")
        .bind(company).bind(user).fetch_optional(c).await?;
    row.map(|r| Ok(BenefitEmployee { person_id:r.try_get("id")?, engagement_id:r.try_get("engagement_id")?, branch_id:r.try_get("branch_id")?, department_id:r.try_get("department_id")? })).transpose()
}
async fn image(c: &mut PgConnection, company: &str, person: &str) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('person',jsonb_build_object('id',p.id,'companyId',p.company_id,'userId',p.user_id,'status',p.status,'version',p.version,'mergedIntoId',p.merged_into_id),'engagements',(SELECT jsonb_agg(jsonb_build_object('id',e.id,'companyId',e.company_id,'personId',e.person_id,'branchId',e.branch_id,'departmentId',e.department_id,'status',e.status,'version',e.version,'startDate',e.start_date,'endDate',e.end_date) ORDER BY e.id) FROM hr_engagements e WHERE e.person_id=p.id AND e.company_id=p.company_id))::text FROM hr_people p WHERE p.company_id=$1 AND p.id=$2")
        .bind(company).bind(person).fetch_optional(c).await?)
}
async fn member(c: &mut PgConnection, company: &str, user: &str, employee: &BenefitEmployee) -> Result<bool, Error> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=$1 AND p.user_id=$2 AND p.id=$3 AND p.status='active' AND p.merged_into_id IS NULL AND e.id=$4 AND e.status='active' AND e.branch_id IS NOT DISTINCT FROM $5 AND e.department_id IS NOT DISTINCT FROM $6)")
        .bind(company).bind(user).bind(&employee.person_id).bind(&employee.engagement_id).bind(&employee.branch_id).bind(&employee.department_id).fetch_one(c).await?)
}
async fn holds_locks(c: &mut PgConnection) -> Result<bool, Error> {
    Ok(sqlx::query_scalar("SELECT count(DISTINCT relation)=2 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation IN ('hr_people'::regclass,'hr_engagements'::regclass) AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')").fetch_one(c).await?)
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() { return Err(PersistenceError::new("PEOPLE_BENEFIT_TRANSACTION_REQUIRED").into()); }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" { return Err(PersistenceError::new("PEOPLE_BENEFIT_SERIALIZABLE_REQUIRED").into()); }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error { BusinessError::new(FailureKind::Conflict, "PEOPLE_BENEFIT_PERSON_CHANGED", "The Benefits People source changed.").into() }
