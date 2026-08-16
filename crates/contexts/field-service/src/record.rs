use crate::FieldServiceDomainError;
use chrono::{DateTime, Utc};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldServiceRecordKind {
    Request,
    Dispatch,
    Visit,
    Handoff,
}
impl FieldServiceRecordKind {
    pub const fn token(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Dispatch => "dispatch",
            Self::Visit => "visit",
            Self::Handoff => "handoff",
        }
    }
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Request => "field-service.request",
            Self::Dispatch => "field-service.dispatch",
            Self::Visit => "field-service.visit",
            Self::Handoff => "field-service.handoff",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldServiceRecordStatus {
    Proposed,
    Accepted,
    Scheduled,
    InProgress,
    Completed,
    Reconciled,
    Rejected,
    Cancelled,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldServiceRecord {
    id: Uuid,
    version: u64,
    scope: TenantScope,
    kind: FieldServiceRecordKind,
    subject: StableRecordRef,
    evidence_digest: String,
    status: FieldServiceRecordStatus,
    updated_at: DateTime<Utc>,
}
impl FieldServiceRecord {
    pub fn propose(
        id: Uuid,
        scope: TenantScope,
        kind: FieldServiceRecordKind,
        subject: StableRecordRef,
        evidence_digest: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, FieldServiceDomainError> {
        let v = Self {
            id,
            version: 1,
            scope,
            kind,
            subject,
            evidence_digest: evidence_digest.into(),
            status: FieldServiceRecordStatus::Proposed,
            updated_at: at,
        };
        v.validate()?;
        Ok(v)
    }
    pub fn accept(&mut self, at: DateTime<Utc>) -> Result<(), FieldServiceDomainError> {
        self.transition(
            FieldServiceRecordStatus::Proposed,
            FieldServiceRecordStatus::Accepted,
            at,
        )
    }
    pub fn schedule(&mut self, at: DateTime<Utc>) -> Result<(), FieldServiceDomainError> {
        self.transition(
            FieldServiceRecordStatus::Accepted,
            FieldServiceRecordStatus::Scheduled,
            at,
        )
    }
    pub fn start(&mut self, at: DateTime<Utc>) -> Result<(), FieldServiceDomainError> {
        self.transition(
            FieldServiceRecordStatus::Scheduled,
            FieldServiceRecordStatus::InProgress,
            at,
        )
    }
    pub fn complete(&mut self, at: DateTime<Utc>) -> Result<(), FieldServiceDomainError> {
        self.transition(
            FieldServiceRecordStatus::InProgress,
            FieldServiceRecordStatus::Completed,
            at,
        )
    }
    pub fn reconcile(&mut self, at: DateTime<Utc>) -> Result<(), FieldServiceDomainError> {
        self.transition(
            FieldServiceRecordStatus::Completed,
            FieldServiceRecordStatus::Reconciled,
            at,
        )
    }
    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), FieldServiceDomainError> {
        if matches!(
            self.status,
            FieldServiceRecordStatus::Reconciled
                | FieldServiceRecordStatus::Rejected
                | FieldServiceRecordStatus::Cancelled
        ) {
            return Err(FieldServiceDomainError::InvalidTransition);
        }
        self.status = FieldServiceRecordStatus::Cancelled;
        self.advance(at)
    }
    pub fn validate(&self) -> Result<(), FieldServiceDomainError> {
        self.scope.validate()?;
        self.subject.validate()?;
        if self.id.is_nil()
            || self.id.get_version_num() != 4
            || self.version == 0
            || self.evidence_digest.len() != 64
            || !self.evidence_digest.bytes().all(|v| v.is_ascii_hexdigit())
        {
            return Err(FieldServiceDomainError::InvalidGovernedRecord);
        }
        Ok(())
    }
    fn transition(
        &mut self,
        e: FieldServiceRecordStatus,
        n: FieldServiceRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), FieldServiceDomainError> {
        if self.status != e {
            return Err(FieldServiceDomainError::InvalidTransition);
        }
        self.status = n;
        self.advance(at)
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), FieldServiceDomainError> {
        if at < self.updated_at {
            return Err(FieldServiceDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(FieldServiceDomainError::Overflow)?;
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
    pub const fn kind(&self) -> FieldServiceRecordKind {
        self.kind
    }
    pub const fn status(&self) -> FieldServiceRecordStatus {
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
            ContextId::FieldService,
            RecordTypeId::try_new("field-service.agreement").unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn four_field_records_are_independent_roots() {
        for k in [
            FieldServiceRecordKind::Request,
            FieldServiceRecordKind::Dispatch,
            FieldServiceRecordKind::Visit,
            FieldServiceRecordKind::Handoff,
        ] {
            let now = Utc::now();
            let mut v =
                FieldServiceRecord::propose(Uuid::new_v4(), scope(), k, s(), "a".repeat(64), now)
                    .unwrap();
            v.accept(now).unwrap();
            v.schedule(now).unwrap();
            v.start(now).unwrap();
            v.complete(now).unwrap();
            v.reconcile(now).unwrap();
            assert_eq!(v.version(), 6)
        }
    }
}
