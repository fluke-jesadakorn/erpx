use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OperationsLifecycleRecordError {
    #[error("Operations lifecycle record identifier cannot be nil")]
    NilIdentifier,
    #[error("Operations lifecycle record requires an exact company or branch scope")]
    InvalidScope,
    #[error("Operations lifecycle record title is invalid")]
    InvalidTitle,
    #[error("Operations lifecycle record transition is not allowed")]
    InvalidTransition,
    #[error("Operations lifecycle record version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Operations lifecycle record version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationsLifecycleRecordKind {
    Operation,
    Case,
    ApprovalWork,
}
impl OperationsLifecycleRecordKind {
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Operation => "operations.operation",
            Self::Case => "operations.case",
            Self::ApprovalWork => "operations.approval-work",
        }
    }
    const fn initial(self) -> OperationsLifecycleRecordStatus {
        match self {
            Self::Operation => OperationsLifecycleRecordStatus::Planned,
            Self::Case => OperationsLifecycleRecordStatus::Open,
            Self::ApprovalWork => OperationsLifecycleRecordStatus::Proposed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationsLifecycleRecordStatus {
    Planned,
    Proposed,
    Open,
    Scheduled,
    Pending,
    Approved,
    Rejected,
    Investigating,
    InProgress,
    Waiting,
    Resolved,
    Completed,
    Expired,
    Closed,
    Cancelled,
}
impl OperationsLifecycleRecordStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Proposed => "proposed",
            Self::Open => "open",
            Self::Scheduled => "scheduled",
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Investigating => "investigating",
            Self::InProgress => "in_progress",
            Self::Waiting => "waiting",
            Self::Resolved => "resolved",
            Self::Completed => "completed",
            Self::Expired => "expired",
            Self::Closed => "closed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsLifecycleRecord {
    id: Uuid,
    version: u64,
    kind: OperationsLifecycleRecordKind,
    scope: TenantScope,
    title: String,
    status: OperationsLifecycleRecordStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl OperationsLifecycleRecord {
    pub fn create(
        id: Uuid,
        kind: OperationsLifecycleRecordKind,
        scope: TenantScope,
        title: String,
        at: DateTime<Utc>,
    ) -> Result<Self, OperationsLifecycleRecordError> {
        if id.is_nil() {
            return Err(OperationsLifecycleRecordError::NilIdentifier);
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
        next: OperationsLifecycleRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), OperationsLifecycleRecordError> {
        if self.version != expected {
            return Err(OperationsLifecycleRecordError::VersionConflict {
                expected,
                actual: self.version,
            });
        }
        if at < self.updated_at || !allowed(self.kind, self.status, next) {
            return Err(OperationsLifecycleRecordError::InvalidTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(OperationsLifecycleRecordError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        Ok(())
    }
    pub fn validate_rehydrated(&self) -> Result<(), OperationsLifecycleRecordError> {
        if self.id.is_nil() || self.version == 0 || self.updated_at < self.created_at {
            return Err(OperationsLifecycleRecordError::InvalidTransition);
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
    pub const fn kind(&self) -> OperationsLifecycleRecordKind {
        self.kind
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn status(&self) -> OperationsLifecycleRecordStatus {
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
    kind: OperationsLifecycleRecordKind,
    current: OperationsLifecycleRecordStatus,
    next: OperationsLifecycleRecordStatus,
) -> bool {
    use OperationsLifecycleRecordKind as K;
    use OperationsLifecycleRecordStatus as S;
    matches!(
        (kind, current, next),
        (K::Operation, S::Planned, S::Scheduled | S::Cancelled)
            | (K::Operation, S::Scheduled, S::InProgress | S::Cancelled)
            | (
                K::Operation,
                S::InProgress,
                S::Waiting | S::Completed | S::Cancelled
            )
            | (K::Operation, S::Waiting, S::InProgress | S::Cancelled)
            | (K::Operation, S::Completed, S::Closed)
            | (K::Case, S::Open, S::Investigating | S::Cancelled)
            | (K::Case, S::Investigating, S::Waiting | S::Resolved)
            | (K::Case, S::Waiting, S::Investigating | S::Resolved)
            | (K::Case, S::Resolved, S::Closed)
            | (K::ApprovalWork, S::Proposed, S::Pending | S::Cancelled)
            | (
                K::ApprovalWork,
                S::Pending,
                S::Approved | S::Rejected | S::Expired
            )
            | (K::ApprovalWork, S::Approved, S::Completed)
            | (
                K::ApprovalWork,
                S::Completed | S::Rejected | S::Expired,
                S::Closed
            )
    )
}
fn validate_scope(scope: &TenantScope) -> Result<(), OperationsLifecycleRecordError> {
    scope
        .validate()
        .map_err(|_| OperationsLifecycleRecordError::InvalidScope)?;
    if matches!(
        scope,
        TenantScope::Company { .. } | TenantScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(OperationsLifecycleRecordError::InvalidScope)
    }
}
fn validate_title(value: &str) -> Result<(), OperationsLifecycleRecordError> {
    let v = value.trim();
    if v.is_empty() || v.len() > 1024 || v.chars().any(char::is_control) {
        Err(OperationsLifecycleRecordError::InvalidTitle)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn operation_and_approval_work_are_governed_separately() {
        let scope = TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let at = Utc.with_ymd_and_hms(2026, 8, 16, 8, 0, 0).single().unwrap();
        let mut operation = OperationsLifecycleRecord::create(
            Uuid::from_u128(3),
            OperationsLifecycleRecordKind::Operation,
            scope,
            "Month end close".into(),
            at,
        )
        .unwrap();
        assert!(
            operation
                .transition(1, OperationsLifecycleRecordStatus::Closed, at)
                .is_err()
        );
        operation
            .transition(1, OperationsLifecycleRecordStatus::Scheduled, at)
            .unwrap();
        operation
            .transition(2, OperationsLifecycleRecordStatus::InProgress, at)
            .unwrap();
        assert_eq!(operation.version(), 3);
    }
}
