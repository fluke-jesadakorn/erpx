//! sqlx execution of the migrated paged own-leave read.
//!
//! This is the only layer that executes SQL for the list operation. The
//! projection, joins, predicates and ordering are the retained TypeScript
//! `ppRd27LeaveRequestListSql` statement with `?` renumbered to `$n`; the
//! `::float8` cast on `calculated_units` matches the existing native leave
//! reader in `adapters/leave.rs`. No Workflow, IAM, commercial, session or
//! command write happens here.

use super::application::{LeaveFuture, LeaveActor, LeaveRequestsRead};
use super::domain::{LeaveCursor, LeaveListFilter, LeaveListRow, invalid};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{AssertSqlSafe, Row};

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_LEAVE_READ_UNAVAILABLE",
        "The leave list is unavailable.",
    )
}

/// Retained projection with no predicate so the exact clause set can be added
/// by `list_sql` without interpolating any caller value into the statement.
pub const SQL_LEAVE_LIST_BASE: &str = "SELECT r.id,r.number,r.start_date,r.end_date,r.start_segment,r.end_segment,r.calculated_units::float8 calculated_units,r.days,r.state,r.cancellation_state,r.version,r.created_at,r.updated_at,r.decision_note,t.name leave_type_name FROM leave_requests r LEFT JOIN leave_types t ON t.id=r.leave_type_id WHERE r.company_id=$1 AND r.engagement_id=$2 AND r.created_by=$3";

/// SHA-256 of `SQL_LEAVE_LIST_BASE`, pinned in the owner contract test.
pub const SQL_ORACLE_SHA256: &str =
    "823955f4b09185f6cfd2ddd829eceda5c98bdab9b917956380832bcce9dae4a3";

/// The caller-supplied filter is a closed enum; only its fixed predicate text
/// reaches SQL. Cursor predicates use three bound values, never interpolation.
pub fn list_sql(filter: LeaveListFilter, cursor: bool) -> String {
    let mut sql = format!(
        "{SQL_LEAVE_LIST_BASE} AND {}",
        filter.clause()
    );
    if cursor {
        sql.push_str(" AND (r.created_at<$4 OR (r.created_at=$4 AND r.id<$5))");
    }
    sql.push_str(" ORDER BY r.created_at DESC,r.id DESC LIMIT $");
    sql.push_str(if cursor { "6" } else { "4" });
    sql
}

fn integer(row: &sqlx::postgres::PgRow, column: &str) -> Result<i64, BusinessError> {
    row.try_get::<Option<i64>, _>(column)
        .ok()
        .flatten()
        .or_else(|| {
            row.try_get::<Option<i32>, _>(column)
                .ok()
                .flatten()
                .map(i64::from)
        })
        .ok_or_else(unavailable)
}

fn pack(row: &sqlx::postgres::PgRow) -> Result<LeaveListRow, BusinessError> {
    let text = |column: &str| -> Result<String, BusinessError> {
        row.try_get::<String, _>(column).map_err(|_| unavailable())
    };
    let optional = |column: &str| -> Result<Option<String>, BusinessError> {
        row.try_get::<Option<String>, _>(column).map_err(|_| unavailable())
    };
    Ok(LeaveListRow {
        id: text("id")?,
        number: text("number")?,
        start_date: text("start_date")?,
        end_date: optional("end_date")?,
        start_segment: optional("start_segment")?,
        end_segment: optional("end_segment")?,
        calculated_units: row
            .try_get::<Option<f64>, _>("calculated_units")
            .map_err(|_| unavailable())?,
        days: integer(row, "days")?,
        state: text("state")?,
        cancellation_state: optional("cancellation_state")?,
        version: integer(row, "version")?,
        created_at: integer(row, "created_at")?,
        updated_at: integer(row, "updated_at")?,
        decision_note: optional("decision_note")?,
        leave_type_name: optional("leave_type_name")?,
    })
}

pub struct PgLeaveRequestsRead {
    pool: DatabasePool,
}

impl PgLeaveRequestsRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl LeaveRequestsRead for PgLeaveRequestsRead {
    fn list<'a>(
        &'a self,
        actor: &'a LeaveActor,
        filter: LeaveListFilter,
        cursor: Option<&'a LeaveCursor>,
        limit: usize,
    ) -> LeaveFuture<'a, Vec<LeaveListRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let actor = actor.clone();
        let cursor = cursor.cloned();
        let fetch = limit.saturating_add(1) as i64;
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor.clone();
                let cursor = cursor.clone();
                Box::pin(async move {
                    if !super::domain::valid_id(&actor.company)
                        || !super::domain::valid_id(&actor.user)
                        || !super::domain::valid_id(&actor.engagement)
                    {
                        return Err(invalid("VALIDATION_FAILED", "Invalid leave actor scope.").into());
                    }
                    let mut query = sqlx::query(AssertSqlSafe(list_sql(filter, cursor.is_some())))
                        .bind(actor.company.as_str())
                        .bind(actor.engagement.as_str())
                        .bind(actor.user.as_str());
                    if let Some(value) = cursor.as_ref() {
                        query = query
                            .bind(value.at)
                            .bind(value.at)
                            .bind(value.id.as_str())
                            .bind(fetch);
                    } else {
                        query = query.bind(fetch);
                    }
                    let rows = query.fetch_all(&mut *connection).await.map_err(|_| unavailable())?;
                    rows.iter().map(pack).collect::<Result<Vec<_>, BusinessError>>().map_err(Into::into)
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_keeps_owner_scope_and_shape() {
        assert!(SQL_LEAVE_LIST_BASE.contains("r.company_id=$1"));
        assert!(SQL_LEAVE_LIST_BASE.contains("r.engagement_id=$2"));
        assert!(SQL_LEAVE_LIST_BASE.contains("r.created_by=$3"));
        assert!(SQL_LEAVE_LIST_BASE.contains("calculated_units::float8"));
        assert!(SQL_LEAVE_LIST_BASE.contains("LEFT JOIN leave_types t ON t.id=r.leave_type_id"));
        assert!(!SQL_LEAVE_LIST_BASE.contains('?'));
    }

    #[test]
    fn filters_and_cursors_bind_without_interpolation() {
        let plain = list_sql(LeaveListFilter::All, false);
        assert!(plain.contains("AND TRUE"));
        assert!(plain.ends_with("LIMIT $4"));
        let paged = list_sql(LeaveListFilter::Drafts, true);
        assert!(paged.contains("r.state='draft'"));
        assert!(paged.contains("(r.created_at<$4 OR (r.created_at=$4 AND r.id<$5))"));
        assert!(paged.ends_with("LIMIT $6"));
    }

    #[test]
    fn sql_oracle_matches_the_owner_constant() {
        use sha2::Digest;
        let hex = sha2::Sha256::digest(SQL_LEAVE_LIST_BASE.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(hex, SQL_ORACLE_SHA256);
    }
}
