use crate::{TalentError, TalentRealm};
use chrono::{DateTime, Utc};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TalentRecordKind {
    Interview,
    ReviewCycle,
}
impl TalentRecordKind {
    pub const fn token(self) -> &'static str {
        match self {
            Self::Interview => "talent_interview",
            Self::ReviewCycle => "talent_review_cycle",
        }
    }
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Interview => "talent.interview",
            Self::ReviewCycle => "talent.review-cycle",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TalentRecordStatus {
    Draft,
    Scheduled,
    InProgress,
    Completed,
    Closed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TalentRecord {
    id: Uuid,
    version: u64,
    scope: TenantScope,
    kind: TalentRecordKind,
    authority_realm: TalentRealm,
    candidate_principal_id: Option<Uuid>,
    subject: StableRecordRef,
    evidence_digest: String,
    status: TalentRecordStatus,
    updated_at: DateTime<Utc>,
}
impl TalentRecord {
    pub fn draft(
        id: Uuid,
        scope: TenantScope,
        kind: TalentRecordKind,
        candidate_principal_id: Option<Uuid>,
        subject: StableRecordRef,
        evidence_digest: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, TalentError> {
        let authority_realm = match kind {
            TalentRecordKind::Interview => TalentRealm::Candidate,
            TalentRecordKind::ReviewCycle => TalentRealm::Workforce,
        };
        let value = Self {
            id,
            version: 1,
            scope,
            kind,
            authority_realm,
            candidate_principal_id,
            subject,
            evidence_digest: evidence_digest.into(),
            status: TalentRecordStatus::Draft,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn schedule(&mut self, at: DateTime<Utc>) -> Result<(), TalentError> {
        self.transition(TalentRecordStatus::Draft, TalentRecordStatus::Scheduled, at)
    }
    pub fn start(&mut self, at: DateTime<Utc>) -> Result<(), TalentError> {
        self.transition(
            TalentRecordStatus::Scheduled,
            TalentRecordStatus::InProgress,
            at,
        )
    }
    pub fn complete(&mut self, at: DateTime<Utc>) -> Result<(), TalentError> {
        self.transition(
            TalentRecordStatus::InProgress,
            TalentRecordStatus::Completed,
            at,
        )
    }
    pub fn close(&mut self, at: DateTime<Utc>) -> Result<(), TalentError> {
        self.transition(
            TalentRecordStatus::Completed,
            TalentRecordStatus::Closed,
            at,
        )
    }
    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), TalentError> {
        if matches!(
            self.status,
            TalentRecordStatus::Closed | TalentRecordStatus::Cancelled
        ) {
            return Err(TalentError::InvalidTransition);
        }
        self.status = TalentRecordStatus::Cancelled;
        self.advance(at)
    }
    pub fn validate(&self) -> Result<(), TalentError> {
        self.scope
            .validate()
            .map_err(|_| TalentError::InvalidScope)?;
        self.subject
            .validate()
            .map_err(|_| TalentError::InvalidGovernedRecord)?;
        let realm_ok = match (self.kind, self.authority_realm, self.candidate_principal_id) {
            (TalentRecordKind::Interview, TalentRealm::Candidate, Some(id)) => !id.is_nil(),
            (TalentRecordKind::ReviewCycle, TalentRealm::Workforce, None) => true,
            _ => false,
        };
        if !realm_ok
            || self.id.is_nil()
            || self.id.get_version_num() != 4
            || self.version == 0
            || self.evidence_digest.len() != 64
            || !self.evidence_digest.bytes().all(|v| v.is_ascii_hexdigit())
        {
            return Err(TalentError::InvalidGovernedRecord);
        }
        Ok(())
    }
    fn transition(
        &mut self,
        e: TalentRecordStatus,
        n: TalentRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), TalentError> {
        if self.status != e {
            return Err(TalentError::InvalidTransition);
        }
        self.status = n;
        self.advance(at)
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), TalentError> {
        if at < self.updated_at {
            return Err(TalentError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(TalentError::VersionOverflow)?;
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
    pub const fn kind(&self) -> TalentRecordKind {
        self.kind
    }
    pub const fn status(&self) -> TalentRecordStatus {
        self.status
    }
    pub const fn authority_realm(&self) -> TalentRealm {
        self.authority_realm
    }
    pub const fn candidate_principal_id(&self) -> Option<Uuid> {
        self.candidate_principal_id
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_integration_contracts::{RecordTypeId, catalog::ContextId};
    use erpx_kernel::RecordId;
    fn scope() -> TenantScope {
        TenantScope::Company {
            holding_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
        }
    }
    fn subject() -> StableRecordRef {
        StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            ContextId::RecruitingTalent,
            RecordTypeId::try_new("talent.application").unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn candidate_and_workforce_records_never_share_authority() {
        let now = Utc::now();
        assert!(
            TalentRecord::draft(
                Uuid::new_v4(),
                scope(),
                TalentRecordKind::Interview,
                None,
                subject(),
                "a".repeat(64),
                now
            )
            .is_err()
        );
        let mut r = TalentRecord::draft(
            Uuid::new_v4(),
            scope(),
            TalentRecordKind::ReviewCycle,
            None,
            subject(),
            "b".repeat(64),
            now,
        )
        .unwrap();
        r.schedule(now).unwrap();
        r.start(now).unwrap();
        r.complete(now).unwrap();
        r.close(now).unwrap();
        assert_eq!(r.version(), 5)
    }
}
