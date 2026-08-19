use chrono::{DateTime, Utc};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QualityLifecycleRecordError {
    #[error("Quality lifecycle record identifier cannot be nil")]
    NilIdentifier,
    #[error("Quality lifecycle record requires an exact company or branch scope")]
    InvalidScope,
    #[error("Quality lifecycle record title is invalid")]
    InvalidTitle,
    #[error("Quality lifecycle record transition is not allowed")]
    InvalidTransition,
    #[error("Quality lifecycle record version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Quality lifecycle record version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityLifecycleRecordKind {
    Plan,
    Nonconformance,
    CorrectiveAction,
}
impl QualityLifecycleRecordKind {
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Plan => "quality.plan",
            Self::Nonconformance => "quality.nonconformance",
            Self::CorrectiveAction => "quality.corrective-action",
        }
    }
    const fn initial(self) -> QualityLifecycleRecordStatus {
        match self {
            Self::Plan => QualityLifecycleRecordStatus::Draft,
            Self::Nonconformance => QualityLifecycleRecordStatus::Open,
            Self::CorrectiveAction => QualityLifecycleRecordStatus::Proposed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityLifecycleRecordStatus {
    Draft,
    Proposed,
    Open,
    Approved,
    Rejected,
    Active,
    Suspended,
    Retired,
    Investigating,
    Dispositioned,
    InProgress,
    Verified,
    VerificationFailed,
    Closed,
    Cancelled,
}
impl QualityLifecycleRecordStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Proposed => "proposed",
            Self::Open => "open",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Retired => "retired",
            Self::Investigating => "investigating",
            Self::Dispositioned => "dispositioned",
            Self::InProgress => "in_progress",
            Self::Verified => "verified",
            Self::VerificationFailed => "verification_failed",
            Self::Closed => "closed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityLifecycleRecord {
    id: Uuid,
    version: u64,
    kind: QualityLifecycleRecordKind,
    scope: AuthorityScope,
    title: String,
    status: QualityLifecycleRecordStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl QualityLifecycleRecord {
    pub fn create(
        id: Uuid,
        kind: QualityLifecycleRecordKind,
        scope: AuthorityScope,
        title: String,
        at: DateTime<Utc>,
    ) -> Result<Self, QualityLifecycleRecordError> {
        if id.is_nil() {
            return Err(QualityLifecycleRecordError::NilIdentifier);
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
        next: QualityLifecycleRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), QualityLifecycleRecordError> {
        if self.version != expected {
            return Err(QualityLifecycleRecordError::VersionConflict {
                expected,
                actual: self.version,
            });
        }
        if at < self.updated_at || !allowed(self.kind, self.status, next) {
            return Err(QualityLifecycleRecordError::InvalidTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(QualityLifecycleRecordError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        Ok(())
    }
    pub fn validate_rehydrated(&self) -> Result<(), QualityLifecycleRecordError> {
        if self.id.is_nil() || self.version == 0 || self.updated_at < self.created_at {
            return Err(QualityLifecycleRecordError::InvalidTransition);
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
    pub const fn kind(&self) -> QualityLifecycleRecordKind {
        self.kind
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn status(&self) -> QualityLifecycleRecordStatus {
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
    kind: QualityLifecycleRecordKind,
    current: QualityLifecycleRecordStatus,
    next: QualityLifecycleRecordStatus,
) -> bool {
    use QualityLifecycleRecordKind as K;
    use QualityLifecycleRecordStatus as S;
    matches!(
        (kind, current, next),
        (K::Plan, S::Draft, S::Approved | S::Cancelled)
            | (K::Plan, S::Approved, S::Active)
            | (K::Plan, S::Active, S::Suspended | S::Retired)
            | (K::Plan, S::Suspended, S::Active | S::Retired)
            | (K::Nonconformance, S::Open, S::Investigating | S::Cancelled)
            | (K::Nonconformance, S::Investigating, S::Dispositioned)
            | (K::Nonconformance, S::Dispositioned, S::Closed)
            | (K::CorrectiveAction, S::Proposed, S::Approved | S::Rejected)
            | (K::CorrectiveAction, S::Approved, S::InProgress)
            | (
                K::CorrectiveAction,
                S::InProgress,
                S::Verified | S::VerificationFailed
            )
            | (
                K::CorrectiveAction,
                S::VerificationFailed,
                S::InProgress | S::Closed
            )
            | (K::CorrectiveAction, S::Verified | S::Rejected, S::Closed)
    )
}
fn validate_scope(scope: &AuthorityScope) -> Result<(), QualityLifecycleRecordError> {
    scope
        .validate()
        .map_err(|_| QualityLifecycleRecordError::InvalidScope)?;
    if matches!(
        scope,
        AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(QualityLifecycleRecordError::InvalidScope)
    }
}
fn validate_title(value: &str) -> Result<(), QualityLifecycleRecordError> {
    let v = value.trim();
    if v.is_empty() || v.len() > 1024 || v.chars().any(char::is_control) {
        Err(QualityLifecycleRecordError::InvalidTitle)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn nonconformance_and_corrective_action_are_governed_separately() {
        let scope = AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let at = Utc.with_ymd_and_hms(2026, 8, 16, 8, 0, 0).single().unwrap();
        let mut nonconformance = QualityLifecycleRecord::create(
            Uuid::from_u128(3),
            QualityLifecycleRecordKind::Nonconformance,
            scope,
            "Supplier defect".into(),
            at,
        )
        .unwrap();
        assert!(
            nonconformance
                .transition(1, QualityLifecycleRecordStatus::Closed, at)
                .is_err()
        );
        nonconformance
            .transition(1, QualityLifecycleRecordStatus::Investigating, at)
            .unwrap();
        nonconformance
            .transition(2, QualityLifecycleRecordStatus::Dispositioned, at)
            .unwrap();
        assert_eq!(nonconformance.version(), 3);
    }
}
