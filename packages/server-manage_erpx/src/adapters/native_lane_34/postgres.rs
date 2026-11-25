//! sqlx execution for the native beta feedback read.
//!
//! The statement is byte-identical to the retired TypeScript literal: one
//! `company_id` bind, `created_at DESC` and `LIMIT 100`. Projection into the
//! pinned owner types happens here; no predicate or bind order is rewritten.

use crate::adapters::native_lane_34::application::{
    BetaFeedbackQuery, BetaInvitationQuery, FeedbackFuture, InvitationFuture,
};
use crate::adapters::native_lane_34::domain::{
    FEEDBACK_SQL, FeedbackItem, FeedbackPage, INVITATIONS_SQL, InvitationItem, InvitationPage,
    unavailable,
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

/// Postgres-backed invitation reader. `beta_team_invitations` is manage_erpx
/// owned; the `organization_nodes` join is the inherited Company boundary and
/// `b.company_id` is the only bind. The owner CASE remains in SQL so the
/// projection is byte-identical to the retired TypeScript literal.
pub struct PgBetaInvitationQuery {
    pool: DatabasePool,
}

impl PgBetaInvitationQuery {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl BetaInvitationQuery for PgBetaInvitationQuery {
    fn list<'a>(&'a self, company: &'a str) -> InvitationFuture<'a> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                Box::pin(async move {
                    let rows = sqlx::query(INVITATIONS_SQL)
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let items = rows
                        .into_iter()
                        .map(|row| {
                            Ok(InvitationItem {
                                id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                email: row
                                    .try_get::<String, _>("email")
                                    .map_err(|_| unavailable())?,
                                branch_id: row
                                    .try_get::<String, _>("branch_id")
                                    .map_err(|_| unavailable())?,
                                expires_at: row
                                    .try_get::<i64, _>("expires_at")
                                    .map_err(|_| unavailable())?,
                                accepted_at: row
                                    .try_get::<Option<i64>, _>("accepted_at")
                                    .map_err(|_| unavailable())?,
                                revoked_at: row
                                    .try_get::<Option<i64>, _>("revoked_at")
                                    .map_err(|_| unavailable())?,
                                invitation_state: row
                                    .try_get::<String, _>("invitation_state")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(InvitationPage { items })
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
