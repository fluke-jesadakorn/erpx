//! sqlx execution for the native Finance reference-option reads. This is the
//! only place in the lane that owns a statement or a Company bind.
//!
//! `TS_*_SQL` are the byte-identical statements rendered by the retired TS
//! reads (the third is rendered by `finPartyIdNameByCompanySql` in
//! `finance-read-tail-r57-sql.ts`, a shared renderer outside this lane).
//! `PG_*_SQL` differ only by the positional placeholder required by Postgres.
//! No predicate, limit or ordering is added here.

use super::application::{FinanceReferenceRead, GuardReadFuture, ReferenceReadFuture};
use super::domain::{GuardKind, GuardRow, ReferenceKind, ReferenceRow, unavailable};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::BusinessError;
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

pub const TS_CHART_ACTIVE_SQL: &str =
    "SELECT id FROM finance_charts WHERE id=? AND company_id=? AND status='active' AND base_currency=?";
pub const TS_ACCOUNT_LIABILITY_SQL: &str =
    "SELECT id FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND currency=? AND category='liability' AND status='active'";
pub const TS_PARTY_CUSTOMER_SQL: &str =
    "SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active' AND currency=? AND person_id IS NULL";

pub const PG_CHART_ACTIVE_SQL: &str =
    "SELECT id FROM finance_charts WHERE id=$1 AND company_id=$2 AND status='active' AND base_currency=$3";
pub const PG_ACCOUNT_LIABILITY_SQL: &str =
    "SELECT id FROM finance_accounts WHERE id=$1 AND company_id=$2 AND chart_id=$3 AND currency=$4 AND category='liability' AND status='active'";
pub const PG_PARTY_CUSTOMER_SQL: &str =
    "SELECT 1 FROM finance_parties WHERE id=$1 AND company_id=$2 AND kind='customer' AND status='active' AND currency=$3 AND person_id IS NULL";

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

    fn guard<'a>(
        &'a self,
        kind: GuardKind,
        company: &'a str,
        id: &'a str,
        chart: Option<&'a str>,
        currency: &'a str,
    ) -> GuardReadFuture<'a> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let id = id.to_owned();
        let chart = chart.map(str::to_owned);
        let currency = currency.to_owned();
        Box::pin(async move {
            let row = match kind {
                GuardKind::ChartActive => sqlx::query(PG_CHART_ACTIVE_SQL)
                    .bind(id.as_str())
                    .bind(company.as_str())
                    .bind(currency.as_str())
                    .fetch_optional(pool.raw())
                    .await
                    .map_err(|_| unavailable())?,
                GuardKind::AccountLiability => {
                    let chart = chart.ok_or_else(unavailable)?;
                    sqlx::query(PG_ACCOUNT_LIABILITY_SQL)
                        .bind(id.as_str())
                        .bind(company.as_str())
                        .bind(chart.as_str())
                        .bind(currency.as_str())
                        .fetch_optional(pool.raw())
                        .await
                        .map_err(|_| unavailable())?
                }
                GuardKind::PartyCustomer => sqlx::query(PG_PARTY_CUSTOMER_SQL)
                    .bind(id.as_str())
                    .bind(company.as_str())
                    .bind(currency.as_str())
                    .fetch_optional(pool.raw())
                    .await
                    .map_err(|_| unavailable())?,
            };
            Ok(row.map(|_| GuardRow { id: id.clone() }))
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

pub fn rendered_guard_sql(kind: GuardKind) -> &'static str {
    match kind {
        GuardKind::ChartActive => TS_CHART_ACTIVE_SQL,
        GuardKind::AccountLiability => TS_ACCOUNT_LIABILITY_SQL,
        GuardKind::PartyCustomer => TS_PARTY_CUSTOMER_SQL,
    }
}

pub fn executed_guard_sql(kind: GuardKind) -> &'static str {
    match kind {
        GuardKind::ChartActive => PG_CHART_ACTIVE_SQL,
        GuardKind::AccountLiability => PG_ACCOUNT_LIABILITY_SQL,
        GuardKind::PartyCustomer => PG_PARTY_CUSTOMER_SQL,
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

    fn positional(sql: &str) -> String {
        let mut out = String::with_capacity(sql.len());
        let mut n = 1;
        for ch in sql.chars() {
            if ch == '?' {
                out.push('$');
                out.push_str(&n.to_string());
                n += 1;
            } else {
                out.push(ch);
            }
        }
        out
    }

    #[test]
    fn guard_statements_are_byte_identical_and_pinned() {
        for kind in [
            GuardKind::ChartActive,
            GuardKind::AccountLiability,
            GuardKind::PartyCustomer,
        ] {
            let ts = rendered_guard_sql(kind);
            assert_eq!(executed_guard_sql(kind), positional(ts));
            assert!(ts.contains("company_id=?"));
            assert!(!ts.contains("FOR UPDATE"));
            assert!(!ts.contains("FOR SHARE"));
        }
        let expected = [
            (GuardKind::ChartActive, "e0d6087cb00a7d070006fbe0e3d60af1a171f69dc789c3377a6769f2b204f50e"),
            (GuardKind::AccountLiability, "58f166cb8e98ac1f60a648e149316ea01e6910645b3af0e4cd80f33bb5c6a061"),
            (GuardKind::PartyCustomer, "7d208210d860c9156ed05c89e3ff05384688efddfece5419eb0cb54ca93e046e"),
        ];
        for (kind, digest) in expected {
            assert_eq!(sha256(rendered_guard_sql(kind)), digest, "{kind:?}");
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
