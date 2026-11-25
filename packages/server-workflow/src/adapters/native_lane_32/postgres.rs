//! sqlx execution for the lane-32 Workflow failed-metrics read.
//!
//! `SOURCE_SQL` is the byte-identical statement retired from
//! `adapters/collaboration-inbox-sources.ts`. `EXECUTED_SQL` keeps the same
//! predicate and bind order with a stable bigint count decode.

use super::application::{FailedMetricsRead, FailedMetricsReadFuture};
use super::domain::unavailable;
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;

pub const SOURCE_SQL: &str =
    "SELECT count(*)::int count FROM outbox_events WHERE dead_letter_at IS NOT NULL";

pub const EXECUTED_SQL: &str =
    "SELECT count(*)::bigint count FROM outbox_events WHERE dead_letter_at IS NOT NULL";

/// Pg-backed owner reader. `outbox_events` is Workflow-owned; this read infers
/// no Collaboration authority and accepts no caller-supplied predicate.
pub struct PgFailedMetricsRead {
    pool: DatabasePool,
}

impl PgFailedMetricsRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl FailedMetricsRead for PgFailedMetricsRead {
    fn failed<'a>(&'a self) -> FailedMetricsReadFuture<'a, i64> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let count = sqlx::query_scalar::<_, i64>(EXECUTED_SQL)
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(count)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_sql_is_byte_identical_to_the_retired_ts_statement() {
        assert_eq!(
            SOURCE_SQL,
            "SELECT count(*)::int count FROM outbox_events WHERE dead_letter_at IS NOT NULL"
        );
    }

    #[test]
    fn executed_sql_keeps_the_exact_predicate() {
        assert_eq!(
            EXECUTED_SQL,
            "SELECT count(*)::bigint count FROM outbox_events WHERE dead_letter_at IS NOT NULL"
        );
        assert!(EXECUTED_SQL.contains("dead_letter_at IS NOT NULL"));
        assert!(!EXECUTED_SQL.contains('?'));
    }
}
