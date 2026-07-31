//! Restart-safe Operations SLA notification delivery.
//!
//! Escalation creates the notification in the same tenant-write transaction as
//! the lifecycle event. This worker claims only due rows, emits the durable
//! tenant outbox handoff, and acknowledges the exact claim. A provider can be
//! injected later without changing the authoritative inbox contract.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_infrastructure::{
    ClaimedOperationsSlaNotification, MAX_OPERATIONS_SLA_NOTIFICATION_LEASE_SECONDS,
    MAX_OPERATIONS_SLA_NOTIFICATION_PAGE_SIZE, MIN_OPERATIONS_SLA_NOTIFICATION_LEASE_SECONDS,
    OperationsSlaNotification, PersistenceError, SqlxOperationsStore,
};
use std::collections::HashSet;
use thiserror::Error;
use uuid::Uuid;

use crate::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const OPERATIONS_SLA_NOTIFICATION_HANDLER_NAME: &str = "operations_sla_notifications";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OperationsSlaNotificationPublishError {
    #[error("Operations SLA notification publisher is unavailable")]
    Unavailable,
    #[error("Operations SLA notification publisher rejected the notification")]
    Rejected,
}

#[async_trait]
pub trait OperationsSlaNotificationSource: Send + Sync + Clone + 'static {
    async fn claim(
        &self,
        worker_id: Uuid,
        at: DateTime<Utc>,
        lease: Duration,
        page_size: u32,
    ) -> Result<Vec<ClaimedOperationsSlaNotification>, PersistenceError>;

    async fn acknowledge(
        &self,
        claim: &ClaimedOperationsSlaNotification,
        at: DateTime<Utc>,
    ) -> Result<(), PersistenceError>;

    async fn release(
        &self,
        claim: &ClaimedOperationsSlaNotification,
    ) -> Result<(), PersistenceError>;
}

#[async_trait]
impl OperationsSlaNotificationSource for SqlxOperationsStore {
    async fn claim(
        &self,
        worker_id: Uuid,
        at: DateTime<Utc>,
        lease: Duration,
        page_size: u32,
    ) -> Result<Vec<ClaimedOperationsSlaNotification>, PersistenceError> {
        self.claim_sla_notifications(worker_id, at, lease, page_size)
            .await
    }

    async fn acknowledge(
        &self,
        claim: &ClaimedOperationsSlaNotification,
        at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        self.acknowledge_sla_notification(claim, at).await
    }

    async fn release(
        &self,
        claim: &ClaimedOperationsSlaNotification,
    ) -> Result<(), PersistenceError> {
        self.release_sla_notification(claim).await
    }
}

#[async_trait]
pub trait OperationsSlaNotificationPublisher: Send + Sync + Clone + 'static {
    async fn publish(
        &self,
        notification: &OperationsSlaNotification,
    ) -> Result<(), OperationsSlaNotificationPublishError>;
}

/// The default publisher is a durable tenant-outbox handoff. It does not
/// pretend that email, chat, push, or SMS delivery has happened; those are
/// downstream consumers of this event.
#[derive(Debug, Clone)]
pub struct OperationsOutboxSlaNotificationPublisher {
    store: SqlxOperationsStore,
    actor_ref: String,
}

impl OperationsOutboxSlaNotificationPublisher {
    pub fn new(
        store: SqlxOperationsStore,
        actor_ref: impl Into<String>,
    ) -> Result<Self, WorkerRuntimeError> {
        let actor_ref = actor_ref.into();
        if actor_ref.trim().is_empty()
            || actor_ref.len() > 255
            || actor_ref.chars().any(char::is_control)
        {
            return Err(WorkerRuntimeError::Configuration(
                "Operations notification actor is invalid".into(),
            ));
        }
        Ok(Self { store, actor_ref })
    }
}

#[async_trait]
impl OperationsSlaNotificationPublisher for OperationsOutboxSlaNotificationPublisher {
    async fn publish(
        &self,
        notification: &OperationsSlaNotification,
    ) -> Result<(), OperationsSlaNotificationPublishError> {
        self.store
            .append_sla_notification_delivery_event(notification, &self.actor_ref)
            .await
            .map(|_| ())
            .map_err(|_| OperationsSlaNotificationPublishError::Unavailable)
    }
}

#[derive(Debug, Clone)]
pub struct OperationsSlaNotificationHandler<S, P> {
    source: S,
    publisher: P,
    batch_size: u32,
    lease: Duration,
}

impl<S, P> OperationsSlaNotificationHandler<S, P>
where
    S: OperationsSlaNotificationSource,
    P: OperationsSlaNotificationPublisher,
{
    pub fn new(
        source: S,
        publisher: P,
        batch_size: u32,
        lease: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        if batch_size == 0 || batch_size > MAX_OPERATIONS_SLA_NOTIFICATION_PAGE_SIZE {
            return Err(WorkerRuntimeError::Configuration(
                "Operations notification batch size is outside its bound".into(),
            ));
        }
        if !(MIN_OPERATIONS_SLA_NOTIFICATION_LEASE_SECONDS
            ..=MAX_OPERATIONS_SLA_NOTIFICATION_LEASE_SECONDS)
            .contains(&lease.num_seconds())
        {
            return Err(WorkerRuntimeError::Configuration(
                "Operations notification lease is outside its bound".into(),
            ));
        }
        Ok(Self {
            source,
            publisher,
            batch_size,
            lease,
        })
    }

    /// Execute a pass at an explicit scheduler time. Production callers use
    /// `run_once`, while acceptance/replay callers can prove deterministic
    /// due-time behavior without sleeping or mutating notification state.
    pub async fn run_once_at(
        &self,
        worker_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "Operations notification worker identity is required".into(),
            ));
        }
        let claimed = self
            .source
            .claim(worker_id, at, self.lease, self.batch_size)
            .await
            .map_err(redacted_operations_notification_error)?;
        if claimed.is_empty() {
            return Ok(WorkerHandlerReport::default());
        }
        if claimed.len() > self.batch_size as usize {
            self.release_claims(&claimed).await?;
            return Err(WorkerRuntimeError::Configuration(
                "Operations notification source exceeded its batch bound".into(),
            ));
        }
        if let Err(error) = validate_claims(&claimed, worker_id, at) {
            self.release_claims(&claimed).await?;
            return Err(error);
        }
        let mut report = WorkerHandlerReport {
            claimed: claimed.len() as u64,
            has_work: true,
            ..WorkerHandlerReport::default()
        };
        for (index, claim) in claimed.iter().enumerate() {
            match self.publisher.publish(&claim.notification).await {
                Ok(()) => {
                    if self.source.acknowledge(claim, at).await.is_err() {
                        self.release_claims(&claimed[index..]).await?;
                        return Err(WorkerRuntimeError::Configuration(
                            "Operations notification acknowledgement mismatch".into(),
                        ));
                    }
                    report.processed = report.processed.saturating_add(1);
                    report.published = report.published.saturating_add(1);
                }
                Err(_) => {
                    // A failed provider is a batch-level retry signal. Stop
                    // calling it and release the failed claim plus every
                    // claim that has not been attempted yet. The durable
                    // source will make those rows eligible again, while any
                    // already acknowledged rows remain terminal.
                    self.release_claims(&claimed[index..]).await?;
                    report.retried = report
                        .retried
                        .saturating_add((claimed.len() - index) as u64);
                    break;
                }
            }
        }
        Ok(report)
    }

    async fn release_claims(
        &self,
        claims: &[ClaimedOperationsSlaNotification],
    ) -> Result<(), WorkerRuntimeError> {
        let mut first_error = None;
        for claim in claims {
            if let Err(error) = self.source.release(claim).await
                && first_error.is_none()
            {
                first_error = Some(redacted_operations_notification_error(error));
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(())
    }
}

#[async_trait]
impl<S, P> NamedWorkerHandler for OperationsSlaNotificationHandler<S, P>
where
    S: OperationsSlaNotificationSource,
    P: OperationsSlaNotificationPublisher,
{
    fn name(&self) -> &str {
        OPERATIONS_SLA_NOTIFICATION_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        self.run_once_at(worker_id, Utc::now()).await
    }
}

fn redacted_operations_notification_error(error: PersistenceError) -> WorkerRuntimeError {
    tracing::error!(error_kind = ?error, "Operations notification persistence failed");
    WorkerRuntimeError::Configuration("Operations notification persistence failed".into())
}

fn validate_claims(
    claims: &[ClaimedOperationsSlaNotification],
    worker_id: Uuid,
    at: DateTime<Utc>,
) -> Result<(), WorkerRuntimeError> {
    let mut keys = HashSet::with_capacity(claims.len());
    for claim in claims {
        if claim.notification.holding_ref.trim().is_empty()
            || claim.notification.holding_ref.len() > 255
            || claim.notification.holding_ref.chars().any(char::is_control)
            || claim.notification.notification_key.trim().is_empty()
            || claim.notification.notification_key.len() > 512
            || claim
                .notification
                .notification_key
                .chars()
                .any(char::is_control)
            || !keys.insert(claim.notification.notification_key.as_str())
            || claim.claimed_by != worker_id
            || claim.claim_token.is_nil()
            || claim.notification.exception_id.is_nil()
            || claim.notification.exception_version == 0
            || claim.notification.recipient_ref.trim().is_empty()
            || claim.notification.recipient_ref.len() > 255
            || claim
                .notification
                .recipient_ref
                .chars()
                .any(char::is_control)
            || claim.notification.not_before > at
            || claim.notification.not_before >= claim.notification.expires_at
            || claim.notification.expires_at <= at
            || claim.claim_expires_at <= at
        {
            return Err(WorkerRuntimeError::Configuration(
                "Operations notification source returned an invalid claim".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_infrastructure::OperationsSlaNotificationKind;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Source {
        claims: Arc<Mutex<Vec<ClaimedOperationsSlaNotification>>>,
        acknowledged: Arc<Mutex<u64>>,
        acknowledged_keys: Arc<Mutex<Vec<String>>>,
        released: Arc<Mutex<u64>>,
        released_keys: Arc<Mutex<Vec<String>>>,
        release_attempted_keys: Arc<Mutex<Vec<String>>>,
        release_fail_on_key: Arc<Mutex<Option<String>>>,
        requeued: Arc<Mutex<Vec<ClaimedOperationsSlaNotification>>>,
    }

    #[async_trait]
    impl OperationsSlaNotificationSource for Source {
        async fn claim(
            &self,
            _worker_id: Uuid,
            _at: DateTime<Utc>,
            _lease: Duration,
            _page_size: u32,
        ) -> Result<Vec<ClaimedOperationsSlaNotification>, PersistenceError> {
            let mut claims = std::mem::take(&mut *self.claims.lock().expect("claims mutex"));
            claims.extend(std::mem::take(
                &mut *self.requeued.lock().expect("requeued mutex"),
            ));
            Ok(claims)
        }

        async fn acknowledge(
            &self,
            claim: &ClaimedOperationsSlaNotification,
            _at: DateTime<Utc>,
        ) -> Result<(), PersistenceError> {
            *self.acknowledged.lock().expect("ack mutex") += 1;
            self.acknowledged_keys
                .lock()
                .expect("acknowledged keys mutex")
                .push(claim.notification.notification_key.clone());
            Ok(())
        }

        async fn release(
            &self,
            claim: &ClaimedOperationsSlaNotification,
        ) -> Result<(), PersistenceError> {
            self.release_attempted_keys
                .lock()
                .expect("release attempts mutex")
                .push(claim.notification.notification_key.clone());
            if self
                .release_fail_on_key
                .lock()
                .expect("release failure mutex")
                .as_deref()
                == Some(claim.notification.notification_key.as_str())
            {
                return Err(PersistenceError::InvalidData("test release failure".into()));
            }
            *self.released.lock().expect("release mutex") += 1;
            self.released_keys
                .lock()
                .expect("released keys mutex")
                .push(claim.notification.notification_key.clone());
            self.requeued
                .lock()
                .expect("requeued mutex")
                .push(claim.clone());
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct Publisher {
        published: Arc<Mutex<u64>>,
        fail_on_publish: Arc<Mutex<Option<u64>>>,
        durable_handoff_keys: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl OperationsSlaNotificationPublisher for Publisher {
        async fn publish(
            &self,
            notification: &OperationsSlaNotification,
        ) -> Result<(), OperationsSlaNotificationPublishError> {
            let mut published = self.published.lock().expect("published mutex");
            let should_fail = {
                let mut failure = self
                    .fail_on_publish
                    .lock()
                    .expect("publisher failure mutex");
                if failure.is_some_and(|failure| failure == *published) {
                    *failure = None;
                    true
                } else {
                    false
                }
            };
            if should_fail {
                return Err(OperationsSlaNotificationPublishError::Unavailable);
            }
            self.durable_handoff_keys
                .lock()
                .expect("durable handoff mutex")
                .push(notification.notification_key.clone());
            *published += 1;
            Ok(())
        }
    }

    fn claim(index: u128, worker_id: Uuid, at: DateTime<Utc>) -> ClaimedOperationsSlaNotification {
        ClaimedOperationsSlaNotification {
            notification: OperationsSlaNotification {
                notification_key: format!("operations-sla:exception:{index}"),
                holding_ref: "holding-a".into(),
                exception_id: Uuid::from_u128(index),
                exception_version: 2,
                recipient_ref: "manager".into(),
                kind: OperationsSlaNotificationKind::SlaEscalated,
                not_before: at,
                expires_at: at + Duration::days(1),
                payload: serde_json::json!({"kind":"sla_escalated"}),
            },
            claimed_by: worker_id,
            claim_token: Uuid::from_u128(3),
            claim_expires_at: at + Duration::seconds(30),
        }
    }

    #[tokio::test]
    async fn handler_acknowledges_durable_claim_after_publish() {
        let worker_id = Uuid::from_u128(4);
        let at = Utc::now();
        let source = Source {
            claims: Arc::new(Mutex::new(vec![claim(1, worker_id, at)])),
            ..Source::default()
        };
        let publisher = Publisher::default();
        let handler = OperationsSlaNotificationHandler::new(
            source.clone(),
            publisher.clone(),
            10,
            Duration::seconds(30),
        )
        .expect("valid handler");
        let report = handler
            .run_once_at(worker_id, at)
            .await
            .expect("notification run");
        assert_eq!(report.processed, 1);
        assert_eq!(*publisher.published.lock().expect("published"), 1);
        assert_eq!(*source.acknowledged.lock().expect("acknowledged"), 1);
        assert_eq!(*source.released.lock().expect("released"), 0);
    }

    #[tokio::test]
    async fn handler_stops_after_publisher_failure_and_releases_remaining_claims() {
        let worker_id = Uuid::from_u128(4);
        let at = Utc::now();
        let source = Source {
            claims: Arc::new(Mutex::new(vec![
                claim(1, worker_id, at),
                claim(2, worker_id, at),
                claim(3, worker_id, at),
            ])),
            ..Source::default()
        };
        let publisher = Publisher {
            fail_on_publish: Arc::new(Mutex::new(Some(1))),
            ..Publisher::default()
        };
        let handler = OperationsSlaNotificationHandler::new(
            source.clone(),
            publisher.clone(),
            3,
            Duration::seconds(30),
        )
        .expect("valid handler");

        let report = handler
            .run_once_at(worker_id, at)
            .await
            .expect("notification retry release");

        assert_eq!(report.claimed, 3);
        assert_eq!(report.processed, 1);
        assert_eq!(report.published, 1);
        assert_eq!(report.retried, 2);
        assert_eq!(*publisher.published.lock().expect("published"), 1);
        assert_eq!(*source.acknowledged.lock().expect("acknowledged"), 1);
        assert_eq!(*source.released.lock().expect("released"), 2);
        assert_eq!(
            *source.acknowledged_keys.lock().expect("acknowledged keys"),
            vec!["operations-sla:exception:1".to_string()]
        );
        assert_eq!(
            *source.released_keys.lock().expect("released keys"),
            vec![
                "operations-sla:exception:2".to_string(),
                "operations-sla:exception:3".to_string(),
            ]
        );
        assert_eq!(
            *publisher
                .durable_handoff_keys
                .lock()
                .expect("durable handoffs"),
            vec!["operations-sla:exception:1".to_string()]
        );

        let retry_report = handler
            .run_once_at(worker_id, at)
            .await
            .expect("released claims must be restartable");
        assert_eq!(retry_report.claimed, 2);
        assert_eq!(retry_report.processed, 2);
        assert_eq!(retry_report.published, 2);
        assert_eq!(retry_report.retried, 0);
        assert_eq!(*source.acknowledged.lock().expect("acknowledged"), 3);
        assert_eq!(*source.released.lock().expect("released"), 2);
        assert_eq!(
            *source.acknowledged_keys.lock().expect("acknowledged keys"),
            vec![
                "operations-sla:exception:1".to_string(),
                "operations-sla:exception:2".to_string(),
                "operations-sla:exception:3".to_string(),
            ]
        );
        assert_eq!(
            *publisher
                .durable_handoff_keys
                .lock()
                .expect("durable handoffs"),
            vec![
                "operations-sla:exception:1".to_string(),
                "operations-sla:exception:2".to_string(),
                "operations-sla:exception:3".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn handler_attempts_all_releases_after_one_release_fails() {
        let worker_id = Uuid::from_u128(4);
        let at = Utc::now();
        let source = Source {
            claims: Arc::new(Mutex::new(vec![
                claim(1, worker_id, at),
                claim(2, worker_id, at),
                claim(3, worker_id, at),
            ])),
            release_fail_on_key: Arc::new(Mutex::new(Some(
                "operations-sla:exception:1".to_string(),
            ))),
            ..Source::default()
        };
        let publisher = Publisher {
            fail_on_publish: Arc::new(Mutex::new(Some(0))),
            ..Publisher::default()
        };
        let handler = OperationsSlaNotificationHandler::new(
            source.clone(),
            publisher.clone(),
            3,
            Duration::seconds(30),
        )
        .expect("valid handler");

        let error = handler
            .run_once_at(worker_id, at)
            .await
            .expect_err("release failure must remain visible");

        assert!(matches!(error, WorkerRuntimeError::Configuration(_)));
        assert_eq!(
            *source
                .release_attempted_keys
                .lock()
                .expect("release attempts"),
            vec![
                "operations-sla:exception:1".to_string(),
                "operations-sla:exception:2".to_string(),
                "operations-sla:exception:3".to_string(),
            ]
        );
        assert_eq!(
            *source.released_keys.lock().expect("released keys"),
            vec![
                "operations-sla:exception:2".to_string(),
                "operations-sla:exception:3".to_string(),
            ]
        );
        assert_eq!(*publisher.published.lock().expect("published"), 0);
        assert!(
            publisher
                .durable_handoff_keys
                .lock()
                .expect("durable handoffs")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn handler_rejects_expired_or_foreign_claim_before_publish() {
        let worker_id = Uuid::from_u128(4);
        let at = Utc::now();
        let mut expired = claim(1, worker_id, at);
        expired.claim_expires_at = at;
        let source = Source {
            claims: Arc::new(Mutex::new(vec![expired])),
            ..Source::default()
        };
        let publisher = Publisher::default();
        let handler = OperationsSlaNotificationHandler::new(
            source.clone(),
            publisher.clone(),
            1,
            Duration::seconds(30),
        )
        .expect("valid handler");

        let error = handler
            .run_once_at(worker_id, at)
            .await
            .expect_err("expired claim must fail closed");

        assert!(matches!(error, WorkerRuntimeError::Configuration(_)));
        assert_eq!(*publisher.published.lock().expect("published"), 0);
        assert_eq!(*source.released.lock().expect("released"), 1);
    }

    #[tokio::test]
    async fn handler_releases_claims_when_source_exceeds_batch_bound() {
        let worker_id = Uuid::from_u128(4);
        let at = Utc::now();
        let source = Source {
            claims: Arc::new(Mutex::new(vec![
                claim(1, worker_id, at),
                claim(2, worker_id, at),
            ])),
            ..Source::default()
        };
        let publisher = Publisher::default();
        let handler = OperationsSlaNotificationHandler::new(
            source.clone(),
            publisher.clone(),
            1,
            Duration::seconds(30),
        )
        .expect("valid handler");

        let error = handler
            .run_once_at(worker_id, at)
            .await
            .expect_err("oversized source response must fail closed");

        assert!(matches!(error, WorkerRuntimeError::Configuration(_)));
        assert_eq!(*publisher.published.lock().expect("published"), 0);
        assert_eq!(*source.released.lock().expect("released"), 2);
    }

    #[test]
    fn handler_rejects_invalid_bounds() {
        let source = Source::default();
        let publisher = Publisher::default();
        assert!(
            OperationsSlaNotificationHandler::new(
                source.clone(),
                publisher.clone(),
                0,
                Duration::seconds(30),
            )
            .is_err()
        );
        assert!(
            OperationsSlaNotificationHandler::new(
                source,
                publisher,
                1,
                Duration::seconds(MIN_OPERATIONS_SLA_NOTIFICATION_LEASE_SECONDS - 1),
            )
            .is_err()
        );
    }
}
