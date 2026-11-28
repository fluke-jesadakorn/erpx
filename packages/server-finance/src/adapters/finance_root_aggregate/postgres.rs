//! sqlx execution of the Finance root aggregate read.
//!
//! This is the only layer that prepares and executes SQL. The chart, journal,
//! invoice and report statements are the owner's already-frozen statements;
//! the payments head is byte-identical to the retired TypeScript authority.
//! One serializable unit binds every read to the same Company and ledger.

use crate::adapters::{
    finance_root_aggregate::{
        application::{AggregateFuture, RootAggregate, RootAggregateRead},
        domain::{
            DEFAULT_LIMIT, PG_PAYMENTS_SQL, project_row, select_chart_id, unavailable,
        },
    },
    finance_reports::read_financial_report,
    journal_list::read_journal_list,
    native_lane_6::domain::{PG_CHART_LIST_SQL, project_row as chart_row},
    native_lane_7::domain::{PG_INVOICE_LIST_SQL, project_row as invoice_row},
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;

/// Postgres-backed owner reader bound to exactly one Holding pool.
pub struct PgRootAggregateRead {
    pool: DatabasePool,
}

impl PgRootAggregateRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl RootAggregateRead for PgRootAggregateRead {
    fn aggregate<'a>(
        &'a self,
        company_id: &'a str,
        chart_id: Option<&'a str>,
    ) -> AggregateFuture<'a> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company_id = company_id.to_owned();
        let chart_id = chart_id.map(str::to_owned);
        Box::pin(async move {
            let result = unit
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let company_id = company_id.clone();
                    let chart_id = chart_id.clone();
                    Box::pin(async move {
                        let company = company_id.as_str();
                        let charts = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(
                            PG_CHART_LIST_SQL,
                        ))
                        .bind(company)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .iter()
                        .map(|row| chart_row(row).map_err(PgTransactionError::Business))
                        .collect::<Result<Vec<Value>, _>>()?;
                        let chart = select_chart_id(&charts, chart_id.as_deref())?;
                        let journals = read_journal_list(&mut *connection, company, Some(&chart), "", None)
                            .await?
                            .journals;
                        let invoices = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(
                            PG_INVOICE_LIST_SQL,
                        ))
                        .bind(company)
                        .bind(DEFAULT_LIMIT)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .iter()
                        .map(|row| invoice_row(row).map_err(PgTransactionError::Business))
                        .collect::<Result<Vec<Value>, _>>()?;
                        let payments = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(
                            PG_PAYMENTS_SQL,
                        ))
                        .bind(company)
                        .bind(DEFAULT_LIMIT)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .iter()
                        .map(|row| project_row(row).map_err(PgTransactionError::Business))
                        .collect::<Result<Vec<Value>, _>>()?;
                        let report = read_financial_report(
                            &mut *connection,
                            company,
                            Some(&chart),
                            None,
                            None,
                            None,
                        )
                        .await?;
                        Ok(RootAggregate {
                            charts,
                            journals,
                            invoices,
                            payments,
                            report: serde_json::to_value(report).map_err(|_| unavailable())?,
                        })
                    })
                })
                .await;
            match result {
                Ok(aggregate) => Ok(aggregate),
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
    fn root_aggregate_owner_statement_keeps_the_retired_head() {
        use crate::adapters::finance_root_aggregate::domain::TS_PAYMENTS_SQL;
        assert!(TS_PAYMENTS_SQL.contains("SELECT p.id,p.payment_number,p.kind"));
        assert!(TS_PAYMENTS_SQL.contains("WHERE p.company_id=?"));
        assert_eq!(TS_PAYMENTS_SQL.matches('?').count(), 2);
        assert!(PG_PAYMENTS_SQL.contains("p.company_id=$1"));
        assert!(PG_PAYMENTS_SQL.contains("LIMIT $2"));
        assert!(PG_CHART_LIST_SQL.contains("finance_charts"));
        assert!(PG_INVOICE_LIST_SQL.contains("finance_invoices"));
    }
}
