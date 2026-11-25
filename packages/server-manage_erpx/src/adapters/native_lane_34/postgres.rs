//! sqlx execution for the native beta feedback read.
//!
//! The statement is byte-identical to the retired TypeScript literal: one
//! `company_id` bind, `created_at DESC` and `LIMIT 100`. Projection into the
//! pinned owner types happens here; no predicate or bind order is rewritten.

use crate::adapters::native_lane_34::application::{BetaFeedbackQuery, FeedbackFuture};
use crate::adapters::native_lane_34::domain::{
    FEEDBACK_SQL, FeedbackItem, FeedbackPage, unavailable,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

/// Postgres-backed owner reader. `beta_feedback` is manage_erpx-owned; the
/// Company scope is the only authorization boundary this read asserts.
pub struct PgBetaFeedbackQuery {
    pool: DatabasePool,
}

impl PgBetaFeedbackQuery {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl BetaFeedbackQuery for PgBetaFeedbackQuery {
    fn list<'a>(&'a self, company: &'a str) -> FeedbackFuture<'a> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                Box::pin(async move {
                    let rows = sqlx::query(FEEDBACK_SQL)
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let items = rows
                        .into_iter()
                        .map(|row| {
                            Ok(FeedbackItem {
                                id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                body: row
                                    .try_get::<String, _>("body")
                                    .map_err(|_| unavailable())?,
                                created_at: row
                                    .try_get::<i64, _>("created_at")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(FeedbackPage { items })
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
