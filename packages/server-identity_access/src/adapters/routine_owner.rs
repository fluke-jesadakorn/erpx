//! Read-only Identity authority fact for native routine commands.
//!
//! The retained `companyAuthority` default requires an active holding-scope
//! assignment whose role grants `organization.manage` for the selected Company.
//! This port reads only Identity-owned rows on the caller's connection and
//! makes no routine/owner decision.
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, UserId};
use sqlx::PgConnection;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "IDENTITY_ROUTINE_OWNER_UNAVAILABLE",
        "The current Identity company-authority fact is unavailable.",
    )
}

/// Whether `user_id` currently holds a Company holding-scope assignment that
/// grants `organization.manage`. This mirrors the retained company-authority
/// predicate; the caller still owns routine policy and assignment selection.
pub async fn company_owner(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
) -> Result<bool, BusinessError> {
    let company = CompanyId::new(company_id)?;
    let user = UserId::new(user_id)?;
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM assignments a \
         WHERE a.company_id=$1 AND a.user_id=$2 AND a.scope_kind='holding' \
           AND a.revoked_at IS NULL \
           AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
           AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
           AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint \
           AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint \
           AND iam_role_allows(a.role,a.company_id,'organization.manage'))",
    )
    .bind(company.as_str())
    .bind(user.as_str())
    .fetch_one(c)
    .await
    .map_err(|_| unavailable())
}
