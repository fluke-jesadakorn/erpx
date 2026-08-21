//! Platform control-plane completion contracts for G3--G6.
//!
//! The control plane owns holding lifecycle, connector binding, backup
//! coordination, and replacement-read state.  This module contains the
//! invariants that must be true before a SQLx adapter is allowed to mutate
//! those records.  It deliberately consumes an authorization proof from the
//! shared authorization boundary; it never resolves roles or invents a
//! fallback identity.

use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use erpx_kernel::{AuthorityScope as KernelAuthorityScope, HumanDecisionProof};
use hex::FromHex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::AuthorityScope;

pub const PLATFORM_COMPLETION_CONTRACT_VERSION: u16 = 1;
pub const PLATFORM_MAX_REFERENCE_BYTES: usize = 200;
pub const PLATFORM_MAX_REASON_BYTES: usize = 1_000;
pub const PLATFORM_MAX_DRIFT_CODES: usize = 64;
pub const PLATFORM_MAX_CONNECTOR_CAPABILITIES: usize = 32;
pub const PLATFORM_MAX_BACKUP_ATTEMPTS: u16 = 5;
pub const PLATFORM_MAX_CONNECTOR_KEY_BYTES: usize = 120;
pub const PLATFORM_MAX_CONFIGURATION_DIGEST_BYTES: usize = 64;
pub const PLATFORM_AUTHORIZATION_MAX_CLOCK_SKEW: Duration = Duration::minutes(5);

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PlatformCompletionError {
    #[error("{field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    FieldTooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} is invalid")]
    InvalidField { field: &'static str },
    #[error("identifier cannot be nil: {field}")]
    NilIdentifier { field: &'static str },
    #[error("authorization proof is expired")]
    AuthorizationExpired,
    #[error("authorization proof actor does not match the command actor")]
    ActorMismatch,
    #[error("authorization scope does not contain the requested control-plane scope")]
    ScopeDenied,
    #[error("authorization permission {actual} does not satisfy {required}")]
    PermissionDenied {
        required: &'static str,
        actual: String,
    },
    #[error("MFA is required for this high-risk platform operation")]
    MfaRequired,
    #[error("break-glass fallback must be local-only, expiring, and explicitly approved")]
    InvalidBreakGlass,
    #[error("holding lifecycle transition {from} -> {to} is not allowed")]
    InvalidLifecycleTransition { from: String, to: String },
    #[error("PITR is unavailable until a WAL/recovery-point contract exists")]
    PitrUnavailable,
    #[error("backup request does not match its selected backup kind")]
    InvalidBackupShape,
    #[error("replacement-read activation lacks a routed source-fence proof")]
    MissingSourceFence,
    #[error("replacement-read state transition is invalid")]
    InvalidReplacementTransition,
    #[error("connector state transition is invalid")]
    InvalidConnectorTransition,
    #[error("connector configuration digest must be a SHA-256 hex digest")]
    InvalidConfigurationDigest,
    #[error("connector capability set is invalid")]
    InvalidConnectorCapabilities,
    #[error("reconciliation report contains too many drift codes")]
    TooManyDriftCodes,
    #[error("reconciliation report state does not match its drift evidence")]
    InvalidReconciliationState,
}

fn bounded(value: &str, field: &'static str, max: usize) -> Result<(), PlatformCompletionError> {
    if value.trim().is_empty() {
        return Err(PlatformCompletionError::EmptyField { field });
    }
    if value.len() > max {
        return Err(PlatformCompletionError::FieldTooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(PlatformCompletionError::ControlCharacter { field });
    }
    Ok(())
}

fn identifier(value: Uuid, field: &'static str) -> Result<(), PlatformCompletionError> {
    if value.is_nil() {
        return Err(PlatformCompletionError::NilIdentifier { field });
    }
    Ok(())
}

/// A proof emitted by the shared authorization evaluator.  The platform
/// adapter accepts this proof only after checking its time window, actor,
/// scope, permission, MFA, and the local break-glass restriction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformAuthorizationProof {
    pub principal_ref: String,
    pub permission: String,
    pub scope: AuthorityScope,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub mfa_verified: bool,
    pub local_break_glass: bool,
    pub break_glass_ticket_ref: Option<String>,
}

impl PlatformAuthorizationProof {
    pub fn validate(
        &self,
        actor_ref: &str,
        requested_scope: &AuthorityScope,
        required_permission: &'static str,
        high_risk: bool,
        at: DateTime<Utc>,
    ) -> Result<(), PlatformCompletionError> {
        bounded(
            &self.principal_ref,
            "authorization.principal_ref",
            PLATFORM_MAX_REFERENCE_BYTES,
        )?;
        bounded(
            &self.permission,
            "authorization.permission",
            PLATFORM_MAX_REFERENCE_BYTES,
        )?;
        bounded(
            actor_ref,
            "authorization.actor_ref",
            PLATFORM_MAX_REFERENCE_BYTES,
        )?;
        if self.principal_ref != actor_ref {
            return Err(PlatformCompletionError::ActorMismatch);
        }
        if self.valid_until <= self.valid_from
            || at < self.valid_from
            || at >= self.valid_until
            || self.valid_until - self.valid_from > PLATFORM_AUTHORIZATION_MAX_CLOCK_SKEW
        {
            return Err(PlatformCompletionError::AuthorizationExpired);
        }
        if !self.scope.contains(requested_scope) {
            return Err(PlatformCompletionError::ScopeDenied);
        }
        if self.permission != required_permission && !self.local_break_glass {
            return Err(PlatformCompletionError::PermissionDenied {
                required: required_permission,
                actual: self.permission.clone(),
            });
        }
        if self.local_break_glass && self.permission != "platform.break_glass" {
            return Err(PlatformCompletionError::PermissionDenied {
                required: "platform.break_glass",
                actual: self.permission.clone(),
            });
        }
        if high_risk && !self.mfa_verified {
            return Err(PlatformCompletionError::MfaRequired);
        }
        if self.local_break_glass {
            let Some(ticket_ref) = self.break_glass_ticket_ref.as_deref() else {
                return Err(PlatformCompletionError::InvalidBreakGlass);
            };
            bounded(
                ticket_ref,
                "authorization.break_glass_ticket_ref",
                PLATFORM_MAX_REFERENCE_BYTES,
            )?;
            if !self.mfa_verified || !matches!(self.scope, AuthorityScope::Platform) {
                return Err(PlatformCompletionError::InvalidBreakGlass);
            }
        } else if self.break_glass_ticket_ref.is_some() {
            return Err(PlatformCompletionError::InvalidBreakGlass);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldingBinding {
    pub holding_id: String,
    pub data_profile_key: String,
}

impl HoldingBinding {
    pub fn new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, PlatformCompletionError> {
        let binding = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
        };
        bounded(
            &binding.holding_id,
            "holding_id",
            PLATFORM_MAX_REFERENCE_BYTES,
        )?;
        bounded(
            &binding.data_profile_key,
            "data_profile_key",
            PLATFORM_MAX_REFERENCE_BYTES,
        )?;
        Ok(binding)
    }

    pub fn scope(&self) -> AuthorityScope {
        // Control-plane bindings carry opaque holding references rather than
        // the UUID-shaped IDs used by the shared authorization scope.  Use a
        // stable, domain-separated projection so a holding-scoped proof can
        // authorize this binding without changing the persisted contract or
        // treating every holding mutation as platform-wide.
        AuthorityScope::Holding {
            holding_id: stable_holding_scope_id(&self.data_profile_key),
        }
    }

    fn validate(&self) -> Result<(), PlatformCompletionError> {
        bounded(&self.holding_id, "holding_id", PLATFORM_MAX_REFERENCE_BYTES)?;
        bounded(
            &self.data_profile_key,
            "data_profile_key",
            PLATFORM_MAX_REFERENCE_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldingLifecycleState {
    Provisioning,
    Active,
    Suspended,
    Recovery,
}

impl HoldingLifecycleState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Provisioning => "provisioning",
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Recovery => "recovery",
        }
    }
}

impl std::str::FromStr for HoldingLifecycleState {
    type Err = PlatformCompletionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "provisioning" => Ok(Self::Provisioning),
            "active" => Ok(Self::Active),
            "suspended" => Ok(Self::Suspended),
            "recovery" => Ok(Self::Recovery),
            _ => Err(PlatformCompletionError::InvalidField {
                field: "holding_lifecycle_state",
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldingLifecycleAction {
    Activate,
    Suspend,
    EnterRecovery,
}

impl HoldingLifecycleAction {
    pub const fn target(self) -> HoldingLifecycleState {
        match self {
            Self::Activate => HoldingLifecycleState::Active,
            Self::Suspend => HoldingLifecycleState::Suspended,
            Self::EnterRecovery => HoldingLifecycleState::Recovery,
        }
    }

    pub const fn permission(self) -> &'static str {
        match self {
            Self::Activate => "platform.holding.provision",
            Self::Suspend => "platform.holding.suspend",
            Self::EnterRecovery => "platform.holding.recovery",
        }
    }
}

pub fn validate_lifecycle_transition(
    current: HoldingLifecycleState,
    action: HoldingLifecycleAction,
) -> Result<HoldingLifecycleState, PlatformCompletionError> {
    let target = action.target();
    let allowed = matches!(
        (current, target),
        (
            HoldingLifecycleState::Provisioning,
            HoldingLifecycleState::Active
        ) | (
            HoldingLifecycleState::Recovery,
            HoldingLifecycleState::Active
        ) | (
            HoldingLifecycleState::Suspended,
            HoldingLifecycleState::Active
        ) | (
            HoldingLifecycleState::Active,
            HoldingLifecycleState::Suspended
        ) | (
            HoldingLifecycleState::Active,
            HoldingLifecycleState::Recovery
        ) | (
            HoldingLifecycleState::Suspended,
            HoldingLifecycleState::Recovery
        )
    );
    if allowed {
        Ok(target)
    } else {
        Err(PlatformCompletionError::InvalidLifecycleTransition {
            from: current.as_str().into(),
            to: target.as_str().into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldingLifecycleCommand {
    pub binding: HoldingBinding,
    pub action: HoldingLifecycleAction,
    pub command_id: Uuid,
    pub actor_ref: String,
    pub correlation_id: Uuid,
    pub idempotency_key: String,
    pub expected_registry_version: Option<u64>,
    pub at: DateTime<Utc>,
    #[serde(skip)]
    pub human_decision: Option<HumanDecisionProof>,
}

impl HoldingLifecycleCommand {
    pub fn validate(
        &self,
        proof: &PlatformAuthorizationProof,
    ) -> Result<(), PlatformCompletionError> {
        self.binding.validate()?;
        identifier(self.command_id, "command_id")?;
        identifier(self.correlation_id, "correlation_id")?;
        bounded(&self.actor_ref, "actor_ref", PLATFORM_MAX_REFERENCE_BYTES)?;
        bounded(
            &self.idempotency_key,
            "idempotency_key",
            PLATFORM_MAX_REFERENCE_BYTES,
        )?;
        if self.action == HoldingLifecycleAction::Activate {
            let expected_version =
                self.expected_registry_version
                    .ok_or(PlatformCompletionError::InvalidField {
                        field: "expected_registry_version",
                    })?;
            let decision =
                self.human_decision
                    .as_ref()
                    .ok_or(PlatformCompletionError::InvalidField {
                        field: "human_decision",
                    })?;
            if !matches!(decision.authority_scope(), KernelAuthorityScope::Platform)
                || decision.principal_ref() != self.actor_ref
                || decision.decision_ref()
                    != holding_activation_decision_ref(
                        self.command_id,
                        &self.binding.holding_id,
                        expected_version,
                    )
                || decision.decided_at() != self.at
            {
                return Err(PlatformCompletionError::InvalidField {
                    field: "human_decision",
                });
            }
        }
        proof.validate(
            &self.actor_ref,
            &self.binding.scope(),
            self.action.permission(),
            true,
            self.at,
        )
    }
}

pub fn holding_activation_decision_ref(
    command_id: Uuid,
    holding_id: &str,
    expected_registry_version: u64,
) -> String {
    format!(
        "platform_administration:activate_holding:{command_id}:{holding_id}:{expected_registry_version}:activate"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformBackupKind {
    Full,
    RestoreTest,
    Pitr,
}

/// The immutable recovery coordinates required before a physical PostgreSQL
/// PITR job may be admitted to the control plane.  A timestamp or an LSN is
/// required, never both; the WAL archive and physical restore target are
/// opaque references resolved only by the infrastructure executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PitrRecoveryPoint {
    pub target_time: Option<DateTime<Utc>>,
    pub target_lsn: Option<String>,
    pub wal_archive_ref: String,
    pub physical_restore_target_ref: String,
}

impl PitrRecoveryPoint {
    pub fn validate(&self, at: DateTime<Utc>) -> Result<(), PlatformCompletionError> {
        if self.target_time.is_some() == self.target_lsn.is_some() {
            return Err(PlatformCompletionError::InvalidBackupShape);
        }
        if self.target_time.is_some_and(|target| target > at) {
            return Err(PlatformCompletionError::InvalidField {
                field: "pitr.target_time",
            });
        }
        bounded(
            &self.wal_archive_ref,
            "pitr.wal_archive_ref",
            PLATFORM_MAX_REFERENCE_BYTES,
        )?;
        bounded(
            &self.physical_restore_target_ref,
            "pitr.physical_restore_target_ref",
            PLATFORM_MAX_REFERENCE_BYTES,
        )?;
        if [&self.wal_archive_ref, &self.physical_restore_target_ref]
            .into_iter()
            .any(|value| {
                value
                    .chars()
                    .any(|character| matches!(character, '/' | '\\' | ':' | ' '))
            })
        {
            return Err(PlatformCompletionError::InvalidField {
                field: "pitr.reference",
            });
        }
        if let Some(lsn) = self.target_lsn.as_deref() {
            bounded(lsn, "pitr.target_lsn", PLATFORM_MAX_REFERENCE_BYTES)?;
            if !lsn.split_once('/').is_some_and(|(high, low)| {
                !high.is_empty()
                    && !low.is_empty()
                    && high.chars().all(|value| value.is_ascii_hexdigit())
                    && low.chars().all(|value| value.is_ascii_hexdigit())
            }) {
                return Err(PlatformCompletionError::InvalidField {
                    field: "pitr.target_lsn",
                });
            }
        }
        Ok(())
    }
}

impl PlatformBackupKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::RestoreTest => "restore_test",
            Self::Pitr => "pitr",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupControlCommand {
    pub binding: HoldingBinding,
    pub kind: PlatformBackupKind,
    pub backup_ref: String,
    pub restore_target_ref: Option<String>,
    pub candidate_read_database_key: Option<String>,
    pub source_version: Option<u64>,
    pub source_projection: String,
    pub pitr_recovery_point: Option<PitrRecoveryPoint>,
    pub available_at: DateTime<Utc>,
    pub max_attempts: u16,
    pub actor_ref: String,
    pub correlation_id: Uuid,
    pub idempotency_key: String,
    pub at: DateTime<Utc>,
}

impl BackupControlCommand {
    pub fn validate(
        &self,
        proof: &PlatformAuthorizationProof,
    ) -> Result<(), PlatformCompletionError> {
        self.binding.validate()?;
        identifier(self.correlation_id, "correlation_id")?;
        bounded(&self.backup_ref, "backup_ref", PLATFORM_MAX_REFERENCE_BYTES)?;
        bounded(&self.source_projection, "source_projection", 80)?;
        bounded(&self.actor_ref, "actor_ref", PLATFORM_MAX_REFERENCE_BYTES)?;
        bounded(
            &self.idempotency_key,
            "idempotency_key",
            PLATFORM_MAX_REFERENCE_BYTES,
        )?;
        if self.max_attempts == 0 || self.max_attempts > PLATFORM_MAX_BACKUP_ATTEMPTS {
            return Err(PlatformCompletionError::InvalidField {
                field: "max_attempts",
            });
        }
        match self.kind {
            PlatformBackupKind::Full => {
                if self.restore_target_ref.is_some()
                    || self.candidate_read_database_key.is_some()
                    || self.source_version.is_some()
                    || self.pitr_recovery_point.is_some()
                {
                    return Err(PlatformCompletionError::InvalidBackupShape);
                }
            }
            PlatformBackupKind::RestoreTest => {
                if self.restore_target_ref.is_none()
                    || self.candidate_read_database_key.is_none()
                    || self.source_version.is_none_or(|version| version == 0)
                {
                    return Err(PlatformCompletionError::InvalidBackupShape);
                }
                for (value, field) in [
                    (self.restore_target_ref.as_deref(), "restore_target_ref"),
                    (
                        self.candidate_read_database_key.as_deref(),
                        "candidate_read_database_key",
                    ),
                ] {
                    bounded(
                        value.ok_or(PlatformCompletionError::InvalidBackupShape)?,
                        field,
                        PLATFORM_MAX_REFERENCE_BYTES,
                    )?;
                }
            }
            PlatformBackupKind::Pitr => {
                return Err(PlatformCompletionError::PitrUnavailable);
            }
        }
        if !matches!(
            self.source_projection.as_str(),
            "operations" | "finance_banking" | "finance_subledger" | "finance_close"
        ) {
            return Err(PlatformCompletionError::InvalidField {
                field: "source_projection",
            });
        }
        proof.validate(
            &self.actor_ref,
            &self.binding.scope(),
            "platform.holding.backup",
            true,
            self.at,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplacementReadState {
    Rebuilding,
    Ready,
    Failed,
    Activated,
}

impl ReplacementReadState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rebuilding => "rebuilding",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Activated => "activated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplacementReadCommand {
    pub binding: HoldingBinding,
    pub projection_name: String,
    pub candidate_read_database_key: String,
    pub source_version: u64,
    pub generation: Option<u64>,
    pub actor_ref: String,
    pub correlation_id: Uuid,
    pub idempotency_key: String,
    pub at: DateTime<Utc>,
}

impl ReplacementReadCommand {
    pub fn validate_start(
        &self,
        proof: &PlatformAuthorizationProof,
    ) -> Result<(), PlatformCompletionError> {
        self.validate_common(proof, "platform.projection.rebuild", true)?;
        if self.source_version == 0 || self.generation.is_some() {
            return Err(PlatformCompletionError::InvalidField {
                field: "replacement_read_start",
            });
        }
        Ok(())
    }

    pub fn validate_transition(
        &self,
        proof: &PlatformAuthorizationProof,
        high_risk: bool,
    ) -> Result<(), PlatformCompletionError> {
        self.validate_common(proof, "platform.projection.rebuild", high_risk)?;
        if self.source_version == 0 || self.generation.is_none_or(|generation| generation == 0) {
            return Err(PlatformCompletionError::InvalidField {
                field: "replacement_read_generation",
            });
        }
        Ok(())
    }

    fn validate_common(
        &self,
        proof: &PlatformAuthorizationProof,
        permission: &'static str,
        high_risk: bool,
    ) -> Result<(), PlatformCompletionError> {
        self.binding.validate()?;
        identifier(self.correlation_id, "correlation_id")?;
        bounded(&self.projection_name, "projection_name", 120)?;
        bounded(
            &self.candidate_read_database_key,
            "candidate_read_database_key",
            PLATFORM_MAX_REFERENCE_BYTES,
        )?;
        bounded(&self.actor_ref, "actor_ref", PLATFORM_MAX_REFERENCE_BYTES)?;
        bounded(
            &self.idempotency_key,
            "idempotency_key",
            PLATFORM_MAX_REFERENCE_BYTES,
        )?;
        proof.validate(
            &self.actor_ref,
            &self.binding.scope(),
            permission,
            high_risk,
            self.at,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplacementActivationProof {
    pub candidate_verified: bool,
    pub source_fence_held: bool,
    pub source_version_verified: u64,
    pub fence_reference: String,
}

impl ReplacementActivationProof {
    pub fn validate(&self, expected_source_version: u64) -> Result<(), PlatformCompletionError> {
        if expected_source_version == 0
            || !self.candidate_verified
            || !self.source_fence_held
            || self.source_version_verified != expected_source_version
        {
            return Err(PlatformCompletionError::MissingSourceFence);
        }
        bounded(
            &self.fence_reference,
            "replacement_read.fence_reference",
            PLATFORM_MAX_REFERENCE_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationState {
    Healthy,
    Drifted,
    Unavailable,
}

impl ReconciliationState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Drifted => "drifted",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformReconciliationReport {
    pub report_id: Uuid,
    pub binding: HoldingBinding,
    pub observed_at: DateTime<Utc>,
    pub state: ReconciliationState,
    pub registry_state: String,
    pub provisioning_state: String,
    pub write_healthy: bool,
    pub read_healthy: bool,
    pub storage_healthy: bool,
    pub replacement_read_state: Option<ReplacementReadState>,
    pub latest_backup_verified: bool,
    pub connector_count: u32,
    pub drift_codes: BTreeSet<String>,
}

impl PlatformReconciliationReport {
    pub fn validate(&self) -> Result<(), PlatformCompletionError> {
        identifier(self.report_id, "report_id")?;
        if self
            .registry_state
            .parse::<HoldingLifecycleState>()
            .is_err()
            || (self.provisioning_state != "missing"
                && self
                    .provisioning_state
                    .parse::<HoldingLifecycleState>()
                    .is_err())
        {
            return Err(PlatformCompletionError::InvalidField {
                field: "reconciliation.lifecycle_state",
            });
        }
        bounded(&self.registry_state, "registry_state", 40)?;
        bounded(&self.provisioning_state, "provisioning_state", 40)?;
        if self.drift_codes.len() > PLATFORM_MAX_DRIFT_CODES {
            return Err(PlatformCompletionError::TooManyDriftCodes);
        }
        self.binding.validate()?;
        for code in &self.drift_codes {
            bounded(code, "drift_code", 120)?;
        }
        match self.state {
            ReconciliationState::Healthy if !self.drift_codes.is_empty() => {
                return Err(PlatformCompletionError::InvalidReconciliationState);
            }
            ReconciliationState::Drifted if self.drift_codes.is_empty() => {
                return Err(PlatformCompletionError::InvalidReconciliationState);
            }
            ReconciliationState::Healthy
            | ReconciliationState::Drifted
            | ReconciliationState::Unavailable => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationCommand {
    pub binding: HoldingBinding,
    pub actor_ref: String,
    pub correlation_id: Uuid,
    pub idempotency_key: String,
    pub at: DateTime<Utc>,
}

impl ReconciliationCommand {
    pub fn validate(
        &self,
        proof: &PlatformAuthorizationProof,
    ) -> Result<(), PlatformCompletionError> {
        self.binding.validate()?;
        identifier(self.correlation_id, "correlation_id")?;
        bounded(&self.actor_ref, "actor_ref", PLATFORM_MAX_REFERENCE_BYTES)?;
        bounded(
            &self.idempotency_key,
            "idempotency_key",
            PLATFORM_MAX_REFERENCE_BYTES,
        )?;
        proof.validate(
            &self.actor_ref,
            &self.binding.scope(),
            "platform.holding.reconcile",
            true,
            self.at,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformConnectorKind {
    Stripe,
    MarketingProvider,
    FinanceBank,
    GenericLdap,
    ActiveDirectory,
    UniventionCorporateServer,
}

impl PlatformConnectorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stripe => "stripe",
            Self::MarketingProvider => "marketing_provider",
            Self::FinanceBank => "finance_bank",
            Self::GenericLdap => "generic_ldap",
            Self::ActiveDirectory => "active_directory",
            Self::UniventionCorporateServer => "univention_corporate_server",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformConnectorScope {
    Platform,
    Holding(HoldingBinding),
}

impl PlatformConnectorScope {
    pub fn validate(&self) -> Result<(), PlatformCompletionError> {
        if let Self::Holding(binding) = self {
            bounded(
                &binding.holding_id,
                "connector.holding_id",
                PLATFORM_MAX_REFERENCE_BYTES,
            )?;
            bounded(
                &binding.data_profile_key,
                "connector.data_profile_key",
                PLATFORM_MAX_REFERENCE_BYTES,
            )?;
        }
        Ok(())
    }

    pub fn binding(&self) -> Option<&HoldingBinding> {
        match self {
            Self::Platform => None,
            Self::Holding(binding) => Some(binding),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformConnectorState {
    Pending,
    Active,
    Suspended,
    Revoked,
}

impl PlatformConnectorState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Revoked => "revoked",
        }
    }
}

/// Connector lifecycle is closed and append-only. Revoke is terminal; a
/// replacement connector receives a new immutable connector ID rather than
/// reopening revoked credentials or configuration.
pub fn validate_connector_transition(
    current: Option<PlatformConnectorState>,
    target: PlatformConnectorState,
) -> Result<(), PlatformCompletionError> {
    let allowed = match current {
        None => matches!(target, PlatformConnectorState::Pending),
        Some(PlatformConnectorState::Pending) => matches!(
            target,
            PlatformConnectorState::Pending
                | PlatformConnectorState::Active
                | PlatformConnectorState::Suspended
                | PlatformConnectorState::Revoked
        ),
        Some(PlatformConnectorState::Active) => {
            matches!(
                target,
                PlatformConnectorState::Active
                    | PlatformConnectorState::Suspended
                    | PlatformConnectorState::Revoked
            )
        }
        Some(PlatformConnectorState::Suspended) => {
            matches!(
                target,
                PlatformConnectorState::Active
                    | PlatformConnectorState::Suspended
                    | PlatformConnectorState::Revoked
            )
        }
        Some(PlatformConnectorState::Revoked) => false,
    };
    allowed
        .then_some(())
        .ok_or(PlatformCompletionError::InvalidConnectorTransition)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorControl {
    pub connector_id: Uuid,
    pub kind: PlatformConnectorKind,
    pub scope: PlatformConnectorScope,
    pub provider_key: String,
    /// Opaque secret-manager reference; credential bytes are never accepted.
    pub credential_ref: String,
    pub configuration_digest: String,
    pub capabilities: BTreeSet<String>,
    pub state: PlatformConnectorState,
    pub version: u64,
    pub updated_at: DateTime<Utc>,
}

impl ConnectorControl {
    pub fn validate(&self) -> Result<(), PlatformCompletionError> {
        identifier(self.connector_id, "connector_id")?;
        self.scope.validate()?;
        bounded(
            &self.provider_key,
            "connector.provider_key",
            PLATFORM_MAX_CONNECTOR_KEY_BYTES,
        )?;
        bounded(
            &self.credential_ref,
            "connector.credential_ref",
            PLATFORM_MAX_REFERENCE_BYTES,
        )?;
        if self.credential_ref.contains("://") || self.credential_ref.contains('=') {
            return Err(PlatformCompletionError::InvalidField {
                field: "connector.credential_ref",
            });
        }
        if self.configuration_digest.len() != PLATFORM_MAX_CONFIGURATION_DIGEST_BYTES
            || <[u8; 32]>::from_hex(&self.configuration_digest).is_err()
        {
            return Err(PlatformCompletionError::InvalidConfigurationDigest);
        }
        if self.version == 0
            || self.capabilities.is_empty()
            || self.capabilities.len() > PLATFORM_MAX_CONNECTOR_CAPABILITIES
        {
            return Err(PlatformCompletionError::InvalidConnectorCapabilities);
        }
        for capability in &self.capabilities {
            bounded(
                capability,
                "connector.capability",
                PLATFORM_MAX_CONNECTOR_KEY_BYTES,
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorControlCommand {
    pub control: ConnectorControl,
    pub actor_ref: String,
    pub correlation_id: Uuid,
    pub idempotency_key: String,
    pub at: DateTime<Utc>,
}

impl ConnectorControlCommand {
    pub fn validate(
        &self,
        proof: &PlatformAuthorizationProof,
    ) -> Result<(), PlatformCompletionError> {
        self.control.validate()?;
        identifier(self.correlation_id, "correlation_id")?;
        bounded(&self.actor_ref, "actor_ref", PLATFORM_MAX_REFERENCE_BYTES)?;
        bounded(
            &self.idempotency_key,
            "idempotency_key",
            PLATFORM_MAX_REFERENCE_BYTES,
        )?;
        let requested_scope = self
            .control
            .scope
            .binding()
            .map_or(AuthorityScope::Platform, HoldingBinding::scope);
        proof.validate(
            &self.actor_ref,
            &requested_scope,
            "platform.connector.manage",
            true,
            self.at,
        )
    }
}

pub fn stable_platform_aggregate_id(kind: &str, key: &str) -> Uuid {
    let material = format!("erpx/platform/{kind}/{key}");
    Uuid::new_v5(&Uuid::NAMESPACE_URL, material.as_bytes())
}

/// Convert an opaque holding reference into the UUID-shaped scope key used by
/// the shared authorization contract.  The namespace keeps this projection
/// distinct from platform aggregate IDs and makes the mapping deterministic
/// across API, worker, and persistence boundaries.
pub fn stable_holding_scope_id(data_profile_key: &str) -> Uuid {
    stable_platform_aggregate_id("holding-scope", data_profile_key)
}

#[cfg(test)]
mod tests {
    use erpx_kernel::{
        ActorAttribution, ActorKind, AuthorityScope as KernelAuthorityScope, SessionId,
    };

    use super::*;

    fn proof(permission: &str, at: DateTime<Utc>) -> PlatformAuthorizationProof {
        PlatformAuthorizationProof {
            principal_ref: "platform-admin".into(),
            permission: permission.into(),
            scope: AuthorityScope::Platform,
            valid_from: at - Duration::minutes(1),
            valid_until: at + Duration::minutes(4),
            mfa_verified: true,
            local_break_glass: false,
            break_glass_ticket_ref: None,
        }
    }

    #[test]
    fn lifecycle_rules_are_closed_and_recovery_is_explicit() {
        assert_eq!(
            validate_lifecycle_transition(
                HoldingLifecycleState::Provisioning,
                HoldingLifecycleAction::Activate,
            )
            .expect("activation"),
            HoldingLifecycleState::Active
        );
        assert_eq!(
            validate_lifecycle_transition(
                HoldingLifecycleState::Suspended,
                HoldingLifecycleAction::EnterRecovery,
            )
            .expect("recovery"),
            HoldingLifecycleState::Recovery
        );
        assert!(
            validate_lifecycle_transition(
                HoldingLifecycleState::Provisioning,
                HoldingLifecycleAction::Suspend,
            )
            .is_err()
        );
    }

    #[test]
    fn holding_activation_rejects_missing_or_mismatched_human_decision_before_persistence() {
        let at = Utc::now();
        let binding = HoldingBinding::new("holding-a", "holding-a").expect("binding");
        let command_id = Uuid::new_v4();
        let mut command = HoldingLifecycleCommand {
            binding,
            action: HoldingLifecycleAction::Activate,
            command_id,
            actor_ref: "platform-admin".into(),
            correlation_id: Uuid::new_v4(),
            idempotency_key: "activate-holding-a".into(),
            expected_registry_version: Some(7),
            at,
            human_decision: None,
        };
        let authorization = proof("platform.holding.provision", at);
        assert_eq!(
            command.validate(&authorization),
            Err(PlatformCompletionError::InvalidField {
                field: "human_decision"
            })
        );

        command.human_decision = Some(
            HumanDecisionProof::try_new(
                ActorAttribution {
                    principal_ref: "platform-admin".into(),
                    kind: ActorKind::PlatformUser,
                    session_id: Some(
                        SessionId::try_from_uuid(Uuid::new_v4()).expect("test session"),
                    ),
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                KernelAuthorityScope::Platform,
                holding_activation_decision_ref(command_id, "holding-a", 6),
                at,
            )
            .expect("human decision"),
        );
        assert_eq!(
            command.validate(&authorization),
            Err(PlatformCompletionError::InvalidField {
                field: "human_decision"
            })
        );
    }

    #[test]
    fn high_risk_platform_command_requires_mfa_and_local_break_glass_is_bounded() {
        let at = Utc::now();
        let binding = HoldingBinding::new("holding-a", "holding-a").expect("binding");
        let mut no_mfa = proof("platform.holding.reconcile", at);
        no_mfa.mfa_verified = false;
        let command = ReconciliationCommand {
            binding: binding.clone(),
            actor_ref: "platform-admin".into(),
            correlation_id: Uuid::new_v4(),
            idempotency_key: "reconcile-1".into(),
            at,
        };
        assert_eq!(
            command.validate(&no_mfa),
            Err(PlatformCompletionError::MfaRequired)
        );
        let mut break_glass = proof("platform.break_glass", at);
        break_glass.local_break_glass = true;
        break_glass.break_glass_ticket_ref = Some("incident-1".into());
        assert!(command.validate(&break_glass).is_ok());
        break_glass.permission = "platform.holding.reconcile".into();
        assert_eq!(
            command.validate(&break_glass),
            Err(PlatformCompletionError::PermissionDenied {
                required: "platform.break_glass",
                actual: "platform.holding.reconcile".into(),
            })
        );
        break_glass.permission = "platform.break_glass".into();
        break_glass.scope = AuthorityScope::Holding {
            holding_id: Uuid::new_v4(),
        };
        assert_eq!(
            command.validate(&break_glass),
            Err(PlatformCompletionError::ScopeDenied)
        );
    }

    #[test]
    fn authorization_proof_window_is_bounded_by_clock_skew_policy() {
        let at = Utc::now();
        let binding = HoldingBinding::new("holding-a", "holding-a").expect("binding");
        let command = ReconciliationCommand {
            binding,
            actor_ref: "platform-admin".into(),
            correlation_id: Uuid::new_v4(),
            idempotency_key: "reconcile-window".into(),
            at,
        };
        let mut proof = proof("platform.holding.reconcile", at);
        proof.valid_until = at + PLATFORM_AUTHORIZATION_MAX_CLOCK_SKEW + Duration::seconds(1);
        assert_eq!(
            command.validate(&proof),
            Err(PlatformCompletionError::AuthorizationExpired)
        );
    }

    #[test]
    fn holding_pitr_is_unavailable_and_recovery_points_remain_strictly_validated() {
        let at = Utc::now();
        let binding = HoldingBinding::new("holding-a", "holding-a").expect("binding");
        let pitr = BackupControlCommand {
            binding: binding.clone(),
            kind: PlatformBackupKind::Pitr,
            backup_ref: "backup-a".into(),
            restore_target_ref: None,
            candidate_read_database_key: None,
            source_version: None,
            source_projection: "operations".into(),
            pitr_recovery_point: None,
            available_at: at,
            max_attempts: 3,
            actor_ref: "platform-admin".into(),
            correlation_id: Uuid::new_v4(),
            idempotency_key: "backup-1".into(),
            at,
        };
        assert_eq!(
            pitr.validate(&proof("platform.holding.backup", at)),
            Err(PlatformCompletionError::PitrUnavailable)
        );
        let mut valid_pitr = pitr.clone();
        valid_pitr.pitr_recovery_point = Some(PitrRecoveryPoint {
            target_time: Some(at),
            target_lsn: None,
            wal_archive_ref: "wal-archive-1".into(),
            physical_restore_target_ref: "physical-target-1".into(),
        });
        assert!(
            valid_pitr
                .pitr_recovery_point
                .as_ref()
                .expect("recovery point")
                .validate(at)
                .is_ok()
        );
        assert_eq!(
            valid_pitr.validate(&proof("platform.holding.backup", at)),
            Err(PlatformCompletionError::PitrUnavailable)
        );
        let mut invalid_lsn = valid_pitr.clone();
        invalid_lsn.pitr_recovery_point = Some(PitrRecoveryPoint {
            target_time: None,
            target_lsn: Some("not-an-lsn".into()),
            wal_archive_ref: "wal-archive-1".into(),
            physical_restore_target_ref: "physical-target-1".into(),
        });
        assert_eq!(
            invalid_lsn
                .pitr_recovery_point
                .as_ref()
                .expect("recovery point")
                .validate(at),
            Err(PlatformCompletionError::InvalidField {
                field: "pitr.target_lsn"
            })
        );
        let mut restore = pitr;
        restore.kind = PlatformBackupKind::RestoreTest;
        restore.restore_target_ref = Some("candidate".into());
        restore.candidate_read_database_key = None;
        restore.source_version = Some(1);
        assert_eq!(
            restore.validate(&proof("platform.holding.backup", at)),
            Err(PlatformCompletionError::InvalidBackupShape)
        );
    }

    #[test]
    fn connector_controls_require_digest_and_never_accept_raw_urls_as_credentials() {
        let at = Utc::now();
        let control = ConnectorControl {
            connector_id: Uuid::new_v4(),
            kind: PlatformConnectorKind::GenericLdap,
            scope: PlatformConnectorScope::Platform,
            provider_key: "directory-primary".into(),
            credential_ref: "secret-manager/directory-primary".into(),
            configuration_digest: "00".repeat(32),
            capabilities: ["full_import".into(), "change_feed".into()]
                .into_iter()
                .collect(),
            state: PlatformConnectorState::Pending,
            version: 1,
            updated_at: at,
        };
        assert!(
            ConnectorControlCommand {
                control,
                actor_ref: "platform-admin".into(),
                correlation_id: Uuid::new_v4(),
                idempotency_key: "connector-1".into(),
                at,
            }
            .validate(&proof("platform.connector.manage", at))
            .is_ok()
        );
        let mut invalid = ConnectorControl {
            connector_id: Uuid::new_v4(),
            kind: PlatformConnectorKind::GenericLdap,
            scope: PlatformConnectorScope::Platform,
            provider_key: "directory-primary".into(),
            credential_ref: "ldaps://directory.example".into(),
            configuration_digest: "00".repeat(32),
            capabilities: ["full_import".into()].into_iter().collect(),
            state: PlatformConnectorState::Pending,
            version: 1,
            updated_at: at,
        };
        assert_eq!(
            invalid.validate(),
            Err(PlatformCompletionError::InvalidField {
                field: "connector.credential_ref"
            })
        );
        invalid.credential_ref = "secret-manager/directory-primary".into();
        invalid.configuration_digest = "not-a-digest".into();
        assert_eq!(
            invalid.validate(),
            Err(PlatformCompletionError::InvalidConfigurationDigest)
        );
    }

    #[test]
    fn activation_requires_the_routed_source_fence() {
        let proof = ReplacementActivationProof {
            candidate_verified: true,
            source_fence_held: true,
            source_version_verified: 7,
            fence_reference: "erpx.source-rebuild:holding-a".into(),
        };
        assert!(proof.validate(7).is_ok());
        assert_eq!(
            ReplacementActivationProof {
                source_fence_held: false,
                ..proof.clone()
            }
            .validate(7),
            Err(PlatformCompletionError::MissingSourceFence)
        );
        assert_eq!(
            proof.validate(0),
            Err(PlatformCompletionError::MissingSourceFence)
        );
    }

    #[test]
    fn connector_revoke_is_terminal_but_same_state_revision_is_allowed() {
        assert!(
            validate_connector_transition(
                Some(PlatformConnectorState::Active),
                PlatformConnectorState::Active,
            )
            .is_ok()
        );
        assert!(
            validate_connector_transition(
                Some(PlatformConnectorState::Active),
                PlatformConnectorState::Revoked,
            )
            .is_ok()
        );
        assert_eq!(
            validate_connector_transition(
                Some(PlatformConnectorState::Revoked),
                PlatformConnectorState::Active,
            ),
            Err(PlatformCompletionError::InvalidConnectorTransition)
        );
    }

    #[test]
    fn holding_binding_scope_is_stable_and_holding_bounded() {
        let first = HoldingBinding::new("holding-a", "holding-a").expect("binding");
        let same_holding = HoldingBinding::new("holding-b", "holding-a").expect("binding");
        let other = HoldingBinding::new("holding-a", "holding-b").expect("binding");

        assert_eq!(first.scope(), same_holding.scope());
        assert_ne!(first.scope(), other.scope());
        assert_eq!(
            first.scope(),
            AuthorityScope::Holding {
                holding_id: stable_holding_scope_id("holding-a"),
            }
        );
    }

    #[test]
    fn holding_connector_requires_matching_holding_scope() {
        let at = Utc::now();
        let binding = HoldingBinding::new("holding-a", "holding-a").expect("binding");
        let control = ConnectorControl {
            connector_id: Uuid::new_v4(),
            kind: PlatformConnectorKind::GenericLdap,
            scope: PlatformConnectorScope::Holding(binding.clone()),
            provider_key: "directory-primary".into(),
            credential_ref: "secret-manager/directory-primary".into(),
            configuration_digest: "00".repeat(32),
            capabilities: ["full_import".into(), "change_feed".into()]
                .into_iter()
                .collect(),
            state: PlatformConnectorState::Pending,
            version: 1,
            updated_at: at,
        };
        let command = ConnectorControlCommand {
            control,
            actor_ref: "holding-admin".into(),
            correlation_id: Uuid::new_v4(),
            idempotency_key: "connector-holding-1".into(),
            at,
        };

        let mut matching = proof("platform.connector.manage", at);
        matching.principal_ref = "holding-admin".into();
        matching.scope = binding.scope();
        assert!(command.validate(&matching).is_ok());

        let mut wrong_holding = matching;
        wrong_holding.scope = AuthorityScope::Holding {
            holding_id: stable_holding_scope_id("holding-b"),
        };
        assert_eq!(
            command.validate(&wrong_holding),
            Err(PlatformCompletionError::ScopeDenied)
        );
    }

    #[test]
    fn reconciliation_report_state_is_bound_to_drift_evidence_and_binding() {
        let binding = HoldingBinding::new("holding-a", "holding-a").unwrap();
        let mut report = PlatformReconciliationReport {
            report_id: Uuid::from_u128(900),
            binding: binding.clone(),
            observed_at: Utc::now(),
            state: ReconciliationState::Healthy,
            registry_state: "active".into(),
            provisioning_state: "active".into(),
            write_healthy: true,
            read_healthy: true,
            storage_healthy: true,
            replacement_read_state: None,
            latest_backup_verified: true,
            connector_count: 1,
            drift_codes: BTreeSet::new(),
        };
        assert!(report.validate().is_ok());

        report.state = ReconciliationState::Drifted;
        assert_eq!(
            report.validate(),
            Err(PlatformCompletionError::InvalidReconciliationState)
        );
        report.drift_codes.insert("read_model_stale".into());
        assert!(report.validate().is_ok());

        report.state = ReconciliationState::Healthy;
        assert_eq!(
            report.validate(),
            Err(PlatformCompletionError::InvalidReconciliationState)
        );
        report.binding = HoldingBinding {
            holding_id: String::new(),
            data_profile_key: binding.data_profile_key,
        };
        assert_eq!(
            report.validate(),
            Err(PlatformCompletionError::EmptyField {
                field: "holding_id"
            })
        );
    }
}
