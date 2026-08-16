use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProcurementRecordError {
    #[error("Procurement record identifier cannot be nil")]
    NilIdentifier,
    #[error("Procurement record requires an exact company or branch scope")]
    InvalidScope,
    #[error("Procurement record title is invalid")]
    InvalidTitle,
    #[error("Procurement record transition is not allowed")]
    InvalidTransition,
    #[error("Procurement record version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Procurement record version overflowed")]
    VersionOverflow,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcurementRecordKind {
    Requisition,
    Rfq,
    Receipt,
    SupplierReturn,
}
impl ProcurementRecordKind {
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Requisition => "procurement.requisition",
            Self::Rfq => "procurement.rfq",
            Self::Receipt => "procurement.receipt",
            Self::SupplierReturn => "procurement.supplier-return",
        }
    }
    const fn initial(self) -> ProcurementRecordStatus {
        match self {
            Self::Requisition | Self::Rfq => ProcurementRecordStatus::Draft,
            Self::Receipt => ProcurementRecordStatus::Expected,
            Self::SupplierReturn => ProcurementRecordStatus::Proposed,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcurementRecordStatus {
    Draft,
    Expected,
    Proposed,
    Submitted,
    Approved,
    Rejected,
    Issued,
    Received,
    Accepted,
    Authorized,
    Shipped,
    Closed,
    Cancelled,
}
impl ProcurementRecordStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Expected => "expected",
            Self::Proposed => "proposed",
            Self::Submitted => "submitted",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Issued => "issued",
            Self::Received => "received",
            Self::Accepted => "accepted",
            Self::Authorized => "authorized",
            Self::Shipped => "shipped",
            Self::Closed => "closed",
            Self::Cancelled => "cancelled",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcurementRecord {
    id: Uuid,
    version: u64,
    kind: ProcurementRecordKind,
    scope: TenantScope,
    title: String,
    status: ProcurementRecordStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl ProcurementRecord {
    pub fn create(
        id: Uuid,
        kind: ProcurementRecordKind,
        scope: TenantScope,
        title: String,
        at: DateTime<Utc>,
    ) -> Result<Self, ProcurementRecordError> {
        if id.is_nil() {
            return Err(ProcurementRecordError::NilIdentifier);
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
        next: ProcurementRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), ProcurementRecordError> {
        if self.version != expected {
            return Err(ProcurementRecordError::VersionConflict {
                expected,
                actual: self.version,
            });
        }
        if at < self.updated_at || !allowed(self.kind, self.status, next) {
            return Err(ProcurementRecordError::InvalidTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(ProcurementRecordError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        Ok(())
    }
    pub fn validate_rehydrated(&self) -> Result<(), ProcurementRecordError> {
        if self.id.is_nil() || self.version == 0 || self.updated_at < self.created_at {
            return Err(ProcurementRecordError::InvalidTransition);
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
    pub const fn kind(&self) -> ProcurementRecordKind {
        self.kind
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn status(&self) -> ProcurementRecordStatus {
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
    kind: ProcurementRecordKind,
    current: ProcurementRecordStatus,
    next: ProcurementRecordStatus,
) -> bool {
    use ProcurementRecordKind as K;
    use ProcurementRecordStatus as S;
    matches!(
        (kind, current, next),
        (K::Requisition, S::Draft, S::Submitted | S::Cancelled)
            | (K::Requisition, S::Submitted, S::Approved | S::Rejected)
            | (K::Requisition, S::Approved, S::Closed)
            | (K::Rfq, S::Draft, S::Issued | S::Cancelled)
            | (K::Rfq, S::Issued, S::Closed | S::Cancelled)
            | (K::Receipt, S::Expected, S::Received)
            | (K::Receipt, S::Received, S::Accepted | S::Rejected)
            | (
                K::SupplierReturn,
                S::Proposed,
                S::Authorized | S::Rejected | S::Cancelled
            )
            | (K::SupplierReturn, S::Authorized, S::Shipped | S::Cancelled)
            | (K::SupplierReturn, S::Shipped, S::Closed)
    )
}
fn validate_scope(scope: &TenantScope) -> Result<(), ProcurementRecordError> {
    scope
        .validate()
        .map_err(|_| ProcurementRecordError::InvalidScope)?;
    if matches!(
        scope,
        TenantScope::Company { .. } | TenantScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(ProcurementRecordError::InvalidScope)
    }
}
fn validate_title(value: &str) -> Result<(), ProcurementRecordError> {
    let v = value.trim();
    if v.is_empty() || v.len() > 255 || v.chars().any(char::is_control) {
        Err(ProcurementRecordError::InvalidTitle)
    } else {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn receiving_and_returning_follow_distinct_lifecycles() {
        let scope = TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let at = Utc.with_ymd_and_hms(2026, 8, 16, 8, 0, 0).single().unwrap();
        let mut receipt = ProcurementRecord::create(
            Uuid::from_u128(3),
            ProcurementRecordKind::Receipt,
            scope,
            "Receipt 1".into(),
            at,
        )
        .unwrap();
        assert!(
            receipt
                .transition(1, ProcurementRecordStatus::Closed, at)
                .is_err()
        );
        receipt
            .transition(1, ProcurementRecordStatus::Received, at)
            .unwrap();
        receipt
            .transition(2, ProcurementRecordStatus::Accepted, at)
            .unwrap();
        assert_eq!(receipt.version(), 3);
    }
}
