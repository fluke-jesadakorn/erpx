use crate::BenefitsDomainError;
use chrono::{DateTime, Utc};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenefitsRecordKind {
    Window,
    Handoff,
}
impl BenefitsRecordKind {
    pub const fn token(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Handoff => "handoff",
        }
    }
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Window => "benefits.window",
            Self::Handoff => "benefits.handoff",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenefitsRecordStatus {
    Draft,
    Open,
    Closed,
    Submitted,
    Accepted,
    Rejected,
    Cancelled,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenefitsRecord {
    id: Uuid,
    version: u64,
    scope: TenantScope,
    kind: BenefitsRecordKind,
    subject: StableRecordRef,
    evidence_digest: String,
    status: BenefitsRecordStatus,
    updated_at: DateTime<Utc>,
}
impl BenefitsRecord {
    pub fn draft(
        id: Uuid,
        scope: TenantScope,
        kind: BenefitsRecordKind,
        subject: StableRecordRef,
        evidence_digest: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, BenefitsDomainError> {
        let v = Self {
            id,
            version: 1,
            scope,
            kind,
            subject,
            evidence_digest: evidence_digest.into(),
            status: BenefitsRecordStatus::Draft,
            updated_at: at,
        };
        v.validate()?;
        Ok(v)
    }
    pub fn open(&mut self, at: DateTime<Utc>) -> Result<(), BenefitsDomainError> {
        self.transition(BenefitsRecordStatus::Draft, BenefitsRecordStatus::Open, at)
    }
    pub fn close(&mut self, at: DateTime<Utc>) -> Result<(), BenefitsDomainError> {
        self.transition(BenefitsRecordStatus::Open, BenefitsRecordStatus::Closed, at)
    }
    pub fn submit(&mut self, at: DateTime<Utc>) -> Result<(), BenefitsDomainError> {
        self.transition(
            BenefitsRecordStatus::Draft,
            BenefitsRecordStatus::Submitted,
            at,
        )
    }
    pub fn accept(&mut self, at: DateTime<Utc>) -> Result<(), BenefitsDomainError> {
        self.transition(
            BenefitsRecordStatus::Submitted,
            BenefitsRecordStatus::Accepted,
            at,
        )
    }
    pub fn reject(&mut self, at: DateTime<Utc>) -> Result<(), BenefitsDomainError> {
        self.transition(
            BenefitsRecordStatus::Submitted,
            BenefitsRecordStatus::Rejected,
            at,
        )
    }
    pub fn validate(&self) -> Result<(), BenefitsDomainError> {
        self.scope.validate()?;
        self.subject.validate()?;
        if self.id.is_nil()
            || self.id.get_version_num() != 4
            || self.version == 0
            || self.evidence_digest.len() != 64
            || !self.evidence_digest.bytes().all(|v| v.is_ascii_hexdigit())
        {
            return Err(match self.kind {
                BenefitsRecordKind::Window => BenefitsDomainError::InvalidWindow,
                BenefitsRecordKind::Handoff => BenefitsDomainError::InvalidHandoff,
            });
        }
        Ok(())
    }
    fn transition(
        &mut self,
        e: BenefitsRecordStatus,
        n: BenefitsRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), BenefitsDomainError> {
        if self.status != e {
            return Err(BenefitsDomainError::InvalidTransition);
        }
        self.status = n;
        self.advance(at)
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), BenefitsDomainError> {
        if at < self.updated_at {
            return Err(BenefitsDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(BenefitsDomainError::Overflow)?;
        self.updated_at = at;
        Ok(())
    }
    pub const fn id(&self) -> Uuid {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn kind(&self) -> BenefitsRecordKind {
        self.kind
    }
    pub const fn status(&self) -> BenefitsRecordStatus {
        self.status
    }
    pub const fn subject(&self) -> &StableRecordRef {
        &self.subject
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use erpx_integration_contracts::{RecordTypeId, catalog::ContextId};
    use erpx_kernel::RecordId;
    fn scope() -> TenantScope {
        TenantScope::Holding {
            holding_id: Uuid::new_v4(),
        }
    }
    fn s() -> StableRecordRef {
        StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            ContextId::Benefits,
            RecordTypeId::try_new("benefits.plan").unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn window_and_handoff_have_distinct_transitions() {
        let now = Utc::now();
        let mut w = BenefitsRecord::draft(
            Uuid::new_v4(),
            scope(),
            BenefitsRecordKind::Window,
            s(),
            "a".repeat(64),
            now,
        )
        .unwrap();
        w.open(now).unwrap();
        w.close(now).unwrap();
        let mut h = BenefitsRecord::draft(
            Uuid::new_v4(),
            scope(),
            BenefitsRecordKind::Handoff,
            s(),
            "b".repeat(64),
            now,
        )
        .unwrap();
        h.submit(now).unwrap();
        h.accept(now).unwrap();
        assert_eq!((w.version(), h.version()), (3, 3))
    }
}
