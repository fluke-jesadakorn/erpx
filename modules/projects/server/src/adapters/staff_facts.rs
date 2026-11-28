//! Read-only Projects facts for native Staff Work composition.
//!
//! The port selects current Projects-owned rows on the caller's connection. It
//! opens no transaction, writes nothing and makes no membership or permission
//! decision; composition supplies the caller-specific visibility.
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, RecordId, UserId};
use sqlx::PgConnection;

/// Current project identity and accountable owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectStaffFact {
    pub owner_user_id: String,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SocialProjectReference {
    pub id: String,
    pub title: String,
    pub version: i64,
}

/// A post audience and an ordinary project preview use exact project
/// membership. A handover additionally recognizes an owner account alias.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SocialProjectAccess {
    AudienceOrPreview,
    Handover,
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

/// Read a Projects source under the same-Company owner/member policy.
/// Composition supplies the current signed-in actor and product purchase gate.
pub async fn social_project_reference(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    project_id: &str,
    access: SocialProjectAccess,
) -> Result<Option<SocialProjectReference>, BusinessError> {
    let company = CompanyId::new(company_id)?;
    let user = UserId::new(user_id)?;
    let project = RecordId::new(project_id)?;
    let row = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT p.id,p.title,p.version FROM staff_projects p
         WHERE p.company_id=$1 AND p.id=$2 AND
           (p.owner_user_id=$3 OR ($4 AND iam_same_person(p.owner_user_id,$3,p.company_id))
            OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=$3))",
    )
    .bind(company.as_str())
    .bind(project.as_str())
    .bind(user.as_str())
    .bind(access == SocialProjectAccess::Handover)
    .fetch_optional(connection)
    .await
    .map_err(|_| unavailable())?;
    Ok(row.map(|(id, title, version)| SocialProjectReference { id, title, version }))
}

/// Current Projects options for the Social composer. Keep the established
/// title ordering and apply the owner/member condition before the limit.
pub async fn social_project_audiences(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
) -> Result<Vec<SocialProjectReference>, BusinessError> {
    let company = CompanyId::new(company_id)?;
    let user = UserId::new(user_id)?;
    let rows = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT p.id,p.title,p.version FROM staff_projects p WHERE p.company_id=$1
           AND (p.owner_user_id=$2 OR EXISTS(SELECT 1 FROM staff_project_members m
                                            WHERE m.project_id=p.id AND m.user_id=$2))
         ORDER BY p.title,p.id LIMIT 100",
    )
    .bind(company.as_str())
    .bind(user.as_str())
    .fetch_all(connection)
    .await
    .map_err(|_| unavailable())?;
    Ok(rows
        .into_iter()
        .map(|(id, title, version)| SocialProjectReference { id, title, version })
        .collect())
}
