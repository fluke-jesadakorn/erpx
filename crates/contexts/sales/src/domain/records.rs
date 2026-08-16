use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SalesLifecycleRecordError {
    #[error("Sales lifecycle record identifier cannot be nil")]
    NilIdentifier,
    #[error("Sales lifecycle record requires an exact company or branch scope")]
    InvalidScope,
    #[error("Sales lifecycle record title is invalid")]
    InvalidTitle,
    #[error("Sales lifecycle record transition is not allowed")]
    InvalidTransition,
    #[error("Sales lifecycle record version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Sales lifecycle record version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SalesLifecycleRecordKind {
    Return,
    FulfillmentHandoff,
}
impl SalesLifecycleRecordKind {
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Return => "sales.return",
            Self::FulfillmentHandoff => "sales.fulfillment-handoff",
        }
    }
    const fn initial(self) -> SalesLifecycleRecordStatus {
        match self {
            Self::Return => SalesLifecycleRecordStatus::Requested,
            Self::FulfillmentHandoff => SalesLifecycleRecordStatus::Proposed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SalesLifecycleRecordStatus {
    Requested,
    Proposed,
    Accepted,
    Approved,
    Authorized,
    Rejected,
    Received,
    Inspected,
    InProgress,
    Completed,
    Failed,
    Closed,
    Cancelled,
}
impl SalesLifecycleRecordStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Approved => "approved",
            Self::Authorized => "authorized",
            Self::Rejected => "rejected",
            Self::Received => "received",
            Self::Inspected => "inspected",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Closed => "closed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesLifecycleRecord {
    id: Uuid,
    version: u64,
    kind: SalesLifecycleRecordKind,
    scope: TenantScope,
    title: String,
    status: SalesLifecycleRecordStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl SalesLifecycleRecord {
    pub fn create(
        id: Uuid,
        kind: SalesLifecycleRecordKind,
        scope: TenantScope,
        title: String,
        at: DateTime<Utc>,
    ) -> Result<Self, SalesLifecycleRecordError> {
        if id.is_nil() {
            return Err(SalesLifecycleRecordError::NilIdentifier);
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
        next: SalesLifecycleRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), SalesLifecycleRecordError> {
        if self.version != expected {
            return Err(SalesLifecycleRecordError::VersionConflict {
                expected,
                actual: self.version,
            });
        }
        if at < self.updated_at || !allowed(self.kind, self.status, next) {
            return Err(SalesLifecycleRecordError::InvalidTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(SalesLifecycleRecordError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        Ok(())
    }
    pub fn validate_rehydrated(&self) -> Result<(), SalesLifecycleRecordError> {
        if self.id.is_nil() || self.version == 0 || self.updated_at < self.created_at {
            return Err(SalesLifecycleRecordError::InvalidTransition);
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
    pub const fn kind(&self) -> SalesLifecycleRecordKind {
        self.kind
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn status(&self) -> SalesLifecycleRecordStatus {
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
    kind: SalesLifecycleRecordKind,
    current: SalesLifecycleRecordStatus,
    next: SalesLifecycleRecordStatus,
) -> bool {
    use SalesLifecycleRecordKind as K;
    use SalesLifecycleRecordStatus as S;
    matches!(
        (kind, current, next),
        (
            K::Return,
            S::Requested,
            S::Authorized | S::Rejected | S::Cancelled
        ) | (K::Return, S::Authorized, S::Received | S::Cancelled)
            | (K::Return, S::Received, S::Inspected)
            | (K::Return, S::Inspected, S::Approved | S::Rejected)
            | (K::Return, S::Approved | S::Rejected, S::Closed)
            | (
                K::FulfillmentHandoff,
                S::Proposed,
                S::Accepted | S::Rejected | S::Cancelled
            )
            | (
                K::FulfillmentHandoff,
                S::Accepted,
                S::InProgress | S::Cancelled
            )
            | (
                K::FulfillmentHandoff,
                S::InProgress,
                S::Completed | S::Failed
            )
            | (
                K::FulfillmentHandoff,
                S::Failed,
                S::InProgress | S::Cancelled
            )
            | (K::FulfillmentHandoff, S::Completed | S::Rejected, S::Closed)
    )
}
fn validate_scope(scope: &TenantScope) -> Result<(), SalesLifecycleRecordError> {
    scope
        .validate()
        .map_err(|_| SalesLifecycleRecordError::InvalidScope)?;
    if matches!(
        scope,
        TenantScope::Company { .. } | TenantScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(SalesLifecycleRecordError::InvalidScope)
    }
}
fn validate_title(value: &str) -> Result<(), SalesLifecycleRecordError> {
    let v = value.trim();
    if v.is_empty() || v.len() > 1024 || v.chars().any(char::is_control) {
        Err(SalesLifecycleRecordError::InvalidTitle)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn return_and_fulfillment_handoff_are_governed_separately() {
        let scope = TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let at = Utc.with_ymd_and_hms(2026, 8, 16, 8, 0, 0).single().unwrap();
        let mut handoff = SalesLifecycleRecord::create(
            Uuid::from_u128(3),
            SalesLifecycleRecordKind::FulfillmentHandoff,
            scope,
            "Ship order 1001".into(),
            at,
        )
        .unwrap();
        assert!(
            handoff
                .transition(1, SalesLifecycleRecordStatus::Closed, at)
                .is_err()
        );
        handoff
            .transition(1, SalesLifecycleRecordStatus::Accepted, at)
            .unwrap();
        handoff
            .transition(2, SalesLifecycleRecordStatus::InProgress, at)
            .unwrap();
        assert_eq!(handoff.version(), 3);
    }
}
