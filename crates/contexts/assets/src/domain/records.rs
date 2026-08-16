use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AssetLifecycleRecordError {
    #[error("Asset lifecycle record identifier cannot be nil")]
    NilIdentifier,
    #[error("Asset lifecycle record requires an exact company or branch scope")]
    InvalidScope,
    #[error("Asset lifecycle record title is invalid")]
    InvalidTitle,
    #[error("Asset lifecycle record transition is not allowed")]
    InvalidTransition,
    #[error("Asset lifecycle record version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Asset lifecycle record version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetLifecycleRecordKind {
    Transfer,
    DepreciationRun,
    Disposal,
}
impl AssetLifecycleRecordKind {
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Transfer => "asset.transfer",
            Self::DepreciationRun => "asset.depreciation-run",
            Self::Disposal => "asset.disposal",
        }
    }
    const fn initial(self) -> AssetLifecycleRecordStatus {
        match self {
            Self::Transfer | Self::Disposal => AssetLifecycleRecordStatus::Proposed,
            Self::DepreciationRun => AssetLifecycleRecordStatus::Draft,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetLifecycleRecordStatus {
    Draft,
    Proposed,
    Calculated,
    Approved,
    Rejected,
    InTransit,
    Received,
    Posted,
    Disposed,
    Closed,
    Cancelled,
}
impl AssetLifecycleRecordStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Proposed => "proposed",
            Self::Calculated => "calculated",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::InTransit => "in_transit",
            Self::Received => "received",
            Self::Posted => "posted",
            Self::Disposed => "disposed",
            Self::Closed => "closed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetLifecycleRecord {
    id: Uuid,
    version: u64,
    kind: AssetLifecycleRecordKind,
    scope: TenantScope,
    title: String,
    status: AssetLifecycleRecordStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl AssetLifecycleRecord {
    pub fn create(
        id: Uuid,
        kind: AssetLifecycleRecordKind,
        scope: TenantScope,
        title: String,
        at: DateTime<Utc>,
    ) -> Result<Self, AssetLifecycleRecordError> {
        if id.is_nil() {
            return Err(AssetLifecycleRecordError::NilIdentifier);
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
        next: AssetLifecycleRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), AssetLifecycleRecordError> {
        if self.version != expected {
            return Err(AssetLifecycleRecordError::VersionConflict {
                expected,
                actual: self.version,
            });
        }
        if at < self.updated_at || !allowed(self.kind, self.status, next) {
            return Err(AssetLifecycleRecordError::InvalidTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(AssetLifecycleRecordError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        Ok(())
    }
    pub fn validate_rehydrated(&self) -> Result<(), AssetLifecycleRecordError> {
        if self.id.is_nil() || self.version == 0 || self.updated_at < self.created_at {
            return Err(AssetLifecycleRecordError::InvalidTransition);
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
    pub const fn kind(&self) -> AssetLifecycleRecordKind {
        self.kind
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn status(&self) -> AssetLifecycleRecordStatus {
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
    kind: AssetLifecycleRecordKind,
    current: AssetLifecycleRecordStatus,
    next: AssetLifecycleRecordStatus,
) -> bool {
    use AssetLifecycleRecordKind as K;
    use AssetLifecycleRecordStatus as S;
    matches!(
        (kind, current, next),
        (
            K::Transfer,
            S::Proposed,
            S::Approved | S::Rejected | S::Cancelled
        ) | (K::Transfer, S::Approved, S::InTransit | S::Cancelled)
            | (K::Transfer, S::InTransit, S::Received)
            | (K::Transfer, S::Received, S::Closed)
            | (K::DepreciationRun, S::Draft, S::Calculated | S::Cancelled)
            | (
                K::DepreciationRun,
                S::Calculated,
                S::Approved | S::Cancelled
            )
            | (K::DepreciationRun, S::Approved, S::Posted)
            | (K::DepreciationRun, S::Posted, S::Closed)
            | (
                K::Disposal,
                S::Proposed,
                S::Approved | S::Rejected | S::Cancelled
            )
            | (K::Disposal, S::Approved, S::Disposed)
            | (K::Disposal, S::Disposed, S::Closed)
    )
}
fn validate_scope(scope: &TenantScope) -> Result<(), AssetLifecycleRecordError> {
    scope
        .validate()
        .map_err(|_| AssetLifecycleRecordError::InvalidScope)?;
    if matches!(
        scope,
        TenantScope::Company { .. } | TenantScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(AssetLifecycleRecordError::InvalidScope)
    }
}
fn validate_title(value: &str) -> Result<(), AssetLifecycleRecordError> {
    let v = value.trim();
    if v.is_empty() || v.len() > 1024 || v.chars().any(char::is_control) {
        Err(AssetLifecycleRecordError::InvalidTitle)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn transfer_and_disposal_are_governed_separately() {
        let scope = TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let at = Utc.with_ymd_and_hms(2026, 8, 16, 8, 0, 0).single().unwrap();
        let mut transfer = AssetLifecycleRecord::create(
            Uuid::from_u128(3),
            AssetLifecycleRecordKind::Transfer,
            scope,
            "Move press to branch".into(),
            at,
        )
        .unwrap();
        assert!(
            transfer
                .transition(1, AssetLifecycleRecordStatus::Closed, at)
                .is_err()
        );
        transfer
            .transition(1, AssetLifecycleRecordStatus::Approved, at)
            .unwrap();
        transfer
            .transition(2, AssetLifecycleRecordStatus::InTransit, at)
            .unwrap();
        assert_eq!(transfer.version(), 3);
    }
}
