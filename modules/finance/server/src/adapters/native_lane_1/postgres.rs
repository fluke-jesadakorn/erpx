//! sqlx execution for lane 1. This is the only module that owns SQL text or
//! touches the pool. The statements below are the byte-identical literals
//! retired from `adapters/expense-posting-projection.ts`; `render_sql` only
//! translates PostgreSQL's `?` placeholders to sqlx's positional `$n`, never
//! a predicate, projection or bind order.

use super::application::{ExpenseProjectionReader, LaneReadFuture};
use super::domain::{CompanyScope, JournalLineRow, PostingAccountRow, ProjectionKind};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::Row;

/// Exact retired TypeScript literal for `expenseJournalProjection`.
pub const SQL_JOURNAL_LINES: &str = "SELECT a.id,a.code,a.name,l.debit_minor,l.credit_minor FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id JOIN finance_accounts a ON a.id=l.account_id WHERE j.company_id=? AND j.id=? ORDER BY l.ordinal";

/// Exact retired TypeScript literal for `expensePostingAccounts`.
pub const SQL_POSTING_ACCOUNTS: &str =
    "SELECT id,code,name,status,chart_id,currency FROM finance_accounts WHERE company_id=?";

/// Static sqlx form of `SQL_JOURNAL_LINES`. A `'static` literal satisfies the
/// sqlx SQL-safety bound; a unit test proves it is exactly `render_sql` of the
/// byte-identical retired literal, so neither predicate nor bind order drifts.
pub const SQLX_JOURNAL_LINES: &str = "SELECT a.id,a.code,a.name,l.debit_minor,l.credit_minor FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id JOIN finance_accounts a ON a.id=l.account_id WHERE j.company_id=$1 AND j.id=$2 ORDER BY l.ordinal";

/// Static sqlx form of `SQL_POSTING_ACCOUNTS`.
pub const SQLX_POSTING_ACCOUNTS: &str =
    "SELECT id,code,name,status,chart_id,currency FROM finance_accounts WHERE company_id=$1";

pub fn sql_for(kind: ProjectionKind) -> &'static str {
    match kind {
        ProjectionKind::JournalLines => SQL_JOURNAL_LINES,
        ProjectionKind::PostingAccounts => SQL_POSTING_ACCOUNTS,
    }
}

/// Translate `?` to `$1..$n` in encounter order. The bind order is unchanged:
#[doc = " journal lines bind `[company_id, journal_id]`, accounts bind `[company_id]`."]
pub fn render_sql(literal: &str) -> String {
    let mut next = 0usize;
    literal
        .chars()
        .map(|c| {
            if c == '?' {
                next += 1;
                format!("${next}")
            } else {
                c.to_string()
            }
        })
        .collect()
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LANE_01_NATIVE_READ_UNAVAILABLE",
        "The native Finance lane 1 read is unavailable.",
    )
}

pub fn hash(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Postgres-backed owner reader. Composition supplies the pool; no caller can
/// supply SQL, a predicate or a Company outside the routing scope.
pub struct PgExpenseProjectionReader {
    pool: DatabasePool,
}

impl PgExpenseProjectionReader {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl ExpenseProjectionReader for PgExpenseProjectionReader {
    fn journal_lines<'a>(
        &'a self,
        scope: &'a CompanyScope,
        journal_id: &'a str,
    ) -> LaneReadFuture<'a, Vec<JournalLineRow>> {
        let pool = self.pool.clone();
        let company = scope.company_id.clone();
        let journal = journal_id.to_owned();
        Box::pin(async move {
            let rows = sqlx::query(SQLX_JOURNAL_LINES)
                .bind(company.as_str())
                .bind(journal.as_str())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(JournalLineRow {
                        id: row.try_get("id").map_err(|_| unavailable())?,
                        code: row.try_get("code").map_err(|_| unavailable())?,
                        name: row.try_get("name").map_err(|_| unavailable())?,
                        debit_minor: row.try_get("debit_minor").map_err(|_| unavailable())?,
                        credit_minor: row.try_get("credit_minor").map_err(|_| unavailable())?,
                    })
                })
                .collect()
        })
    }

    fn posting_accounts<'a>(
        &'a self,
        scope: &'a CompanyScope,
    ) -> LaneReadFuture<'a, Vec<PostingAccountRow>> {
        let pool = self.pool.clone();
        let company = scope.company_id.clone();
        Box::pin(async move {
            let rows = sqlx::query(SQLX_POSTING_ACCOUNTS)
                .bind(company.as_str())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(PostingAccountRow {
                        id: row.try_get("id").map_err(|_| unavailable())?,
                        code: row.try_get("code").map_err(|_| unavailable())?,
                        name: row.try_get("name").map_err(|_| unavailable())?,
                        status: row.try_get("status").map_err(|_| unavailable())?,
                        chart_id: row.try_get("chart_id").map_err(|_| unavailable())?,
                        currency: row.try_get("currency").map_err(|_| unavailable())?,
                    })
                })
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_is_fail_closed_unavailable() {
        let error = unavailable();
        assert_eq!(error.kind, FailureKind::Unavailable);
        assert_eq!(error.code, "LANE_01_NATIVE_READ_UNAVAILABLE");
    }

    #[test]
    fn sql_and_result_oracles_are_pinned() {
        assert_eq!(
            hash(SQL_JOURNAL_LINES),
            "5d0f1e3346f327fd693a72119094b844298e6e57d35552230be1a5c4534714db"
        );
        assert_eq!(
            hash(SQL_POSTING_ACCOUNTS),
            "f8c0fe37f090f6fd6cbeb598c2dc56c56046676d715cb61e675ff4689ba4831f"
        );
        assert_eq!(
            hash(&render_sql(SQL_JOURNAL_LINES)),
            "e91f823204e9be8bf579a500641156e7482a66fe3c9de11004edf17b1bac7422"
        );
        assert_eq!(
            hash(&render_sql(SQL_POSTING_ACCOUNTS)),
            "d8c37f12deb8370201967242f2b7817bc9abd9ca008986ef39e03911aacffac5"
        );
    }

    #[test]
    fn static_sqlx_literals_match_the_rendered_retired_sql() {
        assert_eq!(SQLX_JOURNAL_LINES, render_sql(SQL_JOURNAL_LINES));
        assert_eq!(SQLX_POSTING_ACCOUNTS, render_sql(SQL_POSTING_ACCOUNTS));
    }

    #[test]
    fn placeholder_translation_preserves_bind_order() {
        assert_eq!(
            SQLX_JOURNAL_LINES,
            "SELECT a.id,a.code,a.name,l.debit_minor,l.credit_minor FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id JOIN finance_accounts a ON a.id=l.account_id WHERE j.company_id=$1 AND j.id=$2 ORDER BY l.ordinal"
        );
        assert_eq!(
            SQLX_POSTING_ACCOUNTS,
            "SELECT id,code,name,status,chart_id,currency FROM finance_accounts WHERE company_id=$1"
        );
        assert_eq!(sql_for(ProjectionKind::JournalLines), SQL_JOURNAL_LINES);
        assert_eq!(
            sql_for(ProjectionKind::PostingAccounts),
            SQL_POSTING_ACCOUNTS
        );
    }

    /// The exact response envelope the TS contract test pins. Its digest is
    /// shared between the Rust and TypeScript oracles.
    pub const RESULT_ORACLE_JSON: &str = r#"{"kind":"journal-lines","rows":[{"id":"line-1","code":"1210","name":"Receivable","debit_minor":500,"credit_minor":0}]}"#;

    #[test]
    fn result_oracle_hash_is_pinned() {
        assert_eq!(
            hash(RESULT_ORACLE_JSON),
            "0eeb2dec548bcf68643f4b2e6f1040c51420eacb93d4ced04cb48a8b11f51eea"
        );
    }
}
