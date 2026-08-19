use chrono::{DateTime, Utc};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PosLifecycleRecordError {
    #[error("POS lifecycle record identifier cannot be nil")]
    NilIdentifier,
    #[error("POS lifecycle record requires an exact company or branch scope")]
    InvalidScope,
    #[error("POS lifecycle record title is invalid")]
    InvalidTitle,
    #[error("POS lifecycle record transition is not allowed")]
    InvalidTransition,
    #[error("POS lifecycle record version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("POS lifecycle record version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PosLifecycleRecordKind {
    RegisterSession,
    CashMovement,
}
impl PosLifecycleRecordKind {
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::RegisterSession => "pos.register-session",
            Self::CashMovement => "pos.cash-movement",
        }
    }
    const fn initial(self) -> PosLifecycleRecordStatus {
        match self {
            Self::RegisterSession => PosLifecycleRecordStatus::Opening,
            Self::CashMovement => PosLifecycleRecordStatus::Draft,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PosLifecycleRecordStatus {
    Opening,
    Open,
    Suspended,
    Reconciling,
    Draft,
    Approved,
    Rejected,
    Posted,
    Reversed,
    Closed,
    Cancelled,
}
impl PosLifecycleRecordStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Opening => "opening",
            Self::Open => "open",
            Self::Suspended => "suspended",
            Self::Reconciling => "reconciling",
            Self::Draft => "draft",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Posted => "posted",
            Self::Reversed => "reversed",
            Self::Closed => "closed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosLifecycleRecord {
    id: Uuid,
    version: u64,
    kind: PosLifecycleRecordKind,
    scope: AuthorityScope,
    title: String,
    status: PosLifecycleRecordStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl PosLifecycleRecord {
    pub fn create(
        id: Uuid,
        kind: PosLifecycleRecordKind,
        scope: AuthorityScope,
        title: String,
        at: DateTime<Utc>,
    ) -> Result<Self, PosLifecycleRecordError> {
        if id.is_nil() {
            return Err(PosLifecycleRecordError::NilIdentifier);
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
        next: PosLifecycleRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), PosLifecycleRecordError> {
        if self.version != expected {
            return Err(PosLifecycleRecordError::VersionConflict {
                expected,
                actual: self.version,
            });
        }
        if at < self.updated_at || !allowed(self.kind, self.status, next) {
            return Err(PosLifecycleRecordError::InvalidTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(PosLifecycleRecordError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        Ok(())
    }
    pub fn validate_rehydrated(&self) -> Result<(), PosLifecycleRecordError> {
        if self.id.is_nil() || self.version == 0 || self.updated_at < self.created_at {
            return Err(PosLifecycleRecordError::InvalidTransition);
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
    pub const fn kind(&self) -> PosLifecycleRecordKind {
        self.kind
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn status(&self) -> PosLifecycleRecordStatus {
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
    kind: PosLifecycleRecordKind,
    current: PosLifecycleRecordStatus,
    next: PosLifecycleRecordStatus,
) -> bool {
    use PosLifecycleRecordKind as K;
    use PosLifecycleRecordStatus as S;
    matches!(
        (kind, current, next),
        (K::RegisterSession, S::Opening, S::Open | S::Cancelled)
            | (K::RegisterSession, S::Open, S::Suspended | S::Reconciling)
            | (K::RegisterSession, S::Suspended, S::Open | S::Reconciling)
            | (K::RegisterSession, S::Reconciling, S::Closed)
            | (
                K::CashMovement,
                S::Draft,
                S::Approved | S::Rejected | S::Cancelled
            )
            | (K::CashMovement, S::Approved, S::Posted | S::Cancelled)
            | (K::CashMovement, S::Posted, S::Reversed | S::Closed)
            | (K::CashMovement, S::Reversed | S::Rejected, S::Closed)
    )
}
fn validate_scope(scope: &AuthorityScope) -> Result<(), PosLifecycleRecordError> {
    scope
        .validate()
        .map_err(|_| PosLifecycleRecordError::InvalidScope)?;
    if matches!(
        scope,
        AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(PosLifecycleRecordError::InvalidScope)
    }
}
fn validate_title(value: &str) -> Result<(), PosLifecycleRecordError> {
    let v = value.trim();
    if v.is_empty() || v.len() > 1024 || v.chars().any(char::is_control) {
        Err(PosLifecycleRecordError::InvalidTitle)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn register_session_and_cash_movement_are_governed_separately() {
        let scope = AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let at = Utc.with_ymd_and_hms(2026, 8, 16, 8, 0, 0).single().unwrap();
        let mut session = PosLifecycleRecord::create(
            Uuid::from_u128(3),
            PosLifecycleRecordKind::RegisterSession,
            scope,
            "Register A morning shift".into(),
            at,
        )
        .unwrap();
        assert!(
            session
                .transition(1, PosLifecycleRecordStatus::Closed, at)
                .is_err()
        );
        session
            .transition(1, PosLifecycleRecordStatus::Open, at)
            .unwrap();
        session
            .transition(2, PosLifecycleRecordStatus::Suspended, at)
            .unwrap();
        assert_eq!(session.version(), 3);
    }
}
