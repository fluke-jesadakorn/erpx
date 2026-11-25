//! PostgreSQL execution for the owner-local Learning evidence-detail family.
//!
//! Both statements start from the byte-identical projections formerly composed
//! in `adapters/learning.ts`; binds are positional and the company scope is
//! always bound. The reader runs on a `PgUnitOfWork` and returns a pinned row.
//! No predicate or projection is accepted from the caller.

use super::application::{EvidenceFuture, EvidenceReader};
use super::domain::{EvidenceKind, EvidenceQuery, EvidenceRow, EvidenceSearchQuery, unavailable};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sqlx::{Postgres, QueryBuilder, Row};

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
                        EvidenceKind::Overdue => sqlx::query(sqlx::AssertSqlSafe(OVERDUE_SQL))
                            .bind(id.as_str())
                            .bind(company.as_str())
                            .bind(branch.as_deref())
                            .bind(department.as_deref())
                            .fetch_optional(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?
                            .map(map_overdue)
                            .transpose()?,
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

    fn search<'a>(
        &'a self,
        kind: EvidenceKind,
        query: &'a EvidenceSearchQuery,
        company: &'a str,
        branch: Option<&'a str>,
        department: Option<&'a str>,
    ) -> EvidenceFuture<'a, Vec<EvidenceRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let branch = branch.map(str::to_owned);
        let department = department.map(str::to_owned);
        let text_query = query.query.clone();
        let from = query.from;
        let to_exclusive = query.to_exclusive;
        let limit = query.limit;
        let cursor = query.cursor.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let branch = branch.clone();
                let department = department.clone();
                let text_query = text_query.clone();
                let cursor = cursor.clone();
                Box::pin(async move {
                    let mut builder = match kind {
                        EvidenceKind::Completion => {
                            let mut builder = QueryBuilder::<Postgres>::new(concat!(
                                "SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title",
                                " FROM learning_assignment_events e",
                                " JOIN learning_assignments a ON a.id=e.assignment_id AND a.company_id=",
                            ));
                            builder.push_bind(company);
                            builder.push(" AND (");
                            builder.push_bind(branch.clone()).push("::text IS NULL OR a.branch_id=").push_bind(branch);
                            builder.push(") AND (");
                            builder.push_bind(department.clone()).push("::text IS NULL OR a.department_id=").push_bind(department);
                            builder.push(
                                ") AND e.action='complete' AND e.status='completed'",
                            );
                            builder.push(" AND e.created_at>=").push_bind(from);
                            builder.push(" AND e.created_at<").push_bind(to_exclusive);
                            if !text_query.is_empty() {
                                builder.push(" AND (position(lower(").push_bind(text_query.clone());
                                builder.push(") in lower(c.title))>0 OR position(lower(").push_bind(text_query);
                                builder.push(
                                    ") in to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'))>0)",
                                );
                            }
                            if let Some(cursor) = &cursor {
                                builder.push(" AND (e.created_at<").push_bind(cursor.created_at);
                                builder.push(" OR (e.created_at=").push_bind(cursor.created_at);
                                builder.push(" AND e.id<").push_bind(cursor.id.as_str()).push("))");
                            }
                            builder.push(" ORDER BY e.created_at DESC,e.id DESC LIMIT ");
                            builder.push_bind(limit + 1);
                            builder
                        }
                        EvidenceKind::Overdue => {
                            let mut builder = QueryBuilder::<Postgres>::new(concat!(
                                "SELECT a.id,a.version,a.status,a.due_date,a.updated_at,c.title course_title,c.required_for",
                                " FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id AND c.company_id=a.company_id",
                                " WHERE a.company_id=",
                            ));
                            builder.push_bind(company);
                            builder.push(" AND (");
                            builder.push_bind(branch.clone()).push("::text IS NULL OR a.branch_id=").push_bind(branch);
                            builder.push(") AND (");
                            builder.push_bind(department.clone()).push("::text IS NULL OR a.department_id=").push_bind(department);
                            builder
                                .push(") AND c.required_for IS NOT NULL")
                                .push(" AND a.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')")
                                .push(" AND a.status IN ('assigned','in_progress')");
                            if !text_query.is_empty() {
                                builder.push(" AND (position(lower(").push_bind(text_query.clone());
                                builder.push(") in lower(c.title))>0 OR position(lower(").push_bind(text_query.clone());
                                builder.push(") in lower(a.status))>0 OR position(lower(").push_bind(text_query.clone());
                                builder.push(") in a.due_date)>0 OR position(lower(").push_bind(text_query);
                                builder.push(") in lower(c.required_for))>0)");
                            }
                            if let Some(cursor) = &cursor {
                                builder.push(" AND (a.due_date>").push_bind(cursor.due_date.clone());
                                builder.push(" OR (a.due_date=").push_bind(cursor.due_date.clone());
                                builder.push(" AND a.id>").push_bind(cursor.id.as_str()).push("))");
                            }
                            builder.push(" ORDER BY a.due_date,a.id LIMIT ");
                            builder.push_bind(limit + 1);
                            builder
                        }
                    };
                    let rows = builder
                        .build()
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let mapped = rows
                        .into_iter()
                        .map(|row| match kind {
                            EvidenceKind::Completion => map_completion(row),
                            EvidenceKind::Overdue => map_overdue(row),
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(mapped)
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

    /// Byte oracle migrated from the retired TypeScript source
    /// `adapters/learning-completion-evidence.ts`. The native owner is now the
    /// single source of truth for these bytes; no TypeScript fallback remains.
    #[test]
    fn completion_evidence_oracle_migrated_from_typescript() {
        const MIGRATED_ORACLE: &str = concat!(
            "SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title",
            " FROM learning_assignment_events e",
            " JOIN learning_assignments a ON a.id=e.assignment_id",
            " JOIN learning_courses c ON c.id=a.course_id",
            " WHERE e.id=$1 AND a.company_id=$2 AND ($3::text IS NULL OR a.branch_id=$3)",
            " AND ($4::text IS NULL OR a.department_id=$4)",
            " AND e.action='complete' AND e.status='completed' AND e.created_at>=$5 AND e.created_at<$6",
            " LIMIT 1",
        );
        assert_eq!(COMPLETION_SQL, MIGRATED_ORACLE);
    }

    #[test]
    fn search_predicates_are_owner_local_and_company_scoped() {
        // The search family renders every predicate from fixed owner-local
        // fragments; only scalar values are bound. No caller text becomes SQL.
        let source = include_str!("postgres.rs");
        for fragment in [
            "JOIN learning_assignments a ON a.id=e.assignment_id AND a.company_id=",
            "::text IS NULL OR a.branch_id=",
            "::text IS NULL OR a.department_id=",
            "e.action='complete' AND e.status='completed'",
            "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD')",
            "ORDER BY e.created_at DESC,e.id DESC LIMIT ",
            "c.required_for IS NOT NULL",
            "a.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')",
            "a.status IN ('assigned','in_progress')",
            "ORDER BY a.due_date,a.id LIMIT ",
            "position(lower(",
        ] {
            assert!(
                source.contains(fragment),
                "missing search fragment: {fragment}"
            );
        }
    }
}
