//! Control-side principal search and Holding-side active-assignment facts used
//! by the native Records reference search and mention command. Both statements
//! are owned here; the Records composition binds the Control reads and the
//! target Holding read.
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Executor, PgConnection, Postgres, Row, postgres::PgRow};

const SEARCH_SQL: &str = "SELECT user_id,display_name FROM principals WHERE status='active' AND lower(display_name) LIKE $1 ORDER BY display_name,user_id LIMIT 8";
/// Byte-pinned successor of the Control TS statement
/// `SELECT display_name FROM principals WHERE user_id=? AND status='active'`.
const PRINCIPAL_DISPLAY_NAME_SQL: &str =
    "SELECT display_name FROM principals WHERE user_id=$1 AND status='active'";
const ASSIGNMENT_SQL: &str = "SELECT 1 AS ok FROM assignments WHERE user_id=$1 AND revoked_at IS NULL AND valid_from<=$2 AND valid_until>$2 LIMIT 1";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "RECORDS_PRINCIPAL_SEARCH_UNAVAILABLE",
        "The Records principal search is unavailable.",
    )
}

/// Shared `display_name` mapper for the two Control principal reads.
fn display_name(row: &PgRow) -> Result<String, BusinessError> {
    row.try_get("display_name").map_err(|_| unavailable())
}

/// Shared assignment existence read over either the Holding pool or an
/// already-open Holding transaction, so the SQL and its failure mapping cannot
/// drift between the two ports.
async fn active_assignment_with<'c, E>(
    executor: E,
    user_id: &str,
    now: i64,
) -> Result<bool, BusinessError>
where
    E: Executor<'c, Database = Postgres>,
{
    let row = sqlx::query(ASSIGNMENT_SQL)
        .bind(user_id)
        .bind(now)
        .fetch_optional(executor)
        .await
        .map_err(|_| unavailable())?;
    Ok(row.is_some())
}

/// Active Control principals whose display name matches the bounded needle.
/// `needle` is already the lowercased `@user/`-stripped search term.
pub async fn search_principals(
    control: &DatabasePool,
    needle: &str,
) -> Result<Vec<(String, String)>, BusinessError> {
    search_principals_with(control.raw(), needle).await
}

pub async fn search_principals_on(
    control: &mut PgConnection,
    needle: &str,
) -> Result<Vec<(String, String)>, BusinessError> {
    search_principals_with(control, needle).await
}

async fn search_principals_with<'c, E>(
    control: E,
    needle: &str,
) -> Result<Vec<(String, String)>, BusinessError>
where
    E: Executor<'c, Database = Postgres>,
{
    if needle.is_empty() || needle.len() > 200 || needle.chars().any(char::is_control) {
        return Ok(Vec::new());
    }
    let pattern = format!("%{needle}%");
    let rows = sqlx::query(SEARCH_SQL)
        .bind(pattern)
        .fetch_all(control)
        .await
        .map_err(|_| unavailable())?;
    rows.into_iter()
        .map(|row| {
            let user_id: String = row.try_get("user_id").map_err(|_| unavailable())?;
            Ok((user_id, display_name(&row)?))
        })
        .collect()
}

/// Exact active-principal display name used to resolve a mention target.
pub async fn active_principal_display_name(
    control: &DatabasePool,
    user_id: &str,
) -> Result<Option<String>, BusinessError> {
    let row = sqlx::query(PRINCIPAL_DISPLAY_NAME_SQL)
        .bind(user_id)
        .fetch_optional(control.raw())
        .await
        .map_err(|_| unavailable())?;
    row.as_ref().map(display_name).transpose()
}

/// Exact retained active-assignment existence check for one principal.
pub async fn active_assignment(
    holding: &DatabasePool,
    user_id: &str,
    now: i64,
) -> Result<bool, BusinessError> {
    active_assignment_with(holding.raw(), user_id, now).await
}

/// The same assignment fact on the caller's already-open Holding transaction,
/// so the mention eligibility check shares the Records write's snapshot and
/// lock order.
pub async fn active_assignment_on(
    holding: &mut PgConnection,
    user_id: &str,
    now: i64,
) -> Result<bool, BusinessError> {
    active_assignment_with(&mut *holding, user_id, now).await
}
