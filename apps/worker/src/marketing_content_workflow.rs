//! Registration-free holding write worker seam for expiring marketing previews.
//!
//! This module deliberately contains orchestration only. It does not select a
//! database, mutate SQL state, register itself with the worker runtime, or
//! perform authorization. The injected executor owns those concerns and must
//! treat `operation_key` as its idempotency boundary.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::BTreeSet;
use thiserror::Error;
use uuid::Uuid;

pub const MARKETING_CONTENT_PREVIEW_EXPIRY_MAX_BATCH_SIZE: u32 = 100;
pub const MARKETING_CONTENT_PREVIEW_EXPIRY_HANDLER_NAME: &str = "marketing_content_preview_expiry";

/// Bounded, redacted worker-level failures. Item failures are deliberately
/// reported in [`MarketingContentPreviewExpiryRunReport`] so one bad preview
/// does not prevent the rest of the holding batch from being attempted.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MarketingContentPreviewExpiryWorkerError {
    #[error("marketing content preview expiry configuration error: {0}")]
    Configuration(String),
    #[error("marketing content preview expiry source failed")]
    Source,
    #[error("marketing content preview expiry source returned more than the requested batch")]
    SourceBatchExceeded,
}

/// A bounded failure classification safe for reports and logs. Provider,
/// database, and credential details must not cross this boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketingContentPreviewExpiryFailureKind {
    InvalidCandidate,
    Retryable,
    Permanent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketingContentPreviewExpiryApplyResult {
    Expired,
    AlreadyExpired,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum MarketingContentPreviewExpiryItemError {
    #[error("preview expiry is retryable")]
    Retryable,
    #[error("preview expiry is permanent")]
    Permanent,
}

/// The only holding route accepted by one invocation. A request cannot carry
/// a holding collection or a cross-holding scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingContentPreviewExpiryRequest {
    pub worker_id: Uuid,
    pub holding_id: String,
    pub data_profile_key: String,
    pub batch_size: u32,
    pub now: DateTime<Utc>,
}

impl MarketingContentPreviewExpiryRequest {
    pub fn new(
        worker_id: Uuid,
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        batch_size: u32,
        now: DateTime<Utc>,
    ) -> Result<Self, MarketingContentPreviewExpiryWorkerError> {
        let request = Self {
            worker_id,
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            batch_size,
            now,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> Result<(), MarketingContentPreviewExpiryWorkerError> {
        if self.worker_id.is_nil() {
            return Err(configuration("worker_id is invalid"));
        }
        validate_scope_reference(&self.holding_id, "holding_id")?;
        validate_scope_reference(&self.data_profile_key, "data_profile_key")?;
        if self.batch_size == 0 || self.batch_size > MARKETING_CONTENT_PREVIEW_EXPIRY_MAX_BATCH_SIZE
        {
            return Err(configuration(&format!(
                "batch_size must be 1..={MARKETING_CONTENT_PREVIEW_EXPIRY_MAX_BATCH_SIZE}"
            )));
        }
        Ok(())
    }

    /// Stable across worker restarts, retry attempts, and clock changes. The
    /// worker id and invocation time are intentionally excluded.
    pub fn operation_key(&self, preview_id: Uuid) -> String {
        format!(
            "erpx:marketing:preview-expiry:{}:{}:{preview_id}",
            self.holding_id, self.data_profile_key
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarketingContentPreviewExpiryCandidate {
    pub preview_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingContentPreviewExpiryItemRequest {
    pub worker_id: Uuid,
    pub holding_id: String,
    pub data_profile_key: String,
    pub preview_id: Uuid,
    pub now: DateTime<Utc>,
    pub operation_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketingContentPreviewExpiryItemResult {
    Expired,
    AlreadyExpired,
    Failed {
        kind: MarketingContentPreviewExpiryFailureKind,
    },
    DuplicateSuppressed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingContentPreviewExpiryItemReport {
    pub preview_id: Uuid,
    pub operation_key: String,
    pub result: MarketingContentPreviewExpiryItemResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingContentPreviewExpiryRunReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub scanned: u32,
    pub attempted: u32,
    pub expired: u32,
    pub already_expired: u32,
    pub failed: u32,
    pub duplicate_suppressed: u32,
    pub items: Vec<MarketingContentPreviewExpiryItemReport>,
}

/// Holding-routed source and mutation boundary. Implementations must apply the
/// supplied operation key atomically with the preview-expiry mutation (or
/// return `AlreadyExpired` when that key was already applied). This is what
/// makes a repeated worker invocation safe without a duplicate retry side
/// effect.
#[async_trait]
pub trait MarketingContentPreviewExpiryExecutor: Send + Sync + 'static {
    async fn list_due_previews(
        &self,
        request: &MarketingContentPreviewExpiryRequest,
    ) -> Result<Vec<MarketingContentPreviewExpiryCandidate>, MarketingContentPreviewExpiryWorkerError>;

    async fn expire_due_preview(
        &self,
        request: &MarketingContentPreviewExpiryItemRequest,
    ) -> Result<MarketingContentPreviewExpiryApplyResult, MarketingContentPreviewExpiryItemError>;
}

#[derive(Debug, Clone)]
pub struct MarketingContentPreviewExpiryHandler<E> {
    executor: E,
}

impl<E> MarketingContentPreviewExpiryHandler<E>
where
    E: MarketingContentPreviewExpiryExecutor,
{
    pub fn new(executor: E) -> Self {
        Self { executor }
    }

    pub async fn run_once(
        &self,
        request: MarketingContentPreviewExpiryRequest,
    ) -> Result<MarketingContentPreviewExpiryRunReport, MarketingContentPreviewExpiryWorkerError>
    {
        request.validate()?;
        let candidates = self
            .executor
            .list_due_previews(&request)
            .await
            .map_err(|_| MarketingContentPreviewExpiryWorkerError::Source)?;
        if candidates.len() > request.batch_size as usize {
            return Err(MarketingContentPreviewExpiryWorkerError::SourceBatchExceeded);
        }

        let mut report = MarketingContentPreviewExpiryRunReport {
            holding_id: request.holding_id.clone(),
            data_profile_key: request.data_profile_key.clone(),
            scanned: candidates.len() as u32,
            attempted: 0,
            expired: 0,
            already_expired: 0,
            failed: 0,
            duplicate_suppressed: 0,
            items: Vec::with_capacity(candidates.len()),
        };
        let mut seen = BTreeSet::new();

        for candidate in candidates {
            let operation_key = request.operation_key(candidate.preview_id);
            if !seen.insert(candidate.preview_id) {
                report.duplicate_suppressed = report.duplicate_suppressed.saturating_add(1);
                report.items.push(MarketingContentPreviewExpiryItemReport {
                    preview_id: candidate.preview_id,
                    operation_key,
                    result: MarketingContentPreviewExpiryItemResult::DuplicateSuppressed,
                });
                continue;
            }
            if candidate.preview_id.is_nil() {
                report.failed = report.failed.saturating_add(1);
                report.items.push(MarketingContentPreviewExpiryItemReport {
                    preview_id: candidate.preview_id,
                    operation_key,
                    result: MarketingContentPreviewExpiryItemResult::Failed {
                        kind: MarketingContentPreviewExpiryFailureKind::InvalidCandidate,
                    },
                });
                continue;
            }

            report.attempted = report.attempted.saturating_add(1);
            let item_request = MarketingContentPreviewExpiryItemRequest {
                worker_id: request.worker_id,
                holding_id: request.holding_id.clone(),
                data_profile_key: request.data_profile_key.clone(),
                preview_id: candidate.preview_id,
                now: request.now,
                operation_key: operation_key.clone(),
            };
            let result = match self.executor.expire_due_preview(&item_request).await {
                Ok(MarketingContentPreviewExpiryApplyResult::Expired) => {
                    report.expired = report.expired.saturating_add(1);
                    MarketingContentPreviewExpiryItemResult::Expired
                }
                Ok(MarketingContentPreviewExpiryApplyResult::AlreadyExpired) => {
                    report.already_expired = report.already_expired.saturating_add(1);
                    MarketingContentPreviewExpiryItemResult::AlreadyExpired
                }
                Err(MarketingContentPreviewExpiryItemError::Retryable) => {
                    report.failed = report.failed.saturating_add(1);
                    MarketingContentPreviewExpiryItemResult::Failed {
                        kind: MarketingContentPreviewExpiryFailureKind::Retryable,
                    }
                }
                Err(MarketingContentPreviewExpiryItemError::Permanent) => {
                    report.failed = report.failed.saturating_add(1);
                    MarketingContentPreviewExpiryItemResult::Failed {
                        kind: MarketingContentPreviewExpiryFailureKind::Permanent,
                    }
                }
            };
            report.items.push(MarketingContentPreviewExpiryItemReport {
                preview_id: candidate.preview_id,
                operation_key,
                result,
            });
        }

        Ok(report)
    }
}

fn configuration(message: &str) -> MarketingContentPreviewExpiryWorkerError {
    MarketingContentPreviewExpiryWorkerError::Configuration(message.into())
}

fn validate_scope_reference(
    value: &str,
    field: &str,
) -> Result<(), MarketingContentPreviewExpiryWorkerError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
        || value.contains(':')
        || value.contains(' ')
    {
        return Err(configuration(&format!("{field} is invalid")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone)]
    struct FakeExecutor {
        candidates: Vec<MarketingContentPreviewExpiryCandidate>,
        failures: BTreeSet<Uuid>,
        applied_keys: Arc<Mutex<BTreeSet<String>>>,
        calls: Arc<Mutex<Vec<MarketingContentPreviewExpiryItemRequest>>>,
    }

    #[async_trait]
    impl MarketingContentPreviewExpiryExecutor for FakeExecutor {
        async fn list_due_previews(
            &self,
            _request: &MarketingContentPreviewExpiryRequest,
        ) -> Result<
            Vec<MarketingContentPreviewExpiryCandidate>,
            MarketingContentPreviewExpiryWorkerError,
        > {
            Ok(self.candidates.clone())
        }

        async fn expire_due_preview(
            &self,
            request: &MarketingContentPreviewExpiryItemRequest,
        ) -> Result<MarketingContentPreviewExpiryApplyResult, MarketingContentPreviewExpiryItemError>
        {
            self.calls.lock().expect("calls lock").push(request.clone());
            if self.failures.contains(&request.preview_id) {
                return Err(MarketingContentPreviewExpiryItemError::Retryable);
            }
            let mut applied = self.applied_keys.lock().expect("applied lock");
            if !applied.insert(request.operation_key.clone()) {
                return Ok(MarketingContentPreviewExpiryApplyResult::AlreadyExpired);
            }
            Ok(MarketingContentPreviewExpiryApplyResult::Expired)
        }
    }

    fn request(batch_size: u32) -> MarketingContentPreviewExpiryRequest {
        MarketingContentPreviewExpiryRequest::new(
            Uuid::new_v4(),
            "holding-a",
            "holding-a",
            batch_size,
            Utc::now(),
        )
        .expect("valid request")
    }

    fn candidate() -> MarketingContentPreviewExpiryCandidate {
        MarketingContentPreviewExpiryCandidate {
            preview_id: Uuid::new_v4(),
        }
    }

    fn fake(candidates: Vec<MarketingContentPreviewExpiryCandidate>) -> FakeExecutor {
        FakeExecutor {
            candidates,
            failures: BTreeSet::new(),
            applied_keys: Arc::new(Mutex::new(BTreeSet::new())),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[test]
    fn rejects_invalid_worker_holding_and_batch_inputs() {
        assert!(
            MarketingContentPreviewExpiryRequest::new(
                Uuid::nil(),
                "holding-a",
                "holding-a",
                1,
                Utc::now()
            )
            .is_err()
        );
        assert!(
            MarketingContentPreviewExpiryRequest::new(
                Uuid::new_v4(),
                "holding-a",
                "holding/a",
                1,
                Utc::now()
            )
            .is_err()
        );
        assert!(
            MarketingContentPreviewExpiryRequest::new(
                Uuid::new_v4(),
                "holding-a",
                "holding-a",
                0,
                Utc::now()
            )
            .is_err()
        );
    }

    #[test]
    fn accepts_exact_batch_bounds_and_rejects_outside_them() {
        assert!(request(1).validate().is_ok());
        assert!(
            request(MARKETING_CONTENT_PREVIEW_EXPIRY_MAX_BATCH_SIZE)
                .validate()
                .is_ok()
        );
        assert!(
            MarketingContentPreviewExpiryRequest::new(
                Uuid::new_v4(),
                "holding-a",
                "holding-a",
                MARKETING_CONTENT_PREVIEW_EXPIRY_MAX_BATCH_SIZE + 1,
                Utc::now()
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn reports_partial_failure_and_continues_the_same_holding_batch() {
        let first = candidate();
        let second = candidate();
        let mut executor = fake(vec![first, second]);
        executor.failures.insert(first.preview_id);
        let handler = MarketingContentPreviewExpiryHandler::new(executor.clone());

        let report = handler.run_once(request(2)).await.expect("run succeeds");

        assert_eq!(report.holding_id, "holding-a");
        assert_eq!(report.data_profile_key, "holding-a");
        assert_eq!(report.scanned, 2);
        assert_eq!(report.attempted, 2);
        assert_eq!(report.expired, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.items.len(), 2);
        assert_eq!(executor.calls.lock().expect("calls lock").len(), 2);
    }

    #[tokio::test]
    async fn retry_reuses_operation_key_without_duplicate_side_effect() {
        let preview = candidate();
        let executor = fake(vec![preview]);
        let handler = MarketingContentPreviewExpiryHandler::new(executor.clone());
        let first_request = request(1);
        let second_request = MarketingContentPreviewExpiryRequest {
            worker_id: Uuid::new_v4(),
            now: first_request.now + chrono::Duration::minutes(5),
            ..first_request.clone()
        };

        let first = handler
            .run_once(first_request.clone())
            .await
            .expect("first run");
        let second = handler.run_once(second_request).await.expect("retry run");

        assert_eq!(first.items[0].operation_key, second.items[0].operation_key);
        assert_eq!(first.expired, 1);
        assert_eq!(second.already_expired, 1);
        assert_eq!(executor.calls.lock().expect("calls lock").len(), 2);
        assert_eq!(executor.applied_keys.lock().expect("applied lock").len(), 1);
    }
}
