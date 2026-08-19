//! Resume-safe worker seam for Marketing Journey execution.
//!
//! The worker claims due cursors, asks an injected decision source for a
//! bounded node outcome, and persists the resulting cursor fact. CRM handoff,
//! channel dispatch, segment/tag/conversion/notification effects, and approved
//! integration enqueueing are committed atomically with the cursor by the SQLx
//! source. `NotifyOwner` becomes an internal Webhook delivery aggregate at
//! that boundary; the generic delivery worker then owns its lease, retry,
//! provider binding, and read-model projection. Outbound provider delivery
//! remains a separate claimed worker side effect and fails closed when its
//! authoritative binding is unavailable.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_domain::marketing_journey::{
    JourneyAction, JourneyBranchPredicate, JourneyNodeDefinition, JourneyNodeOutcome, JourneyStatus,
};
use erpx_infrastructure::{
    MarketingJourneyExecutionClaim, MarketingJourneyExecutionFailure,
    MarketingJourneyExecutionWriteReceipt, PersistenceError, SqlxMarketingJourneyExecutionStore,
};
use thiserror::Error;
use uuid::Uuid;

use super::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const MARKETING_JOURNEY_EXECUTION_HANDLER_NAME: &str = "marketing_journey_execution";
pub const MARKETING_JOURNEY_EXECUTION_MAX_BATCH_SIZE: u32 = 100;
pub const MARKETING_JOURNEY_EXECUTION_MIN_LEASE_SECONDS: u64 = 1;
pub const MARKETING_JOURNEY_EXECUTION_MAX_LEASE_SECONDS: u64 = 3_600;
pub const MARKETING_JOURNEY_EXECUTION_RETRY_DELAY_SECONDS: i64 = 30;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum MarketingJourneyExecutionDecisionError {
    #[error("execution decision is temporarily unavailable")]
    Retryable,
    #[error("execution decision is permanently invalid")]
    Permanent,
}

#[async_trait]
pub trait MarketingJourneyExecutionSource: Send + Sync + 'static {
    async fn claim_due(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        limit: u32,
        lease: Duration,
    ) -> Result<Vec<MarketingJourneyExecutionClaim>, PersistenceError>;

    async fn apply_claim(
        &self,
        claim: &MarketingJourneyExecutionClaim,
        worker_id: Uuid,
        outcome: JourneyNodeOutcome,
        at: DateTime<Utc>,
    ) -> Result<MarketingJourneyExecutionWriteReceipt, PersistenceError>;

    async fn release_claim(
        &self,
        claim: &MarketingJourneyExecutionClaim,
        worker_id: Uuid,
        at: DateTime<Utc>,
        failure: MarketingJourneyExecutionFailure,
    ) -> Result<(), PersistenceError>;
}

#[async_trait]
impl MarketingJourneyExecutionSource for SqlxMarketingJourneyExecutionStore {
    async fn claim_due(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        limit: u32,
        lease: Duration,
    ) -> Result<Vec<MarketingJourneyExecutionClaim>, PersistenceError> {
        SqlxMarketingJourneyExecutionStore::claim_due(self, worker_id, now, limit, lease).await
    }

    async fn apply_claim(
        &self,
        claim: &MarketingJourneyExecutionClaim,
        worker_id: Uuid,
        outcome: JourneyNodeOutcome,
        at: DateTime<Utc>,
    ) -> Result<MarketingJourneyExecutionWriteReceipt, PersistenceError> {
        SqlxMarketingJourneyExecutionStore::apply_claim(self, claim, worker_id, outcome, at).await
    }

    async fn release_claim(
        &self,
        claim: &MarketingJourneyExecutionClaim,
        worker_id: Uuid,
        at: DateTime<Utc>,
        failure: MarketingJourneyExecutionFailure,
    ) -> Result<(), PersistenceError> {
        SqlxMarketingJourneyExecutionStore::release_claim(self, claim, worker_id, at, failure).await
    }
}

/// Supplies only a bounded domain outcome. Implementations must not perform
/// channel delivery or integration side effects as part of this decision.
#[async_trait]
pub trait MarketingJourneyExecutionDecisionSource: Send + Sync + 'static {
    async fn decide(
        &self,
        claim: &MarketingJourneyExecutionClaim,
    ) -> Result<JourneyNodeOutcome, MarketingJourneyExecutionDecisionError>;
}

/// Production decision source for holding-routed journey execution. It reads
/// branch attributes and channel contact facts from the authoritative write
/// database; it never treats a missing provider or missing recipient as a
/// successful effect.
#[derive(Debug, Clone)]
pub struct SqlxMarketingJourneyExecutionDecisionSource {
    store: SqlxMarketingJourneyExecutionStore,
}

impl SqlxMarketingJourneyExecutionDecisionSource {
    pub fn new(store: SqlxMarketingJourneyExecutionStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl MarketingJourneyExecutionDecisionSource for SqlxMarketingJourneyExecutionDecisionSource {
    async fn decide(
        &self,
        claim: &MarketingJourneyExecutionClaim,
    ) -> Result<JourneyNodeOutcome, MarketingJourneyExecutionDecisionError> {
        let Some(node_id) = claim.cursor.next_node_id() else {
            return Err(MarketingJourneyExecutionDecisionError::Permanent);
        };
        let Some(node) = claim.journey.nodes().iter().find(|node| node.id == node_id) else {
            return Err(MarketingJourneyExecutionDecisionError::Permanent);
        };
        match &node.definition {
            JourneyNodeDefinition::Trigger { .. }
            | JourneyNodeDefinition::Wait { .. }
            | JourneyNodeDefinition::CrmHandoff { .. } => Ok(JourneyNodeOutcome::Complete),
            JourneyNodeDefinition::Branch { predicate, .. } => {
                let profile = self
                    .store
                    .load_profile_context(claim.cursor.profile_id())
                    .await
                    .map_err(|_| MarketingJourneyExecutionDecisionError::Retryable)?
                    .ok_or(MarketingJourneyExecutionDecisionError::Permanent)?;
                if !claim.cursor.scope().contains(&profile.scope) {
                    return Err(MarketingJourneyExecutionDecisionError::Permanent);
                }
                Ok(JourneyNodeOutcome::Branch(evaluate_branch_predicate(
                    &profile.attributes,
                    predicate,
                )))
            }
            JourneyNodeDefinition::Channel { channel, payload } => {
                let profile = self
                    .store
                    .load_profile_context(claim.cursor.profile_id())
                    .await
                    .map_err(|_| MarketingJourneyExecutionDecisionError::Retryable)?
                    .ok_or(MarketingJourneyExecutionDecisionError::Permanent)?;
                if !claim.cursor.scope().contains(&profile.scope) {
                    return Err(MarketingJourneyExecutionDecisionError::Permanent);
                }
                let has_recipient = match channel {
                    erpx_domain::marketing::MarketingChannel::Email => profile
                        .email
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty()),
                    erpx_domain::marketing::MarketingChannel::Sms => profile
                        .phone
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty()),
                    erpx_domain::marketing::MarketingChannel::Social => {
                        let erpx_domain::marketing_journey::JourneyChannelPayload::Social {
                            network,
                            ..
                        } = payload
                        else {
                            return Err(MarketingJourneyExecutionDecisionError::Permanent);
                        };
                        profile
                            .attributes
                            .get(&format!("social:{network}"))
                            .is_some_and(|value| !value.trim().is_empty())
                    }
                    erpx_domain::marketing::MarketingChannel::Push => profile
                        .attributes
                        .get("push:device_ref")
                        .is_some_and(|value| !value.trim().is_empty()),
                    erpx_domain::marketing::MarketingChannel::Webhook => profile
                        .attributes
                        .get("webhook:destination_ref")
                        .is_some_and(|value| !value.trim().is_empty()),
                };
                if has_recipient {
                    Ok(JourneyNodeOutcome::Complete)
                } else {
                    Err(MarketingJourneyExecutionDecisionError::Permanent)
                }
            }
            JourneyNodeDefinition::SegmentChange { .. } => {
                let profile = self
                    .store
                    .load_profile_context(claim.cursor.profile_id())
                    .await
                    .map_err(|_| MarketingJourneyExecutionDecisionError::Retryable)?
                    .ok_or(MarketingJourneyExecutionDecisionError::Permanent)?;
                if claim.cursor.scope().contains(&profile.scope) {
                    Ok(JourneyNodeOutcome::Complete)
                } else {
                    Err(MarketingJourneyExecutionDecisionError::Permanent)
                }
            }
            JourneyNodeDefinition::Conversion { .. } => {
                let profile = self
                    .store
                    .load_profile_context(claim.cursor.profile_id())
                    .await
                    .map_err(|_| MarketingJourneyExecutionDecisionError::Retryable)?
                    .ok_or(MarketingJourneyExecutionDecisionError::Permanent)?;
                if claim.cursor.scope().contains(&profile.scope) {
                    Ok(JourneyNodeOutcome::Complete)
                } else {
                    Err(MarketingJourneyExecutionDecisionError::Permanent)
                }
            }
            JourneyNodeDefinition::Action {
                action: JourneyAction::InvokeApprovedIntegration { integration_ref },
            } => {
                let profile = self
                    .store
                    .load_profile_context(claim.cursor.profile_id())
                    .await
                    .map_err(|_| MarketingJourneyExecutionDecisionError::Retryable)?
                    .ok_or(MarketingJourneyExecutionDecisionError::Permanent)?;
                if !claim.cursor.scope().contains(&profile.scope) {
                    return Err(MarketingJourneyExecutionDecisionError::Permanent);
                }
                self.store
                    .ensure_approved_integration(integration_ref, claim.cursor.scope())
                    .await
                    .map_err(|error| match error {
                        PersistenceError::Database(_) | PersistenceError::Migration(_) => {
                            MarketingJourneyExecutionDecisionError::Retryable
                        }
                        _ => MarketingJourneyExecutionDecisionError::Permanent,
                    })?;
                Ok(JourneyNodeOutcome::Complete)
            }
            JourneyNodeDefinition::Action {
                action:
                    JourneyAction::AddProfileTag { .. }
                    | JourneyAction::RemoveProfileTag { .. }
                    | JourneyAction::NotifyOwner { .. },
            } => {
                let profile = self
                    .store
                    .load_profile_context(claim.cursor.profile_id())
                    .await
                    .map_err(|_| MarketingJourneyExecutionDecisionError::Retryable)?
                    .ok_or(MarketingJourneyExecutionDecisionError::Permanent)?;
                if claim.cursor.scope().contains(&profile.scope) {
                    Ok(JourneyNodeOutcome::Complete)
                } else {
                    Err(MarketingJourneyExecutionDecisionError::Permanent)
                }
            }
        }
    }
}

fn evaluate_branch_predicate(
    attributes: &std::collections::BTreeMap<String, String>,
    predicate: &JourneyBranchPredicate,
) -> bool {
    match predicate {
        JourneyBranchPredicate::AttributeEquals { attribute, value } => attributes
            .get(attribute)
            .is_some_and(|actual| actual == value),
        JourneyBranchPredicate::AttributeExists { attribute } => attributes.contains_key(attribute),
    }
}

/// Safe default for the explicitly routed execution worker. It can advance
/// triggers, waits, and the internal CRM handoff boundary. It refuses branch
/// decisions and every channel/integration/action node until a permission-
/// aware durable effect provider is supplied.
#[derive(Debug, Clone, Copy, Default)]
pub struct DomainOnlyMarketingJourneyExecutionDecisionSource;

#[async_trait]
impl MarketingJourneyExecutionDecisionSource for DomainOnlyMarketingJourneyExecutionDecisionSource {
    async fn decide(
        &self,
        claim: &MarketingJourneyExecutionClaim,
    ) -> Result<JourneyNodeOutcome, MarketingJourneyExecutionDecisionError> {
        let Some(node_id) = claim.cursor.next_node_id() else {
            return Err(MarketingJourneyExecutionDecisionError::Permanent);
        };
        let Some(node) = claim.journey.nodes().iter().find(|node| node.id == node_id) else {
            return Err(MarketingJourneyExecutionDecisionError::Permanent);
        };
        if !domain_only_node_is_supported(&node.definition) {
            return Err(MarketingJourneyExecutionDecisionError::Permanent);
        }
        Ok(JourneyNodeOutcome::Complete)
    }
}

fn domain_only_node_is_supported(
    definition: &erpx_domain::marketing_journey::JourneyNodeDefinition,
) -> bool {
    matches!(
        definition,
        erpx_domain::marketing_journey::JourneyNodeDefinition::Trigger { .. }
            | erpx_domain::marketing_journey::JourneyNodeDefinition::Wait { .. }
            | erpx_domain::marketing_journey::JourneyNodeDefinition::CrmHandoff { .. }
            | erpx_domain::marketing_journey::JourneyNodeDefinition::Conversion { .. }
    )
}

#[derive(Debug, Clone)]
pub struct MarketingJourneyExecutionHandler<S, D> {
    source: S,
    decision_source: D,
    batch_size: u32,
    lease: Duration,
}

impl<S, D> MarketingJourneyExecutionHandler<S, D>
where
    S: MarketingJourneyExecutionSource,
    D: MarketingJourneyExecutionDecisionSource,
{
    pub fn new(
        source: S,
        decision_source: D,
        batch_size: u32,
        lease: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        if batch_size == 0 || batch_size > MARKETING_JOURNEY_EXECUTION_MAX_BATCH_SIZE {
            return Err(WorkerRuntimeError::Configuration(format!(
                "Marketing Journey execution batch size must be 1..={MARKETING_JOURNEY_EXECUTION_MAX_BATCH_SIZE}"
            )));
        }
        if lease < Duration::seconds(MARKETING_JOURNEY_EXECUTION_MIN_LEASE_SECONDS as i64)
            || lease > Duration::seconds(MARKETING_JOURNEY_EXECUTION_MAX_LEASE_SECONDS as i64)
        {
            return Err(WorkerRuntimeError::Configuration(
                "Marketing Journey execution lease is outside its bound".into(),
            ));
        }
        Ok(Self {
            source,
            decision_source,
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
impl<S, D> NamedWorkerHandler for MarketingJourneyExecutionHandler<S, D>
where
    S: MarketingJourneyExecutionSource,
    D: MarketingJourneyExecutionDecisionSource,
{
    fn name(&self) -> &str {
        MARKETING_JOURNEY_EXECUTION_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "Marketing Journey execution worker identity is required".into(),
            ));
        }
        let now = Utc::now();
        let claims = self
            .source
            .claim_due(worker_id, now, self.batch_size, self.lease)
            .await
            .map_err(redacted_execution_persistence_error)?;
        let mut report = WorkerHandlerReport {
            claimed: claims.len() as u64,
            has_work: !claims.is_empty(),
            ..WorkerHandlerReport::default()
        };
        let decision_budget = std::time::Duration::from_secs(
            u64::try_from((self.lease.num_seconds() / 2).max(1)).unwrap_or(1),
        );
        for claim in claims {
            validate_claim(&claim, worker_id, now)?;
            let decision =
                match tokio::time::timeout(decision_budget, self.decision_source.decide(&claim))
                    .await
                {
                    Ok(decision) => decision,
                    Err(_) => Err(MarketingJourneyExecutionDecisionError::Retryable),
                };
            let outcome = match decision {
                Ok(outcome) => outcome,
                Err(error) => {
                    let failure = match error {
                        MarketingJourneyExecutionDecisionError::Retryable => {
                            MarketingJourneyExecutionFailure::Retryable
                        }
                        MarketingJourneyExecutionDecisionError::Permanent => {
                            MarketingJourneyExecutionFailure::Permanent
                        }
                    };
                    self.source
                        .release_claim(&claim, worker_id, Utc::now(), failure)
                        .await
                        .map_err(redacted_execution_persistence_error)?;
                    match error {
                        MarketingJourneyExecutionDecisionError::Retryable => {
                            report.retried = report.retried.saturating_add(1);
                            tracing::warn!(
                                error_kind = "retryable",
                                "Marketing Journey execution decision deferred"
                            );
                        }
                        MarketingJourneyExecutionDecisionError::Permanent => {
                            report.dead_lettered = report.dead_lettered.saturating_add(1);
                            tracing::error!(
                                error_kind = "permanent",
                                "Marketing Journey execution decision dead-lettered"
                            );
                        }
                    }
                    continue;
                }
            };
            let receipt = self
                .source
                .apply_claim(&claim, worker_id, outcome, Utc::now())
                .await
                .map_err(redacted_execution_persistence_error)?;
            validate_write_receipt(&claim, &receipt)?;
            report.processed = report.processed.saturating_add(1);
            report.published = report.published.saturating_add(1);
        }
        Ok(report)
    }
}

fn validate_claim(
    claim: &MarketingJourneyExecutionClaim,
    worker_id: Uuid,
    at: DateTime<Utc>,
) -> Result<(), WorkerRuntimeError> {
    if claim.claim_id.is_nil()
        || claim.worker_id.is_nil()
        || claim.worker_id != worker_id
        || claim.claim_until <= at
    {
        return Err(WorkerRuntimeError::Configuration(
            "Marketing Journey execution claim lease is invalid or no longer owned".into(),
        ));
    }
    if claim.journey.status() != JourneyStatus::Running
        || claim.journey.validate().is_err()
        || claim.cursor.validate_for_journey(&claim.journey).is_err()
        || !matches!(
            claim.cursor.state(),
            erpx_domain::marketing_journey::JourneyExecutionState::Ready
                | erpx_domain::marketing_journey::JourneyExecutionState::Running
                | erpx_domain::marketing_journey::JourneyExecutionState::Waiting
        )
        || claim.cursor.next_node_id().is_none()
        || claim.cursor.waiting_until().is_some_and(|until| until > at)
        || claim.cursor.updated_at() > at
    {
        return Err(WorkerRuntimeError::Configuration(
            "Marketing Journey execution claim is outside its runnable scope".into(),
        ));
    }
    Ok(())
}

fn validate_write_receipt(
    claim: &MarketingJourneyExecutionClaim,
    receipt: &MarketingJourneyExecutionWriteReceipt,
) -> Result<(), WorkerRuntimeError> {
    let expected_cursor_version = claim.cursor.version().checked_add(1).ok_or_else(|| {
        WorkerRuntimeError::Configuration(
            "Marketing Journey execution cursor version overflowed".into(),
        )
    })?;
    let expected_idempotency_key = format!(
        "marketing-journey-execution:{}:{}",
        claim.cursor.run_id(),
        claim.cursor.version()
    );
    if receipt.journey_id != claim.journey.id()
        || receipt.run_id != claim.cursor.run_id()
        || receipt.cursor_version != expected_cursor_version
        || receipt.idempotency_key != expected_idempotency_key
        || receipt.event_id.is_nil()
    {
        return Err(WorkerRuntimeError::Configuration(
            "Marketing Journey execution write receipt is outside its source fence".into(),
        ));
    }
    Ok(())
}

fn redacted_execution_persistence_error(error: PersistenceError) -> WorkerRuntimeError {
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
    tracing::error!(
        error_kind = kind,
        "Marketing Journey execution persistence failed"
    );
    WorkerRuntimeError::Configuration("Marketing Journey execution worker failed".into())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use erpx_domain::contracts::AuthorityScope;
    use erpx_domain::marketing_journey::{
        Journey, JourneyExecutionCursor, JourneyNode, JourneyNodeDefinition, JourneyTrigger,
    };

    use super::*;

    #[derive(Clone, Default)]
    struct FakeSource {
        state: Arc<Mutex<FakeState>>,
    }

    #[derive(Default)]
    struct FakeState {
        claims: Vec<MarketingJourneyExecutionClaim>,
        applied: Vec<JourneyNodeOutcome>,
        released: Vec<MarketingJourneyExecutionFailure>,
    }

    #[async_trait]
    impl MarketingJourneyExecutionSource for FakeSource {
        async fn claim_due(
            &self,
            _worker_id: Uuid,
            _now: DateTime<Utc>,
            _limit: u32,
            _lease: Duration,
        ) -> Result<Vec<MarketingJourneyExecutionClaim>, PersistenceError> {
            Ok(std::mem::take(
                &mut self.state.lock().expect("state").claims,
            ))
        }

        async fn apply_claim(
            &self,
            claim: &MarketingJourneyExecutionClaim,
            _worker_id: Uuid,
            outcome: JourneyNodeOutcome,
            _at: DateTime<Utc>,
        ) -> Result<MarketingJourneyExecutionWriteReceipt, PersistenceError> {
            self.state.lock().expect("state").applied.push(outcome);
            Ok(MarketingJourneyExecutionWriteReceipt {
                journey_id: claim.journey.id(),
                run_id: claim.cursor.run_id(),
                cursor_version: claim.cursor.version() + 1,
                idempotency_key: format!(
                    "marketing-journey-execution:{}:{}",
                    claim.cursor.run_id(),
                    claim.cursor.version()
                ),
                event_id: Uuid::new_v4(),
                replayed: false,
            })
        }

        async fn release_claim(
            &self,
            _claim: &MarketingJourneyExecutionClaim,
            _worker_id: Uuid,
            _at: DateTime<Utc>,
            failure: MarketingJourneyExecutionFailure,
        ) -> Result<(), PersistenceError> {
            self.state.lock().expect("state").released.push(failure);
            Ok(())
        }
    }

    #[derive(Clone)]
    struct FakeDecision {
        result: Result<JourneyNodeOutcome, MarketingJourneyExecutionDecisionError>,
    }

    #[async_trait]
    impl MarketingJourneyExecutionDecisionSource for FakeDecision {
        async fn decide(
            &self,
            _claim: &MarketingJourneyExecutionClaim,
        ) -> Result<JourneyNodeOutcome, MarketingJourneyExecutionDecisionError> {
            self.result
        }
    }

    #[derive(Clone, Copy)]
    struct TimeoutDecision;

    #[async_trait]
    impl MarketingJourneyExecutionDecisionSource for TimeoutDecision {
        async fn decide(
            &self,
            _claim: &MarketingJourneyExecutionClaim,
        ) -> Result<JourneyNodeOutcome, MarketingJourneyExecutionDecisionError> {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            Ok(JourneyNodeOutcome::Complete)
        }
    }

    fn test_worker_id() -> Uuid {
        Uuid::from_u128(99)
    }

    fn runnable_journey(seed: u128) -> Journey {
        let scope = AuthorityScope::Company {
            holding_id: Uuid::from_u128(seed),
            company_id: Uuid::from_u128(seed + 1),
        };
        let trigger_id = Uuid::from_u128(seed + 10);
        let wait_id = Uuid::from_u128(seed + 11);
        let at = Utc::now();
        let journey = Journey::new(
            Uuid::from_u128(seed + 12),
            scope,
            "test-journey",
            vec![
                JourneyNode::new(
                    trigger_id,
                    0,
                    Some(wait_id),
                    JourneyNodeDefinition::Trigger {
                        trigger: JourneyTrigger::Enrollment,
                    },
                )
                .expect("trigger"),
                JourneyNode::new(
                    wait_id,
                    1,
                    None,
                    JourneyNodeDefinition::Wait {
                        duration_seconds: 1,
                    },
                )
                .expect("wait"),
            ],
            at,
        )
        .expect("journey");
        let mut serialized = serde_json::to_value(journey).expect("journey JSON");
        serialized["status"] = serde_json::json!("running");
        serde_json::from_value(serialized).expect("running journey")
    }

    fn test_claim() -> MarketingJourneyExecutionClaim {
        let journey = runnable_journey(1);
        let cursor = JourneyExecutionCursor::start(
            &journey,
            Uuid::from_u128(20),
            Uuid::from_u128(21),
            Utc::now(),
        )
        .expect("cursor");
        MarketingJourneyExecutionClaim {
            journey,
            cursor,
            claim_id: Uuid::from_u128(22),
            worker_id: test_worker_id(),
            claim_until: Utc::now() + Duration::seconds(30),
        }
    }

    fn handler(
        decision: Result<JourneyNodeOutcome, MarketingJourneyExecutionDecisionError>,
    ) -> (
        FakeSource,
        MarketingJourneyExecutionHandler<FakeSource, FakeDecision>,
    ) {
        let source = FakeSource::default();
        let handler = MarketingJourneyExecutionHandler::new(
            source.clone(),
            FakeDecision { result: decision },
            1,
            Duration::seconds(10),
        )
        .expect("handler");
        (source, handler)
    }

    #[test]
    fn handler_bounds_batch_and_lease() {
        let source = FakeSource::default();
        let decision = FakeDecision {
            result: Ok(JourneyNodeOutcome::Complete),
        };
        assert!(
            MarketingJourneyExecutionHandler::new(
                source.clone(),
                decision.clone(),
                0,
                Duration::seconds(1)
            )
            .is_err()
        );
        assert!(
            MarketingJourneyExecutionHandler::new(source, decision, 1, Duration::zero()).is_err()
        );
    }

    #[test]
    fn domain_only_decisions_fail_closed_for_external_effect_nodes() {
        use erpx_domain::contracts::AuthorityScope;
        use erpx_domain::marketing::MarketingChannel;
        use erpx_domain::marketing_crm::{
            AccountReference, ContactReference, MarketingAcquisitionReference,
            MarketingLeadReference, MarketingQualificationReference, MarketingQualifiedSource,
        };
        use erpx_domain::marketing_journey::{
            JourneyAction, JourneyChannelPayload, JourneyNodeDefinition,
        };
        let scope = AuthorityScope::Company {
            holding_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
        };

        assert!(domain_only_node_is_supported(
            &JourneyNodeDefinition::Trigger {
                trigger: erpx_domain::marketing_journey::JourneyTrigger::Enrollment,
            }
        ));
        assert!(domain_only_node_is_supported(
            &JourneyNodeDefinition::Wait {
                duration_seconds: 1,
            }
        ));
        assert!(domain_only_node_is_supported(
            &JourneyNodeDefinition::CrmHandoff {
                source: Box::new(MarketingQualifiedSource {
                    lead: MarketingLeadReference::new("lead", scope.clone()).expect("lead"),
                    acquisition: MarketingAcquisitionReference::new("acquisition", scope.clone(),)
                        .expect("acquisition"),
                    qualification: MarketingQualificationReference::new(
                        "qualification",
                        scope.clone(),
                    )
                    .expect("qualification"),
                }),
                account: AccountReference::new("account", scope.clone()).expect("account"),
                contact: ContactReference::new("contact", scope).expect("contact"),
            }
        ));
        assert!(!domain_only_node_is_supported(
            &JourneyNodeDefinition::Channel {
                channel: MarketingChannel::Email,
                payload: JourneyChannelPayload::Email {
                    template_ref: "template".into(),
                    subject: "subject".into(),
                },
            }
        ));
        assert!(!domain_only_node_is_supported(
            &JourneyNodeDefinition::Action {
                action: JourneyAction::InvokeApprovedIntegration {
                    integration_ref: "integration".into(),
                },
            }
        ));
        assert!(!domain_only_node_is_supported(
            &JourneyNodeDefinition::Action {
                action: JourneyAction::NotifyOwner {
                    owner_ref: "operations-owner".into(),
                },
            }
        ));
    }

    #[test]
    fn production_branch_predicate_uses_exact_profile_attributes() {
        let attributes = std::collections::BTreeMap::from([
            ("lifecycle".to_owned(), "qualified".to_owned()),
            ("region".to_owned(), "apac".to_owned()),
        ]);
        assert!(evaluate_branch_predicate(
            &attributes,
            &JourneyBranchPredicate::AttributeEquals {
                attribute: "lifecycle".into(),
                value: "qualified".into(),
            }
        ));
        assert!(!evaluate_branch_predicate(
            &attributes,
            &JourneyBranchPredicate::AttributeEquals {
                attribute: "lifecycle".into(),
                value: "new".into(),
            }
        ));
        assert!(evaluate_branch_predicate(
            &attributes,
            &JourneyBranchPredicate::AttributeExists {
                attribute: "region".into(),
            }
        ));
        assert!(!evaluate_branch_predicate(
            &attributes,
            &JourneyBranchPredicate::AttributeExists {
                attribute: "missing".into(),
            }
        ));
    }

    #[tokio::test]
    async fn retryable_decision_is_redacted_and_released() {
        let (source, handler) = handler(Err(MarketingJourneyExecutionDecisionError::Retryable));
        source
            .state
            .lock()
            .expect("state")
            .claims
            .push(test_claim());
        let report = handler.run_once(test_worker_id()).await.expect("run");
        assert_eq!(report.claimed, 1);
        assert_eq!(report.retried, 1);
        assert!(source.state.lock().expect("state").applied.is_empty());
        assert_eq!(
            source.state.lock().expect("state").released,
            vec![MarketingJourneyExecutionFailure::Retryable]
        );
    }

    #[tokio::test]
    async fn decision_timeout_releases_the_claim_for_retry_without_applying() {
        let source = FakeSource::default();
        let handler = MarketingJourneyExecutionHandler::new(
            source.clone(),
            TimeoutDecision,
            1,
            Duration::seconds(2),
        )
        .expect("handler");
        source
            .state
            .lock()
            .expect("state")
            .claims
            .push(test_claim());

        let report = handler.run_once(test_worker_id()).await.expect("run");

        assert_eq!(report.claimed, 1);
        assert_eq!(report.processed, 0);
        assert_eq!(report.retried, 1);
        assert!(source.state.lock().expect("state").applied.is_empty());
        assert_eq!(
            source.state.lock().expect("state").released,
            vec![MarketingJourneyExecutionFailure::Retryable]
        );
    }

    #[tokio::test]
    async fn permanent_decision_is_released_to_the_dead_letter_path() {
        let (source, handler) = handler(Err(MarketingJourneyExecutionDecisionError::Permanent));
        source
            .state
            .lock()
            .expect("state")
            .claims
            .push(test_claim());

        let report = handler.run_once(test_worker_id()).await.expect("run");

        assert_eq!(report.claimed, 1);
        assert_eq!(report.processed, 0);
        assert_eq!(report.dead_lettered, 1);
        assert!(source.state.lock().expect("state").applied.is_empty());
        assert_eq!(
            source.state.lock().expect("state").released,
            vec![MarketingJourneyExecutionFailure::Permanent]
        );
    }

    #[tokio::test]
    async fn successful_claim_applies_only_with_a_matching_source_receipt() {
        let (source, handler) = handler(Ok(JourneyNodeOutcome::Complete));
        source
            .state
            .lock()
            .expect("state")
            .claims
            .push(test_claim());
        let report = handler.run_once(test_worker_id()).await.expect("run");
        assert_eq!(report.claimed, 1);
        assert_eq!(report.processed, 1);
        assert_eq!(report.published, 1);
        assert_eq!(source.state.lock().expect("state").applied.len(), 1);
    }

    #[tokio::test]
    async fn stale_or_mismatched_claims_fail_before_decision_or_release() {
        let (source, first_handler) = handler(Ok(JourneyNodeOutcome::Complete));
        let mut stale = test_claim();
        stale.claim_until = Utc::now() - Duration::seconds(1);
        source.state.lock().expect("state").claims.push(stale);
        assert!(first_handler.run_once(test_worker_id()).await.is_err());
        assert!(source.state.lock().expect("state").applied.is_empty());
        assert!(source.state.lock().expect("state").released.is_empty());

        let (source, second_handler) = handler(Ok(JourneyNodeOutcome::Complete));
        let mut wrong_owner = test_claim();
        wrong_owner.worker_id = Uuid::from_u128(100);
        source.state.lock().expect("state").claims.push(wrong_owner);
        assert!(second_handler.run_once(test_worker_id()).await.is_err());
        assert!(source.state.lock().expect("state").applied.is_empty());
        assert!(source.state.lock().expect("state").released.is_empty());

        let (source, third_handler) = handler(Ok(JourneyNodeOutcome::Complete));
        let mut wrong_scope = test_claim();
        let other_journey = runnable_journey(100);
        wrong_scope.cursor = JourneyExecutionCursor::start(
            &other_journey,
            Uuid::from_u128(200),
            Uuid::from_u128(201),
            Utc::now(),
        )
        .expect("other cursor");
        source.state.lock().expect("state").claims.push(wrong_scope);
        assert!(third_handler.run_once(test_worker_id()).await.is_err());
        assert!(source.state.lock().expect("state").applied.is_empty());
        assert!(source.state.lock().expect("state").released.is_empty());

        let (source, revoked_handler) = handler(Ok(JourneyNodeOutcome::Complete));
        let mut revoked = test_claim();
        let mut serialized = serde_json::to_value(&revoked.journey).expect("journey JSON");
        serialized["status"] = serde_json::json!("paused");
        revoked.journey = serde_json::from_value(serialized).expect("paused journey");
        source.state.lock().expect("state").claims.push(revoked);
        assert!(revoked_handler.run_once(test_worker_id()).await.is_err());
        assert!(source.state.lock().expect("state").applied.is_empty());
        assert!(source.state.lock().expect("state").released.is_empty());

        let (source, paused_cursor_handler) = handler(Ok(JourneyNodeOutcome::Complete));
        let mut paused_cursor = test_claim();
        paused_cursor
            .cursor
            .pause(0, Utc::now())
            .expect("pause cursor");
        source
            .state
            .lock()
            .expect("state")
            .claims
            .push(paused_cursor);
        assert!(
            paused_cursor_handler
                .run_once(test_worker_id())
                .await
                .is_err()
        );
        assert!(source.state.lock().expect("state").applied.is_empty());
        assert!(source.state.lock().expect("state").released.is_empty());
    }

    #[tokio::test]
    async fn nil_worker_identity_fails_closed_before_claiming() {
        let (source, handler) = handler(Ok(JourneyNodeOutcome::Complete));
        source
            .state
            .lock()
            .expect("state")
            .claims
            .push(test_claim());
        assert!(handler.run_once(Uuid::nil()).await.is_err());
        assert_eq!(source.state.lock().expect("state").claims.len(), 1);
    }
}
