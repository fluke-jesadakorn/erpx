//! sqlx execution for the native Finance reference-option reads. This is the
//! only place in the lane that owns a statement or a Company bind.
//!
//! `TS_*_SQL` are the byte-identical statements rendered by the retired TS
//! reads (the third is rendered by `finPartyIdNameByCompanySql` in
//! `finance-read-tail-r57-sql.ts`, a shared renderer outside this lane).
//! `PG_*_SQL` differ only by the positional placeholder required by Postgres.
//! No predicate, limit or ordering is added here.

use super::application::{FinanceReferenceRead, ReferenceReadFuture};
use super::domain::{ReferenceKind, ReferenceRow, unavailable};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

pub const TS_ACCOUNTS_SQL: &str =
    "SELECT id,name,code,chart_id,category FROM finance_accounts WHERE company_id=? AND status='active' AND currency='THB' ORDER BY code";
pub const TS_CHARTS_SQL: &str =
    "SELECT id,name FROM finance_charts WHERE company_id=? AND base_currency='THB' AND status='active'";
pub const TS_PARTIES_SQL: &str =
    "SELECT id,name FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' AND currency='THB' AND person_id IS NULL";

pub const PG_ACCOUNTS_SQL: &str =
    "SELECT id,name,code,chart_id,category FROM finance_accounts WHERE company_id=$1 AND status='active' AND currency='THB' ORDER BY code";
pub const PG_CHARTS_SQL: &str =
    "SELECT id,name FROM finance_charts WHERE company_id=$1 AND base_currency='THB' AND status='active'";
pub const PG_PARTIES_SQL: &str =
    "SELECT id,name FROM finance_parties WHERE company_id=$1 AND kind='customer' AND status='active' AND currency='THB' AND person_id IS NULL";

/// The owner reader. It never sees a caller-supplied statement; each registered
/// kind resolves to exactly one constant above.
pub struct PgFinanceReferenceRead {
    pool: DatabasePool,
}

impl PgFinanceReferenceRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

fn text(row: &sqlx::postgres::PgRow, column: &str) -> Result<String, BusinessError> {
    row.try_get::<String, _>(column).map_err(|_| unavailable())
}

fn optional_text(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<String>, BusinessError> {
    row.try_get::<Option<String>, _>(column)
        .map_err(|_| unavailable())
}

fn accounts_row(row: &sqlx::postgres::PgRow) -> Result<ReferenceRow, BusinessError> {
    Ok(ReferenceRow {
        id: text(row, "id")?,
        name: text(row, "name")?,
        code: optional_text(row, "code")?,
        chart_id: optional_text(row, "chart_id")?,
        category: optional_text(row, "category")?,
    })
}

fn named_row(row: &sqlx::postgres::PgRow) -> Result<ReferenceRow, BusinessError> {
    Ok(ReferenceRow {
        id: text(row, "id")?,
        name: text(row, "name")?,
        code: None,
        chart_id: None,
        category: None,
    })
}

impl FinanceReferenceRead for PgFinanceReferenceRead {
    fn read<'a>(&'a self, kind: ReferenceKind, company: &'a str) -> ReferenceReadFuture<'a> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        Box::pin(async move {
            let rows = match kind {
                ReferenceKind::Accounts => {
                    sqlx::query(PG_ACCOUNTS_SQL)
                        .bind(company.as_str())
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?
                }
                ReferenceKind::Charts => {
                    sqlx::query(PG_CHARTS_SQL)
                        .bind(company.as_str())
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?
                }
                ReferenceKind::Parties => {
                    sqlx::query(PG_PARTIES_SQL)
                        .bind(company.as_str())
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?
                }
            };
            let project = match kind {
                ReferenceKind::Accounts => accounts_row,
                ReferenceKind::Charts => named_row,
                ReferenceKind::Parties => named_row,
            };
            rows.iter().map(project).collect()
        })
    }
}

/// Byte-for-byte mapping from the retired TS renderings to the Postgres
/// statements executed above. Used by the parity oracle test.
pub fn rendered_sql(kind: ReferenceKind) -> &'static str {
    match kind {
        ReferenceKind::Accounts => TS_ACCOUNTS_SQL,
        ReferenceKind::Charts => TS_CHARTS_SQL,
        ReferenceKind::Parties => TS_PARTIES_SQL,
    }
}

pub fn executed_sql(kind: ReferenceKind) -> &'static str {
    match kind {
        ReferenceKind::Accounts => PG_ACCOUNTS_SQL,
        ReferenceKind::Charts => PG_CHARTS_SQL,
        ReferenceKind::Parties => PG_PARTIES_SQL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    fn sha256(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn statements_are_byte_identical_to_the_retired_ts_renderings() {
        for kind in [
            ReferenceKind::Accounts,
            ReferenceKind::Charts,
            ReferenceKind::Parties,
        ] {
            assert_eq!(
                executed_sql(kind),
                rendered_sql(kind).replace('?', "$1"),
                "{kind:?} must not gain a predicate, limit or ordering"
            );
            let sql = rendered_sql(kind);
            assert!(sql.starts_with("SELECT "));
            assert!(sql.contains("company_id=?"));
            assert!(!sql.contains(';'));
        }
    }

    #[test]
    fn sql_and_result_oracles_are_stable() {
        // Rendered SQL hashes are pinned against the retired TS strings; the
        // result oracle pins the exact projected response for one fixed row.
        let expected = [
            (
                ReferenceKind::Accounts,
                "7acdf63122f6310dbf047a2b45c42b14c5fe40c018300d5761e851801aa71dcc",
            ),
            (
                ReferenceKind::Charts,
                "6ce4d387ffc287dd1f9f87b5864e138ebf2da6c085fc35d10cffacd17694a5db",
            ),
            (
                ReferenceKind::Parties,
                "532180fd2b13a268349acafc704a456d9fbaf178bc36cfcc19fbc0e81f6d5e78",
            ),
        ];
        for (kind, digest) in expected {
            assert_eq!(sha256(rendered_sql(kind)), digest, "{kind:?}");
        }
        let row = ReferenceRow {
            id: "acct-1".to_owned(),
            name: "Cash".to_owned(),
            code: Some("100".to_owned()),
            chart_id: Some("chart-1".to_owned()),
            category: Some("asset".to_owned()),
        };
        let result = json!([super::super::domain::project(ReferenceKind::Accounts, &row).unwrap()]).to_string();
        assert_eq!(
            sha256(&result),
            "603f1efb79efc13158e69910dba1850f76377aafec03ee83b5fc8808be23fa13"
        );
    }
}
