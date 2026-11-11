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

/// One current active Staff Work roster row.
///
/// `legal_name`, `department_id`, `position_id`, `level_number`, engagement
/// identity/version and the reporting manager mirror the retained
/// `staffPeople`/`staffSelect` columns. A Person with several current
/// engagements yields several rows; callers that need one row must reject
/// duplicates instead of silently choosing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StaffPersonFact {
    pub user_id: String,
    pub person_id: String,
    pub legal_name: String,
    pub department_id: Option<String>,
    pub position_id: Option<String>,
    pub level_number: Option<i64>,
    pub manager_person_id: Option<String>,
    pub engagement_id: String,
    pub engagement_version: i64,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_FACT_UNAVAILABLE",
        "The current People staff fact is unavailable.",
    )
}

/// The retained `staffPeople` roster read: active, unmerged People with an
/// active Company engagement on the current UTC day, an active account
/// assignment, and the Organization level/position projection.
///
/// The statement is the original `staffSelect` predicate and ordering with no
/// optional department/level filters (the retained `staffPeople` called without
/// filters binds both filters to SQL null). It remains read-only and uses the
/// caller's connection.
pub async fn staff_people(
    c: &mut PgConnection,
    company_id: &str,
) -> Result<Vec<StaffPersonFact>, BusinessError> {
    let company = CompanyId::new(company_id)?;
    let rows = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, Option<i32>, Option<String>, String, i64)>(
        "SELECT p.id,p.user_id,p.legal_name,e.department_id,e.position_id,l.level_number,e.manager_person_id,e.id,e.version \
         FROM hr_people p \
         JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
         LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id \
         LEFT JOIN organization_positions pos ON pos.id=e.position_id AND pos.company_id=e.company_id AND pos.active=1 \
         WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL AND p.user_id IS NOT NULL \
           AND e.status='active' \
           AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
           AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) \
           AND EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=p.user_id AND a.company_id=p.company_id \
             AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
         ORDER BY p.legal_name,e.start_date DESC,e.id",
    )
    .bind(company.as_str())
    .fetch_all(c)
    .await
    .map_err(|_| unavailable())?;
    Ok(rows
        .into_iter()
        .map(
            |(
                person_id,
                user_id,
                legal_name,
                department_id,
                position_id,
                level_number,
                manager_person_id,
                engagement_id,
                engagement_version,
            )| StaffPersonFact {
                user_id,
                person_id,
                legal_name,
                department_id,
                position_id,
                level_number: level_number.map(i64::from),
                manager_person_id,
                engagement_id,
                engagement_version,
            },
        )
        .collect())
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
