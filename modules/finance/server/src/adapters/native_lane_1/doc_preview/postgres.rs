//! sqlx execution for the lane 1 document preview reads. SQL lives only here.
//! The retired TypeScript literals are kept verbatim as oracle constants; the
//! journal-lines read gains an owner Company predicate because the retired
//! statement filtered by journal id alone.

use super::application::{DocPreviewFuture, DocPreviewReader};
use super::domain::{PreviewAccountRow, PreviewJournalLineRow, PreviewStockConfigurationRow};
use crate::adapters::native_lane_1::domain::CompanyScope;
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::Row;
#[cfg(test)]
use super::domain::DocPreviewKind;
#[cfg(test)]
use sha2::{Digest, Sha256};

pub const SQL_DOC_ACCOUNTS: &str =
    "SELECT id,code,name,status FROM finance_accounts WHERE company_id=?";

pub const SQL_DOC_JOURNAL_LINES_ORIGINAL: &str = "SELECT account_id id,debit_minor debit,credit_minor credit FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal";
/// Company-scoped successor of the retired literal. The join to
/// `finance_journals` binds the caller's registered Company before the journal
/// id; no other predicate or projection changed.
pub const SQL_DOC_JOURNAL_LINES: &str = "SELECT l.account_id id,l.debit_minor debit,l.credit_minor credit FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND l.journal_id=? ORDER BY l.ordinal";

pub const SQL_DOC_STOCK_CONFIGURATION: &str = "SELECT s.inventory_account_id,a.id variance_account_id FROM commerce_settings s JOIN finance_accounts a ON a.chart_id=s.chart_id AND a.company_id=s.company_id AND a.code='5400' AND a.category='expense' AND a.status='active' WHERE s.company_id=?";

pub const SQLX_DOC_ACCOUNTS: &str =
    "SELECT id,code,name,status FROM finance_accounts WHERE company_id=$1";
pub const SQLX_DOC_JOURNAL_LINES: &str = "SELECT l.account_id id,l.debit_minor debit,l.credit_minor credit FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=$1 AND l.journal_id=$2 ORDER BY l.ordinal";
pub const SQLX_DOC_STOCK_CONFIGURATION: &str = "SELECT s.inventory_account_id,a.id variance_account_id FROM commerce_settings s JOIN finance_accounts a ON a.chart_id=s.chart_id AND a.company_id=s.company_id AND a.code='5400' AND a.category='expense' AND a.status='active' WHERE s.company_id=$1";

#[cfg(test)]
pub fn sql_for(kind: DocPreviewKind) -> &'static str {
    match kind {
        DocPreviewKind::Accounts => SQL_DOC_ACCOUNTS,
        DocPreviewKind::JournalLines => SQL_DOC_JOURNAL_LINES,
        DocPreviewKind::StockConfiguration => SQL_DOC_STOCK_CONFIGURATION,
    }
}

/// Translate `?` to `$1..$n` in encounter order; bind order never changes.
#[cfg(test)]
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
        "LANE_01_DOC_PREVIEW_UNAVAILABLE",
        "The native document posting preview read is unavailable.",
    )
}

#[cfg(test)]
pub fn hash(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub struct PgDocPreviewReader {
    pool: DatabasePool,
}

impl PgDocPreviewReader {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl DocPreviewReader for PgDocPreviewReader {
    fn accounts<'a>(
        &'a self,
        scope: &'a CompanyScope,
    ) -> DocPreviewFuture<'a, Vec<PreviewAccountRow>> {
        let pool = self.pool.clone();
        let company = scope.company_id.clone();
        Box::pin(async move {
            let rows = sqlx::query(SQLX_DOC_ACCOUNTS)
                .bind(company.as_str())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(PreviewAccountRow {
                        id: row.try_get("id").map_err(|_| unavailable())?,
                        code: row.try_get("code").map_err(|_| unavailable())?,
                        name: row.try_get("name").map_err(|_| unavailable())?,
                        status: row.try_get("status").map_err(|_| unavailable())?,
                    })
                })
                .collect()
        })
    }

    fn journal_lines<'a>(
        &'a self,
        scope: &'a CompanyScope,
        journal_id: &'a str,
    ) -> DocPreviewFuture<'a, Vec<PreviewJournalLineRow>> {
        let pool = self.pool.clone();
        let company = scope.company_id.clone();
        let journal = journal_id.to_owned();
        Box::pin(async move {
            let rows = sqlx::query(SQLX_DOC_JOURNAL_LINES)
                .bind(company.as_str())
                .bind(journal.as_str())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(PreviewJournalLineRow {
                        id: row.try_get("id").map_err(|_| unavailable())?,
                        debit: row.try_get("debit").map_err(|_| unavailable())?,
                        credit: row.try_get("credit").map_err(|_| unavailable())?,
                    })
                })
                .collect()
        })
    }

    fn stock_configuration<'a>(
        &'a self,
        scope: &'a CompanyScope,
    ) -> DocPreviewFuture<'a, Option<PreviewStockConfigurationRow>> {
        let pool = self.pool.clone();
        let company = scope.company_id.clone();
        Box::pin(async move {
            let row = sqlx::query(SQLX_DOC_STOCK_CONFIGURATION)
                .bind(company.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(PreviewStockConfigurationRow {
                    inventory_account_id: row
                        .try_get("inventory_account_id")
                        .map_err(|_| unavailable())?,
                    variance_account_id: row
                        .try_get("variance_account_id")
                        .map_err(|_| unavailable())?,
                })
            })
            .transpose()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_sqlx_literals_match_rendered_owner_sql() {
        assert_eq!(SQLX_DOC_ACCOUNTS, render_sql(SQL_DOC_ACCOUNTS));
        assert_eq!(SQLX_DOC_JOURNAL_LINES, render_sql(SQL_DOC_JOURNAL_LINES));
        assert_eq!(
            SQLX_DOC_STOCK_CONFIGURATION,
            render_sql(SQL_DOC_STOCK_CONFIGURATION)
        );
    }

    #[test]
    fn sql_oracles_are_pinned() {
        assert_eq!(
            hash(SQL_DOC_ACCOUNTS),
            "c408c881433c13cdd0eb41702bc621784053af55769857c8b947206ba06a5fcc"
        );
        assert_eq!(
            hash(SQL_DOC_JOURNAL_LINES_ORIGINAL),
            "521029aa4b962b93a4f4bf06b97c10f6edbf565f599f6f36f83ce9c98ceacfa5"
        );
        assert_eq!(
            hash(SQL_DOC_JOURNAL_LINES),
            "11f89684aee589e754f10306ca8e365d0101ef7879b52d0749e4f8b2b6a31e9e"
        );
        assert_eq!(
            hash(SQL_DOC_STOCK_CONFIGURATION),
            "62919d5460c74425c09bea6aac557d3ad312537332b2f47333c5617c7b489559"
        );
        assert_eq!(sql_for(DocPreviewKind::Accounts), SQL_DOC_ACCOUNTS);
        assert_eq!(sql_for(DocPreviewKind::JournalLines), SQL_DOC_JOURNAL_LINES);
        assert_eq!(
            sql_for(DocPreviewKind::StockConfiguration),
            SQL_DOC_STOCK_CONFIGURATION
        );
    }

    #[test]
    fn failure_is_fail_closed_unavailable() {
        assert_eq!(unavailable().code, "LANE_01_DOC_PREVIEW_UNAVAILABLE");
        assert_eq!(unavailable().kind, FailureKind::Unavailable);
    }

    /// Pinned result envelope shared with the TypeScript contract test.
    pub const RESULT_ORACLE_JSON: &str = r#"{"kind":"doc-accounts","rows":[{"id":"acc-1","code":"1210","name":"Receivable","status":"active"}]}"#;

    #[test]
    fn result_oracle_is_pinned() {
        assert_eq!(
            hash(RESULT_ORACLE_JSON),
            "15ead879f6b450d1c847973bfb50bfc56c3b4bfb9d49cde8a384916565413de3"
        );
    }
}
