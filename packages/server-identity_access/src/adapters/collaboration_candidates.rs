//! IAM-owned picker facts; no People or Collaboration tables are queried.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
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
        return Err(PersistenceError::new("IAM_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn image(
    c: &mut PgConnection,
    company: &CompanyId,
    users: &[String],
    now: TimestampMillis,
    require_write: bool,
) -> Result<Vec<String>, Error> {
    Ok(sqlx::query_scalar("SELECT DISTINCT a.user_id FROM assignments a WHERE a.company_id=$1 AND a.user_id=ANY($2::text[]) AND iam_assignment_active(a.id,$3) AND iam_principal_active(a.user_id,a.company_id,$3) AND (NOT $4::boolean OR iam_role_allows(a.role,a.company_id,'records.write')) ORDER BY a.user_id").bind(company.as_str()).bind(users).bind(now.get()).bind(require_write).fetch_all(c).await?)
}
pub struct CurrentCollaborationCandidates {
    company: CompanyId,
    users: Vec<String>,
    eligible: Vec<String>,
    unit: Unit,
    observed_at: i64,
    require_write: bool,
}
impl CurrentCollaborationCandidates {
    pub fn users(&self) -> &[String] {
        &self.eligible
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if now.get() < self.observed_at
            || unit(c).await? != self.unit
            || image(c, &self.company, &self.users, now, self.require_write).await? != self.eligible
        {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Current authority changed during this read.",
            )
            .into());
        }
        Ok(())
    }
}
/// Candidate membership is display-only. A command still checks the exact card
/// branch/department assignment and People engagement in its retained unit.
async fn read_candidates(
    c: &mut PgConnection,
    company: &CompanyId,
    users: Vec<String>,
    now: TimestampMillis,
    require_write: bool,
) -> Result<CurrentCollaborationCandidates, Error> {
    if users.iter().map(String::len).sum::<usize>() > 2 * 1024 * 1024 {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "COLLABORATION_WORKSPACE_LIMIT",
            "The Collaboration workspace exceeds this bounded read.",
        )
        .into());
    }
    let unit = unit(c).await?;
    let eligible = image(c, company, &users, now, require_write).await?;
    Ok(CurrentCollaborationCandidates {
        company: company.clone(),
        users,
        eligible,
        unit,
        observed_at: now.get(),
        require_write,
    })
}

pub async fn read_collaboration_candidates(
    c: &mut PgConnection,
    company: &CompanyId,
    users: Vec<String>,
    now: TimestampMillis,
) -> Result<CurrentCollaborationCandidates, Error> {
    read_candidates(c, company, users, now, true).await
}
/// Operations preserves its active membership picker. Exact record scope,
/// permission and dated employment are checked by the command, not this list.
pub async fn read_operations_candidates(
    c: &mut PgConnection,
    company: &CompanyId,
    users: Vec<String>,
    now: TimestampMillis,
) -> Result<CurrentCollaborationCandidates, Error> {
    read_candidates(c, company, users, now, false).await
}
