use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

const MAX_TITLE_BYTES: usize = 512;
const MAX_REF_BYTES: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlatformGovernanceRecordError {
    #[error("platform governance record identifier cannot be nil")]
    NilIdentifier,
    #[error("platform governance record identifier must be UUID v4")]
    InvalidIdentifier,
    #[error("platform governance record requires platform or holding scope")]
    InvalidScope,
    #[error("platform governance record title or opaque reference is invalid")]
    InvalidText,
    #[error("platform governance transition is not allowed")]
    InvalidTransition,
    #[error("platform governance transition requires evidence")]
    EvidenceRequired,
    #[error("platform governance record version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("platform governance record version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformGovernanceRecordKind {
    DataPlaneOperation,
    Migration,
    Release,
}

impl PlatformGovernanceRecordKind {
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::DataPlaneOperation => "platform.data-plane-operation",
            Self::Migration => "platform.migration",
            Self::Release => "platform.release",
        }
    }

    const fn initial(self) -> PlatformGovernanceRecordStatus {
        match self {
            Self::DataPlaneOperation => PlatformGovernanceRecordStatus::Requested,
            Self::Migration => PlatformGovernanceRecordStatus::Proposed,
            Self::Release => PlatformGovernanceRecordStatus::Draft,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformGovernanceRecordStatus {
    Requested,
    Provisioning,
    Verified,
    Active,
    Suspended,
    Proposed,
    Approved,
    Rejected,
    Running,
    Draft,
    Ready,
    Deploying,
    Verifying,
    Completed,
    Released,
    Failed,
    RolledBack,
    Retired,
    Cancelled,
}

impl PlatformGovernanceRecordStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Provisioning => "provisioning",
            Self::Verified => "verified",
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Proposed => "proposed",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Running => "running",
            Self::Draft => "draft",
            Self::Ready => "ready",
            Self::Deploying => "deploying",
            Self::Verifying => "verifying",
            Self::Completed => "completed",
            Self::Released => "released",
            Self::Failed => "failed",
            Self::RolledBack => "rolled_back",
            Self::Retired => "retired",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformGovernanceRecord {
    id: Uuid,
    version: u64,
    kind: PlatformGovernanceRecordKind,
    scope: TenantScope,
    title: String,
    subject_ref: String,
    evidence_ref: Option<String>,
    status: PlatformGovernanceRecordStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl PlatformGovernanceRecord {
    pub fn create(
        id: Uuid,
        kind: PlatformGovernanceRecordKind,
        scope: TenantScope,
        title: String,
        subject_ref: String,
        at: DateTime<Utc>,
    ) -> Result<Self, PlatformGovernanceRecordError> {
        if id.is_nil() {
            return Err(PlatformGovernanceRecordError::NilIdentifier);
        }
        if id.get_version_num() != 4 {
            return Err(PlatformGovernanceRecordError::InvalidIdentifier);
        }
        validate_scope(&scope)?;
        validate_text(&title, MAX_TITLE_BYTES)?;
        validate_opaque_ref(&subject_ref)?;
        Ok(Self {
            id,
            version: 1,
            kind,
            scope,
            title,
            subject_ref,
            evidence_ref: None,
            status: kind.initial(),
            created_at: at,
            updated_at: at,
        })
    }

    pub fn transition(
        &mut self,
        expected: u64,
        next: PlatformGovernanceRecordStatus,
        evidence_ref: Option<String>,
        at: DateTime<Utc>,
    ) -> Result<(), PlatformGovernanceRecordError> {
        if self.version != expected {
            return Err(PlatformGovernanceRecordError::VersionConflict {
                expected,
                actual: self.version,
            });
        }
        if at < self.updated_at || !allowed(self.kind, self.status, next) {
            return Err(PlatformGovernanceRecordError::InvalidTransition);
        }
        if evidence_required(next) && evidence_ref.is_none() {
            return Err(PlatformGovernanceRecordError::EvidenceRequired);
        }
        if let Some(reference) = evidence_ref.as_deref() {
            validate_opaque_ref(reference)?;
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(PlatformGovernanceRecordError::VersionOverflow)?;
        self.status = next;
        self.evidence_ref = evidence_ref;
        self.updated_at = at;
        Ok(())
    }

    pub fn validate_rehydrated(&self) -> Result<(), PlatformGovernanceRecordError> {
        if self.id.is_nil() || self.id.get_version_num() != 4 {
            return Err(PlatformGovernanceRecordError::InvalidIdentifier);
        }
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(PlatformGovernanceRecordError::InvalidTransition);
        }
        validate_scope(&self.scope)?;
        validate_text(&self.title, MAX_TITLE_BYTES)?;
        validate_opaque_ref(&self.subject_ref)?;
        if let Some(reference) = self.evidence_ref.as_deref() {
            validate_opaque_ref(reference)?;
        }
        Ok(())
    }

    pub const fn id(&self) -> Uuid {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn kind(&self) -> PlatformGovernanceRecordKind {
        self.kind
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn subject_ref(&self) -> &str {
        &self.subject_ref
    }
    pub fn evidence_ref(&self) -> Option<&str> {
        self.evidence_ref.as_deref()
    }
    pub const fn status(&self) -> PlatformGovernanceRecordStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

const fn evidence_required(status: PlatformGovernanceRecordStatus) -> bool {
    matches!(
        status,
        PlatformGovernanceRecordStatus::Verified
            | PlatformGovernanceRecordStatus::Approved
            | PlatformGovernanceRecordStatus::Completed
            | PlatformGovernanceRecordStatus::Released
            | PlatformGovernanceRecordStatus::RolledBack
    )
}

const fn allowed(
    kind: PlatformGovernanceRecordKind,
    current: PlatformGovernanceRecordStatus,
    next: PlatformGovernanceRecordStatus,
) -> bool {
    use PlatformGovernanceRecordKind as K;
    use PlatformGovernanceRecordStatus as S;
    matches!(
        (kind, current, next),
        (
            K::DataPlaneOperation,
            S::Requested,
            S::Provisioning | S::Cancelled
        ) | (
            K::DataPlaneOperation,
            S::Provisioning,
            S::Verified | S::Failed
        ) | (K::DataPlaneOperation, S::Verified, S::Active | S::Failed)
            | (
                K::DataPlaneOperation,
                S::Active,
                S::Suspended | S::Failed | S::Retired
            )
            | (
                K::DataPlaneOperation,
                S::Suspended,
                S::Active | S::Failed | S::Retired
            )
            | (
                K::DataPlaneOperation,
                S::Failed,
                S::Provisioning | S::Retired
            )
            | (
                K::Migration,
                S::Proposed,
                S::Approved | S::Rejected | S::Cancelled
            )
            | (K::Migration, S::Approved, S::Running | S::Cancelled)
            | (K::Migration, S::Running, S::Verifying | S::Failed)
            | (
                K::Migration,
                S::Verifying,
                S::Completed | S::Failed | S::RolledBack
            )
            | (
                K::Migration,
                S::Failed,
                S::Running | S::RolledBack | S::Cancelled
            )
            | (K::Migration, S::Completed, S::RolledBack)
            | (K::Release, S::Draft, S::Ready | S::Cancelled)
            | (K::Release, S::Ready, S::Deploying | S::Cancelled)
            | (
                K::Release,
                S::Deploying,
                S::Verifying | S::Failed | S::RolledBack
            )
            | (
                K::Release,
                S::Verifying,
                S::Released | S::Failed | S::RolledBack
            )
            | (
                K::Release,
                S::Failed,
                S::Deploying | S::RolledBack | S::Cancelled
            )
            | (K::Release, S::Released, S::RolledBack)
    )
}

fn validate_scope(scope: &TenantScope) -> Result<(), PlatformGovernanceRecordError> {
    scope
        .validate()
        .map_err(|_| PlatformGovernanceRecordError::InvalidScope)?;
    if matches!(scope, TenantScope::Platform | TenantScope::Holding { .. }) {
        Ok(())
    } else {
        Err(PlatformGovernanceRecordError::InvalidScope)
    }
}

fn validate_text(value: &str, maximum: usize) -> Result<(), PlatformGovernanceRecordError> {
    let value = value.trim();
    if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        Err(PlatformGovernanceRecordError::InvalidText)
    } else {
        Ok(())
    }
}

fn validate_opaque_ref(value: &str) -> Result<(), PlatformGovernanceRecordError> {
    validate_text(value, MAX_REF_BYTES)?;
    let lower = value.to_ascii_lowercase();
    if value.contains("://")
        || value.contains('@')
        || value.chars().any(char::is_whitespace)
        || ["password", "secret=", "token=", "private_key"]
            .iter()
            .any(|marker| lower.contains(marker))
    {
        Err(PlatformGovernanceRecordError::InvalidText)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 16, hour, 0, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn kinds_have_distinct_governed_paths_and_evidence_fences() {
        let mut migration = PlatformGovernanceRecord::create(
            Uuid::new_v4(),
            PlatformGovernanceRecordKind::Migration,
            TenantScope::Platform,
            "Generation two cutover".into(),
            "manifest:g2:abc".into(),
            at(8),
        )
        .unwrap();
        assert_eq!(
            migration.transition(1, PlatformGovernanceRecordStatus::Approved, None, at(9)),
            Err(PlatformGovernanceRecordError::EvidenceRequired)
        );
        migration
            .transition(
                1,
                PlatformGovernanceRecordStatus::Approved,
                Some("approval:change-42".into()),
                at(9),
            )
            .unwrap();
        assert!(
            migration
                .transition(2, PlatformGovernanceRecordStatus::Released, None, at(10))
                .is_err()
        );
    }
}
