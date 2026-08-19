use chrono::{DateTime, Utc};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MaintenanceLifecycleRecordError {
    #[error("Maintenance lifecycle record identifier cannot be nil")]
    NilIdentifier,
    #[error("Maintenance lifecycle record requires an exact company or branch scope")]
    InvalidScope,
    #[error("Maintenance lifecycle record title is invalid")]
    InvalidTitle,
    #[error("Maintenance lifecycle record transition is not allowed")]
    InvalidTransition,
    #[error("Maintenance lifecycle record version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Maintenance lifecycle record version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceLifecycleRecordKind {
    Plan,
    Inspection,
    Request,
}
impl MaintenanceLifecycleRecordKind {
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Plan => "maintenance.plan",
            Self::Inspection => "maintenance.inspection",
            Self::Request => "maintenance.request",
        }
    }
    const fn initial(self) -> MaintenanceLifecycleRecordStatus {
        match self {
            Self::Plan => MaintenanceLifecycleRecordStatus::Draft,
            Self::Inspection => MaintenanceLifecycleRecordStatus::Scheduled,
            Self::Request => MaintenanceLifecycleRecordStatus::Open,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceLifecycleRecordStatus {
    Draft,
    Scheduled,
    Open,
    Triaged,
    Approved,
    Rejected,
    Active,
    Suspended,
    Retired,
    InProgress,
    Passed,
    Failed,
    Converted,
    Closed,
    Cancelled,
}
impl MaintenanceLifecycleRecordStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Scheduled => "scheduled",
            Self::Open => "open",
            Self::Triaged => "triaged",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Retired => "retired",
            Self::InProgress => "in_progress",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Converted => "converted",
            Self::Closed => "closed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceLifecycleRecord {
    id: Uuid,
    version: u64,
    kind: MaintenanceLifecycleRecordKind,
    scope: AuthorityScope,
    title: String,
    status: MaintenanceLifecycleRecordStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl MaintenanceLifecycleRecord {
    pub fn create(
        id: Uuid,
        kind: MaintenanceLifecycleRecordKind,
        scope: AuthorityScope,
        title: String,
        at: DateTime<Utc>,
    ) -> Result<Self, MaintenanceLifecycleRecordError> {
        if id.is_nil() {
            return Err(MaintenanceLifecycleRecordError::NilIdentifier);
        }
        validate_scope(&scope)?;
        validate_title(&title)?;
        Ok(Self {
            id,
            version: 1,
            kind,
            scope,
            title,
            status: kind.initial(),
            created_at: at,
            updated_at: at,
        })
    }
    pub fn transition(
        &mut self,
        expected: u64,
        next: MaintenanceLifecycleRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), MaintenanceLifecycleRecordError> {
        if self.version != expected {
            return Err(MaintenanceLifecycleRecordError::VersionConflict {
                expected,
                actual: self.version,
            });
        }
        if at < self.updated_at || !allowed(self.kind, self.status, next) {
            return Err(MaintenanceLifecycleRecordError::InvalidTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(MaintenanceLifecycleRecordError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        Ok(())
    }
    pub fn validate_rehydrated(&self) -> Result<(), MaintenanceLifecycleRecordError> {
        if self.id.is_nil() || self.version == 0 || self.updated_at < self.created_at {
            return Err(MaintenanceLifecycleRecordError::InvalidTransition);
        }
        validate_scope(&self.scope)?;
        validate_title(&self.title)
    }
    pub const fn id(&self) -> Uuid {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn kind(&self) -> MaintenanceLifecycleRecordKind {
        self.kind
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn status(&self) -> MaintenanceLifecycleRecordStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}
const fn allowed(
    kind: MaintenanceLifecycleRecordKind,
    current: MaintenanceLifecycleRecordStatus,
    next: MaintenanceLifecycleRecordStatus,
) -> bool {
    use MaintenanceLifecycleRecordKind as K;
    use MaintenanceLifecycleRecordStatus as S;
    matches!(
        (kind, current, next),
        (K::Plan, S::Draft, S::Approved | S::Cancelled)
            | (K::Plan, S::Approved, S::Active)
            | (K::Plan, S::Active, S::Suspended | S::Retired)
            | (K::Plan, S::Suspended, S::Active | S::Retired)
            | (K::Inspection, S::Scheduled, S::InProgress | S::Cancelled)
            | (K::Inspection, S::InProgress, S::Passed | S::Failed)
            | (K::Inspection, S::Passed | S::Failed, S::Closed)
            | (K::Request, S::Open, S::Triaged | S::Cancelled)
            | (K::Request, S::Triaged, S::Approved | S::Rejected)
            | (K::Request, S::Approved, S::Converted)
            | (K::Request, S::Converted | S::Rejected, S::Closed)
    )
}
fn validate_scope(scope: &AuthorityScope) -> Result<(), MaintenanceLifecycleRecordError> {
    scope
        .validate()
        .map_err(|_| MaintenanceLifecycleRecordError::InvalidScope)?;
    if matches!(
        scope,
        AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(MaintenanceLifecycleRecordError::InvalidScope)
    }
}
fn validate_title(value: &str) -> Result<(), MaintenanceLifecycleRecordError> {
    let v = value.trim();
    if v.is_empty() || v.len() > 1024 || v.chars().any(char::is_control) {
        Err(MaintenanceLifecycleRecordError::InvalidTitle)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn inspection_and_request_are_governed_separately() {
        let scope = AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let at = Utc.with_ymd_and_hms(2026, 8, 16, 8, 0, 0).single().unwrap();
        let mut inspection = MaintenanceLifecycleRecord::create(
            Uuid::from_u128(3),
            MaintenanceLifecycleRecordKind::Inspection,
            scope,
            "Inspect compressor".into(),
            at,
        )
        .unwrap();
        assert!(
            inspection
                .transition(1, MaintenanceLifecycleRecordStatus::Closed, at)
                .is_err()
        );
        inspection
            .transition(1, MaintenanceLifecycleRecordStatus::InProgress, at)
            .unwrap();
        inspection
            .transition(2, MaintenanceLifecycleRecordStatus::Passed, at)
            .unwrap();
        assert_eq!(inspection.version(), 3);
    }
}
