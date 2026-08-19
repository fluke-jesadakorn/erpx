use chrono::{DateTime, Utc};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommercialLifecycleRecordError {
    #[error("Commercial lifecycle record identifier cannot be nil")]
    NilIdentifier,
    #[error("Commercial lifecycle record requires an exact company or branch scope")]
    InvalidScope,
    #[error("Commercial lifecycle record title is invalid")]
    InvalidTitle,
    #[error("Commercial lifecycle record transition is not allowed")]
    InvalidTransition,
    #[error("Commercial lifecycle record version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Commercial lifecycle record version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommercialLifecycleRecordKind {
    Account,
    Adjustment,
}
impl CommercialLifecycleRecordKind {
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Account => "commercial.account",
            Self::Adjustment => "commercial.adjustment",
        }
    }
    const fn initial(self) -> CommercialLifecycleRecordStatus {
        match self {
            Self::Account => CommercialLifecycleRecordStatus::Pending,
            Self::Adjustment => CommercialLifecycleRecordStatus::Proposed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommercialLifecycleRecordStatus {
    Proposed,
    Pending,
    Active,
    Suspended,
    Approved,
    Rejected,
    Applied,
    Closed,
    Cancelled,
}
impl CommercialLifecycleRecordStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Applied => "applied",
            Self::Closed => "closed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommercialLifecycleRecord {
    id: Uuid,
    version: u64,
    kind: CommercialLifecycleRecordKind,
    scope: AuthorityScope,
    title: String,
    status: CommercialLifecycleRecordStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl CommercialLifecycleRecord {
    pub fn create(
        id: Uuid,
        kind: CommercialLifecycleRecordKind,
        scope: AuthorityScope,
        title: String,
        at: DateTime<Utc>,
    ) -> Result<Self, CommercialLifecycleRecordError> {
        if id.is_nil() {
            return Err(CommercialLifecycleRecordError::NilIdentifier);
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
        next: CommercialLifecycleRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), CommercialLifecycleRecordError> {
        if self.version != expected {
            return Err(CommercialLifecycleRecordError::VersionConflict {
                expected,
                actual: self.version,
            });
        }
        if at < self.updated_at || !allowed(self.kind, self.status, next) {
            return Err(CommercialLifecycleRecordError::InvalidTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(CommercialLifecycleRecordError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        Ok(())
    }
    pub fn validate_rehydrated(&self) -> Result<(), CommercialLifecycleRecordError> {
        if self.id.is_nil() || self.version == 0 || self.updated_at < self.created_at {
            return Err(CommercialLifecycleRecordError::InvalidTransition);
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
    pub const fn kind(&self) -> CommercialLifecycleRecordKind {
        self.kind
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn status(&self) -> CommercialLifecycleRecordStatus {
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
    kind: CommercialLifecycleRecordKind,
    current: CommercialLifecycleRecordStatus,
    next: CommercialLifecycleRecordStatus,
) -> bool {
    use CommercialLifecycleRecordKind as K;
    use CommercialLifecycleRecordStatus as S;
    matches!(
        (kind, current, next),
        (K::Account, S::Pending, S::Active | S::Cancelled)
            | (K::Account, S::Active, S::Suspended | S::Closed)
            | (K::Account, S::Suspended, S::Active | S::Closed)
            | (
                K::Adjustment,
                S::Proposed,
                S::Approved | S::Rejected | S::Cancelled
            )
            | (K::Adjustment, S::Approved, S::Applied | S::Cancelled)
            | (K::Adjustment, S::Applied | S::Rejected, S::Closed)
    )
}
fn validate_scope(scope: &AuthorityScope) -> Result<(), CommercialLifecycleRecordError> {
    scope
        .validate()
        .map_err(|_| CommercialLifecycleRecordError::InvalidScope)?;
    if matches!(
        scope,
        AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(CommercialLifecycleRecordError::InvalidScope)
    }
}
fn validate_title(value: &str) -> Result<(), CommercialLifecycleRecordError> {
    let v = value.trim();
    if v.is_empty() || v.len() > 1024 || v.chars().any(char::is_control) {
        Err(CommercialLifecycleRecordError::InvalidTitle)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn account_and_adjustment_are_governed_separately() {
        let scope = AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let at = Utc.with_ymd_and_hms(2026, 8, 16, 8, 0, 0).single().unwrap();
        let mut account = CommercialLifecycleRecord::create(
            Uuid::from_u128(3),
            CommercialLifecycleRecordKind::Account,
            scope,
            "Enterprise account".into(),
            at,
        )
        .unwrap();
        assert!(
            account
                .transition(1, CommercialLifecycleRecordStatus::Closed, at)
                .is_err()
        );
        account
            .transition(1, CommercialLifecycleRecordStatus::Active, at)
            .unwrap();
        account
            .transition(2, CommercialLifecycleRecordStatus::Suspended, at)
            .unwrap();
        assert_eq!(account.version(), 3);
    }
}
