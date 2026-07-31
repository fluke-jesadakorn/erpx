//! G4 support outbox/projector adapter.
//!
//! This adapter is deliberately exported but not added to the central worker
//! registry.  A coordinator or deployment owner must explicitly compose it;
//! merely compiling the support workflow cannot activate customer access. The
//! opt-in runner claims only Support-owned outbox families because the generic
//! outbox runner claims the shared queue before projection ownership is known.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_infrastructure::{
    MAX_OUTBOX_CLAIM_BATCH_SIZE, OutboxEnvelope, OutboxHandler, OutboxWorker, PersistenceError,
    ProjectionHandler, RetryDisposition, RetryPolicy, SUPPORT_CAPABILITY_ISSUED_EVENT,
    SUPPORT_CAPABILITY_REVOKED_EVENT, SUPPORT_DIAGNOSTICS_EVENT, SUPPORT_SESSION_APPROVED_EVENT,
    SUPPORT_SESSION_REVOKED_EVENT, SUPPORT_WORKFLOW_PROJECTION_NAME, SqlxControlPlane,
    SupportWorkflowPersistenceError, SupportWorkflowStore, WorkerError, WorkerRunReport,
};
use sqlx::{Row, postgres::PgRow};
use uuid::Uuid;

use super::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const SUPPORT_WORKFLOW_HANDLER_NAME: &str = SUPPORT_WORKFLOW_PROJECTION_NAME;

const SUPPORT_OUTBOX_CLAIM_TIMEOUT: Duration = Duration::minutes(5);
const SUPPORT_OUTBOX_AGGREGATE_TYPES: &[&str] = &[
    "support_session",
    "support_capability",
    "incident",
    "support_diagnostics_audit",
];

#[derive(Debug, Clone)]
pub struct SupportWorkflowProjection {
    store: SupportWorkflowStore,
}

impl SupportWorkflowProjection {
    pub fn new(store: SupportWorkflowStore) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &SupportWorkflowStore {
        &self.store
    }
}

#[async_trait]
impl ProjectionHandler for SupportWorkflowProjection {
    fn handles_event(&self, projection_name: &str, event: &OutboxEnvelope) -> bool {
        projection_name == SUPPORT_WORKFLOW_PROJECTION_NAME
            && is_support_aggregate_type(&event.aggregate_type)
    }

    async fn clear(&self, projection_name: &str) -> Result<(), WorkerError> {
        if projection_name != SUPPORT_WORKFLOW_PROJECTION_NAME {
            return Err(WorkerError::Permanent(format!(
                "unsupported projection: {projection_name}"
            )));
        }
        self.store
            .clear_read_models()
            .await
            .map_err(classify_support_workflow_error)
    }

    async fn project(
        &self,
        projection_name: &str,
        event: &OutboxEnvelope,
    ) -> Result<(), WorkerError> {
        if projection_name != SUPPORT_WORKFLOW_PROJECTION_NAME {
            return Err(WorkerError::Permanent(format!(
                "unsupported projection: {projection_name}"
            )));
        }
        self.store
            .project_event(event)
            .await
            .map(|_| ())
            .map_err(classify_support_workflow_error)
    }
}

#[derive(Debug, Clone)]
pub struct SupportWorkflowOutboxHandler {
    control: Arc<SqlxControlPlane>,
    worker: OutboxWorker,
    projection: SupportWorkflowProjection,
    worker_id: Uuid,
    batch_size: u32,
}

impl SupportWorkflowOutboxHandler {
    pub fn new(
        control: Arc<SqlxControlPlane>,
        workflow_store: SupportWorkflowStore,
        worker_id: Uuid,
        batch_size: u32,
    ) -> Self {
        let batch_size = batch_size.clamp(1, MAX_OUTBOX_CLAIM_BATCH_SIZE);
        Self {
            control: control.clone(),
            worker: OutboxWorker::new(control, worker_id).with_batch_size(batch_size),
            projection: SupportWorkflowProjection::new(workflow_store),
            worker_id,
            batch_size,
        }
    }

    pub fn projection(&self) -> &SupportWorkflowProjection {
        &self.projection
    }

    pub async fn run_once(&self) -> Result<WorkerRunReport, erpx_infrastructure::PersistenceError> {
        self.run_once_with_worker_id(self.worker_id).await
    }

    pub async fn run_once_with_worker_id(
        &self,
        worker_id: Uuid,
    ) -> Result<WorkerRunReport, erpx_infrastructure::PersistenceError> {
        let runner = Self {
            control: self.control.clone(),
            worker: self.worker.clone().with_worker_id(worker_id),
            projection: self.projection.clone(),
            worker_id,
            batch_size: self.batch_size,
        };
        runner.run_filtered_once().await
    }

    async fn run_filtered_once(&self) -> Result<WorkerRunReport, PersistenceError> {
        let events = claim_support_outbox(
            &self.control,
            self.worker_id,
            self.batch_size,
            Utc::now(),
            SUPPORT_OUTBOX_CLAIM_TIMEOUT,
        )
        .await?;
        let mut report = WorkerRunReport {
            claimed: u32::try_from(events.len()).unwrap_or(u32::MAX),
            ..WorkerRunReport::default()
        };

        for event in events {
            match self.handle(&event).await {
                Ok(()) => {
                    self.control
                        .mark_outbox_published(event.id, self.worker_id, Utc::now())
                        .await?;
                    report.processed = report.processed.saturating_add(1);
                    report.published = report.published.saturating_add(1);
                }
                Err(error @ WorkerError::Retryable(_)) => {
                    let context = error.context(&event, self.worker_id);
                    context
                        .validate_for(&event, self.worker_id)
                        .map_err(|message| PersistenceError::InvalidData(message.into()))?;
                    let WorkerError::Retryable(message) = error else {
                        unreachable!("matched retryable worker error")
                    };
                    match RetryPolicy::default().decide(
                        event.attempts,
                        event.max_attempts,
                        &WorkerError::Retryable(message),
                    ) {
                        RetryDisposition::Retry { delay } => {
                            self.control
                                .retry_outbox_with_context(
                                    event.id,
                                    self.worker_id,
                                    Utc::now() + delay,
                                    &context.message,
                                    Some(&context),
                                    Utc::now(),
                                )
                                .await?;
                            report.retried = report.retried.saturating_add(1);
                        }
                        RetryDisposition::DeadLetter => {
                            self.control
                                .dead_letter_outbox_with_context(
                                    event.id,
                                    self.worker_id,
                                    &context.message,
                                    Some(&context),
                                    Utc::now(),
                                )
                                .await?;
                            report.dead_lettered = report.dead_lettered.saturating_add(1);
                        }
                    }
                }
                Err(error @ WorkerError::Permanent(_)) => {
                    let context = error.context(&event, self.worker_id);
                    context
                        .validate_for(&event, self.worker_id)
                        .map_err(|message| PersistenceError::InvalidData(message.into()))?;
                    self.control
                        .dead_letter_outbox_with_context(
                            event.id,
                            self.worker_id,
                            &context.message,
                            Some(&context),
                            Utc::now(),
                        )
                        .await?;
                    report.dead_lettered = report.dead_lettered.saturating_add(1);
                }
            }
        }
        Ok(report)
    }
}

#[async_trait]
impl OutboxHandler for SupportWorkflowOutboxHandler {
    async fn handle(&self, event: &OutboxEnvelope) -> Result<(), WorkerError> {
        validate_support_event(event)?;
        self.worker
            .project_once(SUPPORT_WORKFLOW_PROJECTION_NAME, event, &self.projection)
            .await
            .map(|_| ())
            .map_err(classify_projection_failure)
    }
}

fn validate_support_event(event: &OutboxEnvelope) -> Result<(), WorkerError> {
    if !support_event_type_allowed(&event.aggregate_type, &event.event_type)
        || event.id.is_nil()
        || event.aggregate_id.is_nil()
        || event.correlation_id.is_nil()
        || event.event_version == 0
        || event.aggregate_version == 0
        || event
            .idempotency_key
            .as_deref()
            .is_none_or(|key| key.trim().is_empty())
    {
        return Err(WorkerError::Permanent(
            "support outbox event identity is invalid".into(),
        ));
    }
    Ok(())
}

fn classify_support_workflow_error(error: SupportWorkflowPersistenceError) -> WorkerError {
    let message = error.to_string();
    let retryable = matches!(
        &error,
        SupportWorkflowPersistenceError::Database(_)
            | SupportWorkflowPersistenceError::Persistence(PersistenceError::Database(_))
            | SupportWorkflowPersistenceError::Persistence(PersistenceError::Migration(_))
    );
    if retryable {
        WorkerError::Retryable(message)
    } else {
        WorkerError::Permanent(message)
    }
}

fn is_support_aggregate_type(aggregate_type: &str) -> bool {
    SUPPORT_OUTBOX_AGGREGATE_TYPES.contains(&aggregate_type)
}

fn support_event_type_allowed(aggregate_type: &str, event_type: &str) -> bool {
    match aggregate_type {
        "support_session" => matches!(
            event_type,
            SUPPORT_SESSION_APPROVED_EVENT | SUPPORT_SESSION_REVOKED_EVENT
        ),
        "support_capability" => matches!(
            event_type,
            SUPPORT_CAPABILITY_ISSUED_EVENT | SUPPORT_CAPABILITY_REVOKED_EVENT
        ),
        "incident" => matches!(event_type, "incident.created" | "incident.updated"),
        "support_diagnostics_audit" => event_type == SUPPORT_DIAGNOSTICS_EVENT,
        _ => false,
    }
}

fn classify_projection_failure(error: PersistenceError) -> WorkerError {
    let message = error.to_string();
    const PERMANENT_PREFIX: &str = "invalid persisted value: permanent handler failure: ";
    if let Some(message) = message.strip_prefix(PERMANENT_PREFIX) {
        WorkerError::Permanent(message.to_owned())
    } else {
        WorkerError::Retryable(message)
    }
}

async fn claim_support_outbox(
    control: &SqlxControlPlane,
    worker_id: Uuid,
    limit: u32,
    now: DateTime<Utc>,
    claim_timeout: Duration,
) -> Result<Vec<OutboxEnvelope>, PersistenceError> {
    if worker_id.is_nil() {
        return Err(PersistenceError::InvalidData(
            "outbox worker identity is required".into(),
        ));
    }
    if limit == 0 || limit > MAX_OUTBOX_CLAIM_BATCH_SIZE {
        return Err(PersistenceError::InvalidData(format!(
            "outbox claim batch must be 1..={MAX_OUTBOX_CLAIM_BATCH_SIZE}"
        )));
    }
    if claim_timeout <= Duration::zero() {
        return Err(PersistenceError::InvalidData(
            "outbox claim timeout must be positive".into(),
        ));
    }
    let stale_before = now - claim_timeout;
    let aggregate_types: Vec<String> = SUPPORT_OUTBOX_AGGREGATE_TYPES
        .iter()
        .map(|value| (*value).to_owned())
        .collect();
    let rows = sqlx::query(
        "WITH exhausted AS (
            UPDATE erpx_control.outbox_events
            SET dead_lettered_at = $1,
                claimed_by = NULL,
                claimed_at = NULL,
                last_error = COALESCE(last_error, 'worker attempt limit exhausted')
            WHERE published_at IS NULL
              AND dead_lettered_at IS NULL
              AND attempts >= max_attempts
              AND (claimed_by IS NULL OR claimed_at IS NULL OR claimed_at < $2)
              AND aggregate_type = ANY($5::text[])
            RETURNING id
         ), candidates AS (
            SELECT e.id
            FROM erpx_control.outbox_events e
            WHERE e.published_at IS NULL
              AND e.dead_lettered_at IS NULL
              AND e.available_at <= $1
              AND e.attempts < e.max_attempts
              AND (e.claimed_by IS NULL OR e.claimed_at IS NULL OR e.claimed_at < $2)
              AND e.aggregate_type = ANY($5::text[])
              AND NOT EXISTS (
                SELECT 1 FROM erpx_control.outbox_events prior
                WHERE prior.aggregate_type = e.aggregate_type
                  AND prior.aggregate_id = e.aggregate_id
                  AND prior.aggregate_version < e.aggregate_version
                  AND prior.published_at IS NULL
              )
            ORDER BY e.occurred_at, e.id
            FOR UPDATE SKIP LOCKED
            LIMIT $3
         )
         UPDATE erpx_control.outbox_events e
         SET claimed_by = $4, claimed_at = $1, attempts = e.attempts + 1
         FROM candidates c
         WHERE e.id = c.id
         RETURNING e.*",
    )
    .bind(now)
    .bind(stale_before)
    .bind(i64::from(limit))
    .bind(worker_id)
    .bind(aggregate_types)
    .fetch_all(&control.pools().write)
    .await?;
    rows.into_iter().map(support_outbox_from_row).collect()
}

fn support_outbox_from_row(row: PgRow) -> Result<OutboxEnvelope, PersistenceError> {
    Ok(OutboxEnvelope {
        id: row.try_get("id")?,
        event_type: row.try_get("event_type")?,
        event_version: u32::try_from(row.try_get::<i32, _>("event_version")?)
            .map_err(|_| PersistenceError::InvalidData("negative event version".into()))?,
        aggregate_type: row.try_get("aggregate_type")?,
        aggregate_id: row.try_get("aggregate_id")?,
        aggregate_version: u64::try_from(row.try_get::<i64, _>("aggregate_version")?)
            .map_err(|_| PersistenceError::InvalidData("negative aggregate version".into()))?,
        tenant_scope: row.try_get("tenant_scope")?,
        actor: row.try_get("actor")?,
        correlation_id: row.try_get("correlation_id")?,
        causation_id: row.try_get("causation_id")?,
        idempotency_key: row.try_get("idempotency_key")?,
        payload: row.try_get("payload")?,
        occurred_at: row.try_get("occurred_at")?,
        available_at: row.try_get("available_at")?,
        attempts: u32::try_from(row.try_get::<i32, _>("attempts")?)
            .map_err(|_| PersistenceError::InvalidData("negative outbox attempts".into()))?,
        max_attempts: u32::try_from(row.try_get::<i32, _>("max_attempts")?)
            .map_err(|_| PersistenceError::InvalidData("negative outbox max attempts".into()))?,
        claimed_by: row.try_get("claimed_by")?,
        claimed_at: row.try_get("claimed_at")?,
        published_at: row.try_get("published_at")?,
        dead_lettered_at: row.try_get("dead_lettered_at")?,
        last_error: row.try_get("last_error")?,
        last_error_context: row.try_get("last_error_context")?,
    })
}

#[derive(Debug, Clone)]
pub struct SupportWorkflowHandler {
    handler: SupportWorkflowOutboxHandler,
}

impl SupportWorkflowHandler {
    pub fn new(
        control: Arc<SqlxControlPlane>,
        workflow_store: SupportWorkflowStore,
        worker_id: Uuid,
        batch_size: u32,
    ) -> Self {
        Self {
            handler: SupportWorkflowOutboxHandler::new(
                control,
                workflow_store,
                worker_id,
                batch_size,
            ),
        }
    }

    pub fn outbox_handler(&self) -> &SupportWorkflowOutboxHandler {
        &self.handler
    }
}

#[async_trait]
impl NamedWorkerHandler for SupportWorkflowHandler {
    fn name(&self) -> &str {
        SUPPORT_WORKFLOW_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        Ok(self
            .handler
            .run_once_with_worker_id(worker_id)
            .await?
            .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_claim_allowlist_is_disjoint_from_unrelated_outbox_families() {
        for aggregate_type in SUPPORT_OUTBOX_AGGREGATE_TYPES {
            assert!(is_support_aggregate_type(aggregate_type));
        }
        for aggregate_type in [
            "customer",
            "support_ticket",
            "finance_invoice",
            "platform_tenant",
            "directory_identity",
        ] {
            assert!(!is_support_aggregate_type(aggregate_type));
        }
    }

    #[test]
    fn malformed_support_projection_errors_are_dead_lettered() {
        let error = classify_support_workflow_error(SupportWorkflowPersistenceError::Invalid(
            "scope mismatch".into(),
        ));
        assert!(matches!(error, WorkerError::Permanent(_)));
    }

    #[test]
    fn support_outbox_event_identity_is_fail_closed() {
        let event = OutboxEnvelope {
            id: Uuid::nil(),
            event_type: "support_session.approved".into(),
            event_version: 1,
            aggregate_type: "support_session".into(),
            aggregate_id: Uuid::new_v4(),
            aggregate_version: 1,
            tenant_scope: None,
            actor: None,
            correlation_id: Uuid::new_v4(),
            causation_id: None,
            idempotency_key: Some("support-test".into()),
            payload: serde_json::json!({}),
            occurred_at: Utc::now(),
            available_at: Utc::now(),
            attempts: 0,
            max_attempts: 3,
            claimed_by: None,
            claimed_at: None,
            published_at: None,
            dead_lettered_at: None,
            last_error: None,
            last_error_context: None,
        };
        assert!(matches!(
            validate_support_event(&event),
            Err(WorkerError::Permanent(_))
        ));
    }

    #[test]
    fn support_outbox_event_family_mismatch_is_fail_closed() {
        let event = OutboxEnvelope {
            id: Uuid::new_v4(),
            event_type: SUPPORT_CAPABILITY_ISSUED_EVENT.into(),
            event_version: 1,
            aggregate_type: "support_session".into(),
            aggregate_id: Uuid::new_v4(),
            aggregate_version: 1,
            tenant_scope: None,
            actor: None,
            correlation_id: Uuid::new_v4(),
            causation_id: None,
            idempotency_key: Some("support-test".into()),
            payload: serde_json::json!({}),
            occurred_at: Utc::now(),
            available_at: Utc::now(),
            attempts: 0,
            max_attempts: 3,
            claimed_by: None,
            claimed_at: None,
            published_at: None,
            dead_lettered_at: None,
            last_error: None,
            last_error_context: None,
        };
        assert!(matches!(
            validate_support_event(&event),
            Err(WorkerError::Permanent(_))
        ));
    }
}
