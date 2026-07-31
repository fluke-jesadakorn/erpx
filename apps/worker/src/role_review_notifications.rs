//! Restart-safe delivery of durable role-expiry review notifications.
//!
//! The control-plane review tables remain authoritative for claim state. This
//! module only supplies the worker lease/reconcile loop and an injected
//! publisher boundary; provider credentials and message transport stay outside
//! the worker contract.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_domain::access::{RoleExpiryNotificationKind, Scope};
use erpx_infrastructure::{
    ClaimedRoleExpiryNotification, RoleReviewPersistence, RoleReviewPersistenceError,
};
use std::collections::HashSet;
use thiserror::Error;
use uuid::Uuid;

use super::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const ROLE_REVIEW_NOTIFICATION_HANDLER_NAME: &str = "role_review_notifications";
const MAX_ROLE_REVIEW_HORIZON: Duration = Duration::days(365);
const MAX_ROLE_REVIEW_LEASE: Duration = Duration::hours(1);
const MAX_ROLE_REVIEW_NOTIFICATION_CLAIM: usize = 100;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RoleReviewNotificationPublishError {
    #[error("notification publisher is unavailable")]
    Unavailable,
    #[error("notification publisher rejected the notification")]
    Rejected,
}

#[async_trait]
pub trait RoleReviewNotificationSource: Send + Sync + Clone + 'static {
    async fn claim(
        &self,
        scope: &Scope,
        claimant_ref: &str,
        at: DateTime<Utc>,
        horizon: Duration,
        lease: Duration,
    ) -> Result<Vec<ClaimedRoleExpiryNotification>, RoleReviewPersistenceError>;

    async fn reconcile(
        &self,
        claim_key: &str,
        notification_keys: &[String],
        at: DateTime<Utc>,
    ) -> Result<u64, RoleReviewPersistenceError>;

    async fn release(
        &self,
        claim_key: &str,
        notification_keys: &[String],
    ) -> Result<(), RoleReviewPersistenceError>;
}

#[async_trait]
impl RoleReviewNotificationSource for RoleReviewPersistence {
    async fn claim(
        &self,
        scope: &Scope,
        claimant_ref: &str,
        at: DateTime<Utc>,
        horizon: Duration,
        lease: Duration,
    ) -> Result<Vec<ClaimedRoleExpiryNotification>, RoleReviewPersistenceError> {
        self.claim_expiry_notifications(scope, claimant_ref, at, horizon, lease)
            .await
    }

    async fn reconcile(
        &self,
        claim_key: &str,
        notification_keys: &[String],
        at: DateTime<Utc>,
    ) -> Result<u64, RoleReviewPersistenceError> {
        self.reconcile_expiry_notifications(claim_key, notification_keys, at)
            .await
    }

    async fn release(
        &self,
        claim_key: &str,
        notification_keys: &[String],
    ) -> Result<(), RoleReviewPersistenceError> {
        self.release_expiry_notifications(claim_key, notification_keys)
            .await
            .map(|_| ())
    }
}

#[async_trait]
pub trait RoleReviewNotificationPublisher: Send + Sync + Clone + 'static {
    async fn publish(
        &self,
        notification: &ClaimedRoleExpiryNotification,
    ) -> Result<(), RoleReviewNotificationPublishError>;
}

#[derive(Debug, Clone)]
pub struct RoleReviewNotificationHandler<S, P> {
    source: S,
    publisher: P,
    scope: Scope,
    claimant_ref: String,
    horizon: Duration,
    lease: Duration,
}

impl<S, P> RoleReviewNotificationHandler<S, P>
where
    S: RoleReviewNotificationSource,
    P: RoleReviewNotificationPublisher,
{
    pub fn new(
        source: S,
        publisher: P,
        scope: Scope,
        claimant_ref: impl Into<String>,
        horizon: Duration,
        lease: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        let claimant_ref = claimant_ref.into();
        if claimant_ref.trim().is_empty()
            || claimant_ref.len() > 255
            || claimant_ref.chars().any(char::is_control)
            || horizon <= Duration::zero()
            || horizon > MAX_ROLE_REVIEW_HORIZON
            || lease <= Duration::zero()
            || lease > MAX_ROLE_REVIEW_LEASE
        {
            return Err(WorkerRuntimeError::Configuration(
                "role-review notification configuration is invalid".into(),
            ));
        }
        Ok(Self {
            source,
            publisher,
            scope,
            claimant_ref,
            horizon,
            lease,
        })
    }
}

#[async_trait]
impl<S, P> NamedWorkerHandler for RoleReviewNotificationHandler<S, P>
where
    S: RoleReviewNotificationSource,
    P: RoleReviewNotificationPublisher,
{
    fn name(&self) -> &str {
        ROLE_REVIEW_NOTIFICATION_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        self.run_once_at(worker_id, Utc::now()).await
    }
}

impl<S, P> RoleReviewNotificationHandler<S, P>
where
    S: RoleReviewNotificationSource,
    P: RoleReviewNotificationPublisher,
{
    /// Execute one notification pass at an explicit scheduler time.
    ///
    /// Keeping the time at the claim/validation boundary makes expiry
    /// classification deterministic and prevents a malformed or stale source
    /// row from reaching the durable publisher. The production loop calls
    /// [`Self::run_once`]; acceptance and replay checks can use this method
    /// without sleeping or relying on wall-clock races.
    pub async fn run_once_at(
        &self,
        worker_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "role-review worker identity is required".into(),
            ));
        }
        let claimant_ref = format!("{}:{worker_id}", self.claimant_ref);
        if claimant_ref.len() > 255 {
            return Err(WorkerRuntimeError::Configuration(
                "role-review claimant identity is too long".into(),
            ));
        }
        let claimed = self
            .source
            .claim(&self.scope, &claimant_ref, at, self.horizon, self.lease)
            .await
            .map_err(redacted_role_review_error)?;
        if claimed.is_empty() {
            return Ok(WorkerHandlerReport::default());
        }
        if claimed.len() > MAX_ROLE_REVIEW_NOTIFICATION_CLAIM {
            self.release_claimed_safely(&claimed).await?;
            return Err(WorkerRuntimeError::Configuration(
                "role-review notification source exceeded its claim bound".into(),
            ));
        }
        if let Err(error) = validate_claimed_notifications(&claimed, at) {
            self.release_claimed_safely(&claimed).await?;
            return Err(error);
        }
        let claim_key = claimed[0].claim_key.clone();
        if claimed.iter().any(|item| item.claim_key != claim_key) {
            self.release_claimed_safely(&claimed).await?;
            return Err(WorkerRuntimeError::Configuration(
                "role-review claim identity is inconsistent".into(),
            ));
        }

        let mut delivered = Vec::with_capacity(claimed.len());
        let mut undelivered = Vec::new();
        for (index, notification) in claimed.iter().enumerate() {
            match self.publisher.publish(notification).await {
                Ok(()) => delivered.push(notification.notification_key.clone()),
                Err(_) => {
                    undelivered.extend(
                        claimed[index..]
                            .iter()
                            .map(|item| item.notification_key.clone()),
                    );
                    break;
                }
            }
        }

        let now = Utc::now();
        if !delivered.is_empty() {
            let reconciled = self
                .source
                .reconcile(&claim_key, &delivered, now)
                .await
                .map_err(redacted_role_review_error)?;
            if reconciled != delivered.len() as u64 {
                // A lease may expire between provider delivery and the
                // authoritative acknowledgement. Never report a provider
                // success unless every key was reconciled under this claim.
                // Releasing the still-claimed subset makes the outcome
                // retryable while already-terminal rows remain terminal.
                self.source
                    .release(&claim_key, &delivered)
                    .await
                    .map_err(redacted_role_review_error)?;
                return Err(WorkerRuntimeError::Configuration(
                    "role-review notification acknowledgement mismatch".into(),
                ));
            }
        }
        if !undelivered.is_empty() {
            self.source
                .release(&claim_key, &undelivered)
                .await
                .map_err(redacted_role_review_error)?;
        }
        let processed = delivered.len() as u64;
        Ok(WorkerHandlerReport {
            claimed: claimed.len() as u64,
            processed,
            published: processed,
            retried: undelivered.len() as u64,
            has_work: true,
            ..WorkerHandlerReport::default()
        })
    }

    async fn release_claimed_safely(
        &self,
        claimed: &[ClaimedRoleExpiryNotification],
    ) -> Result<(), WorkerRuntimeError> {
        let Some(claim_key) = claimed.first().map(|item| item.claim_key.as_str()) else {
            return Ok(());
        };
        if !valid_claim_key(claim_key)
            || claimed.iter().any(|item| {
                item.claim_key != claim_key || !valid_notification_key(&item.notification_key)
            })
        {
            // Do not pass untrusted source-controlled identifiers back to the
            // persistence adapter. The lease will expire and be reclaimed by
            // the authoritative source instead.
            return Ok(());
        }
        let notification_keys = claimed
            .iter()
            .map(|item| item.notification_key.clone())
            .collect::<Vec<_>>();
        self.source
            .release(claim_key, &notification_keys)
            .await
            .map_err(redacted_role_review_error)
    }
}

fn redacted_role_review_error(error: RoleReviewPersistenceError) -> WorkerRuntimeError {
    let kind = match error {
        RoleReviewPersistenceError::Database(_) => "database",
        RoleReviewPersistenceError::Serialization(_) => "serialization",
        RoleReviewPersistenceError::InvalidData(_) => "invalid_data",
        RoleReviewPersistenceError::IdempotencyConflict => "idempotency",
        RoleReviewPersistenceError::TooManyItems => "too_many_items",
    };
    tracing::error!(
        error_kind = kind,
        "role-review notification persistence failed"
    );
    WorkerRuntimeError::Configuration("role-review notification persistence failed".into())
}

fn validate_claimed_notifications(
    claimed: &[ClaimedRoleExpiryNotification],
    at: DateTime<Utc>,
) -> Result<(), WorkerRuntimeError> {
    let claim_key = claimed
        .first()
        .map(|item| item.claim_key.as_str())
        .unwrap_or_default();
    if !valid_claim_key(claim_key) {
        return Err(WorkerRuntimeError::Configuration(
            "role-review source returned an invalid claim identity".into(),
        ));
    }
    let mut notification_keys = HashSet::with_capacity(claimed.len());
    for notification in claimed {
        if notification.claim_key != claim_key
            || notification.assignment_id.is_nil()
            || !valid_notification_key(&notification.notification_key)
            || notification.principal_ref.trim().is_empty()
            || notification.principal_ref.len() > 255
            || notification.principal_ref.chars().any(char::is_control)
            || !notification_keys.insert(notification.notification_key.as_str())
            || !expiry_classification_is_current(notification, at)
        {
            return Err(WorkerRuntimeError::Configuration(
                "role-review source returned an invalid notification".into(),
            ));
        }
    }
    Ok(())
}

fn valid_claim_key(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 255 && !value.chars().any(char::is_control)
}

fn valid_notification_key(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 255 && !value.chars().any(char::is_control)
}

fn expiry_classification_is_current(
    notification: &ClaimedRoleExpiryNotification,
    at: DateTime<Utc>,
) -> bool {
    match notification.kind {
        RoleExpiryNotificationKind::ExpiringSoon => notification.valid_until > at,
        RoleExpiryNotificationKind::Expired => notification.valid_until <= at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeSource {
        pending: Arc<Mutex<Vec<ClaimedRoleExpiryNotification>>>,
        reconciled: Arc<Mutex<Vec<String>>>,
        released: Arc<Mutex<Vec<String>>>,
        reconcile_count: Arc<Mutex<Option<u64>>>,
    }

    #[async_trait]
    impl RoleReviewNotificationSource for FakeSource {
        async fn claim(
            &self,
            _scope: &Scope,
            _claimant_ref: &str,
            _at: DateTime<Utc>,
            _horizon: Duration,
            _lease: Duration,
        ) -> Result<Vec<ClaimedRoleExpiryNotification>, RoleReviewPersistenceError> {
            Ok(std::mem::take(
                &mut *self.pending.lock().expect("pending mutex"),
            ))
        }

        async fn reconcile(
            &self,
            _claim_key: &str,
            notification_keys: &[String],
            _at: DateTime<Utc>,
        ) -> Result<u64, RoleReviewPersistenceError> {
            self.reconciled
                .lock()
                .expect("reconciled mutex")
                .extend(notification_keys.iter().cloned());
            Ok(self
                .reconcile_count
                .lock()
                .expect("reconcile count mutex")
                .unwrap_or(notification_keys.len() as u64))
        }

        async fn release(
            &self,
            _claim_key: &str,
            notification_keys: &[String],
        ) -> Result<(), RoleReviewPersistenceError> {
            self.released
                .lock()
                .expect("released mutex")
                .extend(notification_keys.iter().cloned());
            Ok(())
        }
    }

    #[derive(Clone)]
    struct FakePublisher {
        calls: Arc<AtomicUsize>,
        fail_at: Option<usize>,
    }

    #[async_trait]
    impl RoleReviewNotificationPublisher for FakePublisher {
        async fn publish(
            &self,
            _notification: &ClaimedRoleExpiryNotification,
        ) -> Result<(), RoleReviewNotificationPublishError> {
            let call = self.calls.fetch_add(1, Ordering::Relaxed);
            if self.fail_at == Some(call) {
                Err(RoleReviewNotificationPublishError::Unavailable)
            } else {
                Ok(())
            }
        }
    }

    fn notification(index: u128) -> ClaimedRoleExpiryNotification {
        ClaimedRoleExpiryNotification {
            notification_key: format!("notification-{index}"),
            assignment_id: Uuid::from_u128(index + 1),
            principal_ref: format!("principal-{index}"),
            valid_until: Utc::now() + Duration::hours(1),
            kind: erpx_domain::access::RoleExpiryNotificationKind::ExpiringSoon,
            claim_key: "claim-1".into(),
        }
    }

    #[test]
    fn configuration_is_bounded_and_fail_closed() {
        let result = RoleReviewNotificationHandler::new(
            FakeSource::default(),
            FakePublisher {
                calls: Arc::new(AtomicUsize::new(0)),
                fail_at: None,
            },
            Scope::Platform,
            "worker",
            Duration::days(366),
            Duration::minutes(1),
        );
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn nil_worker_identity_is_rejected_before_claiming() {
        let source = FakeSource {
            pending: Arc::new(Mutex::new(vec![notification(1)])),
            ..FakeSource::default()
        };
        let handler = RoleReviewNotificationHandler::new(
            source.clone(),
            FakePublisher {
                calls: Arc::new(AtomicUsize::new(0)),
                fail_at: None,
            },
            Scope::Platform,
            "worker",
            Duration::days(7),
            Duration::minutes(5),
        )
        .expect("handler");
        assert!(handler.run_once(Uuid::nil()).await.is_err());
        assert_eq!(source.pending.lock().expect("pending mutex").len(), 1);
    }

    #[tokio::test]
    async fn successful_delivery_reconciles_claimed_rows() {
        let source = FakeSource {
            pending: Arc::new(Mutex::new(vec![notification(1), notification(2)])),
            ..FakeSource::default()
        };
        let publisher = FakePublisher {
            calls: Arc::new(AtomicUsize::new(0)),
            fail_at: None,
        };
        let handler = RoleReviewNotificationHandler::new(
            source.clone(),
            publisher,
            Scope::Platform,
            "worker",
            Duration::days(7),
            Duration::minutes(5),
        )
        .expect("handler");
        let report = handler.run_once(Uuid::from_u128(10)).await.expect("run");
        assert_eq!(report.claimed, 2);
        assert_eq!(report.processed, 2);
        assert_eq!(report.published, 2);
        assert_eq!(report.retried, 0);
        assert_eq!(source.reconciled.lock().expect("reconciled mutex").len(), 2);
        assert!(source.released.lock().expect("released mutex").is_empty());
    }

    #[tokio::test]
    async fn failed_delivery_releases_unfinished_claims_for_retry() {
        let source = FakeSource {
            pending: Arc::new(Mutex::new(vec![notification(1), notification(2)])),
            ..FakeSource::default()
        };
        let publisher = FakePublisher {
            calls: Arc::new(AtomicUsize::new(0)),
            fail_at: Some(1),
        };
        let handler = RoleReviewNotificationHandler::new(
            source.clone(),
            publisher,
            Scope::Platform,
            "worker",
            Duration::days(7),
            Duration::minutes(5),
        )
        .expect("handler");
        let report = handler.run_once(Uuid::from_u128(11)).await.expect("run");
        assert_eq!(report.claimed, 2);
        assert_eq!(report.processed, 1);
        assert_eq!(report.retried, 1);
        assert_eq!(source.reconciled.lock().expect("reconciled mutex").len(), 1);
        assert_eq!(source.released.lock().expect("released mutex").len(), 1);
    }

    #[tokio::test]
    async fn partial_authoritative_ack_fails_closed_and_releases_for_retry() {
        let source = FakeSource {
            pending: Arc::new(Mutex::new(vec![notification(1), notification(2)])),
            reconcile_count: Arc::new(Mutex::new(Some(1))),
            ..FakeSource::default()
        };
        let handler = RoleReviewNotificationHandler::new(
            source.clone(),
            FakePublisher {
                calls: Arc::new(AtomicUsize::new(0)),
                fail_at: None,
            },
            Scope::Platform,
            "worker",
            Duration::days(7),
            Duration::minutes(5),
        )
        .expect("handler");

        let error = handler
            .run_once(Uuid::from_u128(12))
            .await
            .expect_err("partial acknowledgement must fail closed");
        assert!(matches!(
            error,
            WorkerRuntimeError::Configuration(message)
                if message == "role-review notification acknowledgement mismatch"
        ));
        assert_eq!(source.released.lock().expect("released mutex").len(), 2);
    }

    #[tokio::test]
    async fn stale_or_misclassified_expiry_claims_are_released_before_publishing() {
        let at = Utc::now();
        let mut stale = notification(1);
        stale.valid_until = at - Duration::seconds(1);
        let mut future_expired = notification(2);
        future_expired.kind = RoleExpiryNotificationKind::Expired;
        future_expired.valid_until = at + Duration::seconds(1);
        let source = FakeSource {
            pending: Arc::new(Mutex::new(vec![stale, future_expired])),
            ..FakeSource::default()
        };
        let publisher = FakePublisher {
            calls: Arc::new(AtomicUsize::new(0)),
            fail_at: None,
        };
        let handler = RoleReviewNotificationHandler::new(
            source.clone(),
            publisher.clone(),
            Scope::Platform,
            "worker",
            Duration::days(7),
            Duration::minutes(5),
        )
        .expect("handler");

        let error = handler
            .run_once_at(Uuid::from_u128(13), at)
            .await
            .expect_err("stale expiring-soon notification must fail closed");
        assert!(matches!(error, WorkerRuntimeError::Configuration(_)));
        assert_eq!(publisher.calls.load(Ordering::Relaxed), 0);
        assert_eq!(source.released.lock().expect("released mutex").len(), 2);
    }

    #[tokio::test]
    async fn source_claim_bound_is_released_before_failing_closed() {
        let source = FakeSource {
            pending: Arc::new(Mutex::new((0..=100).map(notification).collect::<Vec<_>>())),
            ..FakeSource::default()
        };
        let handler = RoleReviewNotificationHandler::new(
            source.clone(),
            FakePublisher {
                calls: Arc::new(AtomicUsize::new(0)),
                fail_at: None,
            },
            Scope::Platform,
            "worker",
            Duration::days(7),
            Duration::minutes(5),
        )
        .expect("handler");

        let error = handler
            .run_once_at(Uuid::from_u128(14), Utc::now())
            .await
            .expect_err("oversized source claim must fail closed");
        assert!(matches!(error, WorkerRuntimeError::Configuration(_)));
        assert_eq!(source.released.lock().expect("released mutex").len(), 101);
    }

    #[test]
    fn malformed_claims_are_rejected_before_publishing() {
        let mut malformed = notification(1);
        malformed.assignment_id = Uuid::nil();
        let error =
            validate_claimed_notifications(&[malformed], Utc::now()).expect_err("invalid claim");
        assert!(matches!(error, WorkerRuntimeError::Configuration(_)));

        let duplicate = vec![notification(1), notification(1)];
        let error =
            validate_claimed_notifications(&duplicate, Utc::now()).expect_err("duplicate claim");
        assert!(matches!(error, WorkerRuntimeError::Configuration(_)));
    }
}
