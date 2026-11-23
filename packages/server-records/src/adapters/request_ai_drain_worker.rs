//! Native Records `request.ai.prepare` drain job unit.
//!
//! This is the native counterpart of the retained jobs-runtime call
//! `drainRequestAi(write, env)` in
//! `packages/server-runtime/src/adapters/runtime/jobs.ts` (and the workflow
//! `job-projector`). The owner port [`drain_request_ai`] deliberately leaves
//! the transaction boundary to its caller; this adapter is that caller. It
//! opens one serializable Holding unit, drives the port once and commits on
//! success. A persistence or input failure is returned without a commit, so
//! dropping the transaction rolls the unit back for the durable runner.
//!
//! No pool, clock, provider, retry or follow-up pass is created here: the
//! composition root supplies the Holding writer connection, the clock reading
//! and the AI provider, and decides retry from
//! [`crate::application::request_ai_drain::should_retry`].

use crate::application::request_ai_drain::{DrainOutcome, RequestAiProvider, drain_request_ai};
use crate::domain::request_ai_drain as domain;
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};

/// Run one native drain job unit on a caller-provided Holding writer
/// connection.
///
/// `connection` is borrowed for the unit only; the caller keeps ownership and
/// may reuse it for the next job. `now` is the caller clock in milliseconds and
/// `request_id` optionally narrows the drain to one request. The provider is
/// called at most once per claimed proposal while the lease is held.
///
/// The unit is committed only when [`drain_request_ai`] succeeds. Any error
/// returns with the transaction uncommitted, which sqlx rolls back on drop.
pub async fn run_drain_job(
    connection: &mut PgConnection,
    now: i64,
    request_id: Option<&str>,
    provider: &impl RequestAiProvider,
) -> Result<DrainOutcome, BusinessError> {
    let mut tx = connection
        .begin()
        .await
        .map_err(|_| domain::drain_unavailable())?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .map_err(|_| domain::drain_unavailable())?;
    let outcome = drain_request_ai(&mut tx, request_id, now, provider).await?;
    tx.commit().await.map_err(|_| domain::drain_unavailable())?;
    Ok(outcome)
}
