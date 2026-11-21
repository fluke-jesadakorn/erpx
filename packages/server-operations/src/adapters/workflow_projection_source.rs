//! Read-only Workflow projection source for Operations staff tasks.
//!
//! Exact port of
//! `packages/server-operations/src/adapters/workflow-projection-source.ts`:
//! `SELECT state,version,company_id,branch_id,department_id FROM staff_tasks WHERE id=?`.
//! It reads on the caller's connection and never writes, opens a transaction or
//! fabricates a row for a missing task.
use erpx_server_runtime::public::postgres::PersistenceError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, Row};

const SQL: &str =
    "SELECT state,version,company_id,branch_id,department_id FROM staff_tasks WHERE id=$1";

/// The retained `ProjectionSource` shape for a staff task.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskProjectionSource {
    pub state: String,
    pub version: i64,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

/// Resolve the current owner fact for one task. `None` means the owner has no
/// such row; the caller must report `PROJECTION_SOURCE_UNAVAILABLE` and never
/// create an empty work item.
pub async fn task_projection_source(
    connection: &mut PgConnection,
    id: &str,
) -> Result<Option<TaskProjectionSource>, BusinessError> {
    let Some(row) = sqlx::query(SQL)
        .bind(id)
        .fetch_optional(connection)
        .await
        .map_err(source_read_failed)?
    else {
        return Ok(None);
    };
    Ok(Some(TaskProjectionSource {
        state: row.try_get("state").map_err(source_read_failed)?,
        version: row.try_get("version").map_err(source_read_failed)?,
        company_id: row.try_get("company_id").map_err(source_read_failed)?,
        branch_id: row.try_get("branch_id").map_err(source_read_failed)?,
        department_id: row.try_get("department_id").map_err(source_read_failed)?,
    }))
}

fn source_read_failed(error: sqlx::Error) -> BusinessError {
    let persistence = PersistenceError::from(error);
    if persistence.retryable() {
        BusinessError::new(
            FailureKind::Conflict,
            "PROJECTION_SOURCE_RETRY_REQUIRED",
            "The Workflow projection source must be retried.",
        )
    } else {
        BusinessError::new(
            FailureKind::Unavailable,
            "PROJECTION_SOURCE_READ_FAILED",
            "The Workflow projection source could not be read.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::SQL;

    #[test]
    fn reads_only_the_retained_owner_columns() {
        assert!(SQL.contains("FROM staff_tasks"));
        for column in [
            "state",
            "version",
            "company_id",
            "branch_id",
            "department_id",
        ] {
            assert!(SQL.contains(column), "missing owner column {column}");
        }
        assert!(!SQL.contains("JOIN"));
    }
}
