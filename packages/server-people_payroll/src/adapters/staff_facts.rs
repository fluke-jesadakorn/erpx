//! Read-only People facts for native Staff Work composition.
//!
//! These functions select current People-owned rows on the caller's connection.
//! They open no transaction, write nothing, make no assignment or permission
//! decision, and never fall back to another owner's SQL. Missing rows are an
//! honest `None`; every persistence failure is an unavailable fact.
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, UserId};
use sqlx::PgConnection;

/// Current branch and department placement for the caller's active staff row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveStaffFact {
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_FACT_UNAVAILABLE",
        "The current People staff fact is unavailable.",
    )
}

/// The retained `activeStaff`/`staffSelect` People predicate: the linked Person
/// is active and unmerged in the caller's Company, and one engagement is active
/// on the current UTC day. Ordering matches the original staff read, so the
/// first row is deterministic when several engagements qualify.
pub async fn active_staff(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
) -> Result<Option<ActiveStaffFact>, BusinessError> {
    let company = CompanyId::new(company_id)?;
    let user = UserId::new(user_id)?;
    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT e.branch_id,e.department_id \
         FROM hr_people p \
         JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
         WHERE p.company_id=$1 AND p.user_id=$2 \
           AND p.status='active' AND p.merged_into_id IS NULL AND p.user_id IS NOT NULL \
           AND e.status='active' \
           AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
           AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) \
         ORDER BY p.legal_name,e.start_date DESC,e.id LIMIT 1",
    )
    .bind(company.as_str())
    .bind(user.as_str())
    .fetch_optional(c)
    .await
    .map_err(|_| unavailable())?;
    Ok(row.map(|(branch_id, department_id)| ActiveStaffFact {
        branch_id,
        department_id,
    }))
}

/// The current legal name of the caller's active, unmerged Person. This is the
/// same label source used by the original Staff Work task read.
pub async fn person_name(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
) -> Result<Option<String>, BusinessError> {
    let company = CompanyId::new(company_id)?;
    let user = UserId::new(user_id)?;
    sqlx::query_scalar::<_, String>(
        "SELECT p.legal_name FROM hr_people p \
         WHERE p.company_id=$1 AND p.user_id=$2 \
           AND p.status='active' AND p.merged_into_id IS NULL \
         ORDER BY p.legal_name,p.id LIMIT 1",
    )
    .bind(company.as_str())
    .bind(user.as_str())
    .fetch_optional(c)
    .await
    .map_err(|_| unavailable())
}
