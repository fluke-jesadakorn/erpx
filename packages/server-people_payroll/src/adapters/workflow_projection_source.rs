//! Read-only Workflow projection sources for People/Payroll requests.
//!
//! Exact port of
//! `packages/server-people_payroll/src/adapters/workflow-projection-source.ts`.
//! Leave and overtime read their own request table; payroll exposes `status` as
//! the projected state and has no branch/department scope. Reads share the
//! caller's connection and never write or open a transaction.
use erpx_server_runtime::public::postgres::PersistenceError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, Row};

const OVERTIME_SQL: &str =
    "SELECT state,version,company_id,branch_id,department_id FROM overtime_requests WHERE id=$1";
const LEAVE_SQL: &str =
    "SELECT state,version,company_id,branch_id,department_id FROM leave_requests WHERE id=$1";
const PAYROLL_SQL: &str = "SELECT status state,version,company_id,NULL branch_id,NULL department_id \
     FROM hr_payroll_runs WHERE id=$1";

/// One of the three People/Payroll projection owners.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeopleProjectionKind {
    Overtime,
    Leave,
    Payroll,
}

/// The retained `ProjectionSource` shape for a People/Payroll request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeopleProjectionSource {
    pub state: String,
    pub version: i64,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

/// Resolve the current owner fact for one overtime, leave or payroll source.
/// `None` means the owner has no such row; the caller must report
/// `PROJECTION_SOURCE_UNAVAILABLE` and never create an empty work item.
pub async fn people_projection_source(
    connection: &mut PgConnection,
    kind: PeopleProjectionKind,
    id: &str,
) -> Result<Option<PeopleProjectionSource>, BusinessError> {
    let sql = match kind {
        PeopleProjectionKind::Overtime => OVERTIME_SQL,
        PeopleProjectionKind::Leave => LEAVE_SQL,
        PeopleProjectionKind::Payroll => PAYROLL_SQL,
    };
    let Some(row) = sqlx::query(sql)
        .bind(id)
        .fetch_optional(connection)
        .await
        .map_err(source_read_failed)?
    else {
        return Ok(None);
    };
    Ok(Some(PeopleProjectionSource {
        state: row.try_get("state").map_err(source_read_failed)?,
        version: if kind == PeopleProjectionKind::Overtime {
            // The retained owner column is INTEGER; the projection fact is a
            // BIGINT version like every other owner.
            i64::from(
                row.try_get::<i32, _>("version")
                    .map_err(source_read_failed)?,
            )
        } else {
            row.try_get("version").map_err(source_read_failed)?
        },
        company_id: row.try_get("company_id").map_err(source_read_failed)?,
        branch_id: row.try_get("branch_id").map_err(source_read_failed)?,
        department_id: row
            .try_get("department_id")
            .map_err(source_read_failed)?,
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
    use super::{LEAVE_SQL, OVERTIME_SQL, PAYROLL_SQL};

    #[test]
    fn leave_and_overtime_read_only_their_retained_owner_columns() {
        assert!(LEAVE_SQL.contains("FROM leave_requests"));
        assert!(OVERTIME_SQL.contains("FROM overtime_requests"));
        for sql in [LEAVE_SQL, OVERTIME_SQL] {
            for column in [
                "state",
                "version",
                "company_id",
                "branch_id",
                "department_id",
            ] {
                assert!(sql.contains(column), "missing owner column {column}");
            }
        }
    }

    #[test]
    fn payroll_projects_status_and_has_no_branch_or_department_scope() {
        assert!(PAYROLL_SQL.contains("status state"));
        assert!(PAYROLL_SQL.contains("NULL branch_id"));
        assert!(PAYROLL_SQL.contains("NULL department_id"));
        assert!(PAYROLL_SQL.contains("FROM hr_payroll_runs"));
    }
}
