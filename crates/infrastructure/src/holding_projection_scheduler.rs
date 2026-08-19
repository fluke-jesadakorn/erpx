//! Durable scheduling contract for per-holding projection work.
//!
//! This module is intentionally self-contained so it can be integrated by the
//! control-plane composition root after the scheduler migration is applied.
//! A scheduler row is never a source-of-truth business record: it only
//! coordinates bounded projection work against a holding's write/read pair.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::{collections::BTreeMap, sync::Arc};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_PROJECTION_PAGE_SIZE: u32 = 500;
pub const MAX_PROJECTION_BATCH_SIZE: u32 = 100;
pub const MAX_PROJECTION_ATTEMPTS: u16 = 25;
pub const MIN_LEASE_SECONDS: u64 = 1;
pub const MAX_LEASE_SECONDS: u64 = 3_600;
pub const MIN_BACKOFF_SECONDS: u64 = 1;
pub const MAX_BACKOFF_SECONDS: u64 = 86_400;
pub const MAX_CLAIM_KEY_LENGTH: usize = 200;
pub const MAX_PROJECTION_NAME_LENGTH: usize = 120;
pub const MAX_JOB_KEY_LENGTH: usize = 200;
pub const MAX_SCOPE_REFERENCE_LENGTH: usize = 200;
pub const MAX_PROJECTION_PAYLOAD_BYTES: usize = 64 * 1024;
pub const MAX_PROJECTION_ERROR_BYTES: usize = 1_000;
pub const MAX_PROJECTION_STATUS_PAGE_SIZE: u32 = 100;
pub const MAX_PROJECTION_STATUS_FRESHNESS_SECONDS: i64 = 300;
pub const HOLDING_PROJECTION_PERMISSION: &str = "holding.projection.execute";
/// current holding projections whose initial jobs must reach a terminal
/// success before a freshly bootstrapped Server can accept traffic.
pub const FRESH_INITIAL_HOLDING_PROJECTIONS: [&str; 26] = [
    "quality.inspections",
    "identity_access.principals",
    "identity_access.roles",
    "organization.nodes",
    "commercial.subscriptions",
    "commercial.entitlements",
    "marketing.lead_candidates",
    "marketing.campaign_budgets",
    "crm.leads",
    "crm.opportunities",
    "procurement.purchase_orders",
    "inventory.stock_items",
    "assets.assets",
    "maintenance.work_orders",
    "maintenance.asset_facts",
    "sales.quotes",
    "sales.orders",
    "sales.revenue_arrangements",
    "finance.customer_invoices",
    "finance.revenue_schedules",
    "people_payroll.employees",
    "people_payroll.leave_requests",
    "people_payroll.expense_claims",
    "people_payroll.payroll_runs",
    "pos.sales",
    "pos.refunds",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionSchedulerAuthorization {
    pub actor_ref: String,
    pub purpose: String,
    pub correlation_id: Uuid,
}

impl ProjectionSchedulerAuthorization {
    pub fn new(
        actor_ref: impl Into<String>,
        purpose: impl Into<String>,
        correlation_id: Uuid,
    ) -> Result<Self, HoldingProjectionSchedulerError> {
        let authorization = Self {
            actor_ref: actor_ref.into(),
            purpose: purpose.into(),
            correlation_id,
        };
        if authorization.actor_ref.trim().is_empty()
            || authorization.actor_ref.len() > 255
            || authorization.actor_ref.chars().any(char::is_control)
        {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "scheduler authorization actor is invalid".into(),
            ));
        }
        if authorization.purpose.trim().is_empty()
            || authorization.purpose.len() > 128
            || authorization.purpose.chars().any(char::is_control)
        {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "scheduler authorization purpose is invalid".into(),
            ));
        }
        if authorization.correlation_id == Uuid::nil() {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "scheduler authorization correlation is required".into(),
            ));
        }
        Ok(authorization)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HoldingProjectionSchedulerError {
    #[error("invalid scheduler request: {0}")]
    InvalidRequest(String),
    #[error("scheduler database error: {0}")]
    Database(String),
    #[error("scheduler serialization error: {0}")]
    Serialization(String),
    #[error("scheduler state transition rejected: {0}")]
    InvalidTransition(&'static str),
    #[error("claim key was already reserved by another worker or scope")]
    ClaimKeyConflict,
    #[error("schedule key was already used with a different request")]
    ScheduleIdempotencyConflict,
    #[error("projection claim lease is no longer owned by this worker")]
    ClaimLost,
    #[error("scheduler record was not found")]
    NotFound,
}

impl From<sqlx::Error> for HoldingProjectionSchedulerError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error.to_string())
    }
}

impl From<serde_json::Error> for HoldingProjectionSchedulerError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldingHoldingBinding {
    pub holding_id: String,
    pub data_profile_key: String,
}

impl HoldingHoldingBinding {
    pub fn new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, HoldingProjectionSchedulerError> {
        let binding = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
        };
        binding.validate()?;
        Ok(binding)
    }

    pub fn validate(&self) -> Result<(), HoldingProjectionSchedulerError> {
        validate_scope_ref(&self.holding_id, "holding_id")?;
        validate_scope_ref(&self.data_profile_key, "data_profile_key")
    }

    pub fn matches(&self, other: &Self) -> bool {
        self == other
    }

    pub fn require_match(&self, other: &Self) -> Result<(), HoldingProjectionSchedulerError> {
        if self.matches(other) {
            Ok(())
        } else {
            Err(HoldingProjectionSchedulerError::InvalidRequest(
                "holding and holding binding mismatch".into(),
            ))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionJobKind {
    Incremental,
    Rebuild,
}

impl ProjectionJobKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Incremental => "incremental",
            Self::Rebuild => "rebuild",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, HoldingProjectionSchedulerError> {
        match value {
            "incremental" => Ok(Self::Incremental),
            "rebuild" => Ok(Self::Rebuild),
            _ => Err(HoldingProjectionSchedulerError::InvalidRequest(
                "persisted projection job kind is invalid".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionJobState {
    Pending,
    Claimed,
    Retryable,
    Succeeded,
    DeadLetter,
    RebuildPending,
    RebuildSucceeded,
}

impl ProjectionJobState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Claimed => "claimed",
            Self::Retryable => "retryable",
            Self::Succeeded => "succeeded",
            Self::DeadLetter => "dead_letter",
            Self::RebuildPending => "rebuild_pending",
            Self::RebuildSucceeded => "rebuild_succeeded",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, HoldingProjectionSchedulerError> {
        match value {
            "pending" => Ok(Self::Pending),
            "claimed" => Ok(Self::Claimed),
            "retryable" => Ok(Self::Retryable),
            "succeeded" => Ok(Self::Succeeded),
            "dead_letter" => Ok(Self::DeadLetter),
            "rebuild_pending" => Ok(Self::RebuildPending),
            "rebuild_succeeded" => Ok(Self::RebuildSucceeded),
            _ => Err(HoldingProjectionSchedulerError::InvalidRequest(
                "persisted projection job state is invalid".into(),
            )),
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::DeadLetter | Self::RebuildSucceeded
        )
    }

    pub fn is_claimable(self) -> bool {
        matches!(self, Self::Pending | Self::Retryable | Self::RebuildPending)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionPolicy {
    pub page_size: u32,
    pub batch_size: u32,
    pub max_attempts: u16,
    pub backoff_base_seconds: u64,
    pub backoff_max_seconds: u64,
}

impl ProjectionPolicy {
    pub fn new(
        page_size: u32,
        batch_size: u32,
        max_attempts: u16,
        backoff_base_seconds: u64,
        backoff_max_seconds: u64,
    ) -> Result<Self, HoldingProjectionSchedulerError> {
        if !(1..=MAX_PROJECTION_PAGE_SIZE).contains(&page_size) {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "page_size is outside the scheduler bound".into(),
            ));
        }
        if !(1..=MAX_PROJECTION_BATCH_SIZE).contains(&batch_size) {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "batch_size is outside the scheduler bound".into(),
            ));
        }
        if !(1..=MAX_PROJECTION_ATTEMPTS).contains(&max_attempts) {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "max_attempts is outside the scheduler bound".into(),
            ));
        }
        if !(MIN_BACKOFF_SECONDS..=MAX_BACKOFF_SECONDS).contains(&backoff_base_seconds)
            || !(MIN_BACKOFF_SECONDS..=MAX_BACKOFF_SECONDS).contains(&backoff_max_seconds)
            || backoff_base_seconds > backoff_max_seconds
        {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "backoff policy is outside the scheduler bound".into(),
            ));
        }
        Ok(Self {
            page_size,
            batch_size,
            max_attempts,
            backoff_base_seconds,
            backoff_max_seconds,
        })
    }

    pub fn backoff(&self, attempt: u16) -> Duration {
        let shift = u32::from(attempt.saturating_sub(1)).min(63);
        let multiplier = 1_u64.checked_shl(shift).unwrap_or(u64::MAX);
        let seconds = self
            .backoff_base_seconds
            .saturating_mul(multiplier)
            .min(self.backoff_max_seconds);
        Duration::seconds(i64::try_from(seconds).unwrap_or(i64::MAX))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionLease {
    pub claim_id: Uuid,
    pub worker_id: Uuid,
    pub claimed_at: DateTime<Utc>,
    pub lease_until: DateTime<Utc>,
}

impl ProjectionLease {
    pub fn new(
        claim_id: Uuid,
        worker_id: Uuid,
        claimed_at: DateTime<Utc>,
        lease: Duration,
    ) -> Result<Self, HoldingProjectionSchedulerError> {
        validate_lease(lease)?;
        Ok(Self {
            claim_id,
            worker_id,
            claimed_at,
            lease_until: claimed_at + lease,
        })
    }

    /// Equality with the expiry boundary is expired. This avoids a worker
    /// retaining authority for even one instant after the lease ends.
    pub fn is_active_at(&self, now: DateTime<Utc>) -> bool {
        now < self.lease_until
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionScheduleRequest {
    pub binding: HoldingHoldingBinding,
    pub projection_name: String,
    pub job_key: String,
    pub kind: ProjectionJobKind,
    pub available_at: DateTime<Utc>,
    pub policy: ProjectionPolicy,
    pub payload: Value,
}

impl ProjectionScheduleRequest {
    pub fn validate(&self) -> Result<(), HoldingProjectionSchedulerError> {
        self.binding.validate()?;
        validate_projection_name(&self.projection_name)?;
        validate_job_key(&self.job_key, "job_key")?;
        if !self.payload.is_object() {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "scheduler payload must be a JSON object".into(),
            ));
        }
        validate_payload(&self.payload)?;
        Ok(())
    }

    fn request_hash(&self) -> Result<String, HoldingProjectionSchedulerError> {
        let bytes = serde_json::to_vec(self)?;
        let mut digest = Sha256::new();
        digest.update(bytes);
        Ok(hex::encode(digest.finalize()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionJobRecord {
    pub id: Uuid,
    pub binding: HoldingHoldingBinding,
    pub projection_name: String,
    pub job_key: String,
    pub kind: ProjectionJobKind,
    pub state: ProjectionJobState,
    pub available_at: DateTime<Utc>,
    pub policy: ProjectionPolicy,
    pub payload: Value,
    pub attempts: u16,
    pub current_claim_id: Option<Uuid>,
    pub worker_id: Option<Uuid>,
    pub lease_until: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub dead_lettered_at: Option<DateTime<Utc>>,
    pub rebuild_id: Option<Uuid>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Opaque keyset cursor for the platform projection-status read.  It contains
/// only ordering fields; scheduler payloads, claim tokens, and credentials
/// are never serialized into a status cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionStatusCursor {
    pub holding_id: String,
    pub data_profile_key: String,
    pub projection_name: String,
    pub job_key: String,
    pub job_id: Uuid,
}

impl ProjectionStatusCursor {
    pub fn new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        projection_name: impl Into<String>,
        job_key: impl Into<String>,
        job_id: Uuid,
    ) -> Result<Self, HoldingProjectionSchedulerError> {
        let cursor = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            projection_name: projection_name.into(),
            job_key: job_key.into(),
            job_id,
        };
        validate_scope_ref(&cursor.holding_id, "holding_id")?;
        validate_scope_ref(&cursor.data_profile_key, "data_profile_key")?;
        validate_projection_name(&cursor.projection_name)?;
        validate_job_key(&cursor.job_key, "job_key")?;
        if cursor.job_id.is_nil() {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "projection status cursor job_id is required".into(),
            ));
        }
        Ok(cursor)
    }
}

/// Durable source-order cursor for incremental status publication. The
/// scheduler's mutable status rows are ordered by their last source update,
/// not by their business identity, so an updated older job cannot be skipped
/// by a checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionStatusChangeCursor {
    pub updated_at: DateTime<Utc>,
    pub job_id: Uuid,
}

impl ProjectionStatusChangeCursor {
    pub fn new(
        updated_at: DateTime<Utc>,
        job_id: Uuid,
    ) -> Result<Self, HoldingProjectionSchedulerError> {
        if job_id.is_nil() {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "projection status change cursor job_id is required".into(),
            ));
        }
        Ok(Self { updated_at, job_id })
    }
}

/// Safe operational metadata exposed to Platform Operations.  This is a
/// projection of scheduler state, not a transport representation of a job.
/// In particular, payloads, claim IDs, worker IDs, and lease ownership are
/// deliberately absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldingProjectionStatus {
    pub holding_id: String,
    pub data_profile_key: String,
    pub projection_name: String,
    pub job_key: String,
    pub job_id: Uuid,
    pub kind: ProjectionJobKind,
    pub state: ProjectionJobState,
    pub available_at: DateTime<Utc>,
    pub attempts: u16,
    pub max_attempts: u16,
    pub lease_until: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub dead_lettered_at: Option<DateTime<Utc>>,
    pub rebuild_id: Option<Uuid>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub published_at: DateTime<Utc>,
}

impl HoldingProjectionStatus {
    /// Return whether this read-side status was published within the bounded
    /// freshness policy. Future timestamps fail closed.
    pub fn is_fresh_at(&self, now: DateTime<Utc>) -> bool {
        self.published_at <= now
            && now.signed_duration_since(self.published_at)
                <= Duration::seconds(MAX_PROJECTION_STATUS_FRESHNESS_SECONDS)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingProjectionStatusPage {
    pub statuses: Vec<HoldingProjectionStatus>,
    pub has_more: bool,
    pub next_cursor: Option<ProjectionStatusCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingProjectionStatusChangePage {
    pub statuses: Vec<HoldingProjectionStatus>,
    pub has_more: bool,
    pub next_cursor: Option<ProjectionStatusChangeCursor>,
}

impl ProjectionJobRecord {
    pub fn initial(
        id: Uuid,
        request: &ProjectionScheduleRequest,
        now: DateTime<Utc>,
    ) -> Result<Self, HoldingProjectionSchedulerError> {
        request.validate()?;
        Ok(Self {
            id,
            binding: request.binding.clone(),
            projection_name: request.projection_name.clone(),
            job_key: request.job_key.clone(),
            kind: request.kind,
            state: match request.kind {
                ProjectionJobKind::Incremental => ProjectionJobState::Pending,
                ProjectionJobKind::Rebuild => ProjectionJobState::RebuildPending,
            },
            available_at: request.available_at,
            policy: request.policy,
            payload: request.payload.clone(),
            attempts: 0,
            current_claim_id: None,
            worker_id: None,
            lease_until: None,
            last_error: None,
            dead_lettered_at: None,
            rebuild_id: None,
            version: 1,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn can_claim_at(&self, now: DateTime<Utc>) -> bool {
        (self.state.is_claimable() && now >= self.available_at)
            || (self.state == ProjectionJobState::Claimed
                && self.lease_until.is_some_and(|until| now >= until))
    }

    pub fn claim(
        &mut self,
        lease: ProjectionLease,
        now: DateTime<Utc>,
    ) -> Result<(), HoldingProjectionSchedulerError> {
        if self.state.is_terminal() {
            return Err(HoldingProjectionSchedulerError::InvalidTransition(
                "terminal projection jobs cannot be claimed",
            ));
        }
        if self.state == ProjectionJobState::Claimed
            && self.lease_until.is_some_and(|until| now < until)
        {
            return Err(HoldingProjectionSchedulerError::InvalidTransition(
                "an active projection lease cannot be stolen",
            ));
        }
        if !self.can_claim_at(now) {
            return Err(HoldingProjectionSchedulerError::InvalidTransition(
                "projection job is not due for claiming",
            ));
        }
        if self.attempts >= self.policy.max_attempts {
            return Err(HoldingProjectionSchedulerError::InvalidTransition(
                "projection retry budget is exhausted",
            ));
        }
        if lease.claimed_at != now
            || lease.worker_id == Uuid::nil()
            || lease.claim_id == Uuid::nil()
        {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "claim lease attribution is invalid".into(),
            ));
        }
        self.state = ProjectionJobState::Claimed;
        self.current_claim_id = Some(lease.claim_id);
        self.worker_id = Some(lease.worker_id);
        self.lease_until = Some(lease.lease_until);
        self.attempts = self.attempts.saturating_add(1);
        self.version = self.version.saturating_add(1);
        self.updated_at = now;
        Ok(())
    }

    pub fn renew(
        &mut self,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> Result<ProjectionLease, HoldingProjectionSchedulerError> {
        self.require_owner(claim_id, worker_id, now)?;
        let renewed = ProjectionLease::new(claim_id, worker_id, now, lease)?;
        self.lease_until = Some(renewed.lease_until);
        self.version = self.version.saturating_add(1);
        self.updated_at = now;
        Ok(renewed)
    }

    pub fn complete(
        &mut self,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), HoldingProjectionSchedulerError> {
        self.require_owner(claim_id, worker_id, now)?;
        self.state = match self.kind {
            ProjectionJobKind::Incremental => ProjectionJobState::Succeeded,
            ProjectionJobKind::Rebuild => ProjectionJobState::RebuildSucceeded,
        };
        self.current_claim_id = None;
        self.worker_id = None;
        self.lease_until = None;
        self.version = self.version.saturating_add(1);
        self.updated_at = now;
        Ok(())
    }

    pub fn fail(
        &mut self,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
        error: impl Into<String>,
    ) -> Result<ProjectionFailureOutcome, HoldingProjectionSchedulerError> {
        self.require_owner(claim_id, worker_id, now)?;
        let error = redact_error(&error.into());
        self.current_claim_id = None;
        self.worker_id = None;
        self.lease_until = None;
        self.last_error = Some(error.clone());
        self.version = self.version.saturating_add(1);
        self.updated_at = now;
        if self.attempts >= self.policy.max_attempts {
            self.state = ProjectionJobState::DeadLetter;
            self.dead_lettered_at = Some(now);
            Ok(ProjectionFailureOutcome::DeadLetter(DeadLetterRecord {
                job_id: self.id,
                binding: self.binding.clone(),
                projection_name: self.projection_name.clone(),
                attempts: self.attempts,
                error,
                dead_lettered_at: now,
            }))
        } else {
            let retry_at = now + self.policy.backoff(self.attempts);
            self.state = ProjectionJobState::Retryable;
            self.available_at = retry_at;
            Ok(ProjectionFailureOutcome::Retry(RetryRecord {
                job_id: self.id,
                binding: self.binding.clone(),
                attempt: self.attempts,
                next_available_at: retry_at,
                backoff: self.policy.backoff(self.attempts),
                error,
            }))
        }
    }

    /// Terminally reject a claimed job when its authenticated rebuild intent
    /// is stale. A stale candidate must not consume retry budget because
    /// replaying it cannot make the source watermark match again.
    pub fn fail_terminal(
        &mut self,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
        error: impl Into<String>,
    ) -> Result<ProjectionFailureOutcome, HoldingProjectionSchedulerError> {
        self.require_owner(claim_id, worker_id, now)?;
        let error = redact_error(&error.into());
        self.current_claim_id = None;
        self.worker_id = None;
        self.lease_until = None;
        self.last_error = Some(error.clone());
        self.dead_lettered_at = Some(now);
        self.state = ProjectionJobState::DeadLetter;
        self.version = self.version.saturating_add(1);
        self.updated_at = now;
        Ok(ProjectionFailureOutcome::DeadLetter(DeadLetterRecord {
            job_id: self.id,
            binding: self.binding.clone(),
            projection_name: self.projection_name.clone(),
            attempts: self.attempts,
            error,
            dead_lettered_at: now,
        }))
    }

    pub fn reclaim_stale(
        &mut self,
        now: DateTime<Utc>,
    ) -> Result<StaleLeaseReclaimRecord, HoldingProjectionSchedulerError> {
        if self.state != ProjectionJobState::Claimed
            || self.lease_until.is_none_or(|until| now < until)
        {
            return Err(HoldingProjectionSchedulerError::InvalidTransition(
                "projection lease is not stale",
            ));
        }
        let previous_claim_id =
            self.current_claim_id
                .ok_or(HoldingProjectionSchedulerError::InvalidTransition(
                    "claimed projection job has no claim identity",
                ))?;
        self.current_claim_id = None;
        self.worker_id = None;
        self.lease_until = None;
        self.updated_at = now;
        self.version = self.version.saturating_add(1);
        if self.attempts >= self.policy.max_attempts {
            self.state = ProjectionJobState::DeadLetter;
            self.dead_lettered_at = Some(now);
        } else {
            self.state = ProjectionJobState::Retryable;
            self.available_at = now;
        }
        Ok(StaleLeaseReclaimRecord {
            job_id: self.id,
            binding: self.binding.clone(),
            previous_claim_id,
            resulting_state: self.state,
            reclaimed_at: now,
        })
    }

    fn require_owner(
        &self,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), HoldingProjectionSchedulerError> {
        if self.state != ProjectionJobState::Claimed
            || self.current_claim_id != Some(claim_id)
            || self.worker_id != Some(worker_id)
            || self.lease_until.is_none_or(|until| now >= until)
        {
            Err(HoldingProjectionSchedulerError::ClaimLost)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionClaimRequest {
    pub binding: HoldingHoldingBinding,
    /// Optional exact projection owner for a single-context worker. A missing
    /// filter is reserved for catalog-aware executors that can dispatch every
    /// claimed projection safely.
    pub projection_name: Option<String>,
    pub worker_id: Uuid,
    pub claim_key: String,
    pub now: DateTime<Utc>,
    pub lease: Duration,
    pub max_jobs: u32,
}

impl ProjectionClaimRequest {
    pub fn validate(&self) -> Result<(), HoldingProjectionSchedulerError> {
        self.binding.validate()?;
        if let Some(projection_name) = self.projection_name.as_deref() {
            validate_projection_name(projection_name)?;
        }
        if self.worker_id == Uuid::nil() {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "worker_id is required".into(),
            ));
        }
        if self.claim_key.trim().is_empty() || self.claim_key.len() > MAX_CLAIM_KEY_LENGTH {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "claim_key is empty or too long".into(),
            ));
        }
        validate_lease(self.lease)?;
        if !(1..=MAX_PROJECTION_BATCH_SIZE).contains(&self.max_jobs) {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "max_jobs is outside the scheduler bound".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionClaimRecord {
    pub claim_key: String,
    pub job: ProjectionJobRecord,
    pub lease: ProjectionLease,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryRecord {
    pub job_id: Uuid,
    pub binding: HoldingHoldingBinding,
    pub attempt: u16,
    pub next_available_at: DateTime<Utc>,
    pub backoff: Duration,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadLetterRecord {
    pub job_id: Uuid,
    pub binding: HoldingHoldingBinding,
    pub projection_name: String,
    pub attempts: u16,
    pub error: String,
    pub dead_lettered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaleLeaseReclaimRecord {
    pub job_id: Uuid,
    pub binding: HoldingHoldingBinding,
    pub previous_claim_id: Uuid,
    pub resulting_state: ProjectionJobState,
    pub reclaimed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RebuildProjectionRequest {
    pub binding: HoldingHoldingBinding,
    pub projection_name: String,
    pub rebuild_key: String,
    pub available_at: DateTime<Utc>,
    pub policy: ProjectionPolicy,
    pub payload: Value,
}

impl RebuildProjectionRequest {
    fn as_schedule(&self) -> ProjectionScheduleRequest {
        ProjectionScheduleRequest {
            binding: self.binding.clone(),
            projection_name: self.projection_name.clone(),
            job_key: self.rebuild_key.clone(),
            kind: ProjectionJobKind::Rebuild,
            available_at: self.available_at,
            policy: self.policy,
            payload: self.payload.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RebuildRecord {
    pub rebuild_id: Uuid,
    pub job: ProjectionJobRecord,
    pub requested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionFailureOutcome {
    Retry(RetryRecord),
    DeadLetter(DeadLetterRecord),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClaimReservation {
    binding: HoldingHoldingBinding,
    worker_id: Uuid,
}

/// Small pure model used by tests and by composition roots that need to make
/// the same claim-key decision before entering a database transaction.
#[derive(Debug, Clone, Default)]
pub struct ClaimKeyRegistry {
    reservations: BTreeMap<String, ClaimReservation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimKeyDecision {
    New,
    Replay,
}

impl ClaimKeyRegistry {
    pub fn reserve(
        &mut self,
        claim_key: impl Into<String>,
        binding: HoldingHoldingBinding,
        worker_id: Uuid,
    ) -> Result<ClaimKeyDecision, HoldingProjectionSchedulerError> {
        binding.validate()?;
        if worker_id == Uuid::nil() {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "worker_id is required".into(),
            ));
        }
        let claim_key = claim_key.into();
        if claim_key.trim().is_empty() || claim_key.len() > MAX_CLAIM_KEY_LENGTH {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "claim_key is empty or too long".into(),
            ));
        }
        match self.reservations.get(&claim_key) {
            Some(existing) if existing.binding == binding && existing.worker_id == worker_id => {
                Ok(ClaimKeyDecision::Replay)
            }
            Some(_) => Err(HoldingProjectionSchedulerError::ClaimKeyConflict),
            None => {
                self.reservations
                    .insert(claim_key, ClaimReservation { binding, worker_id });
                Ok(ClaimKeyDecision::New)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct HoldingProjectionSchedulerRepository {
    pool: PgPool,
    registry: Arc<crate::repositories::SqlxHoldingStoreRegistry>,
    authorization: ProjectionSchedulerAuthorization,
}

impl HoldingProjectionSchedulerRepository {
    /// Construct only with the control-plane registry that will authorize the
    /// holding/holding binding before any scheduler row is touched.
    pub fn new(
        pool: PgPool,
        registry: Arc<crate::repositories::SqlxHoldingStoreRegistry>,
        authorization: ProjectionSchedulerAuthorization,
    ) -> Self {
        Self {
            pool,
            registry,
            authorization,
        }
    }

    /// Read bounded, non-secret scheduler metadata for Platform Operations.
    ///
    /// The scheduler's control-write database is the authoritative source for
    /// these coordination rows.  Callers must apply their platform/customer
    /// authorization and audit policy at the transport boundary; this method
    /// only validates the filters, enforces keyset pagination, and strips
    /// payload and worker/claim identity from the returned DTO.
    pub async fn list_status_page(
        &self,
        holding_id: Option<&str>,
        data_profile_key: Option<&str>,
        projection_name: Option<&str>,
        after: Option<&ProjectionStatusCursor>,
        page_size: u32,
    ) -> Result<HoldingProjectionStatusPage, HoldingProjectionSchedulerError> {
        if !(1..=MAX_PROJECTION_STATUS_PAGE_SIZE).contains(&page_size) {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "projection status page_size is outside the bound".into(),
            ));
        }
        if data_profile_key.is_some() && holding_id.is_none() {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "data_profile_key requires holding_id".into(),
            ));
        }
        if let Some(value) = holding_id {
            validate_scope_ref(value, "holding_id")?;
        }
        if let Some(value) = data_profile_key {
            validate_scope_ref(value, "data_profile_key")?;
        }
        if let Some(value) = projection_name {
            validate_projection_name(value)?;
        }
        if let Some(cursor) = after {
            ProjectionStatusCursor::new(
                cursor.holding_id.clone(),
                cursor.data_profile_key.clone(),
                cursor.projection_name.clone(),
                cursor.job_key.clone(),
                cursor.job_id,
            )?;
            if holding_id.is_some_and(|value| value != cursor.holding_id)
                || data_profile_key.is_some_and(|value| value != cursor.data_profile_key)
                || projection_name.is_some_and(|value| value != cursor.projection_name)
            {
                return Err(HoldingProjectionSchedulerError::InvalidRequest(
                    "projection status cursor does not match its filters".into(),
                ));
            }
        }

        let limit = i64::from(page_size) + 1;
        let rows = sqlx::query(
            "SELECT id AS job_id, holding_id, data_profile_key, projection_name, job_key,
                    kind, state, available_at, attempts, max_attempts, lease_until,
                    last_error, dead_lettered_at, rebuild_id, version, created_at,
                    updated_at, updated_at AS published_at
             FROM erpx_control.holding_projection_jobs
             WHERE ($1::text IS NULL OR holding_id = $1)
               AND ($2::text IS NULL OR data_profile_key = $2)
               AND ($3::text IS NULL OR projection_name = $3)
               AND ($4::text IS NULL OR
                    (holding_id, data_profile_key, projection_name, job_key, id) >
                    ($4, $5, $6, $7, $8))
             ORDER BY holding_id, data_profile_key, projection_name, job_key, id
             LIMIT $9",
        )
        .bind(holding_id.map(str::to_owned))
        .bind(data_profile_key.map(str::to_owned))
        .bind(projection_name.map(str::to_owned))
        .bind(after.map(|cursor| cursor.holding_id.clone()))
        .bind(after.map(|cursor| cursor.data_profile_key.clone()))
        .bind(after.map(|cursor| cursor.projection_name.clone()))
        .bind(after.map(|cursor| cursor.job_key.clone()))
        .bind(after.map(|cursor| cursor.job_id))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let has_more = rows.len() > page_size as usize;
        let statuses = rows
            .into_iter()
            .take(page_size as usize)
            .map(|row| projection_status_from_row(&row))
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor =
            has_more
                .then(|| statuses.last())
                .flatten()
                .map(|status| ProjectionStatusCursor {
                    holding_id: status.holding_id.clone(),
                    data_profile_key: status.data_profile_key.clone(),
                    projection_name: status.projection_name.clone(),
                    job_key: status.job_key.clone(),
                    job_id: status.job_id,
                });
        Ok(HoldingProjectionStatusPage {
            statuses,
            has_more,
            next_cursor,
        })
    }

    /// Resolve the server-side holding route before a worker starts a claim.
    /// The router validates the control-plane registration and the exact
    /// holding binding; no holding database credential is returned to callers.
    pub async fn verify_holding_route(
        router: &crate::provisioning::HoldingDatabaseRouter,
        binding: &HoldingHoldingBinding,
        side: crate::provisioning::RoutedDatabaseSide,
    ) -> Result<crate::provisioning::RoutedHoldingDatabase, HoldingProjectionSchedulerError> {
        router
            .route(&binding.holding_id, &binding.data_profile_key, side)
            .await
            .map_err(|error| HoldingProjectionSchedulerError::Database(error.to_string()))
    }

    pub async fn schedule(
        &self,
        request: ProjectionScheduleRequest,
        now: DateTime<Utc>,
    ) -> Result<ProjectionJobRecord, HoldingProjectionSchedulerError> {
        self.schedule_inner(request, now, true, false).await
    }

    async fn schedule_inner(
        &self,
        request: ProjectionScheduleRequest,
        now: DateTime<Utc>,
        authorize_holding_projection: bool,
        ensure_rebuild_record: bool,
    ) -> Result<ProjectionJobRecord, HoldingProjectionSchedulerError> {
        request.validate()?;
        if authorize_holding_projection {
            self.require_registered_binding(&request.binding, "schedule", now)
                .await?;
        } else {
            self.require_registered_binding_membership(&request.binding)
                .await?;
        }
        let request_hash = request.request_hash()?;
        let mut transaction = self.pool.begin().await?;
        self.lock_registered_binding_in_transaction(&mut transaction, &request.binding)
            .await?;
        let id = Uuid::new_v4();
        let state = match request.kind {
            ProjectionJobKind::Incremental => ProjectionJobState::Pending,
            ProjectionJobKind::Rebuild => ProjectionJobState::RebuildPending,
        };
        let row = sqlx::query(
            "INSERT INTO erpx_control.holding_projection_jobs
                (id, holding_id, data_profile_key, projection_name, job_key, kind, state,
                 available_at, page_size, batch_size, max_attempts,
                 backoff_base_seconds, backoff_max_seconds, payload, request_hash,
                 version, created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,1,$16,$16)
             ON CONFLICT (holding_id, data_profile_key, projection_name, job_key) DO NOTHING
             RETURNING id",
        )
        .bind(id)
        .bind(&request.binding.holding_id)
        .bind(&request.binding.data_profile_key)
        .bind(&request.projection_name)
        .bind(&request.job_key)
        .bind(request.kind.as_str())
        .bind(state.as_str())
        .bind(request.available_at)
        .bind(request.policy.page_size as i32)
        .bind(request.policy.batch_size as i32)
        .bind(i32::from(request.policy.max_attempts))
        .bind(
            i64::try_from(request.policy.backoff_base_seconds).map_err(|_| {
                HoldingProjectionSchedulerError::InvalidRequest("backoff is too large".into())
            })?,
        )
        .bind(
            i64::try_from(request.policy.backoff_max_seconds).map_err(|_| {
                HoldingProjectionSchedulerError::InvalidRequest("backoff is too large".into())
            })?,
        )
        .bind(&request.payload)
        .bind(&request_hash)
        .bind(now)
        .fetch_optional(&mut *transaction)
        .await?;

        let job_id = match row {
            Some(row) => row.try_get("id")?,
            None => {
                let existing = sqlx::query(
                    "SELECT id, request_hash
                     FROM erpx_control.holding_projection_jobs
                     WHERE holding_id=$1 AND data_profile_key=$2 AND projection_name=$3 AND job_key=$4",
                )
                .bind(&request.binding.holding_id)
                .bind(&request.binding.data_profile_key)
                .bind(&request.projection_name)
                .bind(&request.job_key)
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or(HoldingProjectionSchedulerError::NotFound)?;
                let existing_hash: String = existing.try_get("request_hash")?;
                if existing_hash != request_hash {
                    return Err(HoldingProjectionSchedulerError::ScheduleIdempotencyConflict);
                }
                existing.try_get("id")?
            }
        };
        if ensure_rebuild_record {
            let mut job = self
                .get_job_in_transaction(&mut transaction, job_id, &request.binding)
                .await?;
            if job.kind != ProjectionJobKind::Rebuild {
                return Err(HoldingProjectionSchedulerError::InvalidTransition(
                    "rebuild record requires a rebuild job",
                ));
            }
            let requested_rebuild_id = job.rebuild_id.unwrap_or_else(Uuid::new_v4);
            let rebuild_row = sqlx::query(
                "INSERT INTO erpx_control.holding_projection_rebuilds
                    (id, holding_id, data_profile_key, projection_name, rebuild_key, job_id,
                     state, payload, requested_at, updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,'pending',$7,$8,$8)
                 ON CONFLICT (holding_id, data_profile_key, projection_name, rebuild_key)
                 DO NOTHING
                 RETURNING id, job_id",
            )
            .bind(requested_rebuild_id)
            .bind(&job.binding.holding_id)
            .bind(&job.binding.data_profile_key)
            .bind(&job.projection_name)
            .bind(&job.job_key)
            .bind(job.id)
            .bind(&job.payload)
            .bind(now)
            .fetch_optional(&mut *transaction)
            .await?;
            let (rebuild_id, rebuild_job_id) = match rebuild_row {
                Some(row) => (
                    row.try_get::<Uuid, _>("id")?,
                    row.try_get::<Uuid, _>("job_id")?,
                ),
                None => {
                    let existing = sqlx::query(
                        "SELECT id, job_id
                         FROM erpx_control.holding_projection_rebuilds
                         WHERE holding_id=$1 AND data_profile_key=$2
                           AND projection_name=$3 AND rebuild_key=$4",
                    )
                    .bind(&job.binding.holding_id)
                    .bind(&job.binding.data_profile_key)
                    .bind(&job.projection_name)
                    .bind(&job.job_key)
                    .fetch_optional(&mut *transaction)
                    .await?
                    .ok_or(HoldingProjectionSchedulerError::NotFound)?;
                    (
                        existing.try_get::<Uuid, _>("id")?,
                        existing.try_get::<Uuid, _>("job_id")?,
                    )
                }
            };
            if rebuild_job_id != job.id {
                return Err(HoldingProjectionSchedulerError::ScheduleIdempotencyConflict);
            }
            let linked = sqlx::query(
                "UPDATE erpx_control.holding_projection_jobs
                 SET rebuild_id=$1, updated_at=$3
                 WHERE id=$2 AND holding_id=$4 AND data_profile_key=$5
                   AND (rebuild_id IS NULL OR rebuild_id=$1)",
            )
            .bind(rebuild_id)
            .bind(job.id)
            .bind(now)
            .bind(&job.binding.holding_id)
            .bind(&job.binding.data_profile_key)
            .execute(&mut *transaction)
            .await?;
            if linked.rows_affected() != 1 {
                return Err(HoldingProjectionSchedulerError::ScheduleIdempotencyConflict);
            }
            job.rebuild_id = Some(rebuild_id);
        }
        transaction.commit().await?;
        self.get_job(job_id, &request.binding).await
    }

    pub async fn enqueue_rebuild(
        &self,
        request: RebuildProjectionRequest,
        now: DateTime<Utc>,
    ) -> Result<RebuildRecord, HoldingProjectionSchedulerError> {
        self.enqueue_rebuild_inner(request, now, true).await
    }

    /// Enqueue a holding rebuild after a Platform Operations command has
    /// already passed its authoritative platform permission check. The
    /// binding is still checked against the active registry, but this path
    /// deliberately does not require the platform principal to also hold a
    /// holding-scoped worker permission.
    pub async fn enqueue_rebuild_after_platform_authorization(
        &self,
        request: RebuildProjectionRequest,
        now: DateTime<Utc>,
    ) -> Result<RebuildRecord, HoldingProjectionSchedulerError> {
        self.enqueue_rebuild_inner(request, now, false).await
    }

    /// Atomically create an Operations replacement-read switch, its durable
    /// scheduler job/rebuild record, and the Platform Operations audit event.
    /// The caller must already have passed the platform command permission
    /// check; the active holding/holding binding is still rechecked under the
    /// same control-write transaction.
    pub async fn begin_rebuild_after_platform_authorization(
        &self,
        binding: HoldingHoldingBinding,
        projection_name: impl Into<String>,
        candidate_read_database_key: impl Into<String>,
        source_version: u64,
        policy: ProjectionPolicy,
        now: DateTime<Utc>,
    ) -> Result<(u64, RebuildRecord), HoldingProjectionSchedulerError> {
        let projection_name = projection_name.into();
        let candidate_read_database_key = candidate_read_database_key.into();
        validate_candidate_database_key(&candidate_read_database_key)?;
        if source_version == 0 {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "projection source version is required".into(),
            ));
        }
        let rebuild_binding = binding.clone();
        binding.validate()?;
        validate_projection_name(&projection_name)?;
        self.require_registered_binding_membership(&binding).await?;
        let source_version_i64 = i64::try_from(source_version).map_err(|_| {
            HoldingProjectionSchedulerError::InvalidRequest(
                "projection source version is too large".into(),
            )
        })?;
        let mut transaction = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&candidate_read_database_key)
            .execute(&mut *transaction)
            .await?;
        let registry = sqlx::query(
            "SELECT state, data_profile_key, write_database_key, read_database_key
             FROM erpx_control.holding_registry
             WHERE holding_id = $1
             FOR UPDATE",
        )
        .bind(&binding.holding_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| {
            HoldingProjectionSchedulerError::InvalidRequest(
                "holding is not registered for scheduler work".into(),
            )
        })?;
        let state: String = registry.try_get("state")?;
        let registered_holding: String = registry.try_get("data_profile_key")?;
        let write_database_key: String = registry.try_get("write_database_key")?;
        let current_read_database_key: String = registry.try_get("read_database_key")?;
        if state != "active" || registered_holding != binding.data_profile_key {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "holding scheduler binding is not active".into(),
            ));
        }
        if write_database_key == candidate_read_database_key
            || current_read_database_key == candidate_read_database_key
        {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "replacement read database must be distinct from provisioned databases".into(),
            ));
        }
        let candidate_owned: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM erpx_control.holding_registry
                 WHERE holding_id <> $1
                   AND (write_database_key = $2 OR read_database_key = $2)
             ) OR EXISTS (
                 SELECT 1 FROM erpx_control.holding_projection_switches
                 WHERE holding_id <> $1 AND candidate_read_database_key = $2
             )",
        )
        .bind(&binding.holding_id)
        .bind(&candidate_read_database_key)
        .fetch_one(&mut *transaction)
        .await?;
        if candidate_owned {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "replacement read database is already owned by another holding".into(),
            ));
        }
        let generation: i64 = sqlx::query_scalar(
            "INSERT INTO erpx_control.holding_projection_switches
                (holding_id, current_read_database_key, candidate_read_database_key,
                 state, source_version, version)
             VALUES ($1, $2, $3, 'rebuilding', $4, 1)
             ON CONFLICT (holding_id) DO UPDATE SET
                candidate_read_database_key = EXCLUDED.candidate_read_database_key,
                state = 'rebuilding', source_version = EXCLUDED.source_version,
                version = erpx_control.holding_projection_switches.version + 1,
                last_error = NULL, updated_at = now()
             WHERE erpx_control.holding_projection_switches.state IN ('ready', 'activated', 'failed')
             RETURNING version",
        )
        .bind(&binding.holding_id)
        .bind(&current_read_database_key)
        .bind(&candidate_read_database_key)
        .bind(source_version_i64)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(HoldingProjectionSchedulerError::InvalidTransition(
            "projection switch is already active or not resettable",
        ))?;
        let generation = u64::try_from(generation).map_err(|_| {
            HoldingProjectionSchedulerError::InvalidRequest(
                "projection switch generation is invalid".into(),
            )
        })?;
        let payload = serde_json::json!({
            "candidate_read_database_key": candidate_read_database_key,
            "source_version": source_version,
            "generation": generation,
        });
        let schedule = ProjectionScheduleRequest {
            binding: rebuild_binding,
            projection_name,
            job_key: format!("platform-rebuild-{generation}"),
            kind: ProjectionJobKind::Rebuild,
            available_at: now,
            policy,
            payload,
        };
        schedule.validate()?;
        let request_hash = schedule.request_hash()?;
        let job_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO erpx_control.holding_projection_jobs
                (id, holding_id, data_profile_key, projection_name, job_key, kind, state,
                 available_at, page_size, batch_size, max_attempts,
                 backoff_base_seconds, backoff_max_seconds, payload, request_hash,
                 version, created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,1,$16,$16)",
        )
        .bind(job_id)
        .bind(&schedule.binding.holding_id)
        .bind(&schedule.binding.data_profile_key)
        .bind(&schedule.projection_name)
        .bind(&schedule.job_key)
        .bind(schedule.kind.as_str())
        .bind(ProjectionJobState::RebuildPending.as_str())
        .bind(schedule.available_at)
        .bind(i32::try_from(schedule.policy.page_size).map_err(|_| {
            HoldingProjectionSchedulerError::InvalidRequest(
                "projection page size is too large".into(),
            )
        })?)
        .bind(i32::try_from(schedule.policy.batch_size).map_err(|_| {
            HoldingProjectionSchedulerError::InvalidRequest(
                "projection batch size is too large".into(),
            )
        })?)
        .bind(i32::from(schedule.policy.max_attempts))
        .bind(
            i64::try_from(schedule.policy.backoff_base_seconds).map_err(|_| {
                HoldingProjectionSchedulerError::InvalidRequest(
                    "projection backoff is too large".into(),
                )
            })?,
        )
        .bind(
            i64::try_from(schedule.policy.backoff_max_seconds).map_err(|_| {
                HoldingProjectionSchedulerError::InvalidRequest(
                    "projection backoff is too large".into(),
                )
            })?,
        )
        .bind(&schedule.payload)
        .bind(&request_hash)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        let rebuild_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO erpx_control.holding_projection_rebuilds
                (id, holding_id, data_profile_key, projection_name, rebuild_key, job_id,
                 state, payload, requested_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,'pending',$7,$8,$8)",
        )
        .bind(rebuild_id)
        .bind(&schedule.binding.holding_id)
        .bind(&schedule.binding.data_profile_key)
        .bind(&schedule.projection_name)
        .bind(&schedule.job_key)
        .bind(job_id)
        .bind(&schedule.payload)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_control.audit_events
                (id, actor_ref, customer_account_id, action, purpose,
                 correlation_id, recorded_at, payload)
             VALUES ($1, $2, NULL, $3, $4, $5, $6, $7)",
        )
        .bind(Uuid::new_v4())
        .bind(&self.authorization.actor_ref)
        .bind("platform.projection_rebuild.start")
        .bind(&self.authorization.purpose)
        .bind(self.authorization.correlation_id)
        .bind(now)
        .bind(serde_json::json!({
            "holding_id": schedule.binding.holding_id,
            "data_profile_key": schedule.binding.data_profile_key,
            "projection_name": schedule.projection_name,
            "generation": generation,
            "source_version": source_version,
            "job_id": job_id,
        }))
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        let mut job = ProjectionJobRecord::initial(job_id, &schedule, now)?;
        job.rebuild_id = Some(rebuild_id);
        Ok((
            generation,
            RebuildRecord {
                rebuild_id,
                job,
                requested_at: now,
            },
        ))
    }

    async fn enqueue_rebuild_inner(
        &self,
        request: RebuildProjectionRequest,
        now: DateTime<Utc>,
        authorize_holding_projection: bool,
    ) -> Result<RebuildRecord, HoldingProjectionSchedulerError> {
        let schedule = request.as_schedule();
        let job = self
            .schedule_inner(schedule, now, authorize_holding_projection, true)
            .await?;
        let rebuild_id =
            job.rebuild_id
                .ok_or(HoldingProjectionSchedulerError::InvalidTransition(
                    "rebuild job is missing its durable rebuild record",
                ))?;
        Ok(RebuildRecord {
            rebuild_id,
            job,
            requested_at: now,
        })
    }

    pub async fn claim_due(
        &self,
        request: ProjectionClaimRequest,
    ) -> Result<Vec<ProjectionClaimRecord>, HoldingProjectionSchedulerError> {
        request.validate()?;
        self.require_registered_binding(&request.binding, "claim", request.now)
            .await?;
        let mut transaction = self.pool.begin().await?;
        self.lock_registered_binding_in_transaction(&mut transaction, &request.binding)
            .await?;
        sqlx::query(
            "INSERT INTO erpx_control.holding_projection_claim_batches
                (claim_key, holding_id, data_profile_key, worker_id, sealed, created_at)
             VALUES ($1,$2,$3,$4,false,$5)
             ON CONFLICT (claim_key) DO NOTHING",
        )
        .bind(&request.claim_key)
        .bind(&request.binding.holding_id)
        .bind(&request.binding.data_profile_key)
        .bind(request.worker_id)
        .bind(request.now)
        .execute(&mut *transaction)
        .await?;
        let batch = sqlx::query(
            "SELECT holding_id, data_profile_key, worker_id, sealed
             FROM erpx_control.holding_projection_claim_batches
             WHERE claim_key=$1 FOR UPDATE",
        )
        .bind(&request.claim_key)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(HoldingProjectionSchedulerError::NotFound)?;
        let batch_binding = HoldingHoldingBinding::new(
            batch.try_get::<String, _>("holding_id")?,
            batch.try_get::<String, _>("data_profile_key")?,
        )?;
        let batch_worker: Uuid = batch.try_get("worker_id")?;
        if !batch_binding.matches(&request.binding) || batch_worker != request.worker_id {
            return Err(HoldingProjectionSchedulerError::ClaimKeyConflict);
        }
        let sealed: bool = batch.try_get("sealed")?;
        let existing = self
            .claims_for_key(&mut transaction, &request.binding, &request.claim_key)
            .await?;
        if sealed {
            if request
                .projection_name
                .as_deref()
                .is_some_and(|projection| {
                    existing
                        .iter()
                        .any(|claim| claim.job.projection_name != projection)
                })
            {
                return Err(HoldingProjectionSchedulerError::ClaimKeyConflict);
            }
            let replayed = existing
                .into_iter()
                .map(|mut claim| {
                    claim.replayed = true;
                    claim
                })
                .collect();
            transaction.commit().await?;
            return Ok(replayed);
        }

        let mut claims = Vec::with_capacity(request.max_jobs as usize);
        let rows = sqlx::query(
            "SELECT id, holding_id, data_profile_key, projection_name, job_key, kind, state,
                    available_at, page_size, batch_size, max_attempts,
                    backoff_base_seconds, backoff_max_seconds, payload, attempts,
                    current_claim_id, worker_id, lease_until, last_error,
                    dead_lettered_at, rebuild_id, version, created_at, updated_at
             FROM erpx_control.holding_projection_jobs
             WHERE holding_id=$1 AND data_profile_key=$2
               AND ($3::text IS NULL OR projection_name=$3)
               AND ((state IN ('pending','retryable','rebuild_pending') AND available_at <= $4)
                    OR (state='claimed' AND lease_until IS NOT NULL AND lease_until <= $4))
             ORDER BY available_at, id
             LIMIT $5
             FOR UPDATE SKIP LOCKED",
        )
        .bind(&request.binding.holding_id)
        .bind(&request.binding.data_profile_key)
        .bind(request.projection_name.as_deref())
        .bind(request.now)
        .bind(i64::from(request.max_jobs))
        .fetch_all(&mut *transaction)
        .await?;

        for row in rows {
            let mut job = job_from_row(&row)?;
            if job.state == ProjectionJobState::Claimed {
                let previous_claim_id = job.current_claim_id.ok_or(
                    HoldingProjectionSchedulerError::InvalidTransition(
                        "claimed projection job has no claim identity",
                    ),
                )?;
                let _ = job.reclaim_stale(request.now)?;
                let (claim_state, claim_outcome) = reclaimed_claim_transition(job.state);
                let affected = sqlx::query(
                    "UPDATE erpx_control.holding_projection_claims
                     SET state=$3, completed_at=$2, outcome=$4
                     WHERE claim_id=$1 AND state='active'",
                )
                .bind(previous_claim_id)
                .bind(request.now)
                .bind(claim_state)
                .bind(claim_outcome)
                .execute(&mut *transaction)
                .await?;
                if affected.rows_affected() != 1 {
                    return Err(HoldingProjectionSchedulerError::ClaimLost);
                }
            }
            if job.state == ProjectionJobState::DeadLetter {
                persist_dead_letter_reclaim(&mut transaction, &job, request.now).await?;
                continue;
            }
            let claim_id = Uuid::new_v4();
            let lease =
                ProjectionLease::new(claim_id, request.worker_id, request.now, request.lease)?;
            job.claim(lease.clone(), request.now)?;
            sqlx::query(
                "INSERT INTO erpx_control.holding_projection_claims
                    (claim_id, claim_key, job_id, holding_id, data_profile_key, worker_id,
                     claimed_at, lease_until, state)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'active')",
            )
            .bind(claim_id)
            .bind(&request.claim_key)
            .bind(job.id)
            .bind(&request.binding.holding_id)
            .bind(&request.binding.data_profile_key)
            .bind(request.worker_id)
            .bind(request.now)
            .bind(lease.lease_until)
            .execute(&mut *transaction)
            .await?;
            update_claimed_job(&mut transaction, &job).await?;
            if job.kind == ProjectionJobKind::Rebuild {
                sqlx::query(
                    "UPDATE erpx_control.holding_projection_rebuilds
                     SET state='running', updated_at=$2
                     WHERE job_id=$1 AND holding_id=$3 AND data_profile_key=$4 AND state='pending'",
                )
                .bind(job.id)
                .bind(request.now)
                .bind(&request.binding.holding_id)
                .bind(&request.binding.data_profile_key)
                .execute(&mut *transaction)
                .await?;
            }
            claims.push(ProjectionClaimRecord {
                claim_key: request.claim_key.clone(),
                job,
                lease,
                replayed: false,
            });
        }
        sqlx::query(
            "UPDATE erpx_control.holding_projection_claim_batches
             SET sealed=true, result_count=$2
             WHERE claim_key=$1",
        )
        .bind(&request.claim_key)
        .bind(i32::try_from(claims.len()).map_err(|_| {
            HoldingProjectionSchedulerError::InvalidRequest("claim result is too large".into())
        })?)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(claims)
    }

    pub async fn renew_lease(
        &self,
        binding: &HoldingHoldingBinding,
        job_id: Uuid,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> Result<ProjectionLease, HoldingProjectionSchedulerError> {
        binding.validate()?;
        self.require_registered_binding(binding, "renew", now)
            .await?;
        validate_lease(lease)?;
        let lease_until = now + lease;
        let mut transaction = self.pool.begin().await?;
        self.lock_registered_binding_in_transaction(&mut transaction, binding)
            .await?;
        let result = sqlx::query(
            "UPDATE erpx_control.holding_projection_jobs
             SET lease_until=$6, version=version+1, updated_at=$5
             WHERE id=$1 AND holding_id=$2 AND data_profile_key=$3 AND state='claimed'
               AND current_claim_id=$4 AND worker_id=$7 AND lease_until > $5
             RETURNING current_claim_id, worker_id, lease_until, updated_at",
        )
        .bind(job_id)
        .bind(&binding.holding_id)
        .bind(&binding.data_profile_key)
        .bind(claim_id)
        .bind(now)
        .bind(lease_until)
        .bind(worker_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(HoldingProjectionSchedulerError::ClaimLost)?;
        let affected = sqlx::query(
            "UPDATE erpx_control.holding_projection_claims
             SET lease_until=$4
             WHERE claim_id=$1 AND job_id=$2 AND worker_id=$3 AND state='active'",
        )
        .bind(claim_id)
        .bind(job_id)
        .bind(worker_id)
        .bind(lease_until)
        .execute(&mut *transaction)
        .await?;
        if affected.rows_affected() != 1 {
            return Err(HoldingProjectionSchedulerError::ClaimLost);
        }
        transaction.commit().await?;
        Ok(ProjectionLease {
            claim_id: result.try_get("current_claim_id")?,
            worker_id: result.try_get("worker_id")?,
            claimed_at: now,
            lease_until: result.try_get("lease_until")?,
        })
    }

    pub async fn complete(
        &self,
        binding: &HoldingHoldingBinding,
        job_id: Uuid,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), HoldingProjectionSchedulerError> {
        binding.validate()?;
        self.require_registered_binding(binding, "complete", now)
            .await?;
        let mut transaction = self.pool.begin().await?;
        self.lock_registered_binding_in_transaction(&mut transaction, binding)
            .await?;
        let row = sqlx::query(
            "UPDATE erpx_control.holding_projection_jobs
             SET state=CASE WHEN kind='rebuild' THEN 'rebuild_succeeded' ELSE 'succeeded' END,
                 current_claim_id=NULL, worker_id=NULL, lease_until=NULL,
                 version=version+1, updated_at=$5
             WHERE id=$1 AND holding_id=$2 AND data_profile_key=$3 AND state='claimed'
               AND current_claim_id=$4 AND worker_id=$6 AND lease_until > $5
             RETURNING kind",
        )
        .bind(job_id)
        .bind(&binding.holding_id)
        .bind(&binding.data_profile_key)
        .bind(claim_id)
        .bind(now)
        .bind(worker_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(HoldingProjectionSchedulerError::ClaimLost)?;
        let affected = sqlx::query(
            "UPDATE erpx_control.holding_projection_claims
             SET state='completed', completed_at=$4, outcome='succeeded'
             WHERE claim_id=$1 AND job_id=$2 AND worker_id=$3 AND state='active'",
        )
        .bind(claim_id)
        .bind(job_id)
        .bind(worker_id)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        if affected.rows_affected() != 1 {
            return Err(HoldingProjectionSchedulerError::ClaimLost);
        }
        if row.try_get::<String, _>("kind")? == "rebuild" {
            sqlx::query(
                "UPDATE erpx_control.holding_projection_rebuilds
                 SET state='succeeded', completed_at=$2, updated_at=$2
                 WHERE job_id=$1 AND holding_id=$3 AND data_profile_key=$4 AND state IN ('pending','running')",
            )
            .bind(job_id)
            .bind(now)
            .bind(&binding.holding_id)
            .bind(&binding.data_profile_key)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn fail(
        &self,
        binding: &HoldingHoldingBinding,
        job_id: Uuid,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
        error: impl Into<String>,
    ) -> Result<ProjectionFailureOutcome, HoldingProjectionSchedulerError> {
        self.fail_inner(binding, job_id, claim_id, worker_id, now, error, false)
            .await
    }

    pub async fn fail_terminal(
        &self,
        binding: &HoldingHoldingBinding,
        job_id: Uuid,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
        error: impl Into<String>,
    ) -> Result<ProjectionFailureOutcome, HoldingProjectionSchedulerError> {
        self.fail_inner(binding, job_id, claim_id, worker_id, now, error, true)
            .await
    }

    /// Terminalize a stale rebuild intent when an authenticated operator
    /// observes source movement during manual activation. This path has no
    /// worker claim owner, so it updates the matching job, active claim, and
    /// rebuild row together instead of leaving the scheduler eligible to
    /// retry a generation whose candidate has already been failed.
    pub async fn terminalize_stale_rebuild(
        &self,
        binding: &HoldingHoldingBinding,
        projection_name: &str,
        candidate_read_database_key: &str,
        source_version: u64,
        generation: u64,
        now: DateTime<Utc>,
    ) -> Result<u64, HoldingProjectionSchedulerError> {
        binding.validate()?;
        validate_projection_name(projection_name)?;
        if candidate_read_database_key.trim().is_empty()
            || candidate_read_database_key.len() > 63
            || candidate_read_database_key.chars().any(|character| {
                !(character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
            })
            || source_version == 0
            || generation == 0
        {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "stale rebuild binding is invalid".into(),
            ));
        }
        // Platform Operations performs the authoritative permission check at
        // the server boundary, just as it does for rebuild creation. Keep the
        // control adapter's binding check, but do not require holding worker
        // permission from the platform actor.
        self.require_registered_binding_membership(binding).await?;
        let mut transaction = self.pool.begin().await?;
        self.lock_registered_binding_in_transaction(&mut transaction, binding)
            .await?;
        let jobs = sqlx::query(
            "UPDATE erpx_control.holding_projection_jobs
             SET state='dead_letter', current_claim_id=NULL, worker_id=NULL,
                 lease_until=NULL, last_error='projection_candidate_source_moved',
                 dead_lettered_at=$7, version=version+1, updated_at=$7
             WHERE holding_id=$1 AND data_profile_key=$2 AND projection_name=$3
               AND kind='rebuild'
               AND state IN ('pending','retryable','claimed','rebuild_pending')
               AND payload->>'candidate_read_database_key'=$4
               AND payload->>'source_version'=$5
               AND payload->>'generation'=$6
             RETURNING id",
        )
        .bind(&binding.holding_id)
        .bind(&binding.data_profile_key)
        .bind(projection_name)
        .bind(candidate_read_database_key)
        .bind(source_version.to_string())
        .bind(generation.to_string())
        .bind(now)
        .fetch_all(&mut *transaction)
        .await?;
        for job in &jobs {
            let job_id: Uuid = job.try_get("id")?;
            sqlx::query(
                "UPDATE erpx_control.holding_projection_claims
                 SET state='dead_letter', completed_at=$2,
                     outcome='projection_candidate_source_moved'
                 WHERE job_id=$1 AND holding_id=$3 AND data_profile_key=$4
                   AND state='active'",
            )
            .bind(job_id)
            .bind(now)
            .bind(&binding.holding_id)
            .bind(&binding.data_profile_key)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "UPDATE erpx_control.holding_projection_rebuilds
                 SET state='dead_letter', updated_at=$2
                 WHERE job_id=$1 AND holding_id=$3 AND data_profile_key=$4
                   AND state IN ('pending','running')",
            )
            .bind(job_id)
            .bind(now)
            .bind(&binding.holding_id)
            .bind(&binding.data_profile_key)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(jobs.len() as u64)
    }

    #[allow(clippy::too_many_arguments)]
    async fn fail_inner(
        &self,
        binding: &HoldingHoldingBinding,
        job_id: Uuid,
        claim_id: Uuid,
        worker_id: Uuid,
        now: DateTime<Utc>,
        error: impl Into<String>,
        terminal: bool,
    ) -> Result<ProjectionFailureOutcome, HoldingProjectionSchedulerError> {
        binding.validate()?;
        self.require_registered_binding(binding, "fail", now)
            .await?;
        let mut transaction = self.pool.begin().await?;
        self.lock_registered_binding_in_transaction(&mut transaction, binding)
            .await?;
        let mut job = self
            .get_job_in_transaction(&mut transaction, job_id, binding)
            .await?;
        let outcome = if terminal {
            job.fail_terminal(claim_id, worker_id, now, error)?
        } else {
            job.fail(claim_id, worker_id, now, error)?
        };
        let error_text = job.last_error.clone().unwrap_or_default();
        let (state, available_at, dead_lettered_at) = match &outcome {
            ProjectionFailureOutcome::Retry(retry) => {
                (ProjectionJobState::Retryable, retry.next_available_at, None)
            }
            ProjectionFailureOutcome::DeadLetter(dead) => (
                ProjectionJobState::DeadLetter,
                job.available_at,
                Some(dead.dead_lettered_at),
            ),
        };
        let result = sqlx::query(
            "UPDATE erpx_control.holding_projection_jobs
             SET state=$6, available_at=$7, current_claim_id=NULL, worker_id=NULL,
                 lease_until=NULL, last_error=$8, dead_lettered_at=$9,
                 version=version+1, updated_at=$5
             WHERE id=$1 AND holding_id=$2 AND data_profile_key=$3 AND state='claimed'
               AND current_claim_id=$4 AND worker_id=$10 AND lease_until > $5
             RETURNING id",
        )
        .bind(job_id)
        .bind(&binding.holding_id)
        .bind(&binding.data_profile_key)
        .bind(claim_id)
        .bind(now)
        .bind(state.as_str())
        .bind(available_at)
        .bind(&error_text)
        .bind(dead_lettered_at)
        .bind(worker_id)
        .fetch_optional(&mut *transaction)
        .await?;
        if result.is_none() {
            return Err(HoldingProjectionSchedulerError::ClaimLost);
        }
        let affected = sqlx::query(
            "UPDATE erpx_control.holding_projection_claims
             SET state=$4, completed_at=$5, outcome=$6
             WHERE claim_id=$1 AND job_id=$2 AND worker_id=$3 AND state='active'",
        )
        .bind(claim_id)
        .bind(job_id)
        .bind(worker_id)
        .bind(if state == ProjectionJobState::DeadLetter {
            "dead_letter"
        } else {
            "failed"
        })
        .bind(now)
        .bind(&error_text)
        .execute(&mut *transaction)
        .await?;
        if affected.rows_affected() != 1 {
            return Err(HoldingProjectionSchedulerError::ClaimLost);
        }
        if job.kind == ProjectionJobKind::Rebuild && state == ProjectionJobState::DeadLetter {
            sqlx::query(
                "UPDATE erpx_control.holding_projection_rebuilds
                 SET state='dead_letter', updated_at=$2
                 WHERE job_id=$1 AND holding_id=$3 AND data_profile_key=$4
                   AND state IN ('pending','running')",
            )
            .bind(job_id)
            .bind(now)
            .bind(&binding.holding_id)
            .bind(&binding.data_profile_key)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(outcome)
    }

    pub async fn reclaim_stale(
        &self,
        binding: &HoldingHoldingBinding,
        now: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<StaleLeaseReclaimRecord>, HoldingProjectionSchedulerError> {
        binding.validate()?;
        self.require_registered_binding(binding, "reclaim_stale", now)
            .await?;
        if !(1..=MAX_PROJECTION_BATCH_SIZE).contains(&limit) {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "stale reclaim limit is outside the scheduler bound".into(),
            ));
        }
        let mut transaction = self.pool.begin().await?;
        self.lock_registered_binding_in_transaction(&mut transaction, binding)
            .await?;
        let rows = sqlx::query(
            "SELECT id, holding_id, data_profile_key, projection_name, job_key, kind, state,
                    available_at, page_size, batch_size, max_attempts,
                    backoff_base_seconds, backoff_max_seconds, payload, attempts,
                    current_claim_id, worker_id, lease_until, last_error,
                    dead_lettered_at, rebuild_id, version, created_at, updated_at
             FROM erpx_control.holding_projection_jobs
             WHERE holding_id=$1 AND data_profile_key=$2 AND state='claimed'
               AND lease_until IS NOT NULL AND lease_until <= $3
             ORDER BY lease_until, id
             LIMIT $4 FOR UPDATE SKIP LOCKED",
        )
        .bind(&binding.holding_id)
        .bind(&binding.data_profile_key)
        .bind(now)
        .bind(i64::from(limit))
        .fetch_all(&mut *transaction)
        .await?;
        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let mut job = job_from_row(&row)?;
            let previous_claim =
                job.current_claim_id
                    .ok_or(HoldingProjectionSchedulerError::InvalidTransition(
                        "claimed projection job has no claim identity",
                    ))?;
            let record = job.reclaim_stale(now)?;
            let (claim_state, claim_outcome) = reclaimed_claim_transition(job.state);
            let affected = sqlx::query(
                "UPDATE erpx_control.holding_projection_claims
                 SET state=$3, completed_at=$2, outcome=$4
                 WHERE claim_id=$1 AND state='active'",
            )
            .bind(previous_claim)
            .bind(now)
            .bind(claim_state)
            .bind(claim_outcome)
            .execute(&mut *transaction)
            .await?;
            if affected.rows_affected() != 1 {
                return Err(HoldingProjectionSchedulerError::ClaimLost);
            }
            update_reclaimed_job(&mut transaction, &job).await?;
            if job.state == ProjectionJobState::DeadLetter && job.kind == ProjectionJobKind::Rebuild
            {
                sqlx::query(
                    "UPDATE erpx_control.holding_projection_rebuilds
                     SET state='dead_letter', updated_at=$2
                     WHERE job_id=$1 AND holding_id=$3 AND data_profile_key=$4
                       AND state IN ('pending','running')",
                )
                .bind(job.id)
                .bind(now)
                .bind(&job.binding.holding_id)
                .bind(&job.binding.data_profile_key)
                .execute(&mut *transaction)
                .await?;
            }
            records.push(record);
        }
        transaction.commit().await?;
        Ok(records)
    }

    async fn get_job(
        &self,
        job_id: Uuid,
        binding: &HoldingHoldingBinding,
    ) -> Result<ProjectionJobRecord, HoldingProjectionSchedulerError> {
        let row = sqlx::query(
            "SELECT id, holding_id, data_profile_key, projection_name, job_key, kind, state,
                    available_at, page_size, batch_size, max_attempts,
                    backoff_base_seconds, backoff_max_seconds, payload, attempts,
                    current_claim_id, worker_id, lease_until, last_error,
                    dead_lettered_at, rebuild_id, version, created_at, updated_at
             FROM erpx_control.holding_projection_jobs
             WHERE id=$1 AND holding_id=$2 AND data_profile_key=$3",
        )
        .bind(job_id)
        .bind(&binding.holding_id)
        .bind(&binding.data_profile_key)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(HoldingProjectionSchedulerError::NotFound)?;
        job_from_row(&row)
    }

    /// Lock the active holding registry row in the same transaction as every
    /// scheduler mutation. Lifecycle transitions use the same row lock, so a
    /// registry deactivation or read-route change cannot race a claim,
    /// lease, completion, failure, reclaim, or schedule side effect after the
    /// preflight authorization check.
    async fn lock_registered_binding_in_transaction(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        binding: &HoldingHoldingBinding,
    ) -> Result<(), HoldingProjectionSchedulerError> {
        let row = sqlx::query(
            "SELECT state, data_profile_key
             FROM erpx_control.holding_registry
             WHERE holding_id = $1
             FOR UPDATE",
        )
        .bind(&binding.holding_id)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or_else(|| {
            HoldingProjectionSchedulerError::InvalidRequest(
                "holding is not registered for scheduler work".into(),
            )
        })?;
        let state: String = row.try_get("state")?;
        let data_profile_key: String = row.try_get("data_profile_key")?;
        if state != "active" || data_profile_key != binding.data_profile_key {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "holding scheduler binding is not active".into(),
            ));
        }
        Ok(())
    }

    async fn require_registered_binding(
        &self,
        binding: &HoldingHoldingBinding,
        operation: &str,
        now: DateTime<Utc>,
    ) -> Result<(), HoldingProjectionSchedulerError> {
        self.require_registered_binding_membership(binding).await?;
        self.registry
            .authorize_and_audit_projection(crate::repositories::ProjectionAuthorizationRequest {
                holding_id: &binding.holding_id,
                data_profile_key: &binding.data_profile_key,
                actor_ref: &self.authorization.actor_ref,
                permission: HOLDING_PROJECTION_PERMISSION,
                operation,
                purpose: &self.authorization.purpose,
                correlation_id: self.authorization.correlation_id,
                at: now,
            })
            .await
            .map_err(|error| HoldingProjectionSchedulerError::Database(error.to_string()))?;
        Ok(())
    }

    async fn require_registered_binding_membership(
        &self,
        binding: &HoldingHoldingBinding,
    ) -> Result<(), HoldingProjectionSchedulerError> {
        let registration = self
            .registry
            .get(&binding.holding_id)
            .await
            .map_err(|error| HoldingProjectionSchedulerError::Database(error.to_string()))?
            .ok_or_else(|| {
                HoldingProjectionSchedulerError::InvalidRequest(
                    "holding is not registered for scheduler work".into(),
                )
            })?;
        if registration.state != "active"
            || registration.data_profile_key != binding.data_profile_key
        {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "holding scheduler binding is not active".into(),
            ));
        }
        Ok(())
    }

    async fn get_job_in_transaction(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        job_id: Uuid,
        binding: &HoldingHoldingBinding,
    ) -> Result<ProjectionJobRecord, HoldingProjectionSchedulerError> {
        let row = sqlx::query(
            "SELECT id, holding_id, data_profile_key, projection_name, job_key, kind, state,
                    available_at, page_size, batch_size, max_attempts,
                    backoff_base_seconds, backoff_max_seconds, payload, attempts,
                    current_claim_id, worker_id, lease_until, last_error,
                    dead_lettered_at, rebuild_id, version, created_at, updated_at
             FROM erpx_control.holding_projection_jobs
             WHERE id=$1 AND holding_id=$2 AND data_profile_key=$3
             FOR UPDATE",
        )
        .bind(job_id)
        .bind(&binding.holding_id)
        .bind(&binding.data_profile_key)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or(HoldingProjectionSchedulerError::NotFound)?;
        job_from_row(&row)
    }

    async fn claims_for_key(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        binding: &HoldingHoldingBinding,
        claim_key: &str,
    ) -> Result<Vec<ProjectionClaimRecord>, HoldingProjectionSchedulerError> {
        let rows = sqlx::query(
            "SELECT c.claim_key, c.claim_id, c.worker_id, c.claimed_at, c.lease_until,
                    j.id, j.holding_id, j.data_profile_key, j.projection_name, j.job_key,
                    j.kind, j.state, j.available_at, j.page_size, j.batch_size,
                    j.max_attempts, j.backoff_base_seconds, j.backoff_max_seconds,
                    j.payload, j.attempts, j.current_claim_id, j.worker_id AS job_worker_id,
                    j.lease_until AS job_lease_until, j.last_error, j.dead_lettered_at,
                    j.rebuild_id, j.version, j.created_at, j.updated_at
             FROM erpx_control.holding_projection_claims c
             JOIN erpx_control.holding_projection_jobs j ON j.id=c.job_id
             WHERE c.claim_key=$1 AND c.holding_id=$2 AND c.data_profile_key=$3
             ORDER BY c.claim_id",
        )
        .bind(claim_key)
        .bind(&binding.holding_id)
        .bind(&binding.data_profile_key)
        .fetch_all(&mut **transaction)
        .await?;
        rows.iter().map(claim_from_row).collect()
    }

    /// Read scheduler status changes in durable source-update order. This is
    /// the publisher's incremental source boundary; it deliberately returns
    /// the same safe metadata DTO as the ordinary status read.
    pub async fn list_status_changes_page(
        &self,
        after: Option<&ProjectionStatusChangeCursor>,
        page_size: u32,
    ) -> Result<HoldingProjectionStatusChangePage, HoldingProjectionSchedulerError> {
        if !(1..=MAX_PROJECTION_STATUS_PAGE_SIZE).contains(&page_size) {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "projection status change page_size is outside the bound".into(),
            ));
        }
        if let Some(cursor) = after {
            ProjectionStatusChangeCursor::new(cursor.updated_at, cursor.job_id)?;
        }

        let limit = i64::from(page_size) + 1;
        let rows = sqlx::query(
            "SELECT id AS job_id, holding_id, data_profile_key, projection_name, job_key,
                    kind, state, available_at, attempts, max_attempts, lease_until,
                    last_error, dead_lettered_at, rebuild_id, version, created_at,
                    updated_at, updated_at AS published_at
             FROM erpx_control.holding_projection_jobs
             WHERE ($1::timestamptz IS NULL OR (updated_at, id) > ($1, $2))
             ORDER BY updated_at, id
             LIMIT $3",
        )
        .bind(after.map(|cursor| cursor.updated_at))
        .bind(after.map(|cursor| cursor.job_id))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let has_more = rows.len() > page_size as usize;
        let statuses = rows
            .into_iter()
            .take(page_size as usize)
            .map(|row| projection_status_from_row(&row))
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more.then(|| statuses.last()).flatten().map(|status| {
            ProjectionStatusChangeCursor {
                updated_at: status.updated_at,
                job_id: status.job_id,
            }
        });
        Ok(HoldingProjectionStatusChangePage {
            statuses,
            has_more,
            next_cursor,
        })
    }
}

/// Read-only control-plane adapter for the rebuildable projection status
/// table. It deliberately has no scheduler authorization or write-pool
/// handle; authorization and audit belong to the server boundary.
#[derive(Debug, Clone)]
pub struct HoldingProjectionStatusReadRepository {
    pool: PgPool,
}

impl HoldingProjectionStatusReadRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_status_page(
        &self,
        holding_id: Option<&str>,
        data_profile_key: Option<&str>,
        projection_name: Option<&str>,
        after: Option<&ProjectionStatusCursor>,
        page_size: u32,
    ) -> Result<HoldingProjectionStatusPage, HoldingProjectionSchedulerError> {
        validate_status_query(
            holding_id,
            data_profile_key,
            projection_name,
            after,
            page_size,
        )?;
        let limit = i64::from(page_size) + 1;
        let rows = sqlx::query(
            "SELECT job_id, holding_id, data_profile_key, projection_name, job_key,
                    kind, state, available_at, attempts, max_attempts, lease_until,
                    last_error, dead_lettered_at, rebuild_id, version, created_at,
                    updated_at, published_at
             FROM erpx_control.holding_projection_status
             WHERE ($1::text IS NULL OR holding_id = $1)
               AND ($2::text IS NULL OR data_profile_key = $2)
               AND ($3::text IS NULL OR projection_name = $3)
               AND ($4::text IS NULL OR
                    (holding_id, data_profile_key, projection_name, job_key, job_id) >
                    ($4, $5, $6, $7, $8))
             ORDER BY holding_id, data_profile_key, projection_name, job_key, job_id
             LIMIT $9",
        )
        .bind(holding_id.map(str::to_owned))
        .bind(data_profile_key.map(str::to_owned))
        .bind(projection_name.map(str::to_owned))
        .bind(after.map(|cursor| cursor.holding_id.clone()))
        .bind(after.map(|cursor| cursor.data_profile_key.clone()))
        .bind(after.map(|cursor| cursor.projection_name.clone()))
        .bind(after.map(|cursor| cursor.job_key.clone()))
        .bind(after.map(|cursor| cursor.job_id))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let has_more = rows.len() > page_size as usize;
        let statuses = rows
            .into_iter()
            .take(page_size as usize)
            .map(|row| projection_status_from_row(&row))
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor =
            has_more
                .then(|| statuses.last())
                .flatten()
                .map(|status| ProjectionStatusCursor {
                    holding_id: status.holding_id.clone(),
                    data_profile_key: status.data_profile_key.clone(),
                    projection_name: status.projection_name.clone(),
                    job_key: status.job_key.clone(),
                    job_id: status.job_id,
                });
        Ok(HoldingProjectionStatusPage {
            statuses,
            has_more,
            next_cursor,
        })
    }
}

async fn update_claimed_job(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    job: &ProjectionJobRecord,
) -> Result<(), HoldingProjectionSchedulerError> {
    sqlx::query(
        "UPDATE erpx_control.holding_projection_jobs
         SET state=$2, attempts=$3, current_claim_id=$4, worker_id=$5,
             lease_until=$6, version=$7, updated_at=$8
         WHERE id=$1 AND holding_id=$9 AND data_profile_key=$10 AND state IN ('pending','retryable','rebuild_pending','claimed')",
    )
    .bind(job.id)
    .bind(job.state.as_str())
    .bind(i32::from(job.attempts))
    .bind(job.current_claim_id)
    .bind(job.worker_id)
    .bind(job.lease_until)
    .bind(i64::try_from(job.version).map_err(|_| {
        HoldingProjectionSchedulerError::InvalidRequest("job version is too large".into())
    })?)
    .bind(job.updated_at)
    .bind(&job.binding.holding_id)
    .bind(&job.binding.data_profile_key)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn update_reclaimed_job(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    job: &ProjectionJobRecord,
) -> Result<(), HoldingProjectionSchedulerError> {
    sqlx::query(
        "UPDATE erpx_control.holding_projection_jobs
         SET state=$2, available_at=$3, current_claim_id=NULL, worker_id=NULL,
             lease_until=NULL, dead_lettered_at=$4, version=$5, updated_at=$6
         WHERE id=$1 AND holding_id=$7 AND data_profile_key=$8 AND state='claimed'",
    )
    .bind(job.id)
    .bind(job.state.as_str())
    .bind(job.available_at)
    .bind(job.dead_lettered_at)
    .bind(i64::try_from(job.version).map_err(|_| {
        HoldingProjectionSchedulerError::InvalidRequest("job version is too large".into())
    })?)
    .bind(job.updated_at)
    .bind(&job.binding.holding_id)
    .bind(&job.binding.data_profile_key)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn persist_dead_letter_reclaim(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    job: &ProjectionJobRecord,
    now: DateTime<Utc>,
) -> Result<(), HoldingProjectionSchedulerError> {
    sqlx::query(
        "UPDATE erpx_control.holding_projection_jobs
         SET state='dead_letter', dead_lettered_at=COALESCE(dead_lettered_at,$4),
             current_claim_id=NULL, worker_id=NULL, lease_until=NULL,
             updated_at=$4, version=version+1
         WHERE id=$1 AND holding_id=$2 AND data_profile_key=$3 AND state='claimed'",
    )
    .bind(job.id)
    .bind(&job.binding.holding_id)
    .bind(&job.binding.data_profile_key)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    if job.kind == ProjectionJobKind::Rebuild {
        sqlx::query(
            "UPDATE erpx_control.holding_projection_rebuilds
             SET state='dead_letter', updated_at=$2
             WHERE job_id=$1 AND holding_id=$3 AND data_profile_key=$4
               AND state IN ('pending','running')",
        )
        .bind(job.id)
        .bind(now)
        .bind(&job.binding.holding_id)
        .bind(&job.binding.data_profile_key)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

fn job_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<ProjectionJobRecord, HoldingProjectionSchedulerError> {
    let binding = HoldingHoldingBinding::new(
        row.try_get::<String, _>("holding_id")?,
        row.try_get::<String, _>("data_profile_key")?,
    )?;
    let policy = ProjectionPolicy::new(
        row.try_get::<i32, _>("page_size")? as u32,
        row.try_get::<i32, _>("batch_size")? as u32,
        row.try_get::<i32, _>("max_attempts")? as u16,
        row.try_get::<i64, _>("backoff_base_seconds")? as u64,
        row.try_get::<i64, _>("backoff_max_seconds")? as u64,
    )?;
    let payload: Value = row.try_get("payload")?;
    validate_payload(&payload)?;
    Ok(ProjectionJobRecord {
        id: row.try_get("id")?,
        binding,
        projection_name: row.try_get("projection_name")?,
        job_key: row.try_get("job_key")?,
        kind: ProjectionJobKind::parse(row.try_get::<String, _>("kind")?.as_str())?,
        state: ProjectionJobState::parse(row.try_get::<String, _>("state")?.as_str())?,
        available_at: row.try_get("available_at")?,
        policy,
        payload,
        attempts: row.try_get::<i32, _>("attempts")? as u16,
        current_claim_id: row.try_get("current_claim_id")?,
        worker_id: row.try_get("worker_id")?,
        lease_until: row.try_get("lease_until")?,
        last_error: row.try_get("last_error")?,
        dead_lettered_at: row.try_get("dead_lettered_at")?,
        rebuild_id: row.try_get("rebuild_id")?,
        version: row.try_get::<i64, _>("version")? as u64,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn validate_status_query(
    holding_id: Option<&str>,
    data_profile_key: Option<&str>,
    projection_name: Option<&str>,
    after: Option<&ProjectionStatusCursor>,
    page_size: u32,
) -> Result<(), HoldingProjectionSchedulerError> {
    if !(1..=MAX_PROJECTION_STATUS_PAGE_SIZE).contains(&page_size) {
        return Err(HoldingProjectionSchedulerError::InvalidRequest(
            "projection status page_size is outside the bound".into(),
        ));
    }
    if data_profile_key.is_some() && holding_id.is_none() {
        return Err(HoldingProjectionSchedulerError::InvalidRequest(
            "data_profile_key requires holding_id".into(),
        ));
    }
    if let Some(value) = holding_id {
        validate_scope_ref(value, "holding_id")?;
    }
    if let Some(value) = data_profile_key {
        validate_scope_ref(value, "data_profile_key")?;
    }
    if let Some(value) = projection_name {
        validate_projection_name(value)?;
    }
    if let Some(cursor) = after {
        ProjectionStatusCursor::new(
            cursor.holding_id.clone(),
            cursor.data_profile_key.clone(),
            cursor.projection_name.clone(),
            cursor.job_key.clone(),
            cursor.job_id,
        )?;
        if holding_id.is_some_and(|value| value != cursor.holding_id)
            || data_profile_key.is_some_and(|value| value != cursor.data_profile_key)
            || projection_name.is_some_and(|value| value != cursor.projection_name)
        {
            return Err(HoldingProjectionSchedulerError::InvalidRequest(
                "projection status cursor does not match its filters".into(),
            ));
        }
    }
    Ok(())
}

fn projection_status_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<HoldingProjectionStatus, HoldingProjectionSchedulerError> {
    let holding_id: String = row.try_get("holding_id")?;
    let data_profile_key: String = row.try_get("data_profile_key")?;
    let projection_name: String = row.try_get("projection_name")?;
    let job_key: String = row.try_get("job_key")?;
    validate_scope_ref(&holding_id, "holding_id")?;
    validate_scope_ref(&data_profile_key, "data_profile_key")?;
    validate_projection_name(&projection_name)?;
    validate_job_key(&job_key, "job_key")?;
    let attempts = u16::try_from(row.try_get::<i32, _>("attempts")?).map_err(|_| {
        HoldingProjectionSchedulerError::InvalidRequest("attempts is invalid".into())
    })?;
    let max_attempts = u16::try_from(row.try_get::<i32, _>("max_attempts")?).map_err(|_| {
        HoldingProjectionSchedulerError::InvalidRequest("max_attempts is invalid".into())
    })?;
    let last_error = row
        .try_get::<Option<String>, _>("last_error")?
        .map(|error| redact_error(&error));
    Ok(HoldingProjectionStatus {
        holding_id,
        data_profile_key,
        projection_name,
        job_key,
        job_id: row.try_get("job_id")?,
        kind: ProjectionJobKind::parse(row.try_get::<String, _>("kind")?.as_str())?,
        state: ProjectionJobState::parse(row.try_get::<String, _>("state")?.as_str())?,
        available_at: row.try_get("available_at")?,
        attempts,
        max_attempts,
        lease_until: row.try_get("lease_until")?,
        last_error,
        dead_lettered_at: row.try_get("dead_lettered_at")?,
        rebuild_id: row.try_get("rebuild_id")?,
        version: u64::try_from(row.try_get::<i64, _>("version")?).map_err(|_| {
            HoldingProjectionSchedulerError::InvalidRequest("projection version is invalid".into())
        })?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        published_at: row.try_get("published_at")?,
    })
}

fn reclaimed_claim_transition(state: ProjectionJobState) -> (&'static str, &'static str) {
    if state == ProjectionJobState::DeadLetter {
        ("dead_letter", "lease_expired_retry_budget_exhausted")
    } else {
        ("stale", "lease_expired")
    }
}

fn claim_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<ProjectionClaimRecord, HoldingProjectionSchedulerError> {
    let binding = HoldingHoldingBinding::new(
        row.try_get::<String, _>("holding_id")?,
        row.try_get::<String, _>("data_profile_key")?,
    )?;
    let policy = ProjectionPolicy::new(
        row.try_get::<i32, _>("page_size")? as u32,
        row.try_get::<i32, _>("batch_size")? as u32,
        row.try_get::<i32, _>("max_attempts")? as u16,
        row.try_get::<i64, _>("backoff_base_seconds")? as u64,
        row.try_get::<i64, _>("backoff_max_seconds")? as u64,
    )?;
    let claim_id: Uuid = row.try_get("claim_id")?;
    let worker_id: Uuid = row.try_get("worker_id")?;
    let claimed_at: DateTime<Utc> = row.try_get("claimed_at")?;
    let lease_until: DateTime<Utc> = row.try_get("lease_until")?;
    let payload: Value = row.try_get("payload")?;
    validate_payload(&payload)?;
    Ok(ProjectionClaimRecord {
        claim_key: row.try_get("claim_key")?,
        lease: ProjectionLease {
            claim_id,
            worker_id,
            claimed_at,
            lease_until,
        },
        job: ProjectionJobRecord {
            id: row.try_get("id")?,
            binding,
            projection_name: row.try_get("projection_name")?,
            job_key: row.try_get("job_key")?,
            kind: ProjectionJobKind::parse(row.try_get::<String, _>("kind")?.as_str())?,
            state: ProjectionJobState::parse(row.try_get::<String, _>("state")?.as_str())?,
            available_at: row.try_get("available_at")?,
            policy,
            payload,
            attempts: row.try_get::<i32, _>("attempts")? as u16,
            current_claim_id: row.try_get("current_claim_id")?,
            worker_id: row.try_get("job_worker_id")?,
            lease_until: row.try_get("job_lease_until")?,
            last_error: row.try_get("last_error")?,
            dead_lettered_at: row.try_get("dead_lettered_at")?,
            rebuild_id: row.try_get("rebuild_id")?,
            version: row.try_get::<i64, _>("version")? as u64,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        },
        replayed: true,
    })
}

fn validate_scope_ref(value: &str, field: &str) -> Result<(), HoldingProjectionSchedulerError> {
    if value.trim().is_empty()
        || value.len() > MAX_SCOPE_REFERENCE_LENGTH
        || value.chars().any(char::is_control)
    {
        return Err(HoldingProjectionSchedulerError::InvalidRequest(format!(
            "{field} is empty, too long, or contains a control character"
        )));
    }
    Ok(())
}

fn validate_projection_name(value: &str) -> Result<(), HoldingProjectionSchedulerError> {
    if value.trim().is_empty()
        || value.len() > MAX_PROJECTION_NAME_LENGTH
        || value.chars().any(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.'))
        })
    {
        return Err(HoldingProjectionSchedulerError::InvalidRequest(
            "projection_name is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_job_key(value: &str, field: &str) -> Result<(), HoldingProjectionSchedulerError> {
    if value.trim().is_empty()
        || value.len() > MAX_JOB_KEY_LENGTH
        || value.chars().any(char::is_control)
    {
        return Err(HoldingProjectionSchedulerError::InvalidRequest(format!(
            "{field} is empty, too long, or contains a control character"
        )));
    }
    Ok(())
}

fn validate_candidate_database_key(value: &str) -> Result<(), HoldingProjectionSchedulerError> {
    if value.trim().is_empty()
        || value.len() > 63
        || value
            .chars()
            .any(|character| !(character.is_ascii_alphanumeric() || matches!(character, '_' | '-')))
    {
        return Err(HoldingProjectionSchedulerError::InvalidRequest(
            "candidate_read_database_key is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_lease(lease: Duration) -> Result<(), HoldingProjectionSchedulerError> {
    let seconds = lease.num_seconds();
    if !(MIN_LEASE_SECONDS as i64..=MAX_LEASE_SECONDS as i64).contains(&seconds)
        || lease != Duration::seconds(seconds)
    {
        return Err(HoldingProjectionSchedulerError::InvalidRequest(
            "lease duration is outside the scheduler bound".into(),
        ));
    }
    Ok(())
}

pub(crate) fn redact_error(error: &str) -> String {
    let mut redacted = error
        .replace("postgres://", "<database-url>")
        .replace("postgresql://", "<database-url>")
        .replace("Bearer ", "<bearer> ")
        .replace("token=", "token=<redacted>");
    if redacted.len() > MAX_PROJECTION_ERROR_BYTES {
        redacted.truncate(MAX_PROJECTION_ERROR_BYTES);
        redacted.push('…');
    }
    redacted
}

fn validate_payload(payload: &Value) -> Result<(), HoldingProjectionSchedulerError> {
    if !payload.is_object() {
        return Err(HoldingProjectionSchedulerError::InvalidRequest(
            "scheduler payload must be a JSON object".into(),
        ));
    }
    let size = serde_json::to_vec(payload)?.len();
    if size > MAX_PROJECTION_PAYLOAD_BYTES {
        return Err(HoldingProjectionSchedulerError::InvalidRequest(
            "scheduler payload exceeds its bounded size".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> HoldingHoldingBinding {
        HoldingHoldingBinding::new("holding-a", "holding-a").expect("binding")
    }

    fn policy(max_attempts: u16) -> ProjectionPolicy {
        ProjectionPolicy::new(100, 20, max_attempts, 5, 60).expect("policy")
    }

    fn request(kind: ProjectionJobKind, max_attempts: u16) -> ProjectionScheduleRequest {
        ProjectionScheduleRequest {
            binding: binding(),
            projection_name: "sales_projection".into(),
            job_key: "event-page-1".into(),
            kind,
            available_at: DateTime::UNIX_EPOCH,
            policy: policy(max_attempts),
            payload: serde_json::json!({"page": 1}),
        }
    }

    fn claimed_job(max_attempts: u16) -> (ProjectionJobRecord, ProjectionLease, DateTime<Utc>) {
        let now = Utc::now();
        let mut job = ProjectionJobRecord::initial(
            Uuid::new_v4(),
            &request(ProjectionJobKind::Incremental, max_attempts),
            now,
        )
        .expect("job");
        let lease =
            ProjectionLease::new(Uuid::new_v4(), Uuid::new_v4(), now, Duration::seconds(30))
                .expect("lease");
        job.claim(lease.clone(), now).expect("claim");
        (job, lease, now)
    }

    #[test]
    fn state_transitions_fail_closed_after_completion() {
        let (mut job, lease, now) = claimed_job(3);
        job.complete(lease.claim_id, lease.worker_id, now + Duration::seconds(1))
            .expect("complete");
        assert_eq!(job.state, ProjectionJobState::Succeeded);
        assert!(matches!(
            job.complete(lease.claim_id, lease.worker_id, now + Duration::seconds(2)),
            Err(HoldingProjectionSchedulerError::ClaimLost)
        ));
        assert!(job.state.is_terminal());
    }

    #[test]
    fn lease_expiry_boundary_is_exact_and_not_inclusive() {
        let (job, lease, now) = claimed_job(3);
        assert!(lease.is_active_at(now + Duration::seconds(29)));
        assert!(!lease.is_active_at(lease.lease_until));
        assert!(job.can_claim_at(lease.lease_until));
        assert!(!job.can_claim_at(now + Duration::seconds(29)));
        assert!(matches!(
            job.clone().renew(
                lease.claim_id,
                lease.worker_id,
                lease.lease_until,
                Duration::seconds(10)
            ),
            Err(HoldingProjectionSchedulerError::ClaimLost)
        ));
    }

    #[test]
    fn holding_and_holding_binding_cannot_be_crossed() {
        let left = HoldingHoldingBinding::new("holding-a", "holding-a").expect("binding");
        let right = HoldingHoldingBinding::new("holding-a", "holding-b").expect("binding");
        assert!(!left.matches(&right));
        assert!(left.require_match(&right).is_err());
        assert!(HoldingHoldingBinding::new("holding\n", "holding").is_err());
    }

    #[test]
    fn policy_bounds_and_backoff_are_bounded() {
        assert!(ProjectionPolicy::new(0, 1, 1, 1, 1).is_err());
        assert!(ProjectionPolicy::new(1, MAX_PROJECTION_BATCH_SIZE + 1, 1, 1, 1).is_err());
        assert!(ProjectionPolicy::new(1, 1, MAX_PROJECTION_ATTEMPTS + 1, 1, 1).is_err());
        assert!(ProjectionPolicy::new(1, 1, 1, 10, 5).is_err());
        let policy = policy(5);
        assert_eq!(policy.backoff(1), Duration::seconds(5));
        assert_eq!(policy.backoff(4), Duration::seconds(40));
        assert_eq!(policy.backoff(50), Duration::seconds(60));
    }

    #[test]
    fn scheduler_payload_is_bounded_before_persistence() {
        let mut request = request(ProjectionJobKind::Incremental, 3);
        request.payload = serde_json::json!({
            "oversized": "x".repeat(MAX_PROJECTION_PAYLOAD_BYTES)
        });
        assert!(matches!(
            request.validate(),
            Err(HoldingProjectionSchedulerError::InvalidRequest(message))
                if message.contains("payload")
        ));
    }

    #[test]
    fn claim_filter_is_optional_but_exact_when_present() {
        let base = ProjectionClaimRequest {
            binding: binding(),
            projection_name: None,
            worker_id: Uuid::new_v4(),
            claim_key: "claim-filter".into(),
            now: Utc::now(),
            lease: Duration::seconds(30),
            max_jobs: 10,
        };
        assert!(base.validate().is_ok());
        let mut filtered = base.clone();
        filtered.projection_name = Some("support.capabilities".into());
        assert!(filtered.validate().is_ok());
        filtered.projection_name = Some("support capabilities".into());
        assert!(matches!(
            filtered.validate(),
            Err(HoldingProjectionSchedulerError::InvalidRequest(_))
        ));
    }

    #[test]
    fn scheduler_authorization_requires_bounded_audit_context() {
        let correlation = Uuid::new_v4();
        let authorization = ProjectionSchedulerAuthorization::new(
            "worker/projection",
            "scheduled holding projection",
            correlation,
        )
        .expect("authorization");
        assert_eq!(authorization.actor_ref, "worker/projection");
        assert_eq!(authorization.correlation_id, correlation);
        assert!(ProjectionSchedulerAuthorization::new("", "purpose", correlation).is_err());
        assert!(ProjectionSchedulerAuthorization::new("worker", "", correlation).is_err());
        assert!(ProjectionSchedulerAuthorization::new("worker", "purpose", Uuid::nil()).is_err());
    }

    #[test]
    fn stale_lease_reclaim_is_due_at_boundary() {
        let (mut job, lease, _now) = claimed_job(3);
        let reclaimed = job.reclaim_stale(lease.lease_until).expect("stale lease");
        assert_eq!(reclaimed.previous_claim_id, lease.claim_id);
        assert_eq!(reclaimed.resulting_state, ProjectionJobState::Retryable);
        assert_eq!(job.available_at, lease.lease_until);
        assert!(job.can_claim_at(lease.lease_until));
    }

    #[test]
    fn retry_budget_exhaustion_is_terminal_dead_letter() {
        let (mut job, lease, now) = claimed_job(1);
        let outcome = job
            .fail(
                lease.claim_id,
                lease.worker_id,
                now + Duration::seconds(1),
                "secret token=abc",
            )
            .expect("failure");
        let ProjectionFailureOutcome::DeadLetter(dead) = outcome else {
            panic!("expected dead letter");
        };
        assert_eq!(job.state, ProjectionJobState::DeadLetter);
        assert!(job.state.is_terminal());
        assert!(!dead.error.contains("token=abc"));
        assert!(matches!(
            job.reclaim_stale(now + Duration::seconds(2)),
            Err(HoldingProjectionSchedulerError::InvalidTransition(_))
        ));
    }

    #[test]
    fn stale_candidate_failure_is_terminal_without_consuming_retry_budget() {
        let (mut job, lease, now) = claimed_job(5);
        let outcome = job
            .fail_terminal(
                lease.claim_id,
                lease.worker_id,
                now + Duration::seconds(1),
                "candidate source moved during rebuild",
            )
            .expect("terminal failure");
        assert!(matches!(outcome, ProjectionFailureOutcome::DeadLetter(_)));
        assert_eq!(job.attempts, 1);
        assert_eq!(job.state, ProjectionJobState::DeadLetter);
    }

    #[test]
    fn stale_reclaim_claim_evidence_matches_terminal_job_state() {
        assert_eq!(
            reclaimed_claim_transition(ProjectionJobState::Retryable),
            ("stale", "lease_expired")
        );
        assert_eq!(
            reclaimed_claim_transition(ProjectionJobState::DeadLetter),
            ("dead_letter", "lease_expired_retry_budget_exhausted")
        );
    }

    #[test]
    fn claim_key_retries_are_idempotent_and_scope_bound() {
        let mut registry = ClaimKeyRegistry::default();
        let worker = Uuid::new_v4();
        assert_eq!(
            registry
                .reserve("claim-1", binding(), worker)
                .expect("reserve"),
            ClaimKeyDecision::New
        );
        assert_eq!(
            registry
                .reserve("claim-1", binding(), worker)
                .expect("replay"),
            ClaimKeyDecision::Replay
        );
        assert!(matches!(
            registry.reserve(
                "claim-1",
                HoldingHoldingBinding::new("holding-a", "holding-b").expect("binding"),
                worker,
            ),
            Err(HoldingProjectionSchedulerError::ClaimKeyConflict)
        ));
    }

    #[test]
    fn rebuilds_start_in_their_own_pending_state() {
        let now = Utc::now();
        let job = ProjectionJobRecord::initial(
            Uuid::new_v4(),
            &request(ProjectionJobKind::Rebuild, 3),
            now,
        )
        .expect("rebuild job");
        assert_eq!(job.state, ProjectionJobState::RebuildPending);
        assert_eq!(job.kind, ProjectionJobKind::Rebuild);
    }

    #[test]
    fn fresh_bootstrap_projection_inventory_is_unique_and_bounded() {
        let unique = FRESH_INITIAL_HOLDING_PROJECTIONS
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(unique.len(), FRESH_INITIAL_HOLDING_PROJECTIONS.len());
        assert!(FRESH_INITIAL_HOLDING_PROJECTIONS.iter().all(|name| {
            !name.trim().is_empty()
                && name.len() <= MAX_PROJECTION_NAME_LENGTH
                && !name.chars().any(char::is_control)
        }));
    }
}
