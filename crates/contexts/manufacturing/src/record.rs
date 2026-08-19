use chrono::{DateTime, Utc};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ManufacturingDomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManufacturingRecordKind {
    Routing,
    Batch,
    Result,
}

impl ManufacturingRecordKind {
    pub const fn token(self) -> &'static str {
        match self {
            Self::Routing => "routing",
            Self::Batch => "batch",
            Self::Result => "result",
        }
    }
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Routing => "manufacturing.routing",
            Self::Batch => "manufacturing.batch",
            Self::Result => "manufacturing.result",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManufacturingRecordStatus {
    Proposed,
    Active,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManufacturingRecord {
    id: Uuid,
    version: u64,
    scope: AuthorityScope,
    kind: ManufacturingRecordKind,
    parent: StableRecordRef,
    evidence_digest: String,
    status: ManufacturingRecordStatus,
    updated_at: DateTime<Utc>,
}

impl ManufacturingRecord {
    pub fn propose(
        id: Uuid,
        scope: AuthorityScope,
        kind: ManufacturingRecordKind,
        parent: StableRecordRef,
        evidence_digest: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, ManufacturingDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            kind,
            parent,
            evidence_digest: evidence_digest.into(),
            status: ManufacturingRecordStatus::Proposed,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn activate(&mut self, at: DateTime<Utc>) -> Result<(), ManufacturingDomainError> {
        self.transition(
            ManufacturingRecordStatus::Proposed,
            ManufacturingRecordStatus::Active,
            at,
        )
    }
    pub fn complete(&mut self, at: DateTime<Utc>) -> Result<(), ManufacturingDomainError> {
        self.transition(
            ManufacturingRecordStatus::Active,
            ManufacturingRecordStatus::Completed,
            at,
        )
    }
    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), ManufacturingDomainError> {
        if matches!(
            self.status,
            ManufacturingRecordStatus::Completed | ManufacturingRecordStatus::Cancelled
        ) {
            return Err(ManufacturingDomainError::InvalidTransition);
        }
        self.status = ManufacturingRecordStatus::Cancelled;
        self.advance(at)
    }
    pub fn validate(&self) -> Result<(), ManufacturingDomainError> {
        self.scope.validate()?;
        self.parent.validate()?;
        if self.id.is_nil()
            || self.id.get_version_num() != 4
            || self.version == 0
            || self.evidence_digest.len() != 64
            || !self
                .evidence_digest
                .bytes()
                .all(|value| value.is_ascii_hexdigit())
        {
            return Err(ManufacturingDomainError::InvalidRecordLifecycle);
        }
        Ok(())
    }
    fn transition(
        &mut self,
        expected: ManufacturingRecordStatus,
        next: ManufacturingRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), ManufacturingDomainError> {
        if self.status != expected {
            return Err(ManufacturingDomainError::InvalidTransition);
        }
        self.status = next;
        self.advance(at)
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), ManufacturingDomainError> {
        if at < self.updated_at {
            return Err(ManufacturingDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(ManufacturingDomainError::Overflow)?;
        self.updated_at = at;
        Ok(())
    }
    pub const fn id(&self) -> Uuid {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn kind(&self) -> ManufacturingRecordKind {
        self.kind
    }
    pub const fn status(&self) -> ManufacturingRecordStatus {
        self.status
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn parent(&self) -> &StableRecordRef {
        &self.parent
    }
    pub fn evidence_digest(&self) -> &str {
        &self.evidence_digest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_integration_contracts::{RecordTypeId, catalog::ContextId};
    use erpx_kernel::RecordId;

    fn scope() -> AuthorityScope {
        AuthorityScope::Holding {
            holding_id: Uuid::new_v4(),
        }
    }
    fn parent() -> StableRecordRef {
        StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            ContextId::Manufacturing,
            RecordTypeId::try_new("manufacturing.order").unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn all_three_first_class_kinds_have_governed_lifecycles() {
        for kind in [
            ManufacturingRecordKind::Routing,
            ManufacturingRecordKind::Batch,
            ManufacturingRecordKind::Result,
        ] {
            let now = Utc::now();
            let mut value = ManufacturingRecord::propose(
                Uuid::new_v4(),
                scope(),
                kind,
                parent(),
                "a".repeat(64),
                now,
            )
            .unwrap();
            value.activate(now).unwrap();
            value.complete(now).unwrap();
            assert_eq!(value.version(), 3);
            assert!(value.cancel(now).is_err());
        }
    }
}
