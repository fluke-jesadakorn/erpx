//! Holding-scoped projection scheduler worker boundary.
//!
//! The handler claims only control-plane scheduling records bound to one
//! holding/holding, executes an injected projector, and records completion or
//! a bounded failure through the scheduler repository. It never opens holding
//! credentials or chooses a database route itself.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_infrastructure::{
    HoldingDatabaseRouter, HoldingHoldingBinding, HoldingProjectionSchedulerError,
    HoldingProjectionSchedulerRepository, HoldingSourceProjection, ProjectionClaimRecord,
    ProjectionClaimRequest, ProjectionFailureOutcome, ProjectionJobState, ProjectionLease,
    RoutedDatabaseSide, RoutedHoldingDatabase, SqlxHoldingStoreRegistry,
};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use super::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const HOLDING_PROJECTION_SCHEDULER_HANDLER_NAME: &str = "holding_projection_scheduler";
pub const HOLDING_PROJECTION_SCHEDULER_MAX_BATCH_SIZE: u32 = 100;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum HoldingProjectionExecutorError {
    #[error("projection executor is temporarily unavailable")]
    Retryable,
    #[error("candidate projection source moved during rebuild")]
    CandidateSourceMoved,
    #[error("projection executor rejected the job permanently")]
    Permanent,
}

#[async_trait]
pub trait HoldingProjectionSchedulerSource: Send + Sync + 'static {
    async fn claim_due(
        &self,
        request: ProjectionClaimRequest,
    ) -> Result<Vec<ProjectionClaimRecord>, HoldingProjectionSchedulerError>;

    async fn complete(
        &self,
        binding: &HoldingHoldingBinding,
        job_id: Uuid,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), HoldingProjectionSchedulerError>;

    async fn fail(
        &self,
        binding: &HoldingHoldingBinding,
        job_id: Uuid,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
        error: &str,
    ) -> Result<ProjectionFailureOutcome, HoldingProjectionSchedulerError>;

    async fn fail_terminal(
        &self,
        binding: &HoldingHoldingBinding,
        job_id: Uuid,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
        error: &str,
    ) -> Result<ProjectionFailureOutcome, HoldingProjectionSchedulerError> {
        self.fail(binding, job_id, claim_id, worker_id, now, error)
            .await
    }

    async fn renew(
        &self,
        binding: &HoldingHoldingBinding,
        job_id: Uuid,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> Result<ProjectionLease, HoldingProjectionSchedulerError>;
}

#[async_trait]
impl HoldingProjectionSchedulerSource for HoldingProjectionSchedulerRepository {
    async fn claim_due(
        &self,
        request: ProjectionClaimRequest,
    ) -> Result<Vec<ProjectionClaimRecord>, HoldingProjectionSchedulerError> {
        HoldingProjectionSchedulerRepository::claim_due(self, request).await
    }

    async fn complete(
        &self,
        binding: &HoldingHoldingBinding,
        job_id: Uuid,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), HoldingProjectionSchedulerError> {
        HoldingProjectionSchedulerRepository::complete(
            self, binding, job_id, claim_id, worker_id, now,
        )
        .await
    }

    async fn fail(
        &self,
        binding: &HoldingHoldingBinding,
        job_id: Uuid,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
        error: &str,
    ) -> Result<ProjectionFailureOutcome, HoldingProjectionSchedulerError> {
        HoldingProjectionSchedulerRepository::fail(
            self, binding, job_id, claim_id, worker_id, now, error,
        )
        .await
    }

    async fn fail_terminal(
        &self,
        binding: &HoldingHoldingBinding,
        job_id: Uuid,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
        error: &str,
    ) -> Result<ProjectionFailureOutcome, HoldingProjectionSchedulerError> {
        HoldingProjectionSchedulerRepository::fail_terminal(
            self, binding, job_id, claim_id, worker_id, now, error,
        )
        .await
    }

    async fn renew(
        &self,
        binding: &HoldingHoldingBinding,
        job_id: Uuid,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> Result<ProjectionLease, HoldingProjectionSchedulerError> {
        HoldingProjectionSchedulerRepository::renew_lease(
            self, binding, job_id, claim_id, worker_id, now, lease,
        )
        .await
    }
}

#[async_trait]
pub trait HoldingProjectionExecutor: Send + Sync + 'static {
    /// Restricts claims to one exact projection. Executors returning `None`
    /// must be able to dispatch every projection in the shared catalog.
    fn projection_name_filter(&self) -> Option<&str> {
        None
    }

    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), HoldingProjectionExecutorError>;
}

#[async_trait]
pub trait HoldingProjectionRoutedExecutor: Send + Sync + 'static {
    /// Restricts the shared scheduler claim to the exact projection owned by
    /// this routed executor. A routed worker must never claim another
    /// projector's job merely because both use the same scheduler table.
    fn projection_name(&self) -> &str;

    async fn execute_routed(
        &self,
        claim: &ProjectionClaimRecord,
        write: &RoutedHoldingDatabase,
        read: &RoutedHoldingDatabase,
    ) -> Result<(), HoldingProjectionExecutorError>;
}

#[derive(Debug, Clone)]
pub struct RoutedHoldingProjectionExecutor<E> {
    router: HoldingDatabaseRouter,
    executor: E,
    allow_unfenced_rebuilds: bool,
}

impl<E> RoutedHoldingProjectionExecutor<E>
where
    E: HoldingProjectionRoutedExecutor,
{
    pub fn new(router: HoldingDatabaseRouter, executor: E) -> Self {
        Self {
            router,
            executor,
            allow_unfenced_rebuilds: false,
        }
    }

    pub fn new_allow_unfenced_rebuilds(router: HoldingDatabaseRouter, executor: E) -> Self {
        Self {
            router,
            executor,
            allow_unfenced_rebuilds: true,
        }
    }

    async fn activate_ready_candidate(
        &self,
        claim: &ProjectionClaimRecord,
        candidate: &str,
        source_version: u64,
        generation: u64,
    ) -> Result<(), HoldingProjectionExecutorError> {
        let write = self
            .router
            .route(
                &claim.job.binding.holding_id,
                &claim.job.binding.data_profile_key,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
        let mut source_lock = match write.acquire_source_rebuild_lock().await {
            Ok(source_lock) => source_lock,
            Err(_) => return Err(HoldingProjectionExecutorError::Retryable),
        };
        let source_projection = match source_projection_for_name(&claim.job.projection_name) {
            Some(source_projection) => source_projection,
            None => {
                if let Err(error) = source_lock.rollback().await {
                    tracing::error!(error = %error, "holding projection source rollback failed");
                }
                write.close().await;
                return Err(HoldingProjectionExecutorError::Permanent);
            }
        };
        let source_after = match source_lock
            .source_event_high_watermark(source_projection, &claim.job.binding.data_profile_key)
            .await
        {
            Ok(source_after) => source_after,
            Err(_) => {
                if let Err(error) = source_lock.rollback().await {
                    tracing::error!(error = %error, "holding projection source rollback failed");
                }
                write.close().await;
                return Err(HoldingProjectionExecutorError::Retryable);
            }
        };
        if source_after != source_version {
            if let Err(error) = source_lock.rollback().await {
                tracing::error!(error = %error, "holding projection source rollback failed");
            }
            write.close().await;
            let failed = self
                .router
                .fail_replacement_read_database(
                    &claim.job.binding.holding_id,
                    candidate,
                    generation,
                    "ready candidate source moved before activation",
                )
                .await;
            if failed.is_err() {
                return Err(HoldingProjectionExecutorError::Retryable);
            }
            return Err(HoldingProjectionExecutorError::CandidateSourceMoved);
        }
        if self
            .router
            .activate_replacement_read_database(
                &claim.job.binding.holding_id,
                candidate,
                generation,
            )
            .await
            .is_err()
        {
            if let Err(error) = source_lock.rollback().await {
                tracing::error!(error = %error, "holding projection source rollback failed");
            }
            write.close().await;
            return Err(HoldingProjectionExecutorError::Retryable);
        }
        if source_lock.commit().await.is_err() {
            write.close().await;
            return Err(HoldingProjectionExecutorError::Retryable);
        }
        write.close().await;
        Ok(())
    }
}

fn parse_candidate_rebuild_payload(
    claim: &ProjectionClaimRecord,
) -> Result<Option<(String, u64, u64)>, HoldingProjectionExecutorError> {
    let Some(payload) = claim.job.payload.as_object() else {
        if claim.job.payload.is_null() {
            return Ok(None);
        }
        return Err(HoldingProjectionExecutorError::Permanent);
    };
    let candidate = payload.get("candidate_read_database_key");
    let source_version = payload.get("source_version");
    let generation = payload.get("generation");
    let has_candidate_fields =
        candidate.is_some() || source_version.is_some() || generation.is_some();
    if !has_candidate_fields {
        return Ok(None);
    }
    if claim.job.kind != erpx_infrastructure::ProjectionJobKind::Rebuild {
        return Err(HoldingProjectionExecutorError::Permanent);
    }
    let Some(candidate) = candidate.and_then(Value::as_str) else {
        return Err(HoldingProjectionExecutorError::Permanent);
    };
    let Some(source_version) = source_version.and_then(Value::as_u64) else {
        return Err(HoldingProjectionExecutorError::Permanent);
    };
    let Some(generation) = generation.and_then(Value::as_u64) else {
        return Err(HoldingProjectionExecutorError::Permanent);
    };
    if candidate.trim().is_empty()
        || candidate.len() > 63
        || candidate
            .chars()
            .any(|character| !(character.is_ascii_alphanumeric() || matches!(character, '_' | '-')))
        || source_version == 0
        || generation == 0
    {
        return Err(HoldingProjectionExecutorError::Permanent);
    }
    Ok(Some((candidate.to_owned(), source_version, generation)))
}

fn require_candidate_rebuild_payload(
    claim: &ProjectionClaimRecord,
) -> Result<Option<(String, u64, u64)>, HoldingProjectionExecutorError> {
    let candidate_rebuild = parse_candidate_rebuild_payload(claim)?;
    if claim.job.kind == erpx_infrastructure::ProjectionJobKind::Rebuild
        && candidate_rebuild.is_none()
    {
        return Err(HoldingProjectionExecutorError::Permanent);
    }
    Ok(candidate_rebuild)
}

#[async_trait]
impl<E> HoldingProjectionExecutor for RoutedHoldingProjectionExecutor<E>
where
    E: HoldingProjectionRoutedExecutor,
{
    fn projection_name_filter(&self) -> Option<&str> {
        Some(self.executor.projection_name())
    }

    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), HoldingProjectionExecutorError> {
        let candidate_rebuild = if self.allow_unfenced_rebuilds {
            parse_candidate_rebuild_payload(claim)?
        } else {
            require_candidate_rebuild_payload(claim)?
        };

        if let Some((candidate, source_version, generation)) = candidate_rebuild {
            if self
                .router
                .replacement_read_is_activated(
                    &claim.job.binding.holding_id,
                    &candidate,
                    generation,
                )
                .await
                .map_err(|_| HoldingProjectionExecutorError::Retryable)?
            {
                // The worker may have activated the candidate just before a
                // scheduler acknowledgement was lost. Replaying the rebuild
                // would risk clearing the now-active read model.
                return Ok(());
            }

            if self
                .router
                .replacement_read_is_ready(
                    &claim.job.binding.holding_id,
                    &candidate,
                    generation,
                    source_version,
                )
                .await
                .map_err(|_| HoldingProjectionExecutorError::Retryable)?
            {
                return self
                    .activate_ready_candidate(claim, &candidate, source_version, generation)
                    .await;
            }

            if self
                .router
                .prepare_replacement_read_database(
                    &claim.job.binding.holding_id,
                    &claim.job.binding.data_profile_key,
                    &candidate,
                )
                .await
                .is_err()
            {
                let failed = self
                    .router
                    .fail_replacement_read_database(
                        &claim.job.binding.holding_id,
                        &candidate,
                        generation,
                        "candidate preparation failed",
                    )
                    .await;
                if failed.is_err() {
                    return Err(HoldingProjectionExecutorError::Retryable);
                }
                return Err(HoldingProjectionExecutorError::Permanent);
            }

            let write = self
                .router
                .route(
                    &claim.job.binding.holding_id,
                    &claim.job.binding.data_profile_key,
                    RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
            let read = match self
                .router
                .route_replacement_read_for_generation(
                    &claim.job.binding.holding_id,
                    &claim.job.binding.data_profile_key,
                    &candidate,
                    generation,
                    source_version,
                )
                .await
            {
                Ok(read) => read,
                Err(_) => {
                    write.close().await;
                    return Err(HoldingProjectionExecutorError::Retryable);
                }
            };
            let result = self.executor.execute_routed(claim, &write, &read).await;
            read.close().await;

            let mut source_lock = match result {
                Ok(()) => {
                    let source_lock_result = write.acquire_source_rebuild_lock().await;
                    if source_lock_result.is_err() {
                        drop(source_lock_result);
                        write.close().await;
                        return Err(HoldingProjectionExecutorError::Retryable);
                    }
                    match source_lock_result {
                        Ok(source_lock) => source_lock,
                        Err(_) => unreachable!("source lock result was checked above"),
                    }
                }
                Err(error) => {
                    write.close().await;
                    match error {
                        HoldingProjectionExecutorError::Retryable => {
                            return Err(HoldingProjectionExecutorError::Retryable);
                        }
                        HoldingProjectionExecutorError::CandidateSourceMoved => {
                            let failed = self
                                .router
                                .fail_replacement_read_database(
                                    &claim.job.binding.holding_id,
                                    &candidate,
                                    generation,
                                    "candidate source moved during rebuild",
                                )
                                .await;
                            if failed.is_err() {
                                return Err(HoldingProjectionExecutorError::Retryable);
                            }
                            return Err(HoldingProjectionExecutorError::CandidateSourceMoved);
                        }
                        HoldingProjectionExecutorError::Permanent => {
                            let failed = self
                                .router
                                .fail_replacement_read_database(
                                    &claim.job.binding.holding_id,
                                    &candidate,
                                    generation,
                                    "candidate projection failed",
                                )
                                .await;
                            if failed.is_err() {
                                return Err(HoldingProjectionExecutorError::Retryable);
                            }
                            return Err(HoldingProjectionExecutorError::Permanent);
                        }
                    }
                }
            };
            let source_projection = match source_projection_for_name(&claim.job.projection_name) {
                Some(source_projection) => source_projection,
                None => {
                    if let Err(error) = source_lock.rollback().await {
                        tracing::error!(error = %error, "holding projection source rollback failed");
                    }
                    write.close().await;
                    let failed = self
                        .router
                        .fail_replacement_read_database(
                            &claim.job.binding.holding_id,
                            &candidate,
                            generation,
                            "candidate projection source is unsupported",
                        )
                        .await;
                    if failed.is_err() {
                        return Err(HoldingProjectionExecutorError::Retryable);
                    }
                    return Err(HoldingProjectionExecutorError::Permanent);
                }
            };
            let source_after = match source_lock
                .source_event_high_watermark(source_projection, &claim.job.binding.data_profile_key)
                .await
            {
                Ok(source_after) => source_after,
                Err(_) => {
                    if let Err(error) = source_lock.rollback().await {
                        tracing::error!(error = %error, "holding projection source rollback failed");
                    }
                    write.close().await;
                    return Err(HoldingProjectionExecutorError::Retryable);
                }
            };
            if source_after != source_version {
                if let Err(error) = source_lock.rollback().await {
                    tracing::error!(error = %error, "holding projection source rollback failed");
                }
                write.close().await;
                let failed = self
                    .router
                    .fail_replacement_read_database(
                        &claim.job.binding.holding_id,
                        &candidate,
                        generation,
                        "candidate source moved during rebuild",
                    )
                    .await;
                if failed.is_err() {
                    return Err(HoldingProjectionExecutorError::Retryable);
                }
                return Err(HoldingProjectionExecutorError::CandidateSourceMoved);
            }
            if self
                .router
                .complete_replacement_read_database(
                    &claim.job.binding.holding_id,
                    &candidate,
                    generation,
                )
                .await
                .is_err()
            {
                // Keep the generation rebuilding. A retry can either finish
                // the candidate or observe it ready and activate it safely.
                if let Err(error) = source_lock.rollback().await {
                    tracing::error!(error = %error, "holding projection source rollback failed");
                }
                write.close().await;
                return Err(HoldingProjectionExecutorError::Retryable);
            }
            if self
                .router
                .activate_replacement_read_database(
                    &claim.job.binding.holding_id,
                    &candidate,
                    generation,
                )
                .await
                .is_err()
            {
                // Completion is durable and the ready-state retry path above
                // makes activation failure recoverable without rebuilding.
                if let Err(error) = source_lock.rollback().await {
                    tracing::error!(error = %error, "holding projection source rollback failed");
                }
                write.close().await;
                return Err(HoldingProjectionExecutorError::Retryable);
            }
            if source_lock.commit().await.is_err() {
                write.close().await;
                return Err(HoldingProjectionExecutorError::Retryable);
            }
            write.close().await;
            return Ok(());
        }

        let write = self
            .router
            .route(
                &claim.job.binding.holding_id,
                &claim.job.binding.data_profile_key,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
        let read = match self
            .router
            .route(
                &claim.job.binding.holding_id,
                &claim.job.binding.data_profile_key,
                RoutedDatabaseSide::Read,
            )
            .await
        {
            Ok(read) => read,
            Err(_) => {
                write.close().await;
                return Err(HoldingProjectionExecutorError::Retryable);
            }
        };
        let result = self.executor.execute_routed(claim, &write, &read).await;
        write.close().await;
        read.close().await;
        result
    }
}

fn source_projection_for_name(name: &str) -> Option<HoldingSourceProjection> {
    match name {
        erpx_operations_sqlx::EXCEPTION_PROJECTION
        | erpx_operations_sqlx::SOURCE_SIGNAL_PROJECTION
        | erpx_operations_sqlx::POLICY_PROJECTION
        | erpx_operations_sqlx::LIFECYCLE_RECORD_PROJECTION_NAME => {
            Some(HoldingSourceProjection::Operations)
        }
        erpx_infrastructure::FINANCE_BANKING_READ_PROJECTION => {
            Some(HoldingSourceProjection::FinanceBanking)
        }
        erpx_infrastructure::FINANCE_SUBLEDGER_READ_PROJECTION => {
            Some(HoldingSourceProjection::FinanceSubledger)
        }
        erpx_infrastructure::FINANCE_CLOSE_PROJECTION_NAME => {
            Some(HoldingSourceProjection::FinanceClose)
        }
        erpx_integration_contracts::ITEM_DIRECTORY_PROJECTION_NAME => {
            Some(HoldingSourceProjection::ItemDirectory)
        }
        _ => None,
    }
}

#[async_trait]
pub trait HoldingProjectionBindingSource: Send + Sync + 'static {
    async fn active_bindings(
        &self,
    ) -> Result<Vec<HoldingHoldingBinding>, HoldingProjectionSchedulerError>;
}

#[async_trait]
impl HoldingProjectionBindingSource for SqlxHoldingStoreRegistry {
    async fn active_bindings(
        &self,
    ) -> Result<Vec<HoldingHoldingBinding>, HoldingProjectionSchedulerError> {
        self.list_active()
            .await
            .map_err(|error| HoldingProjectionSchedulerError::Database(error.to_string()))?
            .into_iter()
            .map(|registration| {
                HoldingHoldingBinding::new(registration.holding_id, registration.data_profile_key)
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct HoldingProjectionSchedulerHandler<S, E> {
    source: S,
    executor: E,
    binding: HoldingHoldingBinding,
    claim_key_prefix: String,
    batch_size: u32,
    lease: Duration,
}

impl<S, E> HoldingProjectionSchedulerHandler<S, E>
where
    S: HoldingProjectionSchedulerSource,
    E: HoldingProjectionExecutor,
{
    pub fn new(
        source: S,
        executor: E,
        binding: HoldingHoldingBinding,
        claim_key_prefix: impl Into<String>,
        batch_size: u32,
        lease: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        binding
            .validate()
            .map_err(redacted_scheduler_configuration)?;
        let claim_key_prefix = claim_key_prefix.into();
        if claim_key_prefix.trim().is_empty() || claim_key_prefix.len() > 120 {
            return Err(WorkerRuntimeError::Configuration(
                "holding projection claim key prefix is invalid".into(),
            ));
        }
        if batch_size == 0 || batch_size > HOLDING_PROJECTION_SCHEDULER_MAX_BATCH_SIZE {
            return Err(WorkerRuntimeError::Configuration(format!(
                "holding projection batch size must be 1..={HOLDING_PROJECTION_SCHEDULER_MAX_BATCH_SIZE}"
            )));
        }
        if lease < Duration::seconds(1) || lease > Duration::seconds(3_600) {
            return Err(WorkerRuntimeError::Configuration(
                "holding projection lease is outside its bound".into(),
            ));
        }
        Ok(Self {
            source,
            executor,
            binding,
            claim_key_prefix,
            batch_size,
            lease,
        })
    }
}

#[async_trait]
impl<S, E> NamedWorkerHandler for HoldingProjectionSchedulerHandler<S, E>
where
    S: HoldingProjectionSchedulerSource,
    E: HoldingProjectionExecutor,
{
    fn name(&self) -> &str {
        HOLDING_PROJECTION_SCHEDULER_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "holding projection worker ID is required".into(),
            ));
        }
        let now = Utc::now();
        let request = ProjectionClaimRequest {
            binding: self.binding.clone(),
            projection_name: self.executor.projection_name_filter().map(str::to_owned),
            worker_id,
            claim_key: format!("{}:{worker_id}:{}", self.claim_key_prefix, Uuid::new_v4()),
            now,
            lease: self.lease,
            max_jobs: self.batch_size,
        };
        let claims = self
            .source
            .claim_due(request)
            .await
            .map_err(redacted_scheduler_error)?;
        let mut report = WorkerHandlerReport {
            claimed: claims.len() as u64,
            has_work: !claims.is_empty(),
            ..WorkerHandlerReport::default()
        };
        process_claims(
            &self.source,
            &self.executor,
            claims,
            &self.binding,
            worker_id,
            self.lease,
            &mut report,
        )
        .await?;
        Ok(report)
    }
}

#[derive(Debug, Clone)]
pub struct MultiHoldingProjectionSchedulerHandler<S, E, B> {
    source: S,
    executor: E,
    bindings: B,
    claim_key_prefix: String,
    batch_size: u32,
    lease: Duration,
}

impl<S, E, B> MultiHoldingProjectionSchedulerHandler<S, E, B>
where
    S: HoldingProjectionSchedulerSource,
    E: HoldingProjectionExecutor,
    B: HoldingProjectionBindingSource,
{
    pub fn new(
        source: S,
        executor: E,
        bindings: B,
        claim_key_prefix: impl Into<String>,
        batch_size: u32,
        lease: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        let claim_key_prefix = claim_key_prefix.into();
        if claim_key_prefix.trim().is_empty() || claim_key_prefix.len() > 120 {
            return Err(WorkerRuntimeError::Configuration(
                "holding projection claim key prefix is invalid".into(),
            ));
        }
        if batch_size == 0 || batch_size > HOLDING_PROJECTION_SCHEDULER_MAX_BATCH_SIZE {
            return Err(WorkerRuntimeError::Configuration(format!(
                "holding projection batch size must be 1..={HOLDING_PROJECTION_SCHEDULER_MAX_BATCH_SIZE}"
            )));
        }
        if lease < Duration::seconds(1) || lease > Duration::seconds(3_600) {
            return Err(WorkerRuntimeError::Configuration(
                "holding projection lease is outside its bound".into(),
            ));
        }
        Ok(Self {
            source,
            executor,
            bindings,
            claim_key_prefix,
            batch_size,
            lease,
        })
    }
}

#[async_trait]
impl<S, E, B> NamedWorkerHandler for MultiHoldingProjectionSchedulerHandler<S, E, B>
where
    S: HoldingProjectionSchedulerSource,
    E: HoldingProjectionExecutor,
    B: HoldingProjectionBindingSource,
{
    fn name(&self) -> &str {
        HOLDING_PROJECTION_SCHEDULER_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "holding projection worker ID is required".into(),
            ));
        }
        let bindings = self
            .bindings
            .active_bindings()
            .await
            .map_err(redacted_scheduler_error)?;
        let mut report = WorkerHandlerReport::default();
        for binding in bindings {
            let request = ProjectionClaimRequest {
                binding: binding.clone(),
                projection_name: self.executor.projection_name_filter().map(str::to_owned),
                worker_id,
                claim_key: format!("{}:{worker_id}:{}", self.claim_key_prefix, Uuid::new_v4()),
                now: Utc::now(),
                lease: self.lease,
                max_jobs: self.batch_size,
            };
            let claims = self
                .source
                .claim_due(request)
                .await
                .map_err(redacted_scheduler_error)?;
            report.claimed = report.claimed.saturating_add(claims.len() as u64);
            report.has_work |= !claims.is_empty();
            process_claims(
                &self.source,
                &self.executor,
                claims,
                &binding,
                worker_id,
                self.lease,
                &mut report,
            )
            .await?;
        }
        Ok(report)
    }
}

async fn process_claims<S, E>(
    source: &S,
    executor: &E,
    claims: Vec<ProjectionClaimRecord>,
    expected_binding: &HoldingHoldingBinding,
    worker_id: Uuid,
    lease: Duration,
    report: &mut WorkerHandlerReport,
) -> Result<(), WorkerRuntimeError>
where
    S: HoldingProjectionSchedulerSource,
    E: HoldingProjectionExecutor,
{
    for claim in claims {
        validate_claim(&claim, expected_binding, worker_id, Utc::now())?;
        if claim.replayed {
            continue;
        }
        match execute_with_lease_renewal(source, executor, &claim, lease).await? {
            Ok(()) => {
                source
                    .complete(
                        &claim.job.binding,
                        claim.job.id,
                        claim.lease.claim_id,
                        claim.lease.worker_id,
                        Utc::now(),
                    )
                    .await
                    .map_err(redacted_scheduler_error)?;
                report.processed = report.processed.saturating_add(1);
            }
            Err(error) => {
                let failure = match error {
                    HoldingProjectionExecutorError::Retryable => "projection_executor_retryable",
                    HoldingProjectionExecutorError::CandidateSourceMoved => {
                        "projection_executor_candidate_source_moved"
                    }
                    HoldingProjectionExecutorError::Permanent => "projection_executor_permanent",
                };
                let outcome =
                    if matches!(error, HoldingProjectionExecutorError::CandidateSourceMoved) {
                        source
                            .fail_terminal(
                                &claim.job.binding,
                                claim.job.id,
                                claim.lease.claim_id,
                                claim.lease.worker_id,
                                Utc::now(),
                                failure,
                            )
                            .await
                    } else {
                        source
                            .fail(
                                &claim.job.binding,
                                claim.job.id,
                                claim.lease.claim_id,
                                claim.lease.worker_id,
                                Utc::now(),
                                failure,
                            )
                            .await
                    }
                    .map_err(redacted_scheduler_error)?;
                report.processed = report.processed.saturating_add(1);
                match outcome {
                    ProjectionFailureOutcome::Retry(_) => {
                        report.retried = report.retried.saturating_add(1);
                    }
                    ProjectionFailureOutcome::DeadLetter(_) => {
                        report.dead_lettered = report.dead_lettered.saturating_add(1);
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_claim(
    claim: &ProjectionClaimRecord,
    expected_binding: &HoldingHoldingBinding,
    worker_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(), WorkerRuntimeError> {
    let binding_matches = claim.job.binding.matches(expected_binding);
    let attribution_valid = !claim.job.id.is_nil()
        && !claim.lease.claim_id.is_nil()
        && !claim.lease.worker_id.is_nil()
        && claim.lease.worker_id == worker_id
        && claim.lease.claimed_at <= now
        && claim.lease.lease_until > claim.lease.claimed_at;
    if !binding_matches || !attribution_valid {
        return Err(WorkerRuntimeError::Configuration(
            "holding projection claim scope or worker attribution is invalid".into(),
        ));
    }

    // A sealed batch may replay a claim after the durable job has already
    // transitioned, so only active, non-replayed claims require live lease
    // state and the job-to-claim ownership fields below.
    if claim.replayed {
        return Ok(());
    }
    if claim.job.state != ProjectionJobState::Claimed
        || claim.job.current_claim_id != Some(claim.lease.claim_id)
        || claim.job.worker_id != Some(worker_id)
        || claim.job.lease_until != Some(claim.lease.lease_until)
        || !claim.lease.is_active_at(now)
    {
        return Err(WorkerRuntimeError::Configuration(
            "holding projection claim lease is stale or no longer owned".into(),
        ));
    }
    Ok(())
}

async fn execute_with_lease_renewal<S, E>(
    source: &S,
    executor: &E,
    claim: &ProjectionClaimRecord,
    lease: Duration,
) -> Result<Result<(), HoldingProjectionExecutorError>, WorkerRuntimeError>
where
    S: HoldingProjectionSchedulerSource,
    E: HoldingProjectionExecutor,
{
    let half_lease_millis = u64::try_from(lease.num_milliseconds() / 2)
        .unwrap_or(500)
        .max(1);
    let mut renewal = tokio::time::interval(std::time::Duration::from_millis(half_lease_millis));
    // Tokio's first interval tick is immediate. Consume it so a short job is
    // not followed by an unnecessary renewal, and long jobs renew at half-life.
    renewal.tick().await;
    let execution = executor.execute(claim);
    tokio::pin!(execution);
    loop {
        tokio::select! {
            result = &mut execution => return Ok(result),
            _ = renewal.tick() => {
                source
                    .renew(
                        &claim.job.binding,
                        claim.job.id,
                        claim.lease.claim_id,
                        claim.lease.worker_id,
                        Utc::now(),
                        lease,
                    )
                    .await
                    .map_err(|_| {
                        tracing::error!(job_id = %claim.job.id, "holding projection lease renewal failed");
                        WorkerRuntimeError::Configuration(
                            "holding projection lease renewal failed".into(),
                        )
                    })?;
            }
        }
    }
}

fn redacted_scheduler_configuration(_error: HoldingProjectionSchedulerError) -> WorkerRuntimeError {
    WorkerRuntimeError::Configuration(
        "holding projection scheduler configuration is invalid".into(),
    )
}

fn redacted_scheduler_error(_error: HoldingProjectionSchedulerError) -> WorkerRuntimeError {
    tracing::error!("holding projection scheduler operation failed");
    WorkerRuntimeError::Configuration("holding projection scheduler operation failed".into())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use chrono::Duration;
    use erpx_infrastructure::{
        ProjectionJobKind, ProjectionJobRecord, ProjectionLease, ProjectionPolicy,
        ProjectionScheduleRequest,
    };

    use super::*;

    #[derive(Clone)]
    struct FakeSource {
        state: Arc<Mutex<FakeState>>,
    }

    struct FakeState {
        claims: Vec<ProjectionClaimRecord>,
        claim_requests: usize,
        completed: usize,
        renewals: usize,
        failures: Vec<ProjectionFailureOutcome>,
    }

    #[async_trait]
    impl HoldingProjectionSchedulerSource for FakeSource {
        async fn claim_due(
            &self,
            _request: ProjectionClaimRequest,
        ) -> Result<Vec<ProjectionClaimRecord>, HoldingProjectionSchedulerError> {
            self.state.lock().expect("source").claim_requests += 1;
            Ok(std::mem::take(
                &mut self.state.lock().expect("source").claims,
            ))
        }

        async fn complete(
            &self,
            _binding: &HoldingHoldingBinding,
            _job_id: Uuid,
            _claim_id: Uuid,
            _worker_id: Uuid,
            _now: DateTime<Utc>,
        ) -> Result<(), HoldingProjectionSchedulerError> {
            self.state.lock().expect("source").completed += 1;
            Ok(())
        }

        async fn fail(
            &self,
            _binding: &HoldingHoldingBinding,
            _job_id: Uuid,
            _claim_id: Uuid,
            _worker_id: Uuid,
            now: DateTime<Utc>,
            error: &str,
        ) -> Result<ProjectionFailureOutcome, HoldingProjectionSchedulerError> {
            let outcome = ProjectionFailureOutcome::Retry(erpx_infrastructure::RetryRecord {
                job_id: Uuid::new_v4(),
                binding: HoldingHoldingBinding::new("holding", "holding").expect("binding"),
                attempt: 1,
                next_available_at: now + Duration::seconds(1),
                backoff: Duration::seconds(1),
                error: error.into(),
            });
            self.state
                .lock()
                .expect("source")
                .failures
                .push(outcome.clone());
            Ok(outcome)
        }

        async fn renew(
            &self,
            _binding: &HoldingHoldingBinding,
            job_id: Uuid,
            claim_id: Uuid,
            worker_id: Uuid,
            now: DateTime<Utc>,
            lease: Duration,
        ) -> Result<ProjectionLease, HoldingProjectionSchedulerError> {
            self.state.lock().expect("source").renewals += 1;
            let _ = job_id;
            ProjectionLease::new(claim_id, worker_id, now, lease)
        }
    }

    #[derive(Clone)]
    struct FakeExecutor {
        result: Result<(), HoldingProjectionExecutorError>,
    }

    #[async_trait]
    impl HoldingProjectionExecutor for FakeExecutor {
        async fn execute(
            &self,
            _claim: &ProjectionClaimRecord,
        ) -> Result<(), HoldingProjectionExecutorError> {
            self.result
        }
    }

    #[derive(Clone)]
    struct DelayedExecutor {
        delay: std::time::Duration,
    }

    #[async_trait]
    impl HoldingProjectionExecutor for DelayedExecutor {
        async fn execute(
            &self,
            _claim: &ProjectionClaimRecord,
        ) -> Result<(), HoldingProjectionExecutorError> {
            tokio::time::sleep(self.delay).await;
            Ok(())
        }
    }

    #[derive(Clone)]
    struct FakeBindings {
        bindings: Vec<HoldingHoldingBinding>,
    }

    #[async_trait]
    impl HoldingProjectionBindingSource for FakeBindings {
        async fn active_bindings(
            &self,
        ) -> Result<Vec<HoldingHoldingBinding>, HoldingProjectionSchedulerError> {
            Ok(self.bindings.clone())
        }
    }

    fn test_worker_id() -> Uuid {
        Uuid::from_u128(42)
    }

    fn claim() -> ProjectionClaimRecord {
        let now = Utc::now();
        let binding = HoldingHoldingBinding::new("holding", "holding").expect("binding");
        let policy = ProjectionPolicy::new(10, 10, 3, 1, 10).expect("policy");
        let request = ProjectionScheduleRequest {
            binding,
            projection_name: "marketing".into(),
            job_key: "job-1".into(),
            kind: ProjectionJobKind::Incremental,
            available_at: now - Duration::seconds(1),
            policy,
            payload: serde_json::json!({"page": 1}),
        };
        let mut job = ProjectionJobRecord::initial(Uuid::new_v4(), &request, now).expect("job");
        let lease =
            ProjectionLease::new(Uuid::new_v4(), test_worker_id(), now, Duration::seconds(30))
                .expect("lease");
        job.claim(lease.clone(), now).expect("claim");
        ProjectionClaimRecord {
            claim_key: "claim-1".into(),
            job,
            lease,
            replayed: false,
        }
    }

    #[test]
    fn candidate_rebuild_payload_is_complete_and_generation_fenced() {
        let mut claim = claim();
        claim.job.kind = ProjectionJobKind::Rebuild;
        claim.job.payload = serde_json::json!({
            "candidate_read_database_key": "holding_candidate_read_1",
            "source_version": 42,
            "generation": 7,
        });
        assert_eq!(
            parse_candidate_rebuild_payload(&claim).expect("candidate payload"),
            Some(("holding_candidate_read_1".into(), 42, 7))
        );

        claim.job.payload = serde_json::json!({
            "candidate_read_database_key": "holding_candidate_read_1",
            "generation": 7,
        });
        assert_eq!(
            parse_candidate_rebuild_payload(&claim),
            Err(HoldingProjectionExecutorError::Permanent)
        );

        claim.job.payload = serde_json::json!({
            "candidate_read_database_key": "holding candidate",
            "source_version": 42,
            "generation": 7,
        });
        assert_eq!(
            parse_candidate_rebuild_payload(&claim),
            Err(HoldingProjectionExecutorError::Permanent)
        );

        claim.job.kind = ProjectionJobKind::Incremental;
        claim.job.payload = serde_json::json!({
            "candidate_read_database_key": "holding_candidate_read_1",
            "source_version": 42,
            "generation": 7,
        });
        assert_eq!(
            parse_candidate_rebuild_payload(&claim),
            Err(HoldingProjectionExecutorError::Permanent)
        );
    }

    #[test]
    fn untyped_rebuild_is_rejected_before_database_routing() {
        let mut claim = claim();
        claim.job.kind = ProjectionJobKind::Rebuild;
        claim.job.payload = serde_json::json!({"page": 1});

        assert_eq!(
            require_candidate_rebuild_payload(&claim),
            Err(HoldingProjectionExecutorError::Permanent)
        );
    }

    #[test]
    fn claim_validation_rejects_scope_owner_and_stale_lease_drift() {
        let expected_binding = HoldingHoldingBinding::new("holding", "holding").expect("binding");
        let valid = claim();
        let worker_id = valid.lease.worker_id;
        assert!(validate_claim(&valid, &expected_binding, worker_id, Utc::now()).is_ok());

        let mut wrong_scope = claim();
        wrong_scope.job.binding =
            HoldingHoldingBinding::new("other-holding", "holding").expect("binding");
        assert!(validate_claim(&wrong_scope, &expected_binding, worker_id, Utc::now()).is_err());

        let mut wrong_owner = claim();
        wrong_owner.lease.worker_id = Uuid::new_v4();
        assert!(validate_claim(&wrong_owner, &expected_binding, worker_id, Utc::now()).is_err());

        let mut stale = claim();
        let expired_at = Utc::now() - Duration::seconds(1);
        stale.lease.lease_until = expired_at;
        stale.job.lease_until = Some(expired_at);
        assert!(validate_claim(&stale, &expected_binding, worker_id, Utc::now()).is_err());
    }

    #[test]
    fn handler_rejects_unbounded_configuration() {
        let source = FakeSource {
            state: Arc::new(Mutex::new(FakeState {
                claims: vec![],
                claim_requests: 0,
                completed: 0,
                renewals: 0,
                failures: vec![],
            })),
        };
        let executor = FakeExecutor { result: Ok(()) };
        let binding = HoldingHoldingBinding::new("holding", "holding").expect("binding");
        assert!(
            HoldingProjectionSchedulerHandler::new(
                source.clone(),
                executor.clone(),
                binding.clone(),
                "claims",
                0,
                Duration::seconds(1),
            )
            .is_err()
        );
        assert!(
            HoldingProjectionSchedulerHandler::new(
                source,
                executor,
                binding,
                "claims",
                1,
                Duration::zero(),
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn handler_completes_injected_projection_job() {
        let source = FakeSource {
            state: Arc::new(Mutex::new(FakeState {
                claims: vec![claim()],
                claim_requests: 0,
                completed: 0,
                renewals: 0,
                failures: vec![],
            })),
        };
        let state = Arc::clone(&source.state);
        let handler = HoldingProjectionSchedulerHandler::new(
            source,
            FakeExecutor { result: Ok(()) },
            HoldingHoldingBinding::new("holding", "holding").expect("binding"),
            "claims",
            1,
            Duration::seconds(30),
        )
        .expect("handler");
        let report = handler.run_once(test_worker_id()).await.expect("run");
        assert_eq!(report.claimed, 1);
        assert_eq!(report.processed, 1);
        assert_eq!(state.lock().expect("state").completed, 1);
    }

    #[tokio::test]
    async fn long_projection_renews_claim_before_completion() {
        let source = FakeSource {
            state: Arc::new(Mutex::new(FakeState {
                claims: vec![claim()],
                claim_requests: 0,
                completed: 0,
                renewals: 0,
                failures: vec![],
            })),
        };
        let state = Arc::clone(&source.state);
        let handler = HoldingProjectionSchedulerHandler::new(
            source,
            DelayedExecutor {
                delay: std::time::Duration::from_millis(650),
            },
            HoldingHoldingBinding::new("holding", "holding").expect("binding"),
            "claims",
            1,
            Duration::seconds(1),
        )
        .expect("handler");
        let report = handler.run_once(test_worker_id()).await.expect("run");
        assert_eq!(report.processed, 1);
        let state = state.lock().expect("state");
        assert_eq!(state.completed, 1);
        assert!(state.renewals >= 1);
    }

    #[tokio::test]
    async fn handler_records_bounded_retry_outcome() {
        let source = FakeSource {
            state: Arc::new(Mutex::new(FakeState {
                claims: vec![claim()],
                claim_requests: 0,
                completed: 0,
                renewals: 0,
                failures: vec![],
            })),
        };
        let state = Arc::clone(&source.state);
        let handler = HoldingProjectionSchedulerHandler::new(
            source,
            FakeExecutor {
                result: Err(HoldingProjectionExecutorError::Retryable),
            },
            HoldingHoldingBinding::new("holding", "holding").expect("binding"),
            "claims",
            1,
            Duration::seconds(30),
        )
        .expect("handler");
        let report = handler.run_once(test_worker_id()).await.expect("run");
        assert_eq!(report.retried, 1);
        assert_eq!(state.lock().expect("state").failures.len(), 1);
        assert!(matches!(
            state.lock().expect("state").failures[0],
            ProjectionFailureOutcome::Retry(_)
        ));
    }

    #[tokio::test]
    async fn sealed_claim_replay_is_not_executed_again() {
        let mut replay = claim();
        replay.replayed = true;
        let source = FakeSource {
            state: Arc::new(Mutex::new(FakeState {
                claims: vec![replay],
                claim_requests: 0,
                completed: 0,
                renewals: 0,
                failures: vec![],
            })),
        };
        let state = Arc::clone(&source.state);
        let handler = HoldingProjectionSchedulerHandler::new(
            source,
            FakeExecutor {
                result: Err(HoldingProjectionExecutorError::Permanent),
            },
            HoldingHoldingBinding::new("holding", "holding").expect("binding"),
            "claims",
            1,
            Duration::seconds(30),
        )
        .expect("handler");
        let report = handler.run_once(test_worker_id()).await.expect("run");
        assert_eq!(report.claimed, 1);
        assert_eq!(report.processed, 0);
        assert_eq!(state.lock().expect("state").completed, 0);
        assert!(state.lock().expect("state").failures.is_empty());
    }

    #[tokio::test]
    async fn multi_holding_handler_enumerates_active_bindings_before_claiming() {
        let source = FakeSource {
            state: Arc::new(Mutex::new(FakeState {
                claims: vec![claim()],
                claim_requests: 0,
                completed: 0,
                renewals: 0,
                failures: vec![],
            })),
        };
        let state = Arc::clone(&source.state);
        let handler = MultiHoldingProjectionSchedulerHandler::new(
            source,
            FakeExecutor { result: Ok(()) },
            FakeBindings {
                bindings: vec![
                    HoldingHoldingBinding::new("holding", "holding").expect("binding"),
                    HoldingHoldingBinding::new("holding-b", "holding-b").expect("binding"),
                ],
            },
            "claims",
            1,
            Duration::seconds(30),
        )
        .expect("handler");
        let report = handler.run_once(test_worker_id()).await.expect("run");
        assert_eq!(report.claimed, 1);
        assert_eq!(report.processed, 1);
        let state = state.lock().expect("state");
        assert_eq!(state.completed, 1);
        assert_eq!(state.claim_requests, 2);
    }

    #[test]
    fn replacement_activation_supports_all_fenced_finance_projections() {
        assert_eq!(
            source_projection_for_name(erpx_operations_sqlx::EXCEPTION_PROJECTION),
            Some(HoldingSourceProjection::Operations)
        );
        assert_eq!(
            source_projection_for_name(erpx_infrastructure::FINANCE_BANKING_READ_PROJECTION),
            Some(HoldingSourceProjection::FinanceBanking)
        );
        assert_eq!(
            source_projection_for_name(erpx_infrastructure::FINANCE_SUBLEDGER_READ_PROJECTION),
            Some(HoldingSourceProjection::FinanceSubledger)
        );
        assert_eq!(
            source_projection_for_name(erpx_infrastructure::FINANCE_CLOSE_PROJECTION_NAME),
            Some(HoldingSourceProjection::FinanceClose)
        );
        assert_eq!(
            source_projection_for_name(erpx_integration_contracts::ITEM_DIRECTORY_PROJECTION_NAME),
            Some(HoldingSourceProjection::ItemDirectory)
        );
        assert_eq!(source_projection_for_name("unknown_projection"), None);
    }
}
