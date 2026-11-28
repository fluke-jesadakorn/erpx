//! sqlx execution of the Finance-owned chart reference list.
//!
//! This is the only layer that prepares and executes SQL. The statement head
//! is byte-identical to the retired TypeScript authority; only the placeholder
//! and the `to_jsonb` row projection differ. Nothing in TypeScript prepares,
//! executes or reshapes this statement.

use crate::adapters::native_lane_6::{
    application::{ChartListFuture, ChartListRead},
    domain::{PG_CHART_LIST_SQL, TS_CHART_LIST_SQL, project_row, unavailable},
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;

/// Postgres-backed owner reader bound to exactly one Holding pool.
pub struct PgChartListRead {
    pool: DatabasePool,
}

impl PgChartListRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl ChartListRead for PgChartListRead {
    fn chart_list<'a>(&'a self, company_id: &'a str) -> ChartListFuture<'a> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company_id = company_id.to_owned();
        Box::pin(async move {
            let result = unit
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let company_id = company_id.clone();
                    Box::pin(async move {
                        // sqlx's unsafe marker mirrors the already-frozen owner
                        // statement; no caller input becomes SQL.
                        let statement = sqlx::AssertSqlSafe(PG_CHART_LIST_SQL);
                        let rows = sqlx::query_scalar::<_, String>(statement)
                            .bind(company_id.as_str())
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
    fn native_lane_6_owner_statement_keeps_the_retired_head() {
        assert!(PG_CHART_LIST_SQL.contains("FROM finance_charts c"));
        assert!(PG_CHART_LIST_SQL.contains("c.company_id=$1"));
        assert!(PG_CHART_LIST_SQL.ends_with("ORDER BY c.name"));
        assert_eq!(TS_CHART_LIST_SQL.matches('?').count(), 1);
        assert!(!TS_CHART_LIST_SQL.contains("$1"));
    }
}
