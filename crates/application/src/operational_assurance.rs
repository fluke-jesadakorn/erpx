//! Fail-closed operational assurance, release, and customer-exit policies.
//!
//! These application types consume redacted evidence through ports. Backup,
//! restore, provider, filesystem, SQL, and deployment effects stay in outward
//! adapters and no configuration value is accepted as proof of readiness.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_PROJECTION_LAG: Duration = Duration::seconds(60);
pub const MAX_BACKUP_AGE: Duration = Duration::hours(25);
pub const MAX_PITR_REHEARSAL_AGE: Duration = Duration::days(31);
pub const MAX_RPO: Duration = Duration::hours(24);
pub const MAX_RTO: Duration = Duration::hours(4);
pub const REQUIRED_SCHEMA_GENERATION: u16 = 2;
pub const REQUIRED_SERVICE_CATALOG_GENERATION: u16 = 2;
pub const REQUIRED_CONTEXT_COUNT: u16 = 31;
pub const REQUIRED_SERVICE_COUNT: u16 = 65;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentMode {
    ErpxHosted,
    CustomerHosted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedEvidenceRef {
    pub evidence_id: Uuid,
    pub scope_key: String,
    pub observed_at: DateTime<Utc>,
    pub evidence_sha256: String,
    pub result_code: String,
}

impl RedactedEvidenceRef {
    pub fn validate_at(&self, now: DateTime<Utc>) -> Result<(), OperationalAssuranceError> {
        if self.scope_key.trim().is_empty()
            || self.observed_at > now
            || !is_sha256(&self.evidence_sha256)
            || self.result_code.is_empty()
            || self.result_code.len() > 64
            || !self
                .result_code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(OperationalAssuranceError::InvalidEvidence);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryEvidence {
    pub backup_verified_at: DateTime<Utc>,
    pub backup_inventory_observed_at: DateTime<Utc>,
    pub pitr_rehearsed_at: DateTime<Utc>,
    pub restore_started_at: DateTime<Utc>,
    pub restore_completed_at: DateTime<Utc>,
    pub protected_through: DateTime<Utc>,
    pub recovered_through: DateTime<Utc>,
    pub inventory_sha256: String,
    pub restore_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalReadinessSnapshot {
    pub scope_key: String,
    pub deployment_mode: DeploymentMode,
    pub projection_lag: Duration,
    pub projection: RedactedEvidenceRef,
    pub worker_reconciliation: RedactedEvidenceRef,
    pub provider_reconciliation: RedactedEvidenceRef,
    pub security_assurance: RedactedEvidenceRef,
    pub recovery: RecoveryEvidence,
    pub hosted_parity_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalReadinessDecision {
    pub scope_key: String,
    pub deployment_mode: DeploymentMode,
    pub evaluated_at: DateTime<Utc>,
    pub evidence_ids: Vec<Uuid>,
}

impl OperationalReadinessSnapshot {
    pub fn evaluate(
        &self,
        now: DateTime<Utc>,
    ) -> Result<OperationalReadinessDecision, OperationalAssuranceError> {
        if self.scope_key.trim().is_empty()
            || self.projection_lag < Duration::zero()
            || self.projection_lag > MAX_PROJECTION_LAG
        {
            return Err(OperationalAssuranceError::ProjectionNotReady);
        }
        let evidence = [
            &self.projection,
            &self.worker_reconciliation,
            &self.provider_reconciliation,
            &self.security_assurance,
        ];
        for item in evidence {
            item.validate_at(now)?;
            if item.scope_key != self.scope_key || item.result_code != "healthy" {
                return Err(OperationalAssuranceError::ReadinessNotHealthy);
            }
        }
        if self.deployment_mode == DeploymentMode::CustomerHosted
            && self
                .hosted_parity_sha256
                .as_deref()
                .is_none_or(|digest| !is_sha256(digest))
        {
            return Err(OperationalAssuranceError::CustomerHostedParityMissing);
        }
        self.recovery.validate_at(now)?;
        Ok(OperationalReadinessDecision {
            scope_key: self.scope_key.clone(),
            deployment_mode: self.deployment_mode,
            evaluated_at: now,
            evidence_ids: evidence.iter().map(|item| item.evidence_id).collect(),
        })
    }
}

impl RecoveryEvidence {
    fn validate_at(&self, now: DateTime<Utc>) -> Result<(), OperationalAssuranceError> {
        if [
            self.backup_verified_at,
            self.backup_inventory_observed_at,
            self.pitr_rehearsed_at,
            self.restore_started_at,
            self.restore_completed_at,
            self.protected_through,
            self.recovered_through,
        ]
        .into_iter()
        .any(|value| value > now)
            || !is_sha256(&self.inventory_sha256)
            || !is_sha256(&self.restore_sha256)
        {
            return Err(OperationalAssuranceError::InvalidEvidence);
        }
        if now.signed_duration_since(self.backup_verified_at) > MAX_BACKUP_AGE
            || now.signed_duration_since(self.backup_inventory_observed_at) > MAX_BACKUP_AGE
        {
            return Err(OperationalAssuranceError::BackupEvidenceStale);
        }
        if now.signed_duration_since(self.pitr_rehearsed_at) > MAX_PITR_REHEARSAL_AGE {
            return Err(OperationalAssuranceError::PitrEvidenceStale);
        }
        if self.restore_completed_at < self.restore_started_at
            || self
                .restore_completed_at
                .signed_duration_since(self.restore_started_at)
                > MAX_RTO
        {
            return Err(OperationalAssuranceError::RtoExceeded);
        }
        if self.recovered_through > self.protected_through
            || self
                .protected_through
                .signed_duration_since(self.recovered_through)
                > MAX_RPO
        {
            return Err(OperationalAssuranceError::RpoExceeded);
        }
        Ok(())
    }
}

#[async_trait]
pub trait OperationalEvidencePort: Send + Sync {
    async fn latest_readiness(
        &self,
        scope_key: &str,
        mode: DeploymentMode,
    ) -> Result<Option<OperationalReadinessSnapshot>, OperationalAssuranceError>;

    async fn append_evidence(
        &self,
        evidence: &RedactedEvidenceRef,
    ) -> Result<(), OperationalAssuranceError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub release_id: String,
    pub artifact_sha256: String,
    pub signed_manifest_sha256: String,
    pub signature_key_ref: String,
    pub source_revision: String,
    pub requirements_baseline: String,
    pub schema_generation: u16,
    pub service_catalog_generation: u16,
    pub context_count: u16,
    pub service_count: u16,
    pub created_at: DateTime<Utc>,
}

impl ReleaseManifest {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), OperationalAssuranceError> {
        if self.release_id.trim().is_empty()
            || !is_sha256(&self.artifact_sha256)
            || !is_sha256(&self.signed_manifest_sha256)
            || self.signature_key_ref.trim().is_empty()
            || self.signature_key_ref.contains("secret")
            || self.source_revision.trim().is_empty()
            || self.requirements_baseline != "2.0.0"
            || self.schema_generation != REQUIRED_SCHEMA_GENERATION
            || self.service_catalog_generation != REQUIRED_SERVICE_CATALOG_GENERATION
            || self.context_count != REQUIRED_CONTEXT_COUNT
            || self.service_count != REQUIRED_SERVICE_COUNT
            || self.created_at > now
        {
            return Err(OperationalAssuranceError::InvalidReleaseManifest);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseState {
    Planned,
    Canary,
    Promoted,
    Halted,
    RollingForward,
    Recovered,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseRun {
    pub run_id: Uuid,
    pub manifest: ReleaseManifest,
    pub state: ReleaseState,
    pub cohort_version: u32,
    pub last_health_sha256: Option<String>,
    pub version: u64,
}

impl ReleaseRun {
    pub fn start_canary(
        &mut self,
        artifact_sha256: &str,
        expected_version: u64,
        readiness: &OperationalReadinessDecision,
        now: DateTime<Utc>,
    ) -> Result<(), OperationalAssuranceError> {
        self.require_state_version(ReleaseState::Planned, expected_version)?;
        self.manifest.validate(now)?;
        if artifact_sha256 != self.manifest.artifact_sha256
            || readiness.evidence_ids.is_empty()
            || readiness.evaluated_at > now
        {
            return Err(OperationalAssuranceError::ReleaseGateDenied);
        }
        self.state = ReleaseState::Canary;
        self.cohort_version = 1;
        self.version += 1;
        Ok(())
    }

    pub fn record_health_gate(
        &mut self,
        expected_version: u64,
        evidence_sha256: String,
        healthy: bool,
    ) -> Result<(), OperationalAssuranceError> {
        self.require_state_version(ReleaseState::Canary, expected_version)?;
        if !is_sha256(&evidence_sha256) {
            return Err(OperationalAssuranceError::InvalidEvidence);
        }
        self.last_health_sha256 = Some(evidence_sha256);
        self.state = if healthy {
            ReleaseState::Promoted
        } else {
            ReleaseState::Halted
        };
        self.version += 1;
        Ok(())
    }

    pub fn begin_rollback_forward(
        &mut self,
        expected_version: u64,
        recovery_manifest: &ReleaseManifest,
        now: DateTime<Utc>,
    ) -> Result<(), OperationalAssuranceError> {
        self.require_state_version(ReleaseState::Halted, expected_version)?;
        recovery_manifest.validate(now)?;
        if recovery_manifest.artifact_sha256 != self.manifest.artifact_sha256
            || recovery_manifest.schema_generation != self.manifest.schema_generation
            || recovery_manifest.service_catalog_generation
                != self.manifest.service_catalog_generation
        {
            return Err(OperationalAssuranceError::ArtifactDigestMismatch);
        }
        self.state = ReleaseState::RollingForward;
        self.version += 1;
        Ok(())
    }

    fn require_state_version(
        &self,
        state: ReleaseState,
        version: u64,
    ) -> Result<(), OperationalAssuranceError> {
        if self.version != version {
            return Err(OperationalAssuranceError::VersionConflict);
        }
        if self.state != state {
            return Err(OperationalAssuranceError::InvalidTransition);
        }
        Ok(())
    }
}

#[async_trait]
pub trait ReleaseExecutionPort: Send + Sync {
    async fn deploy_canary(
        &self,
        run_id: Uuid,
        artifact_sha256: &str,
        cohort_version: u32,
    ) -> Result<RedactedEvidenceRef, OperationalAssuranceError>;

    async fn halt(&self, run_id: Uuid) -> Result<RedactedEvidenceRef, OperationalAssuranceError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedDatabaseRef {
    pub opaque_database_ref: String,
    pub write_fenced_at: DateTime<Utc>,
    pub retention_until: DateTime<Utc>,
    pub inventory_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicCutoverEvidence {
    pub target_database_refs: Vec<String>,
    pub retained_old_databases: Vec<RetainedDatabaseRef>,
    pub final_delta_sha256: String,
    pub reconciliation_sha256: String,
    pub activated_at: DateTime<Utc>,
}

impl AtomicCutoverEvidence {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), OperationalAssuranceError> {
        if self.target_database_refs.is_empty()
            || self.retained_old_databases.is_empty()
            || !is_sha256(&self.final_delta_sha256)
            || !is_sha256(&self.reconciliation_sha256)
            || self.activated_at > now
        {
            return Err(OperationalAssuranceError::UnsafeCutover);
        }
        for retained in &self.retained_old_databases {
            if retained.opaque_database_ref.trim().is_empty()
                || self
                    .target_database_refs
                    .contains(&retained.opaque_database_ref)
                || retained.write_fenced_at > self.activated_at
                || retained.retention_until <= self.activated_at
                || !is_sha256(&retained.inventory_sha256)
            {
                return Err(OperationalAssuranceError::UnsafeCutover);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitState {
    Requested,
    WriteFenced,
    BaselineExported,
    FinalDeltaExported,
    AccessRevoked,
    DeletionPending,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortabilityObject {
    pub object_ref: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortabilityManifest {
    pub exit_id: Uuid,
    pub tenant_id: String,
    pub scope_sha256: String,
    pub write_fence_sha256: String,
    pub manifest_sha256: String,
    pub schema_generation: u16,
    pub service_catalog_generation: u16,
    pub generated_at: DateTime<Utc>,
    pub objects: Vec<PortabilityObject>,
}

impl PortabilityManifest {
    fn validate_for(&self, run: &CustomerExitRun) -> Result<(), OperationalAssuranceError> {
        if self.exit_id != run.exit_id
            || self.tenant_id != run.tenant_id
            || self.scope_sha256 != run.scope_sha256
            || run.write_fence_sha256.as_deref() != Some(self.write_fence_sha256.as_str())
            || !is_sha256(&self.manifest_sha256)
            || self.schema_generation != REQUIRED_SCHEMA_GENERATION
            || self.service_catalog_generation != REQUIRED_SERVICE_CATALOG_GENERATION
            || self.objects.is_empty()
            || self.objects.iter().any(|object| {
                object.object_ref.trim().is_empty()
                    || object.bytes == 0
                    || !is_sha256(&object.sha256)
            })
        {
            return Err(OperationalAssuranceError::InvalidEvidence);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerExitRun {
    pub exit_id: Uuid,
    pub tenant_id: String,
    pub requested_by: String,
    pub approved_by: String,
    pub scope_sha256: String,
    pub state: ExitState,
    pub version: u64,
    pub legal_hold_count: u32,
    pub write_fence_sha256: Option<String>,
    pub baseline_manifest: Option<PortabilityManifest>,
    pub final_delta_sha256: Option<String>,
    pub revocation_sha256: Option<String>,
    pub deletion_sha256: Option<String>,
}

impl CustomerExitRun {
    pub fn validate_request(&self) -> Result<(), OperationalAssuranceError> {
        if self.tenant_id.trim().is_empty()
            || self.requested_by.trim().is_empty()
            || self.approved_by.trim().is_empty()
            || self.requested_by == self.approved_by
            || !is_sha256(&self.scope_sha256)
        {
            return Err(OperationalAssuranceError::ExitAuthorityDenied);
        }
        Ok(())
    }

    pub fn record_write_fence(
        &mut self,
        expected_version: u64,
        evidence_sha256: String,
    ) -> Result<(), OperationalAssuranceError> {
        require_sha256(&evidence_sha256)?;
        self.transition(
            expected_version,
            ExitState::Requested,
            ExitState::WriteFenced,
        )?;
        self.write_fence_sha256 = Some(evidence_sha256);
        Ok(())
    }

    pub fn record_baseline_export(
        &mut self,
        expected_version: u64,
        manifest: PortabilityManifest,
    ) -> Result<(), OperationalAssuranceError> {
        manifest.validate_for(self)?;
        self.transition(
            expected_version,
            ExitState::WriteFenced,
            ExitState::BaselineExported,
        )?;
        self.baseline_manifest = Some(manifest);
        Ok(())
    }

    pub fn record_final_delta(
        &mut self,
        expected_version: u64,
        evidence_sha256: String,
    ) -> Result<(), OperationalAssuranceError> {
        require_sha256(&evidence_sha256)?;
        self.transition(
            expected_version,
            ExitState::BaselineExported,
            ExitState::FinalDeltaExported,
        )?;
        self.final_delta_sha256 = Some(evidence_sha256);
        Ok(())
    }

    pub fn record_revocation(
        &mut self,
        expected_version: u64,
        evidence_sha256: String,
    ) -> Result<(), OperationalAssuranceError> {
        require_sha256(&evidence_sha256)?;
        self.transition(
            expected_version,
            ExitState::FinalDeltaExported,
            ExitState::AccessRevoked,
        )?;
        self.revocation_sha256 = Some(evidence_sha256);
        Ok(())
    }

    pub fn request_deletion(
        &mut self,
        expected_version: u64,
    ) -> Result<(), OperationalAssuranceError> {
        if self.legal_hold_count > 0 {
            return Err(OperationalAssuranceError::LegalHoldActive);
        }
        self.transition(
            expected_version,
            ExitState::AccessRevoked,
            ExitState::DeletionPending,
        )
    }

    pub fn complete_deletion(
        &mut self,
        expected_version: u64,
        evidence_sha256: String,
    ) -> Result<(), OperationalAssuranceError> {
        require_sha256(&evidence_sha256)?;
        self.transition(
            expected_version,
            ExitState::DeletionPending,
            ExitState::Completed,
        )?;
        self.deletion_sha256 = Some(evidence_sha256);
        Ok(())
    }

    fn transition(
        &mut self,
        expected_version: u64,
        from: ExitState,
        to: ExitState,
    ) -> Result<(), OperationalAssuranceError> {
        self.validate_request()?;
        if self.version != expected_version {
            return Err(OperationalAssuranceError::VersionConflict);
        }
        if self.state != from {
            return Err(OperationalAssuranceError::InvalidTransition);
        }
        self.state = to;
        self.version += 1;
        Ok(())
    }
}

#[async_trait]
pub trait CustomerExitArtifactPort: Send + Sync {
    async fn write_fence(
        &self,
        exit_id: Uuid,
        tenant_id: &str,
    ) -> Result<String, OperationalAssuranceError>;

    async fn export_baseline(
        &self,
        exit_id: Uuid,
        tenant_id: &str,
    ) -> Result<PortabilityManifest, OperationalAssuranceError>;

    async fn export_final_delta(
        &self,
        exit_id: Uuid,
        tenant_id: &str,
    ) -> Result<String, OperationalAssuranceError>;

    async fn revoke_access(
        &self,
        exit_id: Uuid,
        tenant_id: &str,
    ) -> Result<String, OperationalAssuranceError>;
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OperationalAssuranceError {
    #[error("redacted evidence is missing, future-dated, malformed, or contains an unsafe code")]
    InvalidEvidence,
    #[error("projection freshness gate failed")]
    ProjectionNotReady,
    #[error("worker, provider, projection, or security evidence is not healthy")]
    ReadinessNotHealthy,
    #[error("customer-hosted parity evidence is missing")]
    CustomerHostedParityMissing,
    #[error("verified backup or inventory evidence is older than 25 hours")]
    BackupEvidenceStale,
    #[error("PITR rehearsal evidence is older than 31 days")]
    PitrEvidenceStale,
    #[error("recovery point objective exceeds 24 hours")]
    RpoExceeded,
    #[error("recovery time objective exceeds four hours")]
    RtoExceeded,
    #[error("release manifest does not match requirements/schema/catalog generation two")]
    InvalidReleaseManifest,
    #[error("release health/readiness gate denied the transition")]
    ReleaseGateDenied,
    #[error("recovery artifact digest differs from the deployed artifact")]
    ArtifactDigestMismatch,
    #[error("optimistic version conflict")]
    VersionConflict,
    #[error("invalid lifecycle transition")]
    InvalidTransition,
    #[error("atomic cutover does not retain safely fenced old databases")]
    UnsafeCutover,
    #[error("customer-exit maker-checker or scope authority is invalid")]
    ExitAuthorityDenied,
    #[error("legal hold blocks deletion")]
    LegalHoldActive,
    #[error("operational evidence repository failed")]
    Repository,
}

fn require_sha256(value: &str) -> Result<(), OperationalAssuranceError> {
    if is_sha256(value) {
        Ok(())
    } else {
        Err(OperationalAssuranceError::InvalidEvidence)
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, day, hour, 0, 0)
            .single()
            .expect("time")
    }

    fn evidence(scope: &str, observed_at: DateTime<Utc>) -> RedactedEvidenceRef {
        RedactedEvidenceRef {
            evidence_id: Uuid::new_v4(),
            scope_key: scope.into(),
            observed_at,
            evidence_sha256: "a".repeat(64),
            result_code: "healthy".into(),
        }
    }

    fn readiness(now: DateTime<Utc>, mode: DeploymentMode) -> OperationalReadinessSnapshot {
        let scope = "tenant/holding";
        OperationalReadinessSnapshot {
            scope_key: scope.into(),
            deployment_mode: mode,
            projection_lag: Duration::seconds(60),
            projection: evidence(scope, now),
            worker_reconciliation: evidence(scope, now),
            provider_reconciliation: evidence(scope, now),
            security_assurance: evidence(scope, now),
            recovery: RecoveryEvidence {
                backup_verified_at: now - Duration::hours(25),
                backup_inventory_observed_at: now - Duration::hours(25),
                pitr_rehearsed_at: now - Duration::days(31),
                restore_started_at: now - Duration::hours(4),
                restore_completed_at: now,
                protected_through: now,
                recovered_through: now - Duration::hours(24),
                inventory_sha256: "b".repeat(64),
                restore_sha256: "c".repeat(64),
            },
            hosted_parity_sha256: (mode == DeploymentMode::CustomerHosted).then(|| "d".repeat(64)),
        }
    }

    #[test]
    fn readiness_accepts_exact_hard_gates_and_customer_hosted_parity() {
        let now = at(16, 12);
        let decision = readiness(now, DeploymentMode::CustomerHosted)
            .evaluate(now)
            .expect("exact thresholds pass");
        assert_eq!(decision.evidence_ids.len(), 4);
        assert_eq!(decision.deployment_mode, DeploymentMode::CustomerHosted);
    }

    #[test]
    fn stale_backup_projection_or_missing_parity_fails_closed() {
        let now = at(16, 12);
        let mut candidate = readiness(now, DeploymentMode::CustomerHosted);
        candidate.hosted_parity_sha256 = None;
        assert_eq!(
            candidate.evaluate(now),
            Err(OperationalAssuranceError::CustomerHostedParityMissing)
        );
        candidate.hosted_parity_sha256 = Some("d".repeat(64));
        candidate.projection_lag = Duration::seconds(61);
        assert_eq!(
            candidate.evaluate(now),
            Err(OperationalAssuranceError::ProjectionNotReady)
        );
        candidate.projection_lag = Duration::seconds(60);
        candidate.recovery.backup_verified_at = now - Duration::hours(26);
        assert_eq!(
            candidate.evaluate(now),
            Err(OperationalAssuranceError::BackupEvidenceStale)
        );
    }

    fn release(now: DateTime<Utc>) -> ReleaseManifest {
        ReleaseManifest {
            release_id: "release-2".into(),
            artifact_sha256: "1".repeat(64),
            signed_manifest_sha256: "2".repeat(64),
            signature_key_ref: "release-key-v2".into(),
            source_revision: "revision-1".into(),
            requirements_baseline: "2.0.0".into(),
            schema_generation: 2,
            service_catalog_generation: 2,
            context_count: 31,
            service_count: 65,
            created_at: now,
        }
    }

    #[test]
    fn canary_halts_and_rollback_forward_is_bound_to_same_artifact() {
        let now = at(16, 12);
        let readiness = readiness(now, DeploymentMode::ErpxHosted)
            .evaluate(now)
            .expect("ready");
        let mut run = ReleaseRun {
            run_id: Uuid::new_v4(),
            manifest: release(now),
            state: ReleaseState::Planned,
            cohort_version: 0,
            last_health_sha256: None,
            version: 1,
        };
        run.start_canary(&"1".repeat(64), 1, &readiness, now)
            .expect("canary");
        run.record_health_gate(2, "3".repeat(64), false)
            .expect("halt");
        let mut different = release(now);
        different.artifact_sha256 = "9".repeat(64);
        assert_eq!(
            run.begin_rollback_forward(3, &different, now),
            Err(OperationalAssuranceError::ArtifactDigestMismatch)
        );
        run.begin_rollback_forward(3, &release(now), now)
            .expect("same artifact recovery");
        assert_eq!(run.state, ReleaseState::RollingForward);
    }

    #[test]
    fn atomic_cutover_requires_retained_fenced_old_databases() {
        let now = at(16, 12);
        let evidence = AtomicCutoverEvidence {
            target_database_refs: vec!["target-write-v2".into(), "target-read-v2".into()],
            retained_old_databases: vec![RetainedDatabaseRef {
                opaque_database_ref: "retained-old-write".into(),
                write_fenced_at: now - Duration::minutes(1),
                retention_until: now + Duration::days(31),
                inventory_sha256: "a".repeat(64),
            }],
            final_delta_sha256: "b".repeat(64),
            reconciliation_sha256: "c".repeat(64),
            activated_at: now,
        };
        evidence.validate(now).expect("safe retained cutover");
        let mut unsafe_evidence = evidence;
        unsafe_evidence.retained_old_databases.clear();
        assert_eq!(
            unsafe_evidence.validate(now),
            Err(OperationalAssuranceError::UnsafeCutover)
        );
    }

    #[test]
    fn customer_exit_requires_fence_delta_revocation_and_respects_hold() {
        let mut exit = CustomerExitRun {
            exit_id: Uuid::new_v4(),
            tenant_id: "tenant-1".into(),
            requested_by: "customer-owner".into(),
            approved_by: "independent-reviewer".into(),
            scope_sha256: "a".repeat(64),
            state: ExitState::Requested,
            version: 1,
            legal_hold_count: 1,
            write_fence_sha256: None,
            baseline_manifest: None,
            final_delta_sha256: None,
            revocation_sha256: None,
            deletion_sha256: None,
        };
        assert_eq!(
            exit.record_write_fence(1, "invalid".into()),
            Err(OperationalAssuranceError::InvalidEvidence)
        );
        assert_eq!(exit.state, ExitState::Requested);
        assert_eq!(exit.version, 1);
        exit.record_write_fence(1, "b".repeat(64)).expect("fence");
        exit.record_baseline_export(
            2,
            PortabilityManifest {
                exit_id: exit.exit_id,
                tenant_id: exit.tenant_id.clone(),
                scope_sha256: exit.scope_sha256.clone(),
                write_fence_sha256: "b".repeat(64),
                manifest_sha256: "9".repeat(64),
                schema_generation: 2,
                service_catalog_generation: 2,
                generated_at: at(16, 12),
                objects: vec![PortabilityObject {
                    object_ref: "manifest-1".into(),
                    bytes: 42,
                    sha256: "c".repeat(64),
                }],
            },
        )
        .expect("baseline");
        exit.record_final_delta(3, "d".repeat(64)).expect("delta");
        exit.record_revocation(4, "e".repeat(64)).expect("revoke");
        assert_eq!(
            exit.request_deletion(5),
            Err(OperationalAssuranceError::LegalHoldActive)
        );
        assert_eq!(exit.state, ExitState::AccessRevoked);
        exit.legal_hold_count = 0;
        exit.request_deletion(5).expect("deletion pending");
        exit.complete_deletion(6, "f".repeat(64)).expect("complete");
        assert_eq!(exit.state, ExitState::Completed);
    }
}
