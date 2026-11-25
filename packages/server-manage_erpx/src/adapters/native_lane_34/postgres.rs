//! sqlx execution for the native beta feedback read.
//!
//! The statement is byte-identical to the retired TypeScript literal: one
//! `company_id` bind, `created_at DESC` and `LIMIT 100`. Projection into the
//! pinned owner types happens here; no predicate or bind order is rewritten.

use crate::adapters::native_lane_34::application::{
    BetaFeedbackQuery, BetaInvitationQuery, EmailReceiptFuture, EmailReceiptQuery, FeedbackFuture,
    InvitationFuture,
};
use crate::adapters::native_lane_34::domain::{
    EMAIL_RECEIPT_SQL, FEEDBACK_SQL, EmailReceipt, FeedbackItem, FeedbackPage, INVITATIONS_SQL,
    InvitationItem, InvitationPage, unavailable,
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

/// Postgres-backed sandbox email receipt reader. `sandbox_email_deliveries` is
/// manage_erpx-owned; `delivery_key` is the only bind and the projection stays
/// byte-identical to the retired TypeScript literal.
pub struct PgEmailReceiptQuery {
    pool: DatabasePool,
}

impl PgEmailReceiptQuery {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl EmailReceiptQuery for PgEmailReceiptQuery {
    fn receipt<'a>(&'a self, delivery_key: &'a str) -> EmailReceiptFuture<'a> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let delivery_key = delivery_key.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let delivery_key = delivery_key.clone();
                Box::pin(async move {
                    let row = sqlx::query(EMAIL_RECEIPT_SQL)
                        .bind(delivery_key.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let Some(row) = row else {
                        return Ok(None);
                    };
                    Ok(Some(EmailReceipt {
                        delivery_key: row
                            .try_get::<String, _>("delivery_key")
                            .map_err(|_| unavailable())?,
                        recipient: row
                            .try_get::<String, _>("recipient")
                            .map_err(|_| unavailable())?,
                        subject: row
                            .try_get::<String, _>("subject")
                            .map_err(|_| unavailable())?,
                        body_text: row
                            .try_get::<String, _>("body_text")
                            .map_err(|_| unavailable())?,
                        body_ciphertext: row
                            .try_get::<Option<String>, _>("body_ciphertext")
                            .map_err(|_| unavailable())?,
                        provider_id: row
                            .try_get::<String, _>("provider_id")
                            .map_err(|_| unavailable())?,
                    }))
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
