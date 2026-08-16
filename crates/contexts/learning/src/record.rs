use crate::LearningDomainError;
use chrono::{DateTime, Utc};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningRecordKind {
    Assignment,
    Completion,
}
impl LearningRecordKind {
    pub const fn token(self) -> &'static str {
        match self {
            Self::Assignment => "assignment",
            Self::Completion => "completion",
        }
    }
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Assignment => "learning.assignment",
            Self::Completion => "learning.completion",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningRecordStatus {
    Proposed,
    Assigned,
    InProgress,
    Completed,
    Waived,
    Cancelled,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearningRecord {
    id: Uuid,
    version: u64,
    scope: TenantScope,
    kind: LearningRecordKind,
    subject: StableRecordRef,
    evidence_digest: String,
    status: LearningRecordStatus,
    updated_at: DateTime<Utc>,
}
impl LearningRecord {
    pub fn propose(
        id: Uuid,
        scope: TenantScope,
        kind: LearningRecordKind,
        subject: StableRecordRef,
        evidence_digest: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, LearningDomainError> {
        let v = Self {
            id,
            version: 1,
            scope,
            kind,
            subject,
            evidence_digest: evidence_digest.into(),
            status: LearningRecordStatus::Proposed,
            updated_at: at,
        };
        v.validate()?;
        Ok(v)
    }
    pub fn assign(&mut self, at: DateTime<Utc>) -> Result<(), LearningDomainError> {
        self.transition(
            LearningRecordStatus::Proposed,
            LearningRecordStatus::Assigned,
            at,
        )
    }
    pub fn start(&mut self, at: DateTime<Utc>) -> Result<(), LearningDomainError> {
        self.transition(
            LearningRecordStatus::Assigned,
            LearningRecordStatus::InProgress,
            at,
        )
    }
    pub fn complete(&mut self, at: DateTime<Utc>) -> Result<(), LearningDomainError> {
        self.transition(
            LearningRecordStatus::InProgress,
            LearningRecordStatus::Completed,
            at,
        )
    }
    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), LearningDomainError> {
        if matches!(
            self.status,
            LearningRecordStatus::Completed
                | LearningRecordStatus::Waived
                | LearningRecordStatus::Cancelled
        ) {
            return Err(LearningDomainError::InvalidTransition);
        }
        self.status = LearningRecordStatus::Cancelled;
        self.advance(at)
    }
    pub fn validate(&self) -> Result<(), LearningDomainError> {
        self.scope.validate()?;
        self.subject.validate()?;
        if self.id.is_nil()
            || self.id.get_version_num() != 4
            || self.version == 0
            || self.evidence_digest.len() != 64
            || !self.evidence_digest.bytes().all(|v| v.is_ascii_hexdigit())
        {
            return Err(match self.kind {
                LearningRecordKind::Assignment => LearningDomainError::InvalidAssignment,
                LearningRecordKind::Completion => LearningDomainError::InvalidCompletion,
            });
        }
        Ok(())
    }
    fn transition(
        &mut self,
        e: LearningRecordStatus,
        n: LearningRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), LearningDomainError> {
        if self.status != e {
            return Err(LearningDomainError::InvalidTransition);
        }
        self.status = n;
        self.advance(at)
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), LearningDomainError> {
        if at < self.updated_at {
            return Err(LearningDomainError::InvalidTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(LearningDomainError::Overflow)?;
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
    pub const fn kind(&self) -> LearningRecordKind {
        self.kind
    }
    pub const fn status(&self) -> LearningRecordStatus {
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
            ContextId::Learning,
            RecordTypeId::try_new("learning.course").unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn assignment_and_completion_are_versioned_roots() {
        for k in [
            LearningRecordKind::Assignment,
            LearningRecordKind::Completion,
        ] {
            let now = Utc::now();
            let mut v =
                LearningRecord::propose(Uuid::new_v4(), scope(), k, s(), "a".repeat(64), now)
                    .unwrap();
            v.assign(now).unwrap();
            v.start(now).unwrap();
            v.complete(now).unwrap();
            assert_eq!(v.version(), 4)
        }
    }
}
