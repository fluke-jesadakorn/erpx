//! PostgreSQL execution for the owner-local Learning evidence-detail family.
//!
//! Both statements start from the byte-identical projections formerly composed
//! in `adapters/learning.ts`; binds are positional and the company scope is
//! always bound. The reader runs on a `PgUnitOfWork` and returns a pinned row.
//! No predicate or projection is accepted from the caller.

use super::application::{EvidenceFuture, EvidenceReader};
use super::domain::{EvidenceKind, EvidenceQuery, EvidenceRow, unavailable};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

/// Byte-identical completion projection (was `${completionEvidenceColumns}` +
/// the FROM/JOIN/WHERE/LIMIT tail in `learning.ts`). Positional binds replace
/// the TS `?` placeholders in the same order.
pub const COMPLETION_SQL: &str = concat!(
    "SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title",
    " FROM learning_assignment_events e",
    " JOIN learning_assignments a ON a.id=e.assignment_id",
    " JOIN learning_courses c ON c.id=a.course_id",
    " WHERE e.id=$1 AND a.company_id=$2 AND ($3::text IS NULL OR a.branch_id=$3)",
    " AND ($4::text IS NULL OR a.department_id=$4)",
    " AND e.action='complete' AND e.status='completed' AND e.created_at>=$5 AND e.created_at<$6",
    " LIMIT 1",
);

/// Byte-identical overdue projection (was `${overdueEvidenceColumns}` + tail).
pub const OVERDUE_SQL: &str = concat!(
    "SELECT a.id,a.version,a.status,a.due_date,a.updated_at,c.title course_title,c.required_for",
    " FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id",
    " WHERE a.id=$1 AND a.company_id=$2 AND ($3::text IS NULL OR a.branch_id=$3)",
    " AND ($4::text IS NULL OR a.department_id=$4) AND c.required_for IS NOT NULL",
    " AND a.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')",
    " AND a.status IN ('assigned','in_progress')",
    " LIMIT 1",
);

pub struct PgEvidenceReader {
    pool: DatabasePool,
}

impl PgEvidenceReader {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

fn map_completion(row: sqlx::postgres::PgRow) -> Result<EvidenceRow, BusinessError> {
    Ok(EvidenceRow {
        id: row.try_get("id").map_err(|_| unavailable())?,
        assignment_id: row.try_get("assignment_id").map_err(|_| unavailable())?,
        event_version: row.try_get("event_version").map_err(|_| unavailable())?,
        version: None,
        status: row.try_get("status").map_err(|_| unavailable())?,
        created_at: row.try_get("created_at").map_err(|_| unavailable())?,
        due_date: None,
        updated_at: None,
        course_title: row.try_get("course_title").map_err(|_| unavailable())?,
        required_for: None,
    })
}

fn map_overdue(row: sqlx::postgres::PgRow) -> Result<EvidenceRow, BusinessError> {
    Ok(EvidenceRow {
        id: row.try_get("id").map_err(|_| unavailable())?,
        assignment_id: None,
        event_version: None,
        version: row.try_get("version").map_err(|_| unavailable())?,
        status: row.try_get("status").map_err(|_| unavailable())?,
        created_at: None,
        due_date: row.try_get("due_date").map_err(|_| unavailable())?,
        updated_at: row.try_get("updated_at").map_err(|_| unavailable())?,
        course_title: row.try_get("course_title").map_err(|_| unavailable())?,
        required_for: row.try_get("required_for").map_err(|_| unavailable())?,
    })
}

impl EvidenceReader for PgEvidenceReader {
    fn read<'a>(
        &'a self,
        kind: EvidenceKind,
        query: &'a EvidenceQuery,
        company: &'a str,
        branch: Option<&'a str>,
        department: Option<&'a str>,
    ) -> EvidenceFuture<'a, Option<EvidenceRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let id = query.id.clone();
        let from = query.from;
        let to_exclusive = query.to_exclusive;
        let company = company.to_owned();
        let branch = branch.map(str::to_owned);
        let department = department.map(str::to_owned);
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let id = id.clone();
                let company = company.clone();
                let branch = branch.clone();
                let department = department.clone();
                Box::pin(async move {
                    let row = match kind {
                        EvidenceKind::Completion => {
                            sqlx::query(sqlx::AssertSqlSafe(COMPLETION_SQL))
                                .bind(id.as_str())
                                .bind(company.as_str())
                                .bind(branch.as_deref())
                                .bind(department.as_deref())
                                .bind(from)
                                .bind(to_exclusive)
                                .fetch_optional(&mut *connection)
                                .await
                                .map_err(|_| unavailable())?
                                .map(map_completion)
                                .transpose()?
                        }
                        EvidenceKind::Overdue => {
                            sqlx::query(sqlx::AssertSqlSafe(OVERDUE_SQL))
                                .bind(id.as_str())
                                .bind(company.as_str())
                                .bind(branch.as_deref())
                                .bind(department.as_deref())
                                .fetch_optional(&mut *connection)
                                .await
                                .map_err(|_| unavailable())?
                                .map(map_overdue)
                                .transpose()?
                        }
                    };
                    Ok(row)
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
    fn oracle_sql_is_read_only_and_company_scoped() {
        for sql in [COMPLETION_SQL, OVERDUE_SQL] {
            assert!(sql.starts_with("SELECT "));
            assert!(sql.contains("a.company_id=$2"));
            assert!(sql.contains("($3::text IS NULL OR a.branch_id=$3)"));
            assert!(sql.contains("($4::text IS NULL OR a.department_id=$4)"));
            assert!(!sql.to_uppercase().contains("INSERT"));
            assert!(!sql.to_uppercase().contains("UPDATE "));
            assert!(!sql.to_uppercase().contains("DELETE FROM"));
            assert!(!sql.to_uppercase().contains("FOR UPDATE"));
            assert!(!sql.to_uppercase().contains("FOR SHARE"));
            assert!(!sql.to_uppercase().contains("LOCK TABLE"));
            assert!(!sql.to_uppercase().contains("ADVISORY"));
        }
    }
}
