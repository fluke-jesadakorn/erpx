//! G8 Finance completion runtime and reconciliation adapter.
//!
//! This module is deliberately registration-ready rather than registered. The
//! worker composition root owns registration and is outside this packet's file
//! ownership. The adapter therefore exposes a narrow, auditable API that can
//! be composed by that root without duplicating holding routing or projection
//! behavior.
//!
//! The adapter reuses the existing Finance subledger reconciliation runner,
//! Finance close projection runner, and holding-routed subledger projector. It
//! never accepts a database URL or pool from a caller. Every operation is
//! bound to an authorization proof and a holding/holding pair before a runner
//! is invoked.

use chrono::{DateTime, Duration, Utc};
use erpx_domain::finance_subledgers::FinanceSubledgerReconciliationReport;
use erpx_infrastructure::{
    HoldingDatabaseRouter, HoldingProjectionSchedulerError, ProjectionClaimRecord,
    ProjectionJobKind, ProjectionJobState, ProjectionSchedulerAuthorization,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    FinanceCloseProjectionRequest, FinanceCloseProjectionRunReport, FinanceCloseProjectionRunner,
    FinanceCloseProjectionWorkerError, FinanceSubledgerHoldingProjectionExecutor,
    FinanceSubledgerProjectionWorkerError, FinanceSubledgerReconciliationRequest,
    FinanceSubledgerReconciliationRunner, FinanceSubledgerReconciliationWorkerError,
    HoldingProjectionExecutor, HoldingProjectionExecutorError,
};

/// The maximum lifetime of a completion authorization proof.
pub const FINANCE_COMPLETION_AUTHORIZATION_MAX_SECONDS: i64 = 300;
/// Maximum rows inspected by the authoritative subledger reconciliation.
pub const FINANCE_COMPLETION_MAX_RECONCILIATION_ROWS: u32 = 1_000;
/// Maximum source events in one close projection page.
pub const FINANCE_COMPLETION_MAX_PAGE_SIZE: u32 = 500;
/// Maximum close-projection pages in one reconciliation pass.
pub const FINANCE_COMPLETION_MAX_PROJECTION_PAGES: u32 = 1_000;
/// Maximum value retained for any rollback metric.
pub const FINANCE_COMPLETION_MAX_ROLLBACK_METRIC: u64 = 100_000;
/// Permission required for read-side reconciliation and bounded projection.
pub const FINANCE_COMPLETION_RECONCILE_PERMISSION: &str = "finance.reconcile";
/// Permission required for a read-model rollback/rebuild.
pub const FINANCE_COMPLETION_ROLLBACK_PERMISSION: &str = "finance.rollback";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceCompletionOperation {
    Reconcile,
    Rollback,
}

impl FinanceCompletionOperation {
    pub const fn permission(self) -> &'static str {
        match self {
            Self::Reconcile => FINANCE_COMPLETION_RECONCILE_PERMISSION,
            Self::Rollback => FINANCE_COMPLETION_ROLLBACK_PERMISSION,
        }
    }

    pub const fn requires_mfa(self) -> bool {
        matches!(self, Self::Rollback)
    }
}

/// A short-lived, scope-bound authorization handoff from the shared
/// authorization boundary.
///
/// The adapter does not resolve roles, accept bearer credentials, or fall
/// back to local authorization. `permission`, scope, lifetime, and MFA are
/// checked locally before the existing holding-routed runners are called.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCompletionAuthorizationProof {
    pub holding_id: String,
    pub data_profile_key: String,
    pub operation: FinanceCompletionOperation,
    pub permission: String,
    pub actor_ref: String,
    pub purpose: String,
    pub correlation_id: Uuid,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub mfa_verified: bool,
}

impl FinanceCompletionAuthorizationProof {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        operation: FinanceCompletionOperation,
        actor_ref: impl Into<String>,
        purpose: impl Into<String>,
        correlation_id: Uuid,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        mfa_verified: bool,
    ) -> Result<Self, FinanceCompletionRuntimeError> {
        let proof = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            operation,
            permission: operation.permission().into(),
            actor_ref: actor_ref.into(),
            purpose: purpose.into(),
            correlation_id,
            issued_at,
            expires_at,
            mfa_verified,
        };
        proof.validate_shape()?;
        Ok(proof)
    }

    fn validate_shape(&self) -> Result<(), FinanceCompletionRuntimeError> {
        validate_reference(&self.holding_id, "holding_id")?;
        validate_reference(&self.data_profile_key, "data_profile_key")?;
        validate_text(&self.actor_ref, "actor_ref", 255)?;
        validate_text(&self.purpose, "purpose", 128)?;
        if self.correlation_id.is_nil() {
            return Err(FinanceCompletionRuntimeError::Authorization(
                "authorization correlation is required".into(),
            ));
        }
        if self.permission != self.operation.permission() {
            return Err(FinanceCompletionRuntimeError::Authorization(
                "authorization permission does not match operation".into(),
            ));
        }
        let lifetime = self.expires_at - self.issued_at;
        if lifetime <= Duration::zero()
            || lifetime > Duration::seconds(FINANCE_COMPLETION_AUTHORIZATION_MAX_SECONDS)
        {
            return Err(FinanceCompletionRuntimeError::Authorization(
                "authorization lifetime is outside the completion bound".into(),
            ));
        }
        if self.operation.requires_mfa() && !self.mfa_verified {
            return Err(FinanceCompletionRuntimeError::Authorization(
                "rollback authorization requires MFA".into(),
            ));
        }
        Ok(())
    }

    /// Verify the proof against an operation and exact holding/holding scope,
    /// then produce the existing scheduler authorization handoff.
    pub fn authorize(
        &self,
        operation: FinanceCompletionOperation,
        holding_id: &str,
        data_profile_key: &str,
        at: DateTime<Utc>,
    ) -> Result<ProjectionSchedulerAuthorization, FinanceCompletionRuntimeError> {
        self.validate_shape()?;
        if self.operation != operation
            || self.permission != operation.permission()
            || self.holding_id != holding_id
            || self.data_profile_key != data_profile_key
        {
            return Err(FinanceCompletionRuntimeError::Authorization(
                "authorization proof is outside the requested finance scope".into(),
            ));
        }
        if at < self.issued_at || at >= self.expires_at {
            return Err(FinanceCompletionRuntimeError::Authorization(
                "authorization proof is expired or not yet valid".into(),
            ));
        }
        ProjectionSchedulerAuthorization::new(
            self.actor_ref.clone(),
            self.purpose.clone(),
            self.correlation_id,
        )
        .map_err(map_scheduler_authorization_error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCompletionRequest {
    pub holding_id: String,
    pub data_profile_key: String,
    pub reconciliation_max_rows: u32,
    pub page_size: u32,
    pub max_projection_pages: u32,
}

impl FinanceCompletionRequest {
    pub fn new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        reconciliation_max_rows: u32,
        page_size: u32,
    ) -> Result<Self, FinanceCompletionRuntimeError> {
        let request = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            reconciliation_max_rows,
            page_size,
            max_projection_pages: FINANCE_COMPLETION_MAX_PROJECTION_PAGES,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), FinanceCompletionRuntimeError> {
        validate_reference(&self.holding_id, "holding_id")?;
        validate_reference(&self.data_profile_key, "data_profile_key")?;
        if !(1..=FINANCE_COMPLETION_MAX_RECONCILIATION_ROWS).contains(&self.reconciliation_max_rows)
        {
            return Err(FinanceCompletionRuntimeError::Configuration(
                "reconciliation row bound is invalid".into(),
            ));
        }
        if !(1..=FINANCE_COMPLETION_MAX_PAGE_SIZE).contains(&self.page_size) {
            return Err(FinanceCompletionRuntimeError::Configuration(
                "projection page-size bound is invalid".into(),
            ));
        }
        if !(1..=FINANCE_COMPLETION_MAX_PROJECTION_PAGES).contains(&self.max_projection_pages) {
            return Err(FinanceCompletionRuntimeError::Configuration(
                "projection page budget is invalid".into(),
            ));
        }
        Ok(())
    }

    pub fn with_max_projection_pages(
        mut self,
        max_projection_pages: u32,
    ) -> Result<Self, FinanceCompletionRuntimeError> {
        self.max_projection_pages = max_projection_pages;
        self.validate()?;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCompletionReconciliationMetrics {
    pub holding_id: String,
    pub data_profile_key: String,
    pub source_aggregate_count: u64,
    pub read_row_count: u64,
    pub source_event_high_watermark: u64,
    pub source_stable: bool,
    pub complete: bool,
    pub issue_count: u32,
    pub issues_truncated: bool,
    pub close_projection_pages: u64,
    pub close_events_seen: u64,
    pub close_events_applied: u64,
    pub close_has_more: bool,
    pub healthy: bool,
}

impl FinanceCompletionReconciliationMetrics {
    fn from_reports(
        request: &FinanceCompletionRequest,
        subledger: FinanceSubledgerReconciliationReport,
        close: &CloseProjectionAccumulator,
    ) -> Self {
        let healthy = subledger.is_reconciled();
        let issue_count = u32::try_from(subledger.issues.len()).unwrap_or(u32::MAX);
        Self {
            holding_id: request.holding_id.clone(),
            data_profile_key: request.data_profile_key.clone(),
            source_aggregate_count: subledger.source_aggregate_count,
            read_row_count: subledger.read_row_count,
            source_event_high_watermark: subledger.source_event_high_watermark,
            source_stable: subledger.source_stable,
            complete: subledger.complete,
            issue_count,
            issues_truncated: subledger.issues_truncated,
            close_projection_pages: close.pages,
            close_events_seen: close.events_seen,
            close_events_applied: close.events_applied,
            close_has_more: close.has_more,
            healthy: healthy && !close.has_more,
        }
    }
}

#[derive(Debug, Default)]
struct CloseProjectionAccumulator {
    pages: u64,
    events_seen: u64,
    events_applied: u64,
    has_more: bool,
}

impl CloseProjectionAccumulator {
    fn record(&mut self, report: FinanceCloseProjectionRunReport) {
        self.pages = self.pages.saturating_add(1);
        self.events_seen = self.events_seen.saturating_add(report.events_seen);
        self.events_applied = self.events_applied.saturating_add(report.events_applied);
        self.has_more = report.has_more;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCompletionRollbackMetrics {
    pub holding_id: String,
    pub data_profile_key: String,
    pub close_events_replayed: u64,
    pub close_rows_cleared: u64,
    pub close_checkpoints_cleared: u64,
    pub subledger_projection_claimed: bool,
    pub metrics_truncated: bool,
    pub rollback_completed: bool,
}

impl FinanceCompletionRollbackMetrics {
    fn from_close_report(
        holding_id: &str,
        data_profile_key: &str,
        report: &crate::FinanceCloseProjectionRebuildReport,
    ) -> Self {
        let cap = FINANCE_COMPLETION_MAX_ROLLBACK_METRIC;
        let events_replayed = report.events_replayed.min(cap);
        let rows_cleared = report.preparation.rows_cleared.min(cap);
        let checkpoints_cleared = report.preparation.checkpoints_cleared.min(cap);
        Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            close_events_replayed: events_replayed,
            close_rows_cleared: rows_cleared,
            close_checkpoints_cleared: checkpoints_cleared,
            subledger_projection_claimed: false,
            metrics_truncated: report.events_replayed > cap
                || report.preparation.rows_cleared > cap
                || report.preparation.checkpoints_cleared > cap,
            rollback_completed: true,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FinanceCompletionRuntimeError {
    #[error("Finance completion configuration error: {0}")]
    Configuration(String),
    #[error("Finance completion authorization denied: {0}")]
    Authorization(String),
    #[error("Finance completion reconciliation failed")]
    Reconciliation,
    #[error("Finance completion rollback failed")]
    Rollback,
    #[error("Finance completion projection execution is retryable")]
    Retryable,
    #[error("Finance completion projection source moved during execution")]
    CandidateSourceMoved,
    #[error("Finance completion projection execution failed permanently")]
    Permanent,
    #[error("Finance completion projection claim is invalid: {0}")]
    InvalidProjectionClaim(String),
}

/// Runtime composition for the G8 completion pass.
///
/// The router is the only source of holding database handles. The existing
/// runners retain ownership of SQLx persistence, outbox/read-model behavior,
/// and projection fencing. This type only validates the request/proof and
/// translates their bounded reports into safe operational metrics.
#[derive(Debug, Clone)]
pub struct FinanceCompletionRuntimeAdapter {
    close_projection: FinanceCloseProjectionRunner,
    subledger_reconciliation: FinanceSubledgerReconciliationRunner,
    router: HoldingDatabaseRouter,
}

impl FinanceCompletionRuntimeAdapter {
    pub fn new(router: HoldingDatabaseRouter) -> Self {
        Self {
            close_projection: FinanceCloseProjectionRunner::new(router.clone()),
            subledger_reconciliation: FinanceSubledgerReconciliationRunner::new(router.clone()),
            router,
        }
    }

    /// Reconcile authoritative subledger aggregates and advance the existing
    /// close projection by one bounded page. The close projection remains
    /// disposable; only the subledger report determines source/read-model
    /// agreement.
    pub async fn reconcile(
        &self,
        request: FinanceCompletionRequest,
        proof: &FinanceCompletionAuthorizationProof,
        at: DateTime<Utc>,
    ) -> Result<FinanceCompletionReconciliationMetrics, FinanceCompletionRuntimeError> {
        request.validate()?;
        proof.authorize(
            FinanceCompletionOperation::Reconcile,
            &request.holding_id,
            &request.data_profile_key,
            at,
        )?;

        let subledger_request = FinanceSubledgerReconciliationRequest::new(
            request.holding_id.clone(),
            request.data_profile_key.clone(),
            request.reconciliation_max_rows,
        )
        .map_err(map_subledger_request_error)?;
        let subledger = self
            .subledger_reconciliation
            .reconcile(subledger_request)
            .await
            .map_err(map_subledger_reconciliation_error)?;

        let mut close = CloseProjectionAccumulator::default();
        let mut cursor = None;
        loop {
            if close.pages >= u64::from(request.max_projection_pages) {
                return Err(FinanceCompletionRuntimeError::Configuration(
                    "close projection page budget exhausted".into(),
                ));
            }
            let close_request = FinanceCloseProjectionRequest::new(
                request.holding_id.clone(),
                request.data_profile_key.clone(),
                cursor,
                request.page_size,
            )
            .map_err(map_close_projection_error)?;
            let report = self
                .close_projection
                .project_page(close_request)
                .await
                .map_err(map_close_projection_error)?;
            let has_more = report.has_more;
            let next_cursor = report.next_cursor.clone();
            close.record(report);
            if !has_more {
                break;
            }
            cursor = next_cursor;
            if cursor.is_none() {
                return Err(FinanceCompletionRuntimeError::Reconciliation);
            }
        }

        Ok(FinanceCompletionReconciliationMetrics::from_reports(
            &request, subledger, &close,
        ))
    }

    /// Rebuild the existing Finance close read projection under a source
    /// fence. This is the rollback path for the disposable read model; it
    /// never rewinds authoritative holding-write aggregates.
    pub async fn rollback_close_projection(
        &self,
        request: FinanceCompletionRequest,
        proof: &FinanceCompletionAuthorizationProof,
        source_version: Option<u64>,
        at: DateTime<Utc>,
    ) -> Result<FinanceCompletionRollbackMetrics, FinanceCompletionRuntimeError> {
        request.validate()?;
        proof.authorize(
            FinanceCompletionOperation::Rollback,
            &request.holding_id,
            &request.data_profile_key,
            at,
        )?;
        if source_version == Some(0) {
            return Err(FinanceCompletionRuntimeError::Configuration(
                "rollback source version must be positive".into(),
            ));
        }

        let report = match source_version {
            Some(source_version) => {
                self.close_projection
                    .rebuild_with_source_fence(
                        request.holding_id.clone(),
                        request.data_profile_key.clone(),
                        request.page_size,
                        source_version,
                    )
                    .await
            }
            None => {
                self.close_projection
                    .rebuild(
                        request.holding_id.clone(),
                        request.data_profile_key.clone(),
                        request.page_size,
                    )
                    .await
            }
        }
        .map_err(map_close_projection_error)?;

        Ok(FinanceCompletionRollbackMetrics::from_close_report(
            &request.holding_id,
            &request.data_profile_key,
            &report,
        ))
    }

    /// Execute an already-authorized, already-claimed subledger rebuild job.
    /// Claim ownership and holding routing remain enforced by the existing
    /// holding-routed projector; this adapter rejects a claim from another
    /// holding, a non-rebuild job, an inactive lease, or the wrong projection.
    pub async fn rollback_subledger_projection(
        &self,
        request: FinanceCompletionRequest,
        proof: &FinanceCompletionAuthorizationProof,
        claim: &ProjectionClaimRecord,
        at: DateTime<Utc>,
    ) -> Result<FinanceCompletionRollbackMetrics, FinanceCompletionRuntimeError> {
        request.validate()?;
        proof.authorize(
            FinanceCompletionOperation::Rollback,
            &request.holding_id,
            &request.data_profile_key,
            at,
        )?;
        validate_subledger_claim(claim, &request, at)?;

        let projector =
            FinanceSubledgerHoldingProjectionExecutor::new(self.router.clone(), request.page_size)
                .map_err(map_subledger_projection_error)?;
        projector
            .execute(claim)
            .await
            .map_err(map_projection_executor_error)?;

        Ok(FinanceCompletionRollbackMetrics {
            holding_id: request.holding_id,
            data_profile_key: request.data_profile_key,
            close_events_replayed: 0,
            close_rows_cleared: 0,
            close_checkpoints_cleared: 0,
            subledger_projection_claimed: true,
            metrics_truncated: false,
            rollback_completed: true,
        })
    }
}

fn validate_subledger_claim(
    claim: &ProjectionClaimRecord,
    request: &FinanceCompletionRequest,
    at: DateTime<Utc>,
) -> Result<(), FinanceCompletionRuntimeError> {
    if claim.replayed {
        return Err(FinanceCompletionRuntimeError::InvalidProjectionClaim(
            "replayed projection claims cannot execute twice".into(),
        ));
    }
    if claim.job.kind != ProjectionJobKind::Rebuild {
        return Err(FinanceCompletionRuntimeError::InvalidProjectionClaim(
            "rollback requires a rebuild claim".into(),
        ));
    }
    if claim.job.projection_name != crate::FINANCE_SUBLEDGER_PROJECTION_NAME {
        return Err(FinanceCompletionRuntimeError::InvalidProjectionClaim(
            "claim projection does not match Finance subledger".into(),
        ));
    }
    if claim.job.binding.holding_id != request.holding_id
        || claim.job.binding.data_profile_key != request.data_profile_key
    {
        return Err(FinanceCompletionRuntimeError::InvalidProjectionClaim(
            "claim holding/holding does not match request".into(),
        ));
    }
    if claim.lease.claim_id.is_nil()
        || claim.lease.worker_id.is_nil()
        || claim.job.state != ProjectionJobState::Claimed
        || claim.job.current_claim_id != Some(claim.lease.claim_id)
        || claim.job.worker_id != Some(claim.lease.worker_id)
        || claim.job.lease_until != Some(claim.lease.lease_until)
        || at < claim.lease.claimed_at
        || !claim.lease.is_active_at(at)
    {
        return Err(FinanceCompletionRuntimeError::InvalidProjectionClaim(
            "projection claim lease is inactive or no longer owned".into(),
        ));
    }
    if claim.job.policy.page_size == 0
        || claim.job.policy.page_size > FINANCE_COMPLETION_MAX_PAGE_SIZE
    {
        return Err(FinanceCompletionRuntimeError::InvalidProjectionClaim(
            "claim page size is outside the completion bound".into(),
        ));
    }
    Ok(())
}

fn validate_reference(
    value: &str,
    field: &'static str,
) -> Result<(), FinanceCompletionRuntimeError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(FinanceCompletionRuntimeError::Configuration(format!(
            "{field} is invalid"
        )));
    }
    Ok(())
}

fn validate_text(
    value: &str,
    field: &'static str,
    max_len: usize,
) -> Result<(), FinanceCompletionRuntimeError> {
    if value.trim().is_empty() || value.len() > max_len || value.chars().any(char::is_control) {
        return Err(FinanceCompletionRuntimeError::Authorization(format!(
            "{field} is invalid"
        )));
    }
    Ok(())
}

fn map_scheduler_authorization_error(
    error: HoldingProjectionSchedulerError,
) -> FinanceCompletionRuntimeError {
    FinanceCompletionRuntimeError::Authorization(match error {
        HoldingProjectionSchedulerError::InvalidRequest(message) => message,
        _ => "scheduler authorization is invalid".into(),
    })
}

fn map_subledger_request_error(
    error: FinanceSubledgerReconciliationWorkerError,
) -> FinanceCompletionRuntimeError {
    match error {
        FinanceSubledgerReconciliationWorkerError::Configuration(message) => {
            FinanceCompletionRuntimeError::Configuration(message)
        }
        FinanceSubledgerReconciliationWorkerError::Reconciliation { .. } => {
            FinanceCompletionRuntimeError::Reconciliation
        }
    }
}

fn map_subledger_reconciliation_error(
    error: FinanceSubledgerReconciliationWorkerError,
) -> FinanceCompletionRuntimeError {
    match error {
        FinanceSubledgerReconciliationWorkerError::Configuration(message) => {
            FinanceCompletionRuntimeError::Configuration(message)
        }
        FinanceSubledgerReconciliationWorkerError::Reconciliation { .. } => {
            FinanceCompletionRuntimeError::Reconciliation
        }
    }
}

fn map_close_projection_error(
    error: FinanceCloseProjectionWorkerError,
) -> FinanceCompletionRuntimeError {
    match error {
        FinanceCloseProjectionWorkerError::Configuration(message) => {
            FinanceCompletionRuntimeError::Configuration(message)
        }
        FinanceCloseProjectionWorkerError::Projection { .. }
        | FinanceCloseProjectionWorkerError::CandidateSourceMoved => {
            FinanceCompletionRuntimeError::Rollback
        }
    }
}

fn map_subledger_projection_error(
    error: FinanceSubledgerProjectionWorkerError,
) -> FinanceCompletionRuntimeError {
    match error {
        FinanceSubledgerProjectionWorkerError::Configuration(message) => {
            FinanceCompletionRuntimeError::Configuration(message)
        }
        FinanceSubledgerProjectionWorkerError::Projection { .. } => {
            FinanceCompletionRuntimeError::Rollback
        }
    }
}

fn map_projection_executor_error(
    error: HoldingProjectionExecutorError,
) -> FinanceCompletionRuntimeError {
    match error {
        HoldingProjectionExecutorError::Retryable => FinanceCompletionRuntimeError::Retryable,
        HoldingProjectionExecutorError::CandidateSourceMoved => {
            FinanceCompletionRuntimeError::CandidateSourceMoved
        }
        HoldingProjectionExecutorError::Permanent => FinanceCompletionRuntimeError::Permanent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use erpx_infrastructure::{
        FinanceCloseProjectionRebuildPreparation, ProjectionJobRecord, ProjectionLease,
        ProjectionPolicy, ProjectionScheduleRequest,
    };

    fn request() -> FinanceCompletionRequest {
        FinanceCompletionRequest::new("holding-1", "holding-1", 100, 50).expect("request")
    }

    fn proof(
        operation: FinanceCompletionOperation,
        mfa_verified: bool,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> FinanceCompletionAuthorizationProof {
        FinanceCompletionAuthorizationProof::new(
            "holding-1",
            "holding-1",
            operation,
            "worker/finance-completion",
            "g8-completion",
            Uuid::new_v4(),
            issued_at,
            expires_at,
            mfa_verified,
        )
        .expect("proof")
    }

    #[test]
    fn request_bounds_are_enforced() {
        assert!(FinanceCompletionRequest::new("holding/1", "holding-1", 1, 1).is_err());
        assert!(FinanceCompletionRequest::new("holding-1", "holding-1", 0, 1).is_err());
        assert!(
            FinanceCompletionRequest::new(
                "holding-1",
                "holding-1",
                FINANCE_COMPLETION_MAX_RECONCILIATION_ROWS + 1,
                1,
            )
            .is_err()
        );
        assert!(
            FinanceCompletionRequest::new(
                "holding-1",
                "holding-1",
                1,
                FINANCE_COMPLETION_MAX_PAGE_SIZE + 1,
            )
            .is_err()
        );
    }

    #[test]
    fn proof_requires_exact_scope_permission_and_mfa_for_rollback() {
        let issued = Utc.timestamp_opt(1_000, 0).single().expect("time");
        let expires = issued + Duration::seconds(60);
        let reconcile = proof(
            FinanceCompletionOperation::Reconcile,
            false,
            issued,
            expires,
        );
        assert!(
            reconcile
                .authorize(
                    FinanceCompletionOperation::Reconcile,
                    "holding-1",
                    "holding-1",
                    issued + Duration::seconds(1),
                )
                .is_ok()
        );
        assert!(
            reconcile
                .authorize(
                    FinanceCompletionOperation::Reconcile,
                    "holding-2",
                    "holding-1",
                    issued + Duration::seconds(1),
                )
                .is_err()
        );
        assert!(
            FinanceCompletionAuthorizationProof::new(
                "holding-1",
                "holding-1",
                FinanceCompletionOperation::Rollback,
                "worker/finance-completion",
                "g8-completion",
                Uuid::new_v4(),
                issued,
                expires,
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn proof_lifetime_and_expiry_are_bounded() {
        let issued = Utc.timestamp_opt(1_000, 0).single().expect("time");
        assert!(
            FinanceCompletionAuthorizationProof::new(
                "holding-1",
                "holding-1",
                FinanceCompletionOperation::Reconcile,
                "worker",
                "g8",
                Uuid::new_v4(),
                issued,
                issued + Duration::seconds(FINANCE_COMPLETION_AUTHORIZATION_MAX_SECONDS + 1),
                false,
            )
            .is_err()
        );
        let proof = proof(
            FinanceCompletionOperation::Reconcile,
            false,
            issued,
            issued + Duration::seconds(60),
        );
        assert!(
            proof
                .authorize(
                    FinanceCompletionOperation::Reconcile,
                    "holding-1",
                    "holding-1",
                    issued + Duration::seconds(60),
                )
                .is_err()
        );
    }

    #[test]
    fn rollback_metrics_are_capped_without_exposing_payloads() {
        let report = crate::FinanceCloseProjectionRebuildReport {
            holding_id: "holding-1".into(),
            data_profile_key: "holding-1".into(),
            events_replayed: FINANCE_COMPLETION_MAX_ROLLBACK_METRIC + 7,
            preparation: FinanceCloseProjectionRebuildPreparation {
                data_profile_key: "holding-1".into(),
                rows_cleared: FINANCE_COMPLETION_MAX_ROLLBACK_METRIC + 1,
                checkpoints_cleared: 2,
            },
        };
        let metrics =
            FinanceCompletionRollbackMetrics::from_close_report("holding-1", "holding-1", &report);
        assert_eq!(
            metrics.close_events_replayed,
            FINANCE_COMPLETION_MAX_ROLLBACK_METRIC
        );
        assert_eq!(
            metrics.close_rows_cleared,
            FINANCE_COMPLETION_MAX_ROLLBACK_METRIC
        );
        assert!(metrics.metrics_truncated);
        assert!(
            !serde_json::to_string(&metrics)
                .expect("metrics json")
                .contains("payload")
        );
    }

    #[test]
    fn subledger_claim_must_be_rebuild_and_scope_bound() {
        let now = Utc::now();
        let binding = erpx_infrastructure::HoldingHoldingBinding::new("holding-1", "holding-1")
            .expect("binding");
        let policy = ProjectionPolicy::new(50, 50, 3, 1, 10).expect("policy");
        let schedule = ProjectionScheduleRequest {
            binding,
            projection_name: crate::FINANCE_SUBLEDGER_PROJECTION_NAME.into(),
            job_key: "finance-subledger-rollback".into(),
            kind: ProjectionJobKind::Rebuild,
            available_at: now,
            policy,
            payload: serde_json::json!({}),
        };
        let mut job = ProjectionJobRecord::initial(Uuid::new_v4(), &schedule, now).expect("job");
        let lease =
            ProjectionLease::new(Uuid::new_v4(), Uuid::new_v4(), now, Duration::seconds(30))
                .expect("lease");
        job.claim(lease.clone(), now).expect("claim");
        let claim = ProjectionClaimRecord {
            claim_key: "finance-subledger-rollback".into(),
            job,
            lease,
            replayed: false,
        };
        assert!(validate_subledger_claim(&claim, &request(), now).is_ok());
        let wrong_scope =
            FinanceCompletionRequest::new("holding-2", "holding-1", 100, 50).expect("wrong scope");
        assert!(validate_subledger_claim(&claim, &wrong_scope, now).is_err());
    }

    #[test]
    fn subledger_claim_rejects_replays_and_stale_lease_ownership() {
        let now = Utc::now();
        let binding = erpx_infrastructure::HoldingHoldingBinding::new("holding-1", "holding-1")
            .expect("binding");
        let policy = ProjectionPolicy::new(50, 50, 3, 1, 10).expect("policy");
        let schedule = ProjectionScheduleRequest {
            binding,
            projection_name: crate::FINANCE_SUBLEDGER_PROJECTION_NAME.into(),
            job_key: "finance-subledger-rollback-replay".into(),
            kind: ProjectionJobKind::Rebuild,
            available_at: now,
            policy,
            payload: serde_json::json!({}),
        };
        let mut job = ProjectionJobRecord::initial(Uuid::new_v4(), &schedule, now).expect("job");
        let lease =
            ProjectionLease::new(Uuid::new_v4(), Uuid::new_v4(), now, Duration::seconds(30))
                .expect("lease");
        job.claim(lease.clone(), now).expect("claim");
        let claim = ProjectionClaimRecord {
            claim_key: "finance-subledger-rollback-replay".into(),
            job,
            lease,
            replayed: false,
        };

        let mut replayed = claim.clone();
        replayed.replayed = true;
        assert!(validate_subledger_claim(&replayed, &request(), now).is_err());

        let mut stale_owner = claim.clone();
        stale_owner.job.current_claim_id = Some(Uuid::new_v4());
        assert!(validate_subledger_claim(&stale_owner, &request(), now).is_err());

        let before_claim = claim.lease.claimed_at - Duration::seconds(1);
        assert!(validate_subledger_claim(&claim, &request(), before_claim).is_err());
    }

    #[test]
    fn projection_executor_failures_preserve_retry_and_source_fence_outcomes() {
        assert_eq!(
            map_projection_executor_error(HoldingProjectionExecutorError::Retryable),
            FinanceCompletionRuntimeError::Retryable
        );
        assert_eq!(
            map_projection_executor_error(HoldingProjectionExecutorError::CandidateSourceMoved),
            FinanceCompletionRuntimeError::CandidateSourceMoved
        );
        assert_eq!(
            map_projection_executor_error(HoldingProjectionExecutorError::Permanent),
            FinanceCompletionRuntimeError::Permanent
        );
    }
}
