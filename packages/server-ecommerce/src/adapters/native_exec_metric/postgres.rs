//! Native Ecommerce executive metric postgres adapter.
//!
//! The owner statement is rebuilt with `QueryBuilder` by splitting the retired
//! `?` statement on its placeholders and pushing typed binds in the same order.
//! No SQL text or relation name is accepted from the wire; every execution
//! failure maps to the named owner error.

use crate::adapters::native_exec_metric::application::{ExecMetricFuture, ExecMetricRead};
use crate::adapters::native_exec_metric::domain::{BindValue, MetricRow, QuerySpec, unavailable};
use erpx_server_runtime::public::postgres::DatabasePool;
use sqlx::{QueryBuilder, Row};

pub struct PgExecMetricRead {
    pool: DatabasePool,
}

impl PgExecMetricRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl ExecMetricRead for PgExecMetricRead {
    fn read<'a>(&'a self, spec: &'a QuerySpec) -> ExecMetricFuture<'a, Vec<MetricRow>> {
        let pool = self.pool.clone();
        let spec = spec.clone();
        Box::pin(async move {
            let mut parts = spec.sql.split('?');
            let mut query = QueryBuilder::new(parts.next().unwrap_or(""));
            for (bind, tail) in spec.values.iter().zip(parts) {
                match bind {
                    BindValue::Text(Some(value)) => {
                        query.push_bind(value.as_str());
                    }
                    BindValue::Text(None) => {
                        query.push_bind(Option::<&str>::None);
                    }
                    BindValue::Int(value) => {
                        query.push_bind(*value);
                    }
                }
                query.push(tail);
            }
            let rows = query
                .build()
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(MetricRow {
                        label: row.try_get::<String, _>("label").map_err(|_| unavailable())?,
                        value: row.try_get::<String, _>("value").map_err(|_| unavailable())?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_exec_metric::domain::{Breakdown, Metric, Scope, build_query};

    #[test]
    fn postgres_statement_keeps_every_bind_in_order() {
        let spec = build_query(
            Metric::CompletedCheckouts,
            Breakdown::Status,
            &Scope { company_id: "c", branch_id: None, department_id: None },
            1_767_225_600_000,
            1_769_904_000_000,
        );
        let mut parts = spec.sql.split('?');
        let mut query = QueryBuilder::new(parts.next().unwrap());
        for (bind, tail) in spec.values.iter().zip(parts) {
            match bind {
                BindValue::Text(Some(v)) => { query.push_bind(v.as_str()); }
                BindValue::Text(None) => { query.push_bind(Option::<&str>::None); }
                BindValue::Int(v) => { query.push_bind(*v); }
            }
            query.push(tail);
        }
        let rendered = query.into_sql();
        let rendered = rendered.as_str();
        assert_eq!(rendered.matches('$').count(), spec.values.len());
        assert!(rendered.contains("$1"));
        assert!(rendered.contains("::text IS NULL"));
        assert!(!rendered.contains('?'));
    }
}
