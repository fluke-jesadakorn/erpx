//! Borrowed owner-local preference facts and writes. No session selection or
//! grant is written here; the outer unit must retain Identity and audit effects.
use crate::domain::work_role_store::WorkRoleReference;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{AssignmentId, BusinessError, CompanyId, FailureKind, UserId};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
type Image = Option<(String, i64, i64)>;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your access or work role changed. Review and choose again.",
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
        return Err(PersistenceError::new("WORK_PREFERENCE_UNIT_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn image(c: &mut PgConnection, user: &UserId, company: &CompanyId) -> Result<Image, Error> {
    Ok(sqlx::query_as("SELECT assignment_id,assignment_version,updated_at FROM work_role_preferences WHERE user_id=$1 AND company_id=$2 FOR SHARE").bind(user.as_str()).bind(company.as_str()).fetch_optional(c).await?)
}
pub async fn lock_work_role_preferences(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(changed());
    }
    sqlx::query("LOCK TABLE work_role_preferences IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub struct CurrentWorkRolePreference {
    unit: Unit,
    user: UserId,
    company: CompanyId,
    image: Image,
}
impl CurrentWorkRolePreference {
    /// Invalid historical preferences are retained but cannot choose a role.
    pub fn preference(&self) -> Option<WorkRoleReference> {
        let (id, version, _) = self.image.as_ref()?;
        if !(1..=9_007_199_254_740_991).contains(version) {
            return None;
        }
        Some(WorkRoleReference {
            id: AssignmentId::new(id.clone()).ok()?,
            version: *version,
        })
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || image(c, &self.user, &self.company).await? != self.image {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn remember(
        self,
        c: &mut PgConnection,
        assignment: &WorkRoleReference,
        now: i64,
    ) -> Result<Self, Error> {
        self.require_current(c).await?;
        if !(1..=9_007_199_254_740_991).contains(&assignment.version)
            || !(0..=9_007_199_254_740_991).contains(&now)
        {
            return Err(changed());
        }
        sqlx::query("INSERT INTO work_role_preferences(user_id,company_id,assignment_id,assignment_version,updated_at)VALUES($1,$2,$3,$4,$5)ON CONFLICT(user_id,company_id)DO UPDATE SET assignment_id=excluded.assignment_id,assignment_version=excluded.assignment_version,updated_at=excluded.updated_at").bind(self.user.as_str()).bind(self.company.as_str()).bind(assignment.id.as_str()).bind(assignment.version).bind(now).execute(&mut *c).await?;
        let saved = Self {
            image: Some((assignment.id.as_str().into(), assignment.version, now)),
            ..self
        };
        saved.require_current(c).await?;
        Ok(saved)
    }
}
pub async fn read_work_role_preference(
    c: &mut PgConnection,
    user: &UserId,
    company: &CompanyId,
) -> Result<CurrentWorkRolePreference, Error> {
    Ok(CurrentWorkRolePreference {
        unit: unit(c).await?,
        user: user.clone(),
        company: company.clone(),
        image: image(c, user, company).await?,
    })
}
