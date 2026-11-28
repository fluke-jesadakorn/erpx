//! sqlx execution for the native Finance overview-counts and invoice reads.
//!
//! The overview statement is the byte-identical successor of the seven
//! subqueries that used to live in `finance-invoice-read.ts`: only the `?`
//! placeholders were renumbered to `$1..$7`, in the original bind order. The
//! invoice page and historical aging statements are executed by the existing
//! owner reads in `finance_invoice_read.rs`, inside a serializable unit. Every
//! table is Finance-owned and every predicate carries the Company bind.

use crate::adapters::{
    finance_invoice_read::{InvoicePageQuery, read_invoice_aging, read_invoice_page},
    native_lane_2::{
        application::{
            AgingFuture, CompanyToolsFuture, CompanyToolsQuery, InvoicePageFuture, InvoiceReadQuery,
            OverviewCountsFuture, OverviewCountsQuery,
        },
        domain::{
            AgingRequest, CompanyTool, InvoicePageRequest, InvoicePageWire, OverviewCounts,
            unavailable,
        },
    },
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use sqlx::Row;

/// The rendered TS statement with `?` replaced by positional `$n` binds.
pub const OVERVIEW_COUNTS_SQL: &str = r#"SELECT
    (SELECT count(*) FROM finance_journals WHERE company_id=$1 ) AS journals,
    (SELECT count(*) FROM finance_journals WHERE company_id=$2 AND status='draft') AS posting,
    (SELECT count(*) FROM finance_invoices WHERE company_id=$3) AS invoices,
    (SELECT count(*) FROM finance_payments WHERE company_id=$4) AS payments,
    (SELECT count(*) FROM finance_invoices WHERE company_id=$5 AND kind='receivable' AND status IN ('posted','partially_paid') AND outstanding_minor>0) AS receivable,
    (SELECT count(*) FROM finance_invoices WHERE company_id=$6 AND kind='payable' AND status IN ('posted','partially_paid') AND outstanding_minor>0) AS payable,
    (SELECT count(*) FROM finance_bank_transactions WHERE company_id=$7 AND match_status<>'matched') AS reconcile"#;

/// Postgres-backed owner reader. The pool is owned by composition; the reader
/// never accepts caller SQL or a caller-supplied predicate.
pub struct PgOverviewCountsQuery {
    pool: DatabasePool,
}

impl PgOverviewCountsQuery {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl OverviewCountsQuery for PgOverviewCountsQuery {
    fn counts<'a>(&'a self, company_id: &'a str) -> OverviewCountsFuture<'a> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company_id = company_id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company_id = company_id.clone();
                Box::pin(async move {
                    let row = sqlx::query(OVERVIEW_COUNTS_SQL)
                        .bind(company_id.as_str())
                        .bind(company_id.as_str())
                        .bind(company_id.as_str())
                        .bind(company_id.as_str())
                        .bind(company_id.as_str())
                        .bind(company_id.as_str())
                        .bind(company_id.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(OverviewCounts {
                        journals: row.try_get("journals").map_err(|_| unavailable())?,
                        posting: row.try_get("posting").map_err(|_| unavailable())?,
                        invoices: row.try_get("invoices").map_err(|_| unavailable())?,
                        payments: row.try_get("payments").map_err(|_| unavailable())?,
                        receivable: row.try_get("receivable").map_err(|_| unavailable())?,
                        payable: row.try_get("payable").map_err(|_| unavailable())?,
                        reconcile: row.try_get("reconcile").map_err(|_| unavailable())?,
                    })
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

/// The rendered TS statement with its single `?` bind renumbered to `$1`.
pub const COMPANY_TOOLS_SQL: &str =
    "SELECT * FROM finance_company_tools WHERE company_id=$1 ORDER BY tool";

/// Postgres-backed owner reader for the standalone tool-list read.
pub struct PgCompanyToolsQuery {
    pool: DatabasePool,
}

impl PgCompanyToolsQuery {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl CompanyToolsQuery for PgCompanyToolsQuery {
    fn company_tools<'a>(&'a self, company_id: &'a str) -> CompanyToolsFuture<'a> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company_id = company_id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company_id = company_id.clone();
                Box::pin(async move {
                    let rows = sqlx::query(COMPANY_TOOLS_SQL)
                        .bind(company_id.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.into_iter()
                        .map(|row| {
                            Ok(CompanyTool {
                                tool: row.try_get("tool").map_err(|_| unavailable())?,
                                enabled: row.try_get("enabled").map_err(|_| unavailable())?,
                                version: row.try_get("version").map_err(|_| unavailable())?,
                            })
                        })
                        .collect()
                })
            })
            .await
            .map_err(to_business)
        })
    }
}

fn to_business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// Postgres-backed owner reader for the invoice page and aging family.
pub struct PgInvoiceReadQuery {
    pool: DatabasePool,
}

impl PgInvoiceReadQuery {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl InvoiceReadQuery for PgInvoiceReadQuery {
    fn page<'a>(
        &'a self,
        company_id: &'a str,
        request: &'a InvoicePageRequest,
    ) -> InvoicePageFuture<'a> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company_id = company_id.to_owned();
        let query = InvoicePageQuery {
            q: request.q.clone().unwrap_or_default(),
            kind: request.kind.clone().unwrap_or_default(),
            status: request.status.clone().unwrap_or_default(),
            queue: request.queue.clone().unwrap_or_default(),
            chart_id: request.chart_id.clone(),
            cursor: request.cursor.clone(),
        };
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company_id = company_id.clone();
                let query = query.clone();
                Box::pin(async move {
                    let page = read_invoice_page(connection, &company_id, &query)
                        .await
                        .map_err(to_business)?;
                    Ok(InvoicePageWire {
                        items: page.items,
                        next_cursor: page.next_cursor,
                        previous_cursor: page.previous_cursor,
                        has_more: page.has_more,
                    })
                })
            })
            .await
            .map_err(to_business)
        })
    }

    fn aging<'a>(
        &'a self,
        company_id: &'a str,
        request: &'a AgingRequest,
    ) -> AgingFuture<'a> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company_id = company_id.to_owned();
        let kind = request.kind.clone();
        let as_of = request.as_of.clone();
        let chart_id = request.chart_id.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company_id = company_id.clone();
                let kind = kind.clone();
                let as_of = as_of.clone();
                let chart_id = chart_id.clone();
                Box::pin(async move {
                    let rows = read_invoice_aging(
                        connection,
                        &company_id,
                        &kind,
                        &as_of,
                        chart_id.as_deref(),
                    )
                    .await
                    .map_err(to_business)?;
                    Ok(rows)
                })
            })
            .await
            .map_err(to_business)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact text `finance-invoice-read.ts` rendered before migration:
    /// helpers expanded, `?` placeholders left intact.
    const RENDERED_TS_SQL: &str = r#"SELECT
    (SELECT count(*) FROM finance_journals WHERE company_id=? ) AS journals,
    (SELECT count(*) FROM finance_journals WHERE company_id=? AND status='draft') AS posting,
    (SELECT count(*) FROM finance_invoices WHERE company_id=?) AS invoices,
    (SELECT count(*) FROM finance_payments WHERE company_id=?) AS payments,
    (SELECT count(*) FROM finance_invoices WHERE company_id=? AND kind='receivable' AND status IN ('posted','partially_paid') AND outstanding_minor>0) AS receivable,
    (SELECT count(*) FROM finance_invoices WHERE company_id=? AND kind='payable' AND status IN ('posted','partially_paid') AND outstanding_minor>0) AS payable,
    (SELECT count(*) FROM finance_bank_transactions WHERE company_id=? AND match_status<>'matched') AS reconcile"#;

    /// Renumber `?` binds in order; the result must be the native statement.
    fn renumber(sql: &str) -> String {
        let mut n = 0;
        sql.split('?')
            .map(|part| {
                if n == 0 {
                    n += 1;
                    part.to_owned()
                } else {
                    let out = format!("${n}{part}");
                    n += 1;
                    out
                }
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Pinned hashes recorded in `.scratch/migrate_rs.lane_02-oracles.json`.
    const RENDERED_TS_SQL_SHA256: &str =
        "6838b422a683a0cd6edb276ad1cd9526c3c2385ac51ace4017c5c506692096c6";
    const NATIVE_SQL_SHA256: &str =
        "f7d0f294516f35b635bc3a64833382c2442bfd2473fd25760eac2699cb57b583";
    const RESULT_SHA256: &str =
        "fb11782c1575f339f56e957b8b65596daef63876a8c2e7f377351e2ac518177c";

    fn hash(value: &str) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(value.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn native_sql_is_the_rendered_ts_sql_with_numbered_binds() {
        assert_eq!(renumber(RENDERED_TS_SQL), OVERVIEW_COUNTS_SQL);
        assert_eq!(OVERVIEW_COUNTS_SQL.matches('$').count(), 7);
        assert_eq!(hash(RENDERED_TS_SQL), RENDERED_TS_SQL_SHA256);
        assert_eq!(hash(OVERVIEW_COUNTS_SQL), NATIVE_SQL_SHA256);
    }

    #[test]
    fn projected_counts_have_a_stable_wire_shape() {
        let counts = OverviewCounts {
            journals: 12,
            posting: 3,
            invoices: 40,
            payments: 9,
            receivable: 7,
            payable: 2,
            reconcile: 1,
        };
        let json = serde_json::to_string(&counts).expect("serialize");
        assert_eq!(
            json,
            r#"{"journals":12,"posting":3,"invoices":40,"payments":9,"receivable":7,"payable":2,"reconcile":1}"#
        );
        assert_eq!(hash(&json), RESULT_SHA256);
    }
}
