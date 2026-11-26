//! sqlx execution of the Finance-owned invoice list.
//!
//! This is the only layer that prepares and executes SQL. The statement head
//! is byte-identical to the retired TypeScript authority; only the placeholder
//! and the `to_jsonb` row projection differ. Nothing in TypeScript prepares,
//! executes or reshapes this statement.

use crate::adapters::native_lane_7::{
    application::{InvoiceListFuture, InvoiceListRead},
    domain::{PG_INVOICE_LIST_SQL, TS_INVOICE_LIST_SQL, project_row, unavailable},
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;

/// Postgres-backed owner reader bound to exactly one Holding pool.
pub struct PgInvoiceListRead {
    pool: DatabasePool,
}

impl PgInvoiceListRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl InvoiceListRead for PgInvoiceListRead {
    fn invoice_list<'a>(
        &'a self,
        company_id: &'a str,
        limit: i64,
    ) -> InvoiceListFuture<'a> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company_id = company_id.to_owned();
        Box::pin(async move {
            let result = unit
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let company_id = company_id.clone();
                    Box::pin(async move {
                        // sqlx's unsafe marker mirrors the already-frozen owner
                        // statement; no caller input becomes SQL.
                        let statement = sqlx::AssertSqlSafe(PG_INVOICE_LIST_SQL);
                        let rows = sqlx::query_scalar::<_, String>(statement)
                            .bind(company_id.as_str())
                            .bind(limit)
                            .fetch_all(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?;
                        rows.iter()
                            .map(|row| project_row(row).map_err(PgTransactionError::Business))
                            .collect::<Result<Vec<_>, _>>()
                    })
                })
                .await;
            match result {
                Ok(rows) => Ok(rows),
                Err(PgTransactionError::Business(error)) => Err(error),
                Err(PgTransactionError::Persistence(_)) => Err(unavailable()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_lane_7_owner_statement_keeps_the_retired_head() {
        assert!(PG_INVOICE_LIST_SQL.contains(
            "FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id"
        ));
        assert!(PG_INVOICE_LIST_SQL.contains("i.company_id=$1"));
        assert!(PG_INVOICE_LIST_SQL.ends_with("LIMIT $2) listed"));
        assert_eq!(TS_INVOICE_LIST_SQL.matches('?').count(), 2);
        assert!(!TS_INVOICE_LIST_SQL.contains("$1"));
    }
}
