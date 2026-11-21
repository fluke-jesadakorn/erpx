//! Read-only Projects facts for native Staff Work composition.
//!
//! The port selects current Projects-owned rows on the caller's connection. It
//! opens no transaction, writes nothing and makes no membership or permission
//! decision; composition supplies the caller-specific visibility.
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use sqlx::PgConnection;

/// Current project identity and accountable owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectStaffFact {
    pub owner_user_id: String,
    pub title: String,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_FACT_UNAVAILABLE",
        "The current Projects fact is unavailable.",
    )
}

/// The original `getProject` owner read keeps Company isolation and applies no
/// lifecycle/state filter; lifecycle decisions belong to the command paths.
/// The caller-specific owner/member predicate is intentionally not repeated
/// here because this fact carries identity and owner only.
pub async fn staff_project(
    c: &mut PgConnection,
    company_id: &str,
    project_id: &str,
) -> Result<Option<ProjectStaffFact>, BusinessError> {
    let company = CompanyId::new(company_id)?;
    let project = RecordId::new(project_id)?;
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT p.owner_user_id,p.title FROM staff_projects p \
         WHERE p.id=$1 AND p.company_id=$2 LIMIT 1",
    )
    .bind(project.as_str())
    .bind(company.as_str())
    .fetch_optional(c)
    .await
    .map_err(|_| unavailable())?;
    Ok(row.map(|(owner_user_id, title)| ProjectStaffFact {
        owner_user_id,
        title,
    }))
}
