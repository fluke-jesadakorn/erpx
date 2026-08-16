use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InventoryLifecycleRecordError {
    #[error("Inventory lifecycle record identifier cannot be nil")]
    NilIdentifier,
    #[error("Inventory lifecycle record requires an exact company or branch scope")]
    InvalidScope,
    #[error("Inventory lifecycle record title is invalid")]
    InvalidTitle,
    #[error("Inventory lifecycle record transition is not allowed")]
    InvalidTransition,
    #[error("Inventory lifecycle record version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Inventory lifecycle record version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryLifecycleRecordKind {
    Document,
    Transfer,
    Count,
    Lot,
    Serial,
}
impl InventoryLifecycleRecordKind {
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Document => "inventory.document",
            Self::Transfer => "inventory.transfer",
            Self::Count => "inventory.count",
            Self::Lot => "inventory.lot",
            Self::Serial => "inventory.serial",
        }
    }
    const fn initial(self) -> InventoryLifecycleRecordStatus {
        match self {
            Self::Document => InventoryLifecycleRecordStatus::Draft,
            Self::Transfer => InventoryLifecycleRecordStatus::Proposed,
            Self::Count => InventoryLifecycleRecordStatus::Planned,
            Self::Lot => InventoryLifecycleRecordStatus::Active,
            Self::Serial => InventoryLifecycleRecordStatus::Registered,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryLifecycleRecordStatus {
    Draft,
    Proposed,
    Planned,
    Active,
    Registered,
    Approved,
    Rejected,
    Posted,
    Reversed,
    InTransit,
    Received,
    InProgress,
    Reconciled,
    Quarantined,
    Expired,
    InStock,
    Allocated,
    Shipped,
    Returned,
    Retired,
    Closed,
    Cancelled,
}
impl InventoryLifecycleRecordStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Proposed => "proposed",
            Self::Planned => "planned",
            Self::Active => "active",
            Self::Registered => "registered",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Posted => "posted",
            Self::Reversed => "reversed",
            Self::InTransit => "in_transit",
            Self::Received => "received",
            Self::InProgress => "in_progress",
            Self::Reconciled => "reconciled",
            Self::Quarantined => "quarantined",
            Self::Expired => "expired",
            Self::InStock => "in_stock",
            Self::Allocated => "allocated",
            Self::Shipped => "shipped",
            Self::Returned => "returned",
            Self::Retired => "retired",
            Self::Closed => "closed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryLifecycleRecord {
    id: Uuid,
    version: u64,
    kind: InventoryLifecycleRecordKind,
    scope: TenantScope,
    title: String,
    status: InventoryLifecycleRecordStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl InventoryLifecycleRecord {
    pub fn create(
        id: Uuid,
        kind: InventoryLifecycleRecordKind,
        scope: TenantScope,
        title: String,
        at: DateTime<Utc>,
    ) -> Result<Self, InventoryLifecycleRecordError> {
        if id.is_nil() {
            return Err(InventoryLifecycleRecordError::NilIdentifier);
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
        next: InventoryLifecycleRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), InventoryLifecycleRecordError> {
        if self.version != expected {
            return Err(InventoryLifecycleRecordError::VersionConflict {
                expected,
                actual: self.version,
            });
        }
        if at < self.updated_at || !allowed(self.kind, self.status, next) {
            return Err(InventoryLifecycleRecordError::InvalidTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(InventoryLifecycleRecordError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        Ok(())
    }
    pub fn validate_rehydrated(&self) -> Result<(), InventoryLifecycleRecordError> {
        if self.id.is_nil() || self.version == 0 || self.updated_at < self.created_at {
            return Err(InventoryLifecycleRecordError::InvalidTransition);
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
    pub const fn kind(&self) -> InventoryLifecycleRecordKind {
        self.kind
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn status(&self) -> InventoryLifecycleRecordStatus {
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
    kind: InventoryLifecycleRecordKind,
    current: InventoryLifecycleRecordStatus,
    next: InventoryLifecycleRecordStatus,
) -> bool {
    use InventoryLifecycleRecordKind as K;
    use InventoryLifecycleRecordStatus as S;
    matches!(
        (kind, current, next),
        (K::Document, S::Draft, S::Posted | S::Cancelled)
            | (K::Document, S::Posted, S::Reversed | S::Closed)
            | (K::Document, S::Reversed, S::Closed)
            | (
                K::Transfer,
                S::Proposed,
                S::Approved | S::Rejected | S::Cancelled
            )
            | (K::Transfer, S::Approved, S::InTransit | S::Cancelled)
            | (K::Transfer, S::InTransit, S::Received)
            | (K::Transfer, S::Received, S::Closed)
            | (K::Count, S::Planned, S::InProgress | S::Cancelled)
            | (K::Count, S::InProgress, S::Reconciled)
            | (K::Count, S::Reconciled, S::Approved | S::Rejected)
            | (K::Count, S::Approved | S::Rejected, S::Closed)
            | (K::Lot, S::Active, S::Quarantined | S::Expired | S::Closed)
            | (K::Lot, S::Quarantined, S::Active | S::Closed)
            | (K::Lot, S::Expired, S::Closed)
            | (K::Serial, S::Registered, S::InStock | S::Retired)
            | (
                K::Serial,
                S::InStock,
                S::Allocated | S::Shipped | S::Retired
            )
            | (
                K::Serial,
                S::Allocated,
                S::InStock | S::Shipped | S::Retired
            )
            | (K::Serial, S::Shipped, S::Returned | S::Retired)
            | (K::Serial, S::Returned, S::InStock | S::Retired)
    )
}
fn validate_scope(scope: &TenantScope) -> Result<(), InventoryLifecycleRecordError> {
    scope
        .validate()
        .map_err(|_| InventoryLifecycleRecordError::InvalidScope)?;
    if matches!(
        scope,
        TenantScope::Company { .. } | TenantScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(InventoryLifecycleRecordError::InvalidScope)
    }
}
fn validate_title(value: &str) -> Result<(), InventoryLifecycleRecordError> {
    let v = value.trim();
    if v.is_empty() || v.len() > 1024 || v.chars().any(char::is_control) {
        Err(InventoryLifecycleRecordError::InvalidTitle)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn transfer_and_serial_are_governed_separately() {
        let scope = TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let at = Utc.with_ymd_and_hms(2026, 8, 16, 8, 0, 0).single().unwrap();
        let mut transfer = InventoryLifecycleRecord::create(
            Uuid::from_u128(3),
            InventoryLifecycleRecordKind::Transfer,
            scope,
            "Transfer stock to branch".into(),
            at,
        )
        .unwrap();
        assert!(
            transfer
                .transition(1, InventoryLifecycleRecordStatus::Closed, at)
                .is_err()
        );
        transfer
            .transition(1, InventoryLifecycleRecordStatus::Approved, at)
            .unwrap();
        transfer
            .transition(2, InventoryLifecycleRecordStatus::InTransit, at)
            .unwrap();
        assert_eq!(transfer.version(), 3);
    }
}
