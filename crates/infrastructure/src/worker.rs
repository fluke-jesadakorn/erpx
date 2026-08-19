use std::{sync::Arc, time::Duration as StdDuration};

use async_trait::async_trait;
use chrono::{Duration, Utc};
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    MAX_OUTBOX_CLAIM_BATCH_SIZE, OutboxEnvelope, PersistenceError, ProjectionCheckpointDecision,
    SqlxControlPlane,
};

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("retryable handler failure: {0}")]
    Retryable(String),
    #[error("permanent handler failure: {0}")]
    Permanent(String),
}

/// Bounded, correlation-safe failure data for worker logs and diagnostics.
/// The durable outbox retains only the sanitized message; event identity and
/// correlation are carried separately so operators never need raw payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkerErrorContext {
    pub worker_id: Uuid,
    pub event_id: Uuid,
    pub event_type: String,
    pub event_version: u32,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub active_scope: Option<Value>,
    pub actor: Option<Value>,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
    pub idempotency_key: Option<String>,
    pub retryable: bool,
    pub message: String,
}

impl WorkerError {
    pub fn context(&self, event: &OutboxEnvelope, worker_id: Uuid) -> WorkerErrorContext {
        WorkerErrorContext {
            worker_id,
            event_id: event.id,
            event_type: event.event_type.clone(),
            event_version: event.event_version,
            aggregate_type: event.aggregate_type.clone(),
            aggregate_id: event.aggregate_id,
            aggregate_version: event.aggregate_version,
            active_scope: sanitize_worker_context_value(event.active_scope.as_ref()),
            actor: sanitize_worker_context_value(event.actor.as_ref()),
            correlation_id: event.correlation_id,
            causation_id: event.causation_id,
            idempotency_key: sanitize_worker_idempotency_key(event.idempotency_key.as_ref()),
            retryable: matches!(self, Self::Retryable(_)),
            message: sanitize_worker_message(self.message()),
        }
    }

    fn message(&self) -> &str {
        match self {
            Self::Retryable(message) | Self::Permanent(message) => message,
        }
    }
}

impl WorkerErrorContext {
    /// Ensure a durable diagnostic context still describes the exact claimed
    /// event. This prevents a caller from persisting a different holding,
    /// actor, or correlation context against a valid worker claim.
    pub fn validate_for(
        &self,
        event: &OutboxEnvelope,
        worker_id: Uuid,
    ) -> Result<(), &'static str> {
        let expected = WorkerErrorContext {
            worker_id,
            event_id: event.id,
            event_type: event.event_type.clone(),
            event_version: event.event_version,
            aggregate_type: event.aggregate_type.clone(),
            aggregate_id: event.aggregate_id,
            aggregate_version: event.aggregate_version,
            active_scope: sanitize_worker_context_value(event.active_scope.as_ref()),
            actor: sanitize_worker_context_value(event.actor.as_ref()),
            correlation_id: event.correlation_id,
            causation_id: event.causation_id,
            idempotency_key: sanitize_worker_idempotency_key(event.idempotency_key.as_ref()),
            retryable: self.retryable,
            message: sanitize_worker_message(&self.message),
        };
        if self.worker_id != expected.worker_id {
            return Err("worker identity does not match the claim");
        }
        if self.event_id != expected.event_id {
            return Err("event identity does not match the claim");
        }
        if self.event_type != expected.event_type || self.event_version != expected.event_version {
            return Err("event contract does not match the claim");
        }
        if self.aggregate_type != expected.aggregate_type
            || self.aggregate_id != expected.aggregate_id
            || self.aggregate_version != expected.aggregate_version
        {
            return Err("aggregate identity does not match the claim");
        }
        if self.active_scope != expected.active_scope {
            return Err("holding scope does not match the claim");
        }
        if self.actor != expected.actor {
            return Err("actor does not match the claim");
        }
        if self.correlation_id != expected.correlation_id
            || self.causation_id != expected.causation_id
            || self.idempotency_key != expected.idempotency_key
        {
            return Err("causation or correlation does not match the claim");
        }
        if self.message != expected.message {
            return Err("worker error message is not sanitized");
        }
        Ok(())
    }
}

fn sanitize_worker_context_value(value: Option<&Value>) -> Option<Value> {
    let value = value?;
    let encoded = serde_json::to_string(value).ok()?;
    if encoded.len() > 1_024
        || encoded.chars().any(char::is_control)
        || [
            "password",
            "secret",
            "token",
            "credential",
            "private_key",
            "raw_body",
            "signature",
            "postgres://",
            "ldaps://",
            "https://",
        ]
        .iter()
        .any(|marker| encoded.to_ascii_lowercase().contains(marker))
    {
        return None;
    }
    Some(value.clone())
}

fn sanitize_worker_idempotency_key(value: Option<&String>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty()
        || value.len() > 255
        || value.chars().any(char::is_control)
        || ["password", "secret", "token", "credential", "://"]
            .iter()
            .any(|marker| value.to_ascii_lowercase().contains(marker))
    {
        return None;
    }
    Some(value.to_owned())
}

fn sanitize_worker_message(message: &str) -> String {
    let message = message.trim();
    if message.is_empty()
        || message.len() > 256
        || message.chars().any(char::is_control)
        || ["password", "secret", "token", "postgres://", "://"]
            .iter()
            .any(|marker| message.to_ascii_lowercase().contains(marker))
    {
        return "worker handler failed".into();
    }
    message.into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDisposition {
    Retry { delay: Duration },
    DeadLetter,
}

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            base_delay: Duration::seconds(2),
            max_delay: Duration::minutes(5),
        }
    }
}

impl RetryPolicy {
    pub fn normalized(self) -> Self {
        let minimum = Duration::milliseconds(1);
        let max_delay = self.max_delay.max(minimum);
        Self {
            base_delay: self.base_delay.max(minimum).min(max_delay),
            max_delay,
        }
    }

    pub fn decide(
        &self,
        attempts: u32,
        max_attempts: u32,
        error: &WorkerError,
    ) -> RetryDisposition {
        if matches!(error, WorkerError::Permanent(_)) || attempts >= max_attempts {
            return RetryDisposition::DeadLetter;
        }
        let policy = self.normalized();
        let exponent = attempts.saturating_sub(1).min(10);
        let multiplier = 1_i64 << exponent;
        let delay_milliseconds = policy
            .base_delay
            .num_milliseconds()
            .checked_mul(multiplier)
            .unwrap_or(i64::MAX);
        RetryDisposition::Retry {
            delay: Duration::milliseconds(delay_milliseconds).min(policy.max_delay),
        }
    }
}

#[async_trait]
pub trait OutboxHandler: Send + Sync {
    /// Restrict claims to aggregates owned by this handler.  A handler that
    /// does not provide a filter retains the historical all-aggregate claim
    /// behavior; specialized projectors must opt in so an event they do not
    /// own can never be acknowledged as published by their worker.
    fn aggregate_type_filter(&self) -> Option<&'static [&'static str]> {
        None
    }

    async fn handle(&self, event: &OutboxEnvelope) -> Result<(), WorkerError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorkerRunReport {
    pub claimed: u32,
    pub processed: u32,
    pub published: u32,
    pub retried: u32,
    pub dead_lettered: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorkerLoopReport {
    pub passes: u64,
    pub claimed: u64,
    pub processed: u64,
    pub published: u64,
    pub retried: u64,
    pub dead_lettered: u64,
}

#[derive(Debug, Clone)]
pub struct OutboxWorker {
    store: Arc<SqlxControlPlane>,
    worker_id: Uuid,
    batch_size: u32,
    claim_timeout: Duration,
    retry_policy: RetryPolicy,
}

impl OutboxWorker {
    pub fn new(store: Arc<SqlxControlPlane>, worker_id: Uuid) -> Self {
        Self {
            store,
            worker_id,
            batch_size: 100,
            claim_timeout: Duration::minutes(5),
            retry_policy: RetryPolicy::default(),
        }
    }

    pub fn with_batch_size(mut self, batch_size: u32) -> Self {
        self.batch_size = batch_size.clamp(1, MAX_OUTBOX_CLAIM_BATCH_SIZE);
        self
    }

    pub fn with_claim_timeout(mut self, claim_timeout: Duration) -> Self {
        self.claim_timeout = claim_timeout.max(Duration::seconds(1));
        self
    }

    pub fn with_retry_base(mut self, retry_base: Duration) -> Self {
        self.retry_policy.base_delay = retry_base.max(Duration::milliseconds(1));
        self
    }

    pub fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy.normalized();
        self
    }

    pub async fn run_once<H: OutboxHandler + ?Sized>(
        &self,
        handler: &H,
    ) -> Result<WorkerRunReport, PersistenceError> {
        let now = Utc::now();
        let events = self
            .store
            .claim_outbox_for_aggregate_types(
                self.worker_id,
                self.batch_size,
                now,
                self.claim_timeout,
                handler.aggregate_type_filter().unwrap_or(&[]),
            )
            .await?;
        let mut report = WorkerRunReport {
            claimed: u32::try_from(events.len()).unwrap_or(u32::MAX),
            ..WorkerRunReport::default()
        };
        for event in events {
            match handler.handle(&event).await {
                Ok(()) => {
                    self.store
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
                    tracing::warn!(
                        target: "erpx.worker",
                        worker_id = %context.worker_id,
                        event_id = %context.event_id,
                        event_type = %context.event_type,
                        aggregate_type = %context.aggregate_type,
                        aggregate_id = %context.aggregate_id,
                        correlation_id = %context.correlation_id,
                        retryable = context.retryable,
                        error = %context.message,
                        "worker_event_failed"
                    );
                    let WorkerError::Retryable(error) = error else {
                        unreachable!("matched retryable worker error")
                    };
                    match self.retry_policy.decide(
                        event.attempts,
                        event.max_attempts,
                        &WorkerError::Retryable(error.clone()),
                    ) {
                        RetryDisposition::Retry { delay } => {
                            self.store
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
                            self.store
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
                    tracing::error!(
                        target: "erpx.worker",
                        worker_id = %context.worker_id,
                        event_id = %context.event_id,
                        event_type = %context.event_type,
                        aggregate_type = %context.aggregate_type,
                        aggregate_id = %context.aggregate_id,
                        correlation_id = %context.correlation_id,
                        retryable = context.retryable,
                        error = %context.message,
                        "worker_event_dead_lettered"
                    );
                    self.store
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

    pub async fn run_until_stopped<H: OutboxHandler + ?Sized>(
        &self,
        handler: &H,
        mut stop: tokio::sync::watch::Receiver<bool>,
        poll_interval: StdDuration,
    ) -> Result<WorkerLoopReport, PersistenceError> {
        let mut total = WorkerLoopReport::default();
        loop {
            if *stop.borrow() {
                break;
            }
            let report = self.run_once(handler).await?;
            total.passes = total.passes.saturating_add(1);
            total.claimed = total.claimed.saturating_add(u64::from(report.claimed));
            total.processed = total.processed.saturating_add(u64::from(report.processed));
            total.published = total.published.saturating_add(u64::from(report.published));
            total.retried = total.retried.saturating_add(u64::from(report.retried));
            total.dead_lettered = total
                .dead_lettered
                .saturating_add(u64::from(report.dead_lettered));
            if report.claimed == 0 {
                tokio::select! {
                    _ = tokio::time::sleep(poll_interval) => {}
                    changed = stop.changed() => {
                        if changed.is_err() || *stop.borrow() {
                            break;
                        }
                    }
                }
            }
        }
        Ok(total)
    }

    pub async fn run_until_shutdown<H: OutboxHandler + ?Sized>(
        &self,
        handler: &H,
        poll_interval: StdDuration,
    ) -> Result<WorkerLoopReport, PersistenceError> {
        let (sender, receiver) = tokio::sync::watch::channel(false);
        let signal_task = tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            let _ = sender.send(true);
        });
        let result = self
            .run_until_stopped(handler, receiver, poll_interval)
            .await;
        signal_task.abort();
        result
    }

    pub fn worker_id(&self) -> Uuid {
        self.worker_id
    }

    pub fn with_worker_id(mut self, worker_id: Uuid) -> Self {
        self.worker_id = worker_id;
        self
    }
}

#[async_trait]
pub trait ProjectionHandler: Send + Sync {
    /// A projection must not checkpoint events owned by another context.
    /// Concrete handlers must explicitly opt into the event families they
    /// own; an unconfigured handler ignores every event.
    fn handles_event(&self, _projection_name: &str, _event: &OutboxEnvelope) -> bool {
        false
    }

    async fn clear(&self, projection_name: &str) -> Result<(), WorkerError>;
    async fn project(
        &self,
        projection_name: &str,
        event: &OutboxEnvelope,
    ) -> Result<(), WorkerError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionRebuildReport {
    pub projection_name: String,
    pub events_replayed: u64,
}

impl OutboxWorker {
    pub async fn rebuild_projection<H: ProjectionHandler + ?Sized>(
        &self,
        projection_name: &str,
        handler: &H,
    ) -> Result<ProjectionRebuildReport, PersistenceError> {
        handler
            .clear(projection_name)
            .await
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        self.store.reset_projection(projection_name).await?;
        let events = self.store.events_for_rebuild().await?;
        let mut replayed = 0_u64;
        for event in events {
            if self.project_once(projection_name, &event, handler).await? {
                replayed = replayed.saturating_add(1);
            }
        }
        Ok(ProjectionRebuildReport {
            projection_name: projection_name.into(),
            events_replayed: replayed,
        })
    }

    pub async fn project_once<H: ProjectionHandler + ?Sized>(
        &self,
        projection_name: &str,
        event: &OutboxEnvelope,
        handler: &H,
    ) -> Result<bool, PersistenceError> {
        if !handler.handles_event(projection_name, event) {
            return Ok(false);
        }
        let claimed = self
            .store
            .try_claim_projection_event(
                projection_name,
                event,
                self.worker_id,
                Utc::now(),
                self.claim_timeout,
            )
            .await?;
        if !claimed {
            return Ok(false);
        }
        match self
            .store
            .projection_checkpoint_decision(projection_name, event)
            .await?
        {
            ProjectionCheckpointDecision::AlreadyApplied => {
                self.store
                    .complete_projection_event(
                        projection_name,
                        event.id,
                        self.worker_id,
                        Utc::now(),
                    )
                    .await?;
                return Ok(false);
            }
            ProjectionCheckpointDecision::ConflictingEvent { .. } => {
                self.store
                    .release_projection_event(projection_name, event.id, self.worker_id)
                    .await?;
                return Err(PersistenceError::InvalidData(
                    "projection event conflicts with the current checkpoint event identity".into(),
                ));
            }
            ProjectionCheckpointDecision::OutOfOrder { .. } => {
                self.store
                    .release_projection_event(projection_name, event.id, self.worker_id)
                    .await?;
                return Err(PersistenceError::InvalidData(
                    "projection event is out of order".into(),
                ));
            }
            ProjectionCheckpointDecision::Apply => {}
        }
        if let Err(error) = handler.project(projection_name, event).await {
            let context = error.context(event, self.worker_id);
            tracing::warn!(
                target: "erpx.worker",
                worker_id = %context.worker_id,
                event_id = %context.event_id,
                event_type = %context.event_type,
                aggregate_type = %context.aggregate_type,
                aggregate_id = %context.aggregate_id,
                correlation_id = %context.correlation_id,
                retryable = context.retryable,
                error = %context.message,
                projection = projection_name,
                "worker_projection_failed"
            );
            self.store
                .release_projection_event(projection_name, event.id, self.worker_id)
                .await?;
            return Err(PersistenceError::InvalidData(error.to_string()));
        }
        let advanced = self
            .store
            .advance_projection_checkpoint(projection_name, event, Utc::now())
            .await?;
        if !advanced {
            match self
                .store
                .projection_checkpoint_decision(projection_name, event)
                .await?
            {
                ProjectionCheckpointDecision::AlreadyApplied => {}
                ProjectionCheckpointDecision::Apply
                | ProjectionCheckpointDecision::ConflictingEvent { .. }
                | ProjectionCheckpointDecision::OutOfOrder { .. } => {
                    self.store
                        .release_projection_event(projection_name, event.id, self.worker_id)
                        .await?;
                    return Err(PersistenceError::InvalidData(
                        "projection checkpoint could not advance".into(),
                    ));
                }
            }
        }
        self.store
            .complete_projection_event(projection_name, event.id, self.worker_id, Utc::now())
            .await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct UnconfiguredProjectionHandler;

    #[async_trait::async_trait]
    impl ProjectionHandler for UnconfiguredProjectionHandler {
        async fn clear(&self, _projection_name: &str) -> Result<(), WorkerError> {
            Ok(())
        }

        async fn project(
            &self,
            _projection_name: &str,
            _event: &OutboxEnvelope,
        ) -> Result<(), WorkerError> {
            Ok(())
        }
    }

    fn sample_event() -> OutboxEnvelope {
        OutboxEnvelope {
            id: Uuid::from_u128(100),
            event_type: "unowned.event".into(),
            event_version: 1,
            aggregate_type: "unowned_aggregate".into(),
            aggregate_id: Uuid::from_u128(101),
            aggregate_version: 1,
            active_scope: None,
            actor: None,
            correlation_id: Uuid::from_u128(102),
            causation_id: None,
            idempotency_key: None,
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
        }
    }

    #[test]
    fn unconfigured_projection_handlers_fail_closed() {
        let handler = UnconfiguredProjectionHandler;
        assert!(!handler.handles_event("projection", &sample_event()));
    }

    #[test]
    fn retry_policy_is_bounded_and_dead_letters_at_the_attempt_limit() {
        let policy = RetryPolicy {
            base_delay: Duration::seconds(2),
            max_delay: Duration::seconds(5),
        };
        assert_eq!(
            policy.decide(1, 3, &WorkerError::Retryable("temporary".into())),
            RetryDisposition::Retry {
                delay: Duration::seconds(2)
            }
        );
        assert_eq!(
            policy.decide(3, 3, &WorkerError::Retryable("temporary".into())),
            RetryDisposition::DeadLetter
        );
        assert_eq!(
            policy.decide(1, 3, &WorkerError::Permanent("invalid payload".into())),
            RetryDisposition::DeadLetter
        );
        assert_eq!(
            policy.decide(5, 20, &WorkerError::Retryable("temporary".into())),
            RetryDisposition::Retry {
                delay: Duration::seconds(5)
            }
        );
    }

    #[test]
    fn worker_error_context_is_correlated_and_redacted() {
        let event = OutboxEnvelope {
            id: Uuid::from_u128(1),
            event_type: "fixture.created".into(),
            event_version: 1,
            aggregate_type: "fixture".into(),
            aggregate_id: Uuid::from_u128(2),
            aggregate_version: 1,
            active_scope: Some(serde_json::json!({"data_profile_key": "holding-1"})),
            actor: Some(serde_json::json!({"principal_ref": "worker-1"})),
            correlation_id: Uuid::from_u128(3),
            causation_id: Some(Uuid::from_u128(5)),
            idempotency_key: Some("fixture-1".into()),
            payload: serde_json::json!({"secret":"must not be logged"}),
            occurred_at: Utc::now(),
            available_at: Utc::now(),
            attempts: 1,
            max_attempts: 3,
            claimed_by: None,
            claimed_at: None,
            published_at: None,
            dead_lettered_at: None,
            last_error: None,
            last_error_context: None,
        };
        let context = WorkerError::Retryable("postgres://user:password@host/db".into())
            .context(&event, Uuid::from_u128(4));
        assert_eq!(context.event_id, event.id);
        assert_eq!(context.correlation_id, event.correlation_id);
        assert_eq!(context.active_scope, event.active_scope);
        assert_eq!(context.actor, event.actor);
        assert_eq!(context.causation_id, event.causation_id);
        assert_eq!(context.message, "worker handler failed");
        context
            .validate_for(&event, Uuid::from_u128(4))
            .expect("context must bind to the claimed event");
        assert!(
            !serde_json::to_string(&context)
                .unwrap()
                .contains("password")
        );
    }

    #[test]
    fn worker_error_context_rejects_holding_and_correlation_mismatch() {
        let event = OutboxEnvelope {
            id: Uuid::from_u128(11),
            event_type: "fixture.created".into(),
            event_version: 1,
            aggregate_type: "fixture".into(),
            aggregate_id: Uuid::from_u128(12),
            aggregate_version: 1,
            active_scope: Some(serde_json::json!({"holding_id": "holding-1"})),
            actor: Some(serde_json::json!({"actor_ref": "actor-1"})),
            correlation_id: Uuid::from_u128(13),
            causation_id: None,
            idempotency_key: Some("fixture-11".into()),
            payload: serde_json::json!({}),
            occurred_at: Utc::now(),
            available_at: Utc::now(),
            attempts: 1,
            max_attempts: 3,
            claimed_by: None,
            claimed_at: None,
            published_at: None,
            dead_lettered_at: None,
            last_error: None,
            last_error_context: None,
        };
        let mut context =
            WorkerError::Retryable("temporary".into()).context(&event, Uuid::from_u128(14));
        context.active_scope = Some(serde_json::json!({"holding_id": "holding-2"}));
        assert_eq!(
            context.validate_for(&event, Uuid::from_u128(14)),
            Err("holding scope does not match the claim")
        );
        context.active_scope = event.active_scope.clone();
        context.correlation_id = Uuid::from_u128(99);
        assert_eq!(
            context.validate_for(&event, Uuid::from_u128(14)),
            Err("causation or correlation does not match the claim")
        );
    }

    #[test]
    fn worker_error_context_drops_secret_bearing_actor_metadata() {
        let event = OutboxEnvelope {
            id: Uuid::from_u128(21),
            event_type: "fixture.created".into(),
            event_version: 1,
            aggregate_type: "fixture".into(),
            aggregate_id: Uuid::from_u128(22),
            aggregate_version: 1,
            active_scope: Some(serde_json::json!({"holding_id": "holding-1"})),
            actor: Some(serde_json::json!({"token": "do-not-persist"})),
            correlation_id: Uuid::from_u128(23),
            causation_id: None,
            idempotency_key: Some("secret-token-value".into()),
            payload: serde_json::json!({}),
            occurred_at: Utc::now(),
            available_at: Utc::now(),
            attempts: 1,
            max_attempts: 3,
            claimed_by: None,
            claimed_at: None,
            published_at: None,
            dead_lettered_at: None,
            last_error: None,
            last_error_context: None,
        };
        let context =
            WorkerError::Permanent("invalid event".into()).context(&event, Uuid::from_u128(24));
        assert_eq!(context.actor, None);
        assert_eq!(context.idempotency_key, None);
        assert!(
            !serde_json::to_string(&context)
                .unwrap()
                .contains("do-not-persist")
        );
    }
}
