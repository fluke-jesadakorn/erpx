//! Durable Marketing channel-delivery worker boundary.
//!
//! The handler owns claim/lease and provider-result persistence orchestration,
//! but it does not select holdings, construct credentials, or implement a
//! channel provider. Those dependencies must be explicitly composed by the
//! runtime through the traits below.

use std::collections::HashSet;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_domain::marketing_delivery::{
    AttemptAuthorization, DeliveryDispatch, DeliveryIdempotencyKey, DeliveryState,
    MAX_DELIVERY_TEXT, ProviderResult,
};
use erpx_infrastructure::{
    MarketingDeliveryClaim, MarketingDeliveryWriteReceipt, MarketingProviderAccountBinding,
    PersistenceError, SqlxMarketingDeliveryStore,
};
use thiserror::Error;
use uuid::Uuid;

use super::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const MARKETING_DELIVERY_HANDLER_NAME: &str = "marketing_delivery";
pub const MARKETING_DELIVERY_MAX_BATCH_SIZE: u32 = 100;
pub const MARKETING_DELIVERY_MIN_LEASE_SECONDS: u64 = 1;
pub const MARKETING_DELIVERY_MAX_LEASE_SECONDS: u64 = 3_600;
const MARKETING_DELIVERY_INVALID_PROVIDER_REFERENCE_REASON: &str = "provider_reference_unusable";
const MARKETING_DELIVERY_DUPLICATE_PROVIDER_REFERENCE_REASON: &str = "provider_reference_duplicate";

/// The provider boundary may return only a bounded classification. Provider
/// credentials, response bodies, bearer tokens, and secret material must not
/// cross into this error type or be persisted by the worker.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum MarketingDeliveryProviderError {
    #[error("provider adapter is not configured")]
    Configuration,
    #[error("provider is temporarily unavailable")]
    Retryable,
    #[error("provider rejected the dispatch permanently")]
    Permanent,
}

/// Holding-scoped durable delivery source. Implementations must perform claim
/// and provider-result writes atomically with their own holding scope.
#[async_trait]
pub trait MarketingDeliverySource: Send + Sync + 'static {
    async fn claim_due(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        limit: u32,
        lease: Duration,
    ) -> Result<Vec<MarketingDeliveryClaim>, PersistenceError>;

    async fn record_provider_result(
        &self,
        worker_id: Uuid,
        delivery_id: Uuid,
        claim_id: Uuid,
        authorization: &AttemptAuthorization,
        result: ProviderResult,
    ) -> Result<MarketingDeliveryWriteReceipt, PersistenceError>;

    async fn renew_claim(
        &self,
        worker_id: Uuid,
        delivery_id: Uuid,
        claim_id: Uuid,
        lease: Duration,
    ) -> Result<DateTime<Utc>, PersistenceError>;
}

#[async_trait]
impl MarketingDeliverySource for SqlxMarketingDeliveryStore {
    async fn claim_due(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        limit: u32,
        lease: Duration,
    ) -> Result<Vec<MarketingDeliveryClaim>, PersistenceError> {
        SqlxMarketingDeliveryStore::claim_due(self, worker_id, now, limit, lease).await
    }

    async fn record_provider_result(
        &self,
        worker_id: Uuid,
        delivery_id: Uuid,
        claim_id: Uuid,
        authorization: &AttemptAuthorization,
        result: ProviderResult,
    ) -> Result<MarketingDeliveryWriteReceipt, PersistenceError> {
        SqlxMarketingDeliveryStore::record_provider_result(
            self,
            worker_id,
            delivery_id,
            claim_id,
            authorization,
            result,
        )
        .await
    }

    async fn renew_claim(
        &self,
        worker_id: Uuid,
        delivery_id: Uuid,
        claim_id: Uuid,
        lease: Duration,
    ) -> Result<DateTime<Utc>, PersistenceError> {
        SqlxMarketingDeliveryStore::renew_claim(self, worker_id, delivery_id, claim_id, lease).await
    }
}

/// Provider adapter boundary. A successful call must return an explicit
/// provider result; acceptance is not treated as delivery by this handler.
#[async_trait]
pub trait MarketingDeliveryProvider: Send + Sync + 'static {
    async fn submit(
        &self,
        dispatch: &DeliveryDispatch,
        authorization: &AttemptAuthorization,
    ) -> Result<ProviderResult, MarketingDeliveryProviderError>;

    /// Submit only after the caller has selected and revalidated the exact
    /// control-plane provider account. The default deliberately fails closed:
    /// an account-bound wrapper must never silently call an unbound provider
    /// implementation that could route across holdings or holdings.
    async fn submit_with_account_binding(
        &self,
        _binding: &MarketingProviderAccountBinding,
        _dispatch: &DeliveryDispatch,
        _authorization: &AttemptAuthorization,
    ) -> Result<ProviderResult, MarketingDeliveryProviderError> {
        Err(MarketingDeliveryProviderError::Configuration)
    }

    /// Resolve an ambiguous submit outcome using the provider's idempotency
    /// state. Implementations must not submit the message again here. The
    /// default is deliberately fail-closed: without provider reconciliation,
    /// the worker records a bounded retryable outcome and the next attempt
    /// retains the same idempotency key.
    async fn reconcile(
        &self,
        _dispatch: &DeliveryDispatch,
        _authorization: &AttemptAuthorization,
    ) -> Result<Option<ProviderResult>, MarketingDeliveryProviderError> {
        Ok(None)
    }

    /// Resolve an ambiguous account-bound submit without issuing a second
    /// provider submission. Implementations that support account-bound
    /// reconciliation must revalidate the same binding before querying the
    /// provider's idempotency state.
    async fn reconcile_with_account_binding(
        &self,
        _binding: &MarketingProviderAccountBinding,
        dispatch: &DeliveryDispatch,
        authorization: &AttemptAuthorization,
    ) -> Result<Option<ProviderResult>, MarketingDeliveryProviderError> {
        self.reconcile(dispatch, authorization).await
    }
}

#[derive(Debug, Clone)]
pub struct MarketingDeliveryHandler<S, P> {
    source: S,
    provider: P,
    batch_size: u32,
    lease: Duration,
}

impl<S, P> MarketingDeliveryHandler<S, P>
where
    S: MarketingDeliverySource,
    P: MarketingDeliveryProvider,
{
    pub fn new(
        source: S,
        provider: P,
        batch_size: u32,
        lease: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        if batch_size == 0 || batch_size > MARKETING_DELIVERY_MAX_BATCH_SIZE {
            return Err(WorkerRuntimeError::Configuration(format!(
                "Marketing delivery batch size must be 1..={MARKETING_DELIVERY_MAX_BATCH_SIZE}"
            )));
        }
        if lease < Duration::seconds(MARKETING_DELIVERY_MIN_LEASE_SECONDS as i64)
            || lease > Duration::seconds(MARKETING_DELIVERY_MAX_LEASE_SECONDS as i64)
        {
            return Err(WorkerRuntimeError::Configuration(
                "Marketing delivery lease is outside its bound".into(),
            ));
        }
        Ok(Self {
            source,
            provider,
            batch_size,
            lease,
        })
    }

    pub const fn batch_size(&self) -> u32 {
        self.batch_size
    }

    pub const fn lease(&self) -> Duration {
        self.lease
    }
}

#[async_trait]
impl<S, P> NamedWorkerHandler for MarketingDeliveryHandler<S, P>
where
    S: MarketingDeliverySource,
    P: MarketingDeliveryProvider,
{
    fn name(&self) -> &str {
        MARKETING_DELIVERY_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "Marketing delivery worker identity is required".into(),
            ));
        }
        let now = Utc::now();
        let claims = self
            .source
            .claim_due(worker_id, now, self.batch_size, self.lease)
            .await
            .map_err(redacted_delivery_persistence_error)?;
        let mut report = WorkerHandlerReport {
            claimed: claims.len() as u64,
            has_work: !claims.is_empty(),
            ..WorkerHandlerReport::default()
        };
        let mut accepted_provider_refs = HashSet::new();
        for claim in claims {
            validate_claim(&claim, worker_id, now)?;
            let provider_budget = std::time::Duration::from_secs(
                u64::try_from((self.lease.num_seconds() / 2).max(1)).unwrap_or(1),
            );
            let result = match tokio::time::timeout(
                provider_budget,
                self.provider.submit(&claim.dispatch, &claim.authorization),
            )
            .await
            {
                Err(_) => match self
                    .provider
                    .reconcile(&claim.dispatch, &claim.authorization)
                    .await
                {
                    Ok(Some(result)) => result,
                    Ok(None) | Err(_) => ProviderResult::RetryableFailure {
                        reason: "provider_ambiguous_or_retryable".into(),
                    },
                },
                Ok(Ok(result)) => result,
                Ok(Err(MarketingDeliveryProviderError::Configuration)) => {
                    ProviderResult::RetryableFailure {
                        reason: "provider_configuration_unavailable".into(),
                    }
                }
                Ok(Err(MarketingDeliveryProviderError::Retryable)) => match self
                    .provider
                    .reconcile(&claim.dispatch, &claim.authorization)
                    .await
                {
                    Ok(Some(result)) => result,
                    Ok(None) | Err(_) => ProviderResult::RetryableFailure {
                        reason: "provider_ambiguous_or_retryable".into(),
                    },
                },
                Ok(Err(MarketingDeliveryProviderError::Permanent)) => {
                    ProviderResult::PermanentFailure {
                        reason: "provider_permanent_failure".into(),
                    }
                }
            };
            // A provider reference is the later callback's binding key. Do
            // not persist an accepted result that cannot be bound to one
            // callback-safe reference; retaining the deterministic delivery
            // key as retryable work allows reconciliation to resolve it
            // without treating an unbindable response as accepted.
            let result = guard_provider_result(result);
            let result = guard_duplicate_provider_reference(result, &mut accepted_provider_refs);
            self.source
                .renew_claim(worker_id, claim.dispatch.id, claim.claim_id, self.lease)
                .await
                .map_err(redacted_delivery_persistence_error)?;
            let was_terminal = matches!(result, ProviderResult::PermanentFailure { .. })
                || matches!(result, ProviderResult::RetryableFailure { .. })
                    && claim.dispatch.attempt_count >= claim.dispatch.retry_policy.max_attempts;
            let was_retry =
                matches!(result, ProviderResult::RetryableFailure { .. }) && !was_terminal;
            self.source
                .record_provider_result(
                    worker_id,
                    claim.dispatch.id,
                    claim.claim_id,
                    &claim.authorization,
                    result.clone(),
                )
                .await
                .map_err(redacted_delivery_persistence_error)?;
            report.processed = report.processed.saturating_add(1);
            match result {
                ProviderResult::Accepted { .. } => {
                    report.published = report.published.saturating_add(1);
                }
                ProviderResult::RetryableFailure { .. } if was_retry => {
                    report.retried = report.retried.saturating_add(1);
                }
                ProviderResult::RetryableFailure { .. }
                | ProviderResult::PermanentFailure { .. } => {
                    report.dead_lettered = report.dead_lettered.saturating_add(1);
                }
            }
        }
        Ok(report)
    }
}

fn validate_claim(
    claim: &MarketingDeliveryClaim,
    worker_id: Uuid,
    at: DateTime<Utc>,
) -> Result<(), WorkerRuntimeError> {
    if claim.claim_id.is_nil()
        || claim.worker_id.is_nil()
        || claim.worker_id != worker_id
        || claim.claim_until <= at
    {
        return Err(WorkerRuntimeError::Configuration(
            "Marketing delivery claim lease is invalid or no longer owned".into(),
        ));
    }

    let dispatch = &claim.dispatch;
    if dispatch.id.is_nil()
        || dispatch.version == 0
        || dispatch.state != DeliveryState::Sending
        || dispatch.attempt_count == 0
        || claim.authorization.dispatch_id != dispatch.id
        || claim.authorization.attempt != dispatch.attempt_count
    {
        return Err(WorkerRuntimeError::Configuration(
            "Marketing delivery claim does not match its dispatch".into(),
        ));
    }

    dispatch.record.validate().map_err(|_| {
        WorkerRuntimeError::Configuration("Marketing delivery record is invalid".into())
    })?;
    dispatch.retry_policy.validate().map_err(|_| {
        WorkerRuntimeError::Configuration("Marketing delivery retry policy is invalid".into())
    })?;
    let expected_key = DeliveryIdempotencyKey::derive(
        &dispatch.scope,
        dispatch.profile_id,
        dispatch.campaign_id,
        dispatch.purpose,
        dispatch.channel,
        &dispatch.logical_message_ref,
        dispatch.template_revision,
    )
    .map_err(|_| WorkerRuntimeError::Configuration("Marketing delivery scope is invalid".into()))?;
    if claim.authorization.idempotency_key != dispatch.idempotency_key
        || dispatch.idempotency_key != expected_key
    {
        return Err(WorkerRuntimeError::Configuration(
            "Marketing delivery idempotency binding is invalid".into(),
        ));
    }
    Ok(())
}

fn guard_provider_result(result: ProviderResult) -> ProviderResult {
    match result {
        ProviderResult::Accepted {
            provider_message_ref,
        } if provider_message_ref.trim().is_empty()
            || provider_message_ref.len() > MAX_DELIVERY_TEXT
            || provider_message_ref
                .chars()
                .any(|character| character.is_control() || character.is_whitespace()) =>
        {
            ProviderResult::RetryableFailure {
                reason: MARKETING_DELIVERY_INVALID_PROVIDER_REFERENCE_REASON.into(),
            }
        }
        result => result,
    }
}

fn guard_duplicate_provider_reference(
    result: ProviderResult,
    accepted_provider_refs: &mut HashSet<String>,
) -> ProviderResult {
    let ProviderResult::Accepted {
        provider_message_ref,
    } = &result
    else {
        return result;
    };
    if accepted_provider_refs.insert(provider_message_ref.clone()) {
        result
    } else {
        ProviderResult::RetryableFailure {
            reason: MARKETING_DELIVERY_DUPLICATE_PROVIDER_REFERENCE_REASON.into(),
        }
    }
}

fn redacted_delivery_persistence_error(error: PersistenceError) -> WorkerRuntimeError {
    let kind = match error {
        PersistenceError::Database(_) => "database",
        PersistenceError::Migration(_) => "migration",
        PersistenceError::Serialization(_) => "serialization",
        PersistenceError::OptimisticConcurrency { .. } => "concurrency",
        PersistenceError::IdempotencyConflict { .. } => "idempotency",
        PersistenceError::NotFound { .. } => "not_found",
        PersistenceError::ClaimLost { .. } => "claim_lost",
        PersistenceError::InvalidData(_) => "invalid_data",
        PersistenceError::Configuration(_) => "configuration",
    };
    tracing::error!(error_kind = kind, "Marketing delivery persistence failed");
    WorkerRuntimeError::Configuration("Marketing delivery worker failed".into())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use chrono::Duration;
    use erpx_domain::{
        contracts::AuthorityScope,
        marketing::{MarketingChannel, MarketingPurpose},
        marketing_delivery::{
            ConsentRecheckDecision, DeliveryRecheckDecision, DeliveryRecheckResult,
            SuppressionRecheckDecision,
        },
    };

    use super::*;

    #[derive(Clone)]
    struct FakeSource {
        state: Arc<Mutex<FakeSourceState>>,
    }

    struct FakeSourceState {
        claims: Vec<MarketingDeliveryClaim>,
        results: Vec<ProviderResult>,
        renewals: u32,
    }

    #[async_trait]
    impl MarketingDeliverySource for FakeSource {
        async fn claim_due(
            &self,
            _worker_id: Uuid,
            _now: DateTime<Utc>,
            _limit: u32,
            _lease: Duration,
        ) -> Result<Vec<MarketingDeliveryClaim>, PersistenceError> {
            Ok(std::mem::take(
                &mut self.state.lock().expect("source").claims,
            ))
        }

        async fn record_provider_result(
            &self,
            _worker_id: Uuid,
            _delivery_id: Uuid,
            _claim_id: Uuid,
            _authorization: &AttemptAuthorization,
            result: ProviderResult,
        ) -> Result<MarketingDeliveryWriteReceipt, PersistenceError> {
            self.state.lock().expect("source").results.push(result);
            Ok(MarketingDeliveryWriteReceipt {
                delivery_id: Uuid::new_v4(),
                version: 2,
                replayed: false,
            })
        }

        async fn renew_claim(
            &self,
            _worker_id: Uuid,
            _delivery_id: Uuid,
            _claim_id: Uuid,
            lease: Duration,
        ) -> Result<DateTime<Utc>, PersistenceError> {
            let mut state = self.state.lock().expect("source");
            state.renewals = state.renewals.saturating_add(1);
            Ok(Utc::now() + lease)
        }
    }

    #[derive(Clone)]
    struct FakeProvider {
        result: ProviderResult,
    }

    fn test_worker_id() -> Uuid {
        Uuid::from_u128(99)
    }

    #[async_trait]
    impl MarketingDeliveryProvider for FakeProvider {
        async fn submit(
            &self,
            _dispatch: &DeliveryDispatch,
            _authorization: &AttemptAuthorization,
        ) -> Result<ProviderResult, MarketingDeliveryProviderError> {
            Ok(self.result.clone())
        }
    }

    #[derive(Clone)]
    struct ReconcilingProvider {
        calls: Arc<Mutex<(u32, u32)>>,
        reconciled_result: ProviderResult,
    }

    #[async_trait]
    impl MarketingDeliveryProvider for ReconcilingProvider {
        async fn submit(
            &self,
            _dispatch: &DeliveryDispatch,
            _authorization: &AttemptAuthorization,
        ) -> Result<ProviderResult, MarketingDeliveryProviderError> {
            self.calls.lock().expect("calls").0 += 1;
            Err(MarketingDeliveryProviderError::Retryable)
        }

        async fn reconcile(
            &self,
            _dispatch: &DeliveryDispatch,
            _authorization: &AttemptAuthorization,
        ) -> Result<Option<ProviderResult>, MarketingDeliveryProviderError> {
            self.calls.lock().expect("calls").1 += 1;
            Ok(Some(self.reconciled_result.clone()))
        }
    }

    #[derive(Clone)]
    struct TimeoutProvider {
        calls: Arc<Mutex<(u32, u32)>>,
    }

    #[async_trait]
    impl MarketingDeliveryProvider for TimeoutProvider {
        async fn submit(
            &self,
            _dispatch: &DeliveryDispatch,
            _authorization: &AttemptAuthorization,
        ) -> Result<ProviderResult, MarketingDeliveryProviderError> {
            self.calls.lock().expect("calls").0 += 1;
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            Ok(ProviderResult::Accepted {
                provider_message_ref: "late-provider-message".into(),
            })
        }

        async fn reconcile(
            &self,
            _dispatch: &DeliveryDispatch,
            _authorization: &AttemptAuthorization,
        ) -> Result<Option<ProviderResult>, MarketingDeliveryProviderError> {
            self.calls.lock().expect("calls").1 += 1;
            Ok(None)
        }
    }

    fn claim(result: ProviderResult) -> (FakeSource, FakeProvider) {
        let scope = AuthorityScope::Holding {
            holding_id: Uuid::new_v4(),
        };
        let mut dispatch = DeliveryDispatch::new(
            Uuid::new_v4(),
            scope,
            Uuid::new_v4(),
            Uuid::new_v4(),
            MarketingPurpose::Promotional,
            MarketingChannel::Email,
            erpx_domain::marketing_delivery::ChannelDispatchRecord::Email {
                recipient: "opaque-recipient".into(),
                template_ref: "template-v1".into(),
            },
            "logical-message",
            1,
            Utc::now() - Duration::seconds(1),
            Utc::now() + Duration::hours(1),
            erpx_domain::marketing_delivery::RetryPolicy::default(),
        )
        .expect("dispatch");
        let now = Utc::now();
        let authorization = dispatch
            .begin_attempt(
                &DeliveryRecheckDecision {
                    checked_at: now,
                    consent: ConsentRecheckDecision::Granted,
                    suppression: SuppressionRecheckDecision::Clear,
                    result: DeliveryRecheckResult::Allowed,
                },
                now,
            )
            .expect("attempt");
        (
            FakeSource {
                state: Arc::new(Mutex::new(FakeSourceState {
                    claims: vec![MarketingDeliveryClaim {
                        dispatch,
                        authorization,
                        claim_id: Uuid::new_v4(),
                        worker_id: test_worker_id(),
                        claim_until: Utc::now() + Duration::minutes(1),
                    }],
                    results: Vec::new(),
                    renewals: 0,
                })),
            },
            FakeProvider { result },
        )
    }

    #[test]
    fn handler_bounds_batch_and_lease() {
        let (source, provider) = claim(ProviderResult::Accepted {
            provider_message_ref: "provider-message".into(),
        });
        assert!(
            MarketingDeliveryHandler::new(
                source.clone(),
                provider.clone(),
                0,
                Duration::seconds(1)
            )
            .is_err()
        );
        assert!(MarketingDeliveryHandler::new(source, provider, 1, Duration::zero()).is_err());
    }

    #[tokio::test]
    async fn stale_or_wrong_owner_claims_fail_before_provider_submission() {
        let (source, provider) = claim(ProviderResult::Accepted {
            provider_message_ref: "provider-message".into(),
        });
        let state = Arc::clone(&source.state);
        state.lock().expect("source").claims[0].claim_until = Utc::now() - Duration::seconds(1);
        let handler = MarketingDeliveryHandler::new(
            source.clone(),
            provider.clone(),
            1,
            Duration::seconds(30),
        )
        .expect("handler");
        assert!(handler.run_once(test_worker_id()).await.is_err());
        assert!(state.lock().expect("state").results.is_empty());
        assert_eq!(state.lock().expect("state").renewals, 0);

        let (source, provider) = claim(ProviderResult::Accepted {
            provider_message_ref: "provider-message".into(),
        });
        let state = Arc::clone(&source.state);
        state.lock().expect("source").claims[0].worker_id = Uuid::from_u128(100);
        let handler = MarketingDeliveryHandler::new(source, provider, 1, Duration::seconds(30))
            .expect("handler");
        assert!(handler.run_once(test_worker_id()).await.is_err());
        assert!(state.lock().expect("state").results.is_empty());
        assert_eq!(state.lock().expect("state").renewals, 0);
    }

    #[tokio::test]
    async fn nil_worker_identity_fails_closed_before_claiming() {
        let (source, provider) = claim(ProviderResult::Accepted {
            provider_message_ref: "provider-message".into(),
        });
        let state = Arc::clone(&source.state);
        let handler = MarketingDeliveryHandler::new(source, provider, 1, Duration::seconds(30))
            .expect("handler");
        assert!(handler.run_once(Uuid::nil()).await.is_err());
        assert_eq!(state.lock().expect("state").claims.len(), 1);
    }

    #[tokio::test]
    async fn accepted_result_is_published_but_not_marked_delivered() {
        let (source, provider) = claim(ProviderResult::Accepted {
            provider_message_ref: "provider-message".into(),
        });
        let state = Arc::clone(&source.state);
        let handler = MarketingDeliveryHandler::new(source, provider, 1, Duration::seconds(30))
            .expect("handler");
        let report = handler.run_once(test_worker_id()).await.expect("run");
        assert_eq!(report.claimed, 1);
        assert_eq!(report.processed, 1);
        assert_eq!(report.published, 1);
        assert_eq!(state.lock().expect("state").renewals, 1);
        assert_eq!(state.lock().expect("state").results.len(), 1);
        assert!(matches!(
            state.lock().expect("state").results[0],
            ProviderResult::Accepted { .. }
        ));
    }

    #[tokio::test]
    async fn duplicate_provider_references_are_retryable_within_one_claim_batch() {
        let duplicate = ProviderResult::Accepted {
            provider_message_ref: "same-provider-message".into(),
        };
        let (source, provider) = claim(duplicate.clone());
        let (second_source, _) = claim(duplicate);
        let second_claim = second_source.state.lock().expect("second source").claims[0].clone();
        source
            .state
            .lock()
            .expect("source")
            .claims
            .push(second_claim);
        let state = Arc::clone(&source.state);
        let handler = MarketingDeliveryHandler::new(source, provider, 2, Duration::seconds(30))
            .expect("handler");

        let report = handler.run_once(test_worker_id()).await.expect("run");

        assert_eq!(report.claimed, 2);
        assert_eq!(report.processed, 2);
        assert_eq!(report.published, 1);
        assert_eq!(report.retried, 1);
        assert!(matches!(
            state.lock().expect("source").results.as_slice(),
            [ProviderResult::Accepted { .. }, ProviderResult::RetryableFailure { reason }]
                if reason == MARKETING_DELIVERY_DUPLICATE_PROVIDER_REFERENCE_REASON
        ));
    }

    #[tokio::test]
    async fn provider_timeout_reconciles_once_without_a_duplicate_submission() {
        let (source, _) = claim(ProviderResult::Accepted {
            provider_message_ref: "unused".into(),
        });
        let state = Arc::clone(&source.state);
        let calls = Arc::new(Mutex::new((0, 0)));
        let provider = TimeoutProvider {
            calls: Arc::clone(&calls),
        };
        let handler = MarketingDeliveryHandler::new(source, provider, 1, Duration::seconds(1))
            .expect("handler");

        let report = handler.run_once(test_worker_id()).await.expect("run");

        assert_eq!(report.claimed, 1);
        assert_eq!(report.processed, 1);
        assert_eq!(report.published, 0);
        assert_eq!(report.retried, 1);
        assert_eq!(*calls.lock().expect("calls"), (1, 1));
        assert!(matches!(
            state.lock().expect("source").results.as_slice(),
            [ProviderResult::RetryableFailure { reason }]
                if reason == "provider_ambiguous_or_retryable"
        ));
    }

    #[test]
    fn retry_authorization_reuses_the_same_idempotency_key() {
        let (source, _) = claim(ProviderResult::Accepted {
            provider_message_ref: "unused".into(),
        });
        let claimed = source.state.lock().expect("source").claims[0].clone();
        let mut dispatch = claimed.dispatch;
        let first_authorization = claimed.authorization;
        let idempotency_key = first_authorization.idempotency_key.clone();
        let now = Utc::now();

        dispatch
            .record_provider_result(
                first_authorization.attempt,
                now,
                ProviderResult::RetryableFailure {
                    reason: "provider_timeout".into(),
                },
            )
            .expect("schedule retry");
        let second_authorization = dispatch
            .begin_attempt(
                &DeliveryRecheckDecision {
                    checked_at: now + Duration::minutes(1),
                    consent: ConsentRecheckDecision::Granted,
                    suppression: SuppressionRecheckDecision::Clear,
                    result: DeliveryRecheckResult::Allowed,
                },
                now + Duration::minutes(1),
            )
            .expect("authorize retry");

        assert_eq!(
            first_authorization.dispatch_id,
            second_authorization.dispatch_id
        );
        assert_eq!(second_authorization.attempt, 2);
        assert_eq!(second_authorization.idempotency_key, idempotency_key);
        assert_eq!(
            second_authorization.idempotency_key,
            dispatch.idempotency_key
        );
    }

    #[tokio::test]
    async fn invalid_platform_scope_fails_before_provider_submission() {
        let (source, provider) = claim(ProviderResult::Accepted {
            provider_message_ref: "provider-message".into(),
        });
        let state = Arc::clone(&source.state);
        state.lock().expect("source").claims[0].dispatch.scope = AuthorityScope::Platform;
        let handler = MarketingDeliveryHandler::new(source, provider, 1, Duration::seconds(30))
            .expect("handler");

        assert!(handler.run_once(test_worker_id()).await.is_err());
        assert!(state.lock().expect("source").results.is_empty());
        assert_eq!(state.lock().expect("source").renewals, 0);
    }

    #[tokio::test]
    async fn retryable_and_permanent_results_are_classified() {
        for (result, retried, dead_lettered) in [
            (
                ProviderResult::RetryableFailure {
                    reason: "temporary".into(),
                },
                1,
                0,
            ),
            (
                ProviderResult::PermanentFailure {
                    reason: "permanent".into(),
                },
                0,
                1,
            ),
        ] {
            let (source, provider) = claim(result);
            let handler = MarketingDeliveryHandler::new(source, provider, 1, Duration::seconds(30))
                .expect("handler");
            let report = handler.run_once(test_worker_id()).await.expect("run");
            assert_eq!(report.retried, retried);
            assert_eq!(report.dead_lettered, dead_lettered);
        }
    }

    #[tokio::test]
    async fn ambiguous_submit_is_reconciled_without_a_duplicate_submit() {
        let (source, _) = claim(ProviderResult::Accepted {
            provider_message_ref: "unused".into(),
        });
        let state = Arc::clone(&source.state);
        let calls = Arc::new(Mutex::new((0, 0)));
        let provider = ReconcilingProvider {
            calls: Arc::clone(&calls),
            reconciled_result: ProviderResult::Accepted {
                provider_message_ref: "reconciled-provider-message".into(),
            },
        };
        let handler = MarketingDeliveryHandler::new(source, provider, 1, Duration::seconds(30))
            .expect("handler");

        let report = handler.run_once(test_worker_id()).await.expect("run");

        assert_eq!(report.claimed, 1);
        assert_eq!(report.processed, 1);
        assert_eq!(report.published, 1);
        assert_eq!(*calls.lock().expect("calls"), (1, 1));
        assert!(matches!(
            state.lock().expect("state").results.as_slice(),
            [ProviderResult::Accepted {
                provider_message_ref
            }] if provider_message_ref == "reconciled-provider-message"
        ));
    }

    #[tokio::test]
    async fn reconciliation_with_unbindable_provider_reference_stays_retryable() {
        let (source, _) = claim(ProviderResult::Accepted {
            provider_message_ref: "unused".into(),
        });
        let state = Arc::clone(&source.state);
        let provider = ReconcilingProvider {
            calls: Arc::new(Mutex::new((0, 0))),
            reconciled_result: ProviderResult::Accepted {
                provider_message_ref: "provider\nmessage".into(),
            },
        };
        let handler = MarketingDeliveryHandler::new(source, provider, 1, Duration::seconds(30))
            .expect("handler");

        let report = handler.run_once(test_worker_id()).await.expect("run");

        assert_eq!(report.processed, 1);
        assert_eq!(report.published, 0);
        assert_eq!(report.retried, 1);
        assert!(matches!(
            state.lock().expect("state").results.as_slice(),
            [ProviderResult::RetryableFailure { reason }]
                if reason == MARKETING_DELIVERY_INVALID_PROVIDER_REFERENCE_REASON
        ));
    }
}
