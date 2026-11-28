//! PostgreSQL execution for native lane 3. The owner owns the statement bytes,
//! the bind order and the row projection; there is no TypeScript SQL fallback.

use super::application::{InvoiceLineRead, InvoiceLineReadFuture};
use super::domain::{unavailable, InvoiceLine, InvoiceLineRequest};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::Row;

/// Positional bind order of `INVOICE_LINES_SQL`, preserved from the retired TS
/// reader: invoice, company, revision.
const POSTGRES_INVOICE_LINES_SQL: &str = "SELECT * FROM finance_invoice_line_versions WHERE invoice_id=$1 AND company_id=$2 AND source_version=$3 ORDER BY ordinal";

pub const BIND_ORDER: [&str; 3] = ["invoice_id", "company_id", "source_version"];

fn decode(row: &sqlx::postgres::PgRow) -> Result<InvoiceLine, sqlx::Error> {
    Ok(InvoiceLine {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        invoice_id: row.try_get("invoice_id")?,
        source_version: row.try_get("source_version")?,
        ordinal: row.try_get("ordinal")?,
        description: row.try_get("description")?,
        quantity_millis: row.try_get("quantity_millis")?,
        unit_price_minor: row.try_get("unit_price_minor")?,
        tax_bps: row.try_get("tax_bps")?,
        subtotal_minor: row.try_get("subtotal_minor")?,
        tax_minor: row.try_get("tax_minor")?,
        total_minor: row.try_get("total_minor")?,
        account_id: row.try_get("account_id")?,
        tax_account_id: row.try_get("tax_account_id")?,
        source_snapshot_json: row.try_get("source_snapshot_json")?,
        currency: row.try_get("currency")?,
        origin_kind: row.try_get("origin_kind")?,
    })
}

fn to_business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

pub struct PgInvoiceLineRead {
    pool: DatabasePool,
}

impl PgInvoiceLineRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl InvoiceLineRead for PgInvoiceLineRead {
    fn lines<'a>(&'a self, request: &'a InvoiceLineRequest) -> InvoiceLineReadFuture<'a> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = request.company_id.clone();
        let invoice = request.invoice_id.clone();
        let revision = request.revision;
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let invoice = invoice.clone();
                Box::pin(async move {
                    let rows = sqlx::query(POSTGRES_INVOICE_LINES_SQL)
                        .bind(invoice.as_str())
                        .bind(company.as_str())
                        .bind(revision)
                        .fetch_all(&mut *connection)
                        .await?;
                    rows.iter()
                        .map(decode)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(PgTransactionError::from)
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
    use crate::adapters::native_lane_3::domain::{sql_oracle, INVOICE_LINES_SQL};

    #[test]
    fn statement_bytes_match_the_retired_typescript_reader() {
        assert_eq!(
            INVOICE_LINES_SQL,
            "SELECT * FROM finance_invoice_line_versions WHERE invoice_id=? AND company_id=? AND source_version=? ORDER BY ordinal"
        );
        assert_eq!(
            POSTGRES_INVOICE_LINES_SQL.replace("$1", "?").replace("$2", "?").replace("$3", "?"),
            INVOICE_LINES_SQL
        );
        assert_eq!(
            sql_oracle(),
            "f49986445a3c579345afe3114abe22f4a5a06d715c043d5c94ead51a5ba00754"
        );
    }

    #[test]
    fn bind_order_is_company_scoped_and_stable() {
        assert_eq!(BIND_ORDER, ["invoice_id", "company_id", "source_version"]);
    }

    #[test]
    fn persistence_failures_fail_closed() {
        let error = to_business(PgTransactionError::Persistence(
            erpx_platform_runtime_server::public::postgres::PersistenceError::new("POOL_CLOSED"),
        ));
        assert_eq!(error.code, "LANE3_INVOICE_LINES_UNAVAILABLE");
    }
}
