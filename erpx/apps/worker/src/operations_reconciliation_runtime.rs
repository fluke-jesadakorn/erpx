//! Fail-closed runtime boundary for serialized G7 Operations passes.
//!
//! The executable composition root owns handler registration. This module is
//! intentionally a child of the existing Operations completion module so it
//! can be verified without changing worker registration. It binds a serialized
//! pass to the current worker identity, validates the authoritative proof
//! before any SQLx work, and exposes reconciliation/rollback evidence rather
//! than treating a completed loop as success by itself.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use erpx_domain::operations::completion::OperationsWorkflowAction;
use erpx_infrastructure::{
    MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE, OPERATIONS_READ_PROJECTION, RoutedDatabaseSide,
    SqlxOperationsStore, TenantDatabaseRouter,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::{
    OperationsCompletionPass, OperationsCompletionRunReport, OperationsCompletionWorker,
    OperationsCompletionWorkerError,
};
use crate::{
    OperationsProjectionRebuildRequest, OperationsProjectionRebuildRunReport,
    OperationsProjectionRunner,
};

/// Hard bound for a serialized pass. Passes are control-plane capabilities,
/// not an unbounded transport channel; rejecting oversized input avoids a
/// memory-amplification path before deserialization.
pub const MAX_SERIALIZED_OPERATIONS_PASS_BYTES: usize = 256 * 1024;

/// A completion pass is a holding-scoped command. The underlying SQLx store
/// independently enforces the same requirement, but rejecting Platform and
/// descendant scopes here makes the runtime boundary fail closed before it
/// opens a tenant route.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OperationsReconciliationRuntimeError {
    #[error("serialized Operations pass is required")]
    MissingSerializedPass,
    #[error("serialized Operations pass exceeds the bounded input size")]
    SerializedPassTooLarge,
    #[error("serialized Operations pass is invalid")]
    InvalidSerializedPass,
    #[error("Operations pass worker identity does not match the current worker")]
    WorkerIdentityMismatch,
    #[error("Operations pass requires an explicit holding scope")]
    HoldingScopeRequired,
    #[error("Operations pass authorization was rejected")]
    AuthorizationRejected,
    #[error("Operations pass contains an invalid nested authorization")]
    NestedAuthorizationRejected,
    #[error("rollback requires a pass without lifecycle mutations")]
    RollbackPassContainsMutations,
    #[error("Operations completion execution failed")]
    CompletionFailed,
    #[error("Operations completion was dead-lettered after {attempts} attempts")]
    CompletionDeadLettered { attempts: u32 },
    #[error("Operations reconciliation is unhealthy")]
    ReconciliationUnhealthy,
    #[error("Operations projection rollback failed")]
    RollbackFailed,
    #[error("Operations projection rollback did not reconcile cleanly")]
    RollbackReconciliationUnhealthy,
    #[error("runtime metrics are unavailable")]
    MetricsUnavailable,
}

/// Per-pass reconciliation evidence. It is deliberately copied from the
/// worker report into a runtime-owned shape so callers cannot mistake handler
/// counters for authoritative health.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsRuntimeReconciliationMetrics {
    pub healthy: bool,
    pub missing_outbox_events: u64,
    pub stale_aggregate_count: u64,
    pub projection_lag_seconds: Option<i64>,
    #[serde(default)]
    pub folio_reconciliation_healthy: Option<bool>,
}

impl From<&OperationsCompletionRunReport> for OperationsRuntimeReconciliationMetrics {
    fn from(report: &OperationsCompletionRunReport) -> Self {
        Self {
            healthy: report.reconciliation.healthy,
            missing_outbox_events: report.reconciliation.missing_outbox_events,
            stale_aggregate_count: report.reconciliation.stale_aggregate_count,
            projection_lag_seconds: report.reconciliation.projection_lag_seconds,
            folio_reconciliation_healthy: report
                .reconciliation
                .folio_reconciliation
                .as_ref()
                .map(|evidence| evidence.healthy),
        }
    }
}

/// Rollback evidence is explicit about what happened. G7 never reverses
/// append-only Operations events. The only supported rollback action is an
/// authenticated rebuild of the disposable read projection; a failed normal
/// pass therefore records that operator rollback/rebuild is required instead
/// of claiming an automatic undo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsRuntimeRollbackMetrics {
    pub required: bool,
    pub attempted: bool,
    pub completed: bool,
    pub failed: bool,
    pub read_rows_cleared: u64,
    pub checkpoints_cleared: u64,
    pub events_replayed: u64,
}

impl OperationsRuntimeRollbackMetrics {
    const fn not_required() -> Self {
        Self {
            required: false,
            attempted: false,
            completed: false,
            failed: false,
            read_rows_cleared: 0,
            checkpoints_cleared: 0,
            events_replayed: 0,
        }
    }

    const fn required_without_attempt() -> Self {
        Self {
            required: true,
            attempted: false,
            completed: false,
            failed: false,
            read_rows_cleared: 0,
            checkpoints_cleared: 0,
            events_replayed: 0,
        }
    }
}

/// Cumulative process metrics. The values are counters only; no payload,
/// principal, tenant credential, or serialized capability is retained.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsRuntimeMetrics {
    pub attempted_passes: u64,
    pub authenticated_passes: u64,
    pub rejected_passes: u64,
    pub successful_passes: u64,
    pub failed_passes: u64,
    pub healthy_reconciliations: u64,
    pub unhealthy_reconciliations: u64,
    pub missing_outbox_events: u64,
    pub stale_aggregate_count: u64,
    pub projection_events_seen: u64,
    pub projection_pages: u64,
    #[serde(default)]
    pub detections_scheduled: u64,
    #[serde(default)]
    pub reassignments_scheduled: u64,
    #[serde(default)]
    pub escalations_scheduled: u64,
    #[serde(default)]
    pub detections_applied: u64,
    pub reassignments_applied: u64,
    pub escalations_applied: u64,
    pub rollback_required: u64,
    pub rollback_attempted: u64,
    pub rollback_completed: u64,
    pub rollback_failed: u64,
    pub rollback_read_rows_cleared: u64,
    pub rollback_checkpoints_cleared: u64,
    pub rollback_events_replayed: u64,
}

impl OperationsRuntimeMetrics {
    fn record_rejection(&mut self) {
        self.rejected_passes = self.rejected_passes.saturating_add(1);
    }

    fn record_success(&mut self, report: &OperationsCompletionRunReport) {
        self.successful_passes = self.successful_passes.saturating_add(1);
        self.projection_events_seen = self
            .projection_events_seen
            .saturating_add(report.projection_events_seen);
        self.projection_pages = self
            .projection_pages
            .saturating_add(report.projection_pages);
        self.detections_scheduled = self
            .detections_scheduled
            .saturating_add(report.detections_scheduled);
        self.reassignments_scheduled = self
            .reassignments_scheduled
            .saturating_add(report.reassignments_scheduled);
        self.escalations_scheduled = self
            .escalations_scheduled
            .saturating_add(report.escalations_scheduled);
        self.detections_applied = self
            .detections_applied
            .saturating_add(report.detections_applied);
        self.reassignments_applied = self
            .reassignments_applied
            .saturating_add(report.reassignments_applied);
        self.escalations_applied = self
            .escalations_applied
            .saturating_add(report.escalations_applied);
    }

    fn record_reconciliation(&mut self, reconciliation: &OperationsRuntimeReconciliationMetrics) {
        if reconciliation.healthy {
            self.healthy_reconciliations = self.healthy_reconciliations.saturating_add(1);
        } else {
            self.unhealthy_reconciliations = self.unhealthy_reconciliations.saturating_add(1);
        }
        self.missing_outbox_events = self
            .missing_outbox_events
            .saturating_add(reconciliation.missing_outbox_events);
        self.stale_aggregate_count = self
            .stale_aggregate_count
            .saturating_add(reconciliation.stale_aggregate_count);
    }

    fn record_rollback(&mut self, rollback: OperationsRuntimeRollbackMetrics) {
        if rollback.required {
            self.rollback_required = self.rollback_required.saturating_add(1);
        }
        if rollback.attempted {
            self.rollback_attempted = self.rollback_attempted.saturating_add(1);
        }
        if rollback.completed {
            self.rollback_completed = self.rollback_completed.saturating_add(1);
        }
        if rollback.failed {
            self.rollback_failed = self.rollback_failed.saturating_add(1);
        }
        self.rollback_read_rows_cleared = self
            .rollback_read_rows_cleared
            .saturating_add(rollback.read_rows_cleared);
        self.rollback_checkpoints_cleared = self
            .rollback_checkpoints_cleared
            .saturating_add(rollback.checkpoints_cleared);
        self.rollback_events_replayed = self
            .rollback_events_replayed
            .saturating_add(rollback.events_replayed);
    }
}

/// Result of one authenticated completion pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsRuntimePassReport {
    pub completion: OperationsCompletionRunReport,
    pub reconciliation: OperationsRuntimeReconciliationMetrics,
    pub rollback: OperationsRuntimeRollbackMetrics,
}

/// Result of an explicit, authenticated disposable read-model rollback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsRuntimeRollbackReport {
    pub projection_name: String,
    pub preparation_read_rows_cleared: u64,
    pub preparation_checkpoints_cleared: u64,
    pub events_replayed: u64,
    pub pages_read: u64,
    pub rollback: OperationsRuntimeRollbackMetrics,
}

/// Runtime adapter used by an integration boundary or scheduler. It is not a
/// handler registration; the composition root remains the coordinator-owned
/// concern. The router is retained only for the explicit read-model rollback.
#[derive(Debug, Clone)]
pub struct OperationsReconciliationRuntime {
    worker: OperationsCompletionWorker,
    router: TenantDatabaseRouter,
    metrics: Arc<Mutex<OperationsRuntimeMetrics>>,
}

impl OperationsReconciliationRuntime {
    pub fn new(router: TenantDatabaseRouter) -> Self {
        Self {
            worker: OperationsCompletionWorker::new(router.clone()),
            router,
            metrics: Arc::new(Mutex::new(OperationsRuntimeMetrics::default())),
        }
    }

    /// Decode and authenticate a pass without opening a tenant route. This is
    /// public so an integration test can prove the fail-closed boundary in
    /// isolation from PostgreSQL.
    pub fn authenticate_serialized_pass(
        serialized: &str,
        worker_id: Uuid,
        action: OperationsWorkflowAction,
    ) -> Result<OperationsCompletionPass, OperationsReconciliationRuntimeError> {
        Self::authenticate_serialized_pass_at(serialized, worker_id, action, Utc::now())
    }

    fn authenticate_serialized_pass_at(
        serialized: &str,
        worker_id: Uuid,
        action: OperationsWorkflowAction,
        now: DateTime<Utc>,
    ) -> Result<OperationsCompletionPass, OperationsReconciliationRuntimeError> {
        if serialized.trim().is_empty() {
            return Err(OperationsReconciliationRuntimeError::MissingSerializedPass);
        }
        if serialized.len() > MAX_SERIALIZED_OPERATIONS_PASS_BYTES {
            return Err(OperationsReconciliationRuntimeError::SerializedPassTooLarge);
        }
        if worker_id.is_nil() {
            return Err(OperationsReconciliationRuntimeError::WorkerIdentityMismatch);
        }
        let pass: OperationsCompletionPass = serde_json::from_str(serialized)
            .map_err(|_| OperationsReconciliationRuntimeError::InvalidSerializedPass)?;
        if pass.worker_id != worker_id {
            return Err(OperationsReconciliationRuntimeError::WorkerIdentityMismatch);
        }
        if !matches!(pass.scope, erpx_domain::access::Scope::Holding { .. }) {
            return Err(OperationsReconciliationRuntimeError::HoldingScopeRequired);
        }
        pass.validate_shape()
            .map_err(|_| OperationsReconciliationRuntimeError::InvalidSerializedPass)?;
        pass.validate_execution_time(now)
            .map_err(|_| OperationsReconciliationRuntimeError::AuthorizationRejected)?;
        pass.authorization
            .validate_for(action, &pass.scope, now)
            .map_err(|_| OperationsReconciliationRuntimeError::AuthorizationRejected)?;
        if action == OperationsWorkflowAction::Reconcile {
            pass.validate()
                .map_err(|_| OperationsReconciliationRuntimeError::InvalidSerializedPass)?;
            pass.validate_current_authorization(now)
                .map_err(|_| OperationsReconciliationRuntimeError::AuthorizationRejected)?;
        } else {
            Self::validate_nested_authorizations(&pass, now)?;
        }
        Ok(pass)
    }

    fn validate_nested_authorizations(
        pass: &OperationsCompletionPass,
        now: DateTime<Utc>,
    ) -> Result<(), OperationsReconciliationRuntimeError> {
        for request in &pass.reassignments {
            if request.at != pass.at
                || request.authorization.principal_ref != pass.authorization.principal_ref
                || request.manager_chain.scope != pass.scope
                || request.manager_chain.evaluated_at != pass.at
                || request
                    .authorization
                    .validate_for(
                        OperationsWorkflowAction::ReassignExpiredPrincipal,
                        &pass.scope,
                        now,
                    )
                    .is_err()
            {
                return Err(OperationsReconciliationRuntimeError::NestedAuthorizationRejected);
            }
        }
        for request in &pass.escalations {
            if request.at != pass.at
                || request.authorization.principal_ref != pass.authorization.principal_ref
                || request.manager_chain.scope != pass.scope
                || request.manager_chain.evaluated_at != pass.at
                || request
                    .authorization
                    .validate_for(OperationsWorkflowAction::EscalateSla, &pass.scope, now)
                    .is_err()
            {
                return Err(OperationsReconciliationRuntimeError::NestedAuthorizationRejected);
            }
        }
        Ok(())
    }

    fn update_metrics(
        &self,
        update: impl FnOnce(&mut OperationsRuntimeMetrics),
    ) -> Result<(), OperationsReconciliationRuntimeError> {
        let mut metrics = self
            .metrics
            .lock()
            .map_err(|_| OperationsReconciliationRuntimeError::MetricsUnavailable)?;
        update(&mut metrics);
        Ok(())
    }

    pub fn metrics(
        &self,
    ) -> Result<OperationsRuntimeMetrics, OperationsReconciliationRuntimeError> {
        self.metrics
            .lock()
            .map(|metrics| metrics.clone())
            .map_err(|_| OperationsReconciliationRuntimeError::MetricsUnavailable)
    }

    /// Execute one authenticated serialized completion pass. A non-healthy
    /// reconciliation is returned as an error, so a scheduler cannot mark the
    /// pass successful while the read model or outbox is stale.
    pub async fn run_serialized(
        &self,
        serialized: &str,
        worker_id: Uuid,
    ) -> Result<OperationsRuntimePassReport, OperationsReconciliationRuntimeError> {
        self.update_metrics(|metrics| {
            metrics.attempted_passes = metrics.attempted_passes.saturating_add(1);
        })?;
        let pass = match Self::authenticate_serialized_pass(
            serialized,
            worker_id,
            OperationsWorkflowAction::Reconcile,
        ) {
            Ok(pass) => pass,
            Err(error) => {
                self.update_metrics(OperationsRuntimeMetrics::record_rejection)?;
                return Err(error);
            }
        };
        self.update_metrics(|metrics| {
            metrics.authenticated_passes = metrics.authenticated_passes.saturating_add(1);
        })?;

        let completion = match self.worker.run_once(pass).await {
            Ok(report) => report,
            Err(error) => {
                self.update_metrics(|metrics| {
                    metrics.failed_passes = metrics.failed_passes.saturating_add(1);
                    metrics.record_rollback(
                        OperationsRuntimeRollbackMetrics::required_without_attempt(),
                    );
                })?;
                return Err(Self::map_worker_error(error));
            }
        };
        let reconciliation = OperationsRuntimeReconciliationMetrics::from(&completion);
        self.update_metrics(|metrics| metrics.record_reconciliation(&reconciliation))?;
        if !reconciliation.healthy {
            self.update_metrics(|metrics| {
                metrics.failed_passes = metrics.failed_passes.saturating_add(1);
                metrics
                    .record_rollback(OperationsRuntimeRollbackMetrics::required_without_attempt());
            })?;
            return Err(OperationsReconciliationRuntimeError::ReconciliationUnhealthy);
        }
        self.update_metrics(|metrics| metrics.record_success(&completion))?;
        Ok(OperationsRuntimePassReport {
            completion,
            reconciliation,
            rollback: OperationsRuntimeRollbackMetrics::not_required(),
        })
    }

    /// Explicitly rebuild only the disposable Operations read projection.
    /// This path is separate from normal completion and requires the same
    /// serialized capability to authorize `RebuildProjection`; it refuses any
    /// pass carrying lifecycle mutations, so rollback cannot secretly become a
    /// reassignment/escalation command.
    pub async fn rollback_serialized(
        &self,
        serialized: &str,
        worker_id: Uuid,
    ) -> Result<OperationsRuntimeRollbackReport, OperationsReconciliationRuntimeError> {
        let pass = Self::authenticate_serialized_pass(
            serialized,
            worker_id,
            OperationsWorkflowAction::RebuildProjection,
        )?;
        if rollback_pass_contains_mutations(&pass) {
            return Err(OperationsReconciliationRuntimeError::RollbackPassContainsMutations);
        }
        self.update_metrics(|metrics| {
            metrics.attempted_passes = metrics.attempted_passes.saturating_add(1);
            metrics.authenticated_passes = metrics.authenticated_passes.saturating_add(1);
            metrics.rollback_required = metrics.rollback_required.saturating_add(1);
            metrics.rollback_attempted = metrics.rollback_attempted.saturating_add(1);
        })?;

        // A rebuild is destructive only to the disposable read model. Before
        // clearing it, prove from a fresh write-side transaction that the
        // authoritative Operations outbox is complete and that every source
        // reference still links to its immutable Operations event. A stale
        // or corrupted write/source boundary is not repairable by rebuilding
        // a projection and must fail closed without clearing any read rows.
        if let Err(error) = self.verify_rollback_preconditions(&pass).await {
            self.update_metrics(|metrics| {
                metrics.rollback_failed = metrics.rollback_failed.saturating_add(1);
            })?;
            return Err(error);
        }

        let request = OperationsProjectionRebuildRequest::new(
            pass.tenant_id.clone(),
            pass.holding_ref.clone(),
            OPERATIONS_READ_PROJECTION,
        )
        .and_then(|request| request.with_page_size(pass.projection_page_size))
        .map_err(|_| OperationsReconciliationRuntimeError::RollbackFailed);
        let request = match request {
            Ok(request) => request,
            Err(error) => {
                self.update_metrics(|metrics| {
                    metrics.rollback_failed = metrics.rollback_failed.saturating_add(1);
                })?;
                return Err(error);
            }
        };
        let report: OperationsProjectionRebuildRunReport =
            match OperationsProjectionRunner::new(self.router.clone())
                .rebuild(request)
                .await
            {
                Ok(report) => report,
                Err(_) => {
                    self.update_metrics(|metrics| {
                        metrics.rollback_failed = metrics.rollback_failed.saturating_add(1);
                    })?;
                    return Err(OperationsReconciliationRuntimeError::RollbackFailed);
                }
            };
        if let Err(error) = self.verify_rollback_reconciliation(&pass).await {
            self.update_metrics(|metrics| {
                metrics.rollback_failed = metrics.rollback_failed.saturating_add(1);
            })?;
            return Err(error);
        }
        let rollback = OperationsRuntimeRollbackMetrics {
            required: true,
            attempted: true,
            completed: true,
            failed: false,
            read_rows_cleared: report.preparation.read_rows_cleared,
            checkpoints_cleared: report.preparation.checkpoints_cleared,
            events_replayed: report.events_replayed,
        };
        self.update_metrics(|metrics| {
            metrics.rollback_completed = metrics.rollback_completed.saturating_add(1);
            metrics.rollback_read_rows_cleared = metrics
                .rollback_read_rows_cleared
                .saturating_add(rollback.read_rows_cleared);
            metrics.rollback_checkpoints_cleared = metrics
                .rollback_checkpoints_cleared
                .saturating_add(rollback.checkpoints_cleared);
            metrics.rollback_events_replayed = metrics
                .rollback_events_replayed
                .saturating_add(rollback.events_replayed);
        })?;
        Ok(OperationsRuntimeRollbackReport {
            projection_name: report.projection_name,
            preparation_read_rows_cleared: report.preparation.read_rows_cleared,
            preparation_checkpoints_cleared: report.preparation.checkpoints_cleared,
            events_replayed: report.events_replayed,
            pages_read: report.pages_read,
            rollback,
        })
    }

    /// A projection rebuild is only a completed rollback after the same
    /// authoritative write/read reconciliation used by normal completion is
    /// green. The rebuild runner's page counters prove replay work happened,
    /// but cannot prove that an outbox gap, stale aggregate, or freshness
    /// violation was not left behind.
    async fn verify_rollback_reconciliation(
        &self,
        pass: &OperationsCompletionPass,
    ) -> Result<(), OperationsReconciliationRuntimeError> {
        let write = self
            .router
            .route(
                &pass.tenant_id,
                &pass.holding_ref,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| OperationsReconciliationRuntimeError::RollbackFailed)?;
        let read = match self
            .router
            .route(&pass.tenant_id, &pass.holding_ref, RoutedDatabaseSide::Read)
            .await
        {
            Ok(read) => read,
            Err(_) => {
                write.close().await;
                return Err(OperationsReconciliationRuntimeError::RollbackFailed);
            }
        };
        let result = async {
            let store = SqlxOperationsStore::from_pools(
                write.database_pool(),
                read.database_pool(),
                pass.holding_ref.clone(),
            )
            .map_err(|_| OperationsReconciliationRuntimeError::RollbackFailed)?;
            let report = store
                .reconciliation_report(&pass.authorization, &pass.scope, pass.at)
                .await
                .map_err(|_| OperationsReconciliationRuntimeError::RollbackFailed)?;
            let source = OperationsCompletionWorker::reconcile_source_events(
                &store,
                &pass.scope,
                pass.projection_page_size
                    .min(MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE),
                pass.max_projection_pages,
            )
            .await
            .map_err(|_| OperationsReconciliationRuntimeError::RollbackFailed)?;
            if !report.healthy || !source.source_event_reconciliation_healthy {
                Err(OperationsReconciliationRuntimeError::RollbackReconciliationUnhealthy)
            } else {
                Ok(())
            }
        }
        .await;
        write.close().await;
        read.close().await;
        result
    }

    /// Validate only the immutable write/source boundary before a rollback.
    /// The disposable read model is allowed to be stale here—the rebuild is
    /// what repairs it—but missing/tampered outbox rows or source links are
    /// authoritative corruption and cannot be made safe by replaying events.
    async fn verify_rollback_preconditions(
        &self,
        pass: &OperationsCompletionPass,
    ) -> Result<(), OperationsReconciliationRuntimeError> {
        let write = self
            .router
            .route(
                &pass.tenant_id,
                &pass.holding_ref,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| OperationsReconciliationRuntimeError::RollbackFailed)?;
        let read = match self
            .router
            .route(&pass.tenant_id, &pass.holding_ref, RoutedDatabaseSide::Read)
            .await
        {
            Ok(read) => read,
            Err(_) => {
                write.close().await;
                return Err(OperationsReconciliationRuntimeError::RollbackFailed);
            }
        };
        let result = async {
            let store = SqlxOperationsStore::from_pools(
                write.database_pool(),
                read.database_pool(),
                pass.holding_ref.clone(),
            )
            .map_err(|_| OperationsReconciliationRuntimeError::RollbackFailed)?;
            let report = store
                .reconciliation_report(&pass.authorization, &pass.scope, pass.at)
                .await
                .map_err(|_| OperationsReconciliationRuntimeError::RollbackFailed)?;
            if report.missing_outbox_events != 0 || report.outbox_mismatch_events != 0 {
                return Err(OperationsReconciliationRuntimeError::RollbackReconciliationUnhealthy);
            }
            let source = OperationsCompletionWorker::reconcile_source_events(
                &store,
                &pass.scope,
                pass.projection_page_size
                    .min(MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE),
                pass.max_projection_pages,
            )
            .await
            .map_err(|_| OperationsReconciliationRuntimeError::RollbackFailed)?;
            if source.source_event_reconciliation_healthy {
                Ok(())
            } else {
                Err(OperationsReconciliationRuntimeError::RollbackReconciliationUnhealthy)
            }
        }
        .await;
        write.close().await;
        read.close().await;
        result
    }

    fn map_worker_error(
        error: OperationsCompletionWorkerError,
    ) -> OperationsReconciliationRuntimeError {
        match error {
            OperationsCompletionWorkerError::Configuration(_) => {
                OperationsReconciliationRuntimeError::CompletionFailed
            }
            OperationsCompletionWorkerError::Failed {
                attempts,
                dead_lettered: true,
                ..
            } => OperationsReconciliationRuntimeError::CompletionDeadLettered { attempts },
            OperationsCompletionWorkerError::Failed { .. } => {
                OperationsReconciliationRuntimeError::CompletionFailed
            }
        }
    }
}

fn rollback_pass_contains_mutations(pass: &OperationsCompletionPass) -> bool {
    !pass.detections.is_empty()
        || pass.detection_schedule.is_some()
        || pass.reassignment_schedule.is_some()
        || pass.escalation_schedule.is_some()
        || !pass.reassignments.is_empty()
        || !pass.escalations.is_empty()
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use erpx_domain::{
        access::Scope,
        contracts::{ActorAttribution, ActorKind},
        operations::ExceptionPrincipalReference,
        operations::completion::{OperationsManagerChainProof, OperationsPermission},
    };

    use crate::OperationsDetectionSchedule;

    use super::*;

    fn pass(worker_id: Uuid, permission: OperationsPermission) -> OperationsCompletionPass {
        let at = Utc::now();
        let holding_id = Uuid::from_u128(0x7001);
        OperationsCompletionPass {
            tenant_id: "tenant-1".into(),
            holding_ref: "holding-1".into(),
            scope: Scope::Holding { holding_id },
            authorization: erpx_domain::operations::completion::OperationsAuthorizationProof {
                principal_ref: "operator-1".into(),
                permission,
                scope: Scope::Holding { holding_id },
                valid_from: at - Duration::minutes(1),
                valid_until: at + Duration::minutes(5),
                mfa_verified: true,
                break_glass: None,
            },
            at,
            worker_id,
            outbox_repair_limit: 10,
            projection_page_size: 10,
            max_projection_pages: 10,
            detections: Vec::new(),
            detection_schedule: None,
            reassignment_schedule: None,
            escalation_schedule: None,
            reassignments: Vec::new(),
            escalations: Vec::new(),
            folio_operations_baseline: None,
        }
    }

    fn empty_detection_schedule(pass: &OperationsCompletionPass) -> OperationsDetectionSchedule {
        let scope = pass.scope.clone();
        OperationsDetectionSchedule {
            rules: Vec::new(),
            authorization: pass.authorization.clone(),
            actor_attribution: ActorAttribution {
                principal_ref: pass.authorization.principal_ref.clone(),
                kind: ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            assignee: ExceptionPrincipalReference {
                principal_ref: "assignee".into(),
                valid_from: pass.at - Duration::minutes(1),
                valid_until: pass.at + Duration::minutes(5),
            },
            manager: ExceptionPrincipalReference {
                principal_ref: "manager".into(),
                valid_from: pass.at - Duration::minutes(1),
                valid_until: pass.at + Duration::minutes(5),
            },
            manager_chain: OperationsManagerChainProof {
                subject_principal_ref: "assignee".into(),
                manager_principal_ref: "manager".into(),
                scope,
                evaluated_at: pass.at,
                assignments: Vec::new(),
                authority_source_ref: "test".into(),
            },
            at: pass.at,
        }
    }

    #[test]
    fn authenticated_pass_is_bound_to_worker_and_reconcile_permission() {
        let worker_id = Uuid::from_u128(0x7002);
        let serialized = serde_json::to_string(&pass(worker_id, OperationsPermission::Reconcile))
            .expect("test pass serializes");
        let decoded = OperationsReconciliationRuntime::authenticate_serialized_pass(
            &serialized,
            worker_id,
            OperationsWorkflowAction::Reconcile,
        )
        .expect("valid serialized pass authenticates");
        assert_eq!(decoded.worker_id, worker_id);
        assert_eq!(decoded.authorization.principal_ref, "operator-1");
    }

    #[test]
    fn expired_serialized_pass_is_rejected_at_execution_time() {
        let worker_id = Uuid::from_u128(0x7002);
        let mut replayed = pass(worker_id, OperationsPermission::Reconcile);
        replayed.at = Utc::now() - Duration::hours(2);
        replayed.authorization.valid_from = replayed.at - Duration::minutes(1);
        replayed.authorization.valid_until = replayed.at + Duration::minutes(5);
        let serialized = serde_json::to_string(&replayed).expect("test pass serializes");

        assert!(matches!(
            OperationsReconciliationRuntime::authenticate_serialized_pass_at(
                &serialized,
                worker_id,
                OperationsWorkflowAction::Reconcile,
                Utc::now(),
            ),
            Err(OperationsReconciliationRuntimeError::AuthorizationRejected)
        ));
    }

    #[test]
    fn stale_but_not_expired_pass_is_rejected_at_execution_time() {
        let worker_id = Uuid::from_u128(0x7002);
        let now = Utc::now();
        let mut stale = pass(worker_id, OperationsPermission::Reconcile);
        stale.at = now - Duration::minutes(6);
        stale.authorization.valid_from = stale.at - Duration::minutes(1);
        stale.authorization.valid_until = now + Duration::minutes(1);
        let serialized = serde_json::to_string(&stale).expect("test pass serializes");

        assert!(matches!(
            OperationsReconciliationRuntime::authenticate_serialized_pass_at(
                &serialized,
                worker_id,
                OperationsWorkflowAction::Reconcile,
                now,
            ),
            Err(OperationsReconciliationRuntimeError::AuthorizationRejected)
        ));
    }

    #[test]
    fn wrong_worker_and_permission_fail_closed() {
        let worker_id = Uuid::from_u128(0x7002);
        let serialized = serde_json::to_string(&pass(worker_id, OperationsPermission::Read))
            .expect("test pass serializes");
        assert!(matches!(
            OperationsReconciliationRuntime::authenticate_serialized_pass(
                &serialized,
                Uuid::from_u128(0x7003),
                OperationsWorkflowAction::Reconcile,
            ),
            Err(OperationsReconciliationRuntimeError::WorkerIdentityMismatch)
        ));
        assert!(matches!(
            OperationsReconciliationRuntime::authenticate_serialized_pass(
                &serialized,
                worker_id,
                OperationsWorkflowAction::Reconcile,
            ),
            Err(OperationsReconciliationRuntimeError::AuthorizationRejected)
        ));
    }

    #[test]
    fn reconcile_authentication_rejects_invalid_schedule_before_execution() {
        let worker_id = Uuid::from_u128(0x7002);
        let mut invalid = pass(worker_id, OperationsPermission::Reconcile);
        invalid.detection_schedule = Some(empty_detection_schedule(&invalid));
        let serialized = serde_json::to_string(&invalid).expect("test pass serializes");

        assert!(matches!(
            OperationsReconciliationRuntime::authenticate_serialized_pass(
                &serialized,
                worker_id,
                OperationsWorkflowAction::Reconcile,
            ),
            Err(OperationsReconciliationRuntimeError::InvalidSerializedPass)
        ));
    }

    #[test]
    fn rollback_rejects_lifecycle_schedule_inputs() {
        let worker_id = Uuid::from_u128(0x7002);
        let mut rollback = pass(worker_id, OperationsPermission::Reconcile);
        rollback.detection_schedule = Some(empty_detection_schedule(&rollback));
        assert!(rollback_pass_contains_mutations(&rollback));
    }

    #[test]
    fn dead_letter_attempts_are_preserved_at_runtime_boundary() {
        let error = OperationsCompletionWorkerError::Failed {
            operation: "Operations projection",
            attempts: 5,
            retryable: false,
            dead_lettered: true,
        };
        assert_eq!(
            OperationsReconciliationRuntime::map_worker_error(error),
            OperationsReconciliationRuntimeError::CompletionDeadLettered { attempts: 5 }
        );
    }

    #[test]
    fn platform_scope_and_oversized_input_are_rejected_before_sqlx() {
        let worker_id = Uuid::from_u128(0x7002);
        let mut platform_pass = pass(worker_id, OperationsPermission::Reconcile);
        platform_pass.scope = Scope::Platform;
        let platform_serialized = serde_json::to_string(&platform_pass).expect("serializes");
        assert!(matches!(
            OperationsReconciliationRuntime::authenticate_serialized_pass(
                &platform_serialized,
                worker_id,
                OperationsWorkflowAction::Reconcile,
            ),
            Err(OperationsReconciliationRuntimeError::HoldingScopeRequired)
        ));
        let oversized = "{".repeat(MAX_SERIALIZED_OPERATIONS_PASS_BYTES + 1);
        assert!(matches!(
            OperationsReconciliationRuntime::authenticate_serialized_pass(
                &oversized,
                worker_id,
                OperationsWorkflowAction::Reconcile,
            ),
            Err(OperationsReconciliationRuntimeError::SerializedPassTooLarge)
        ));
    }

    #[test]
    fn rollback_metrics_are_explicit_and_append_only_safe() {
        let required = OperationsRuntimeRollbackMetrics::required_without_attempt();
        assert!(required.required);
        assert!(!required.attempted);
        assert!(!required.completed);
        assert_eq!(required.read_rows_cleared, 0);
        assert!(!OperationsRuntimeRollbackMetrics::not_required().required);
    }
}
