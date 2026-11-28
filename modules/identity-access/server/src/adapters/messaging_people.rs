//! Source facts only: callers must authorize the request and any team scope.
use erpx_platform_shared_server::public::CompanyId;
use sqlx::PgConnection;

/// Match the retained assignment predicate, including its two clock reads.
/// The caller intersects a team membership set if a team filter is present.
pub async fn active_assignment_users(
    connection: &mut PgConnection,
    company: &CompanyId,
    valid_from_at: i64,
    valid_until_at: i64,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT DISTINCT a.user_id FROM assignments a WHERE a.company_id=$1 AND a.revoked_at IS NULL AND a.valid_from<=$2 AND a.valid_until>$3")
        .bind(company.as_str())
        .bind(valid_from_at)
        .bind(valid_until_at)
        .fetch_all(connection)
        .await
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct MessagingAccountName {
    pub id: String,
    pub name: String,
    pub email: String,
}

/// The Control database is authoritative for names, email, and ordering.
pub async fn control_account_names(
    connection: &mut PgConnection,
    ids: &[String],
) -> Result<Vec<MessagingAccountName>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id,name,email FROM \"user\" WHERE id = ANY($1::text[]) ORDER BY name,id",
    )
    .bind(ids)
    .fetch_all(connection)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, email)| MessagingAccountName { id, name, email })
        .collect())
}
