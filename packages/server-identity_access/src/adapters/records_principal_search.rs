//! Control-side principal search and Holding-side active-assignment fact used
//! by the native Records reference search. Both statements are owned here; the
//! Records composition binds the Control read and the target Holding read.
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;

const SEARCH_SQL: &str = "SELECT user_id,display_name FROM principals WHERE status='active' AND lower(display_name) LIKE $1 ORDER BY display_name,user_id LIMIT 8";
const ASSIGNMENT_SQL: &str = "SELECT 1 AS ok FROM assignments WHERE user_id=$1 AND revoked_at IS NULL AND valid_from<=$2 AND valid_until>$2 LIMIT 1";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "RECORDS_PRINCIPAL_SEARCH_UNAVAILABLE",
        "The Records principal search is unavailable.",
    )
}

/// Active Control principals whose display name matches the bounded needle.
/// `needle` is already the lowercased `@user/`-stripped search term.
pub async fn search_principals(
    control: &DatabasePool,
    needle: &str,
) -> Result<Vec<(String, String)>, BusinessError> {
    if needle.is_empty() || needle.len() > 200 || needle.chars().any(char::is_control) {
        return Ok(Vec::new());
    }
    let pattern = format!("%{needle}%");
    let rows = sqlx::query(SEARCH_SQL)
        .bind(pattern)
        .fetch_all(control.raw())
        .await
        .map_err(|_| unavailable())?;
    rows.into_iter()
        .map(|row| {
            let user_id: String = row.try_get("user_id").map_err(|_| unavailable())?;
            let display_name: String = row.try_get("display_name").map_err(|_| unavailable())?;
            Ok((user_id, display_name))
        })
        .collect()
}

/// Exact retained active-assignment existence check for one principal.
pub async fn active_assignment(
    holding: &DatabasePool,
    user_id: &str,
    now: i64,
) -> Result<bool, BusinessError> {
    let row = sqlx::query(ASSIGNMENT_SQL)
        .bind(user_id)
        .bind(now)
        .fetch_optional(holding.raw())
        .await
        .map_err(|_| unavailable())?;
    Ok(row.is_some())
}
