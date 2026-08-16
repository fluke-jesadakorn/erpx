use super::LegalDomainError;
use chrono::{DateTime, Utc};
use erpx_integration_contracts::{
    StableRecordRef,
    catalog::ContextId,
    reference::{RecordTypeId, ReferenceValidationError},
};
use erpx_kernel::{RecordId, TenantScope};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegalCaseStatus {
    Proposed,
    Open,
    OnHold,
    Resolved,
    Closed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalCase {
    id: RecordId,
    version: u64,
    scope: TenantScope,
    case_reference: String,
    title: String,
    related_matter: Option<StableRecordRef>,
    source_version: Option<u64>,
    evidence_sha256: String,
    status: LegalCaseStatus,
    owner_ref: String,
    closed_by: Option<String>,
    retention_until: DateTime<Utc>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl LegalCase {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: RecordId,
        version: u64,
        scope: TenantScope,
        case_reference: String,
        title: String,
        related_matter: Option<StableRecordRef>,
        source_version: Option<u64>,
        evidence_sha256: String,
        status: LegalCaseStatus,
        owner_ref: String,
        closed_by: Option<String>,
        retention_until: DateTime<Utc>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, LegalDomainError> {
        let value = Self {
            id,
            version,
            scope,
            case_reference,
            title,
            related_matter,
            source_version,
            evidence_sha256,
            status,
            owner_ref,
            closed_by,
            retention_until,
            created_at,
            updated_at,
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn propose(
        id: RecordId,
        scope: TenantScope,
        case_reference: String,
        title: String,
        related_matter: Option<StableRecordRef>,
        source_version: Option<u64>,
        evidence_sha256: String,
        owner_ref: String,
        retention_until: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<Self, LegalDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            case_reference,
            title,
            related_matter,
            source_version,
            evidence_sha256,
            status: LegalCaseStatus::Proposed,
            owner_ref,
            closed_by: None,
            retention_until,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn transition(
        &mut self,
        next: LegalCaseStatus,
        actor: String,
        at: DateTime<Utc>,
    ) -> Result<(), LegalDomainError> {
        let allowed = matches!(
            (self.status, next),
            (
                LegalCaseStatus::Proposed,
                LegalCaseStatus::Open | LegalCaseStatus::Cancelled
            ) | (
                LegalCaseStatus::Open,
                LegalCaseStatus::OnHold | LegalCaseStatus::Resolved | LegalCaseStatus::Cancelled
            ) | (
                LegalCaseStatus::OnHold,
                LegalCaseStatus::Open | LegalCaseStatus::Resolved
            ) | (
                LegalCaseStatus::Resolved,
                LegalCaseStatus::Open | LegalCaseStatus::Closed
            )
        );
        if !allowed || actor.trim().is_empty() || at < self.updated_at {
            return Err(LegalDomainError::InvalidLegalCase);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(LegalDomainError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        self.closed_by =
            matches!(next, LegalCaseStatus::Closed | LegalCaseStatus::Cancelled).then_some(actor);
        Ok(())
    }
    pub fn validate(&self) -> Result<(), LegalDomainError> {
        if self.version == 0
            || self.scope.validate().is_err()
            || !matches!(
                self.scope,
                TenantScope::Company { .. } | TenantScope::Branch { .. }
            )
            || self.case_reference.trim().is_empty()
            || self.case_reference.len() > 128
            || self.title.trim().is_empty()
            || self.title.len() > 255
            || self.owner_ref.trim().is_empty()
            || self.retention_until <= self.created_at
            || self.updated_at < self.created_at
            || !valid_digest(&self.evidence_sha256)
            || self.related_matter.is_some() != self.source_version.is_some()
            || self.source_version == Some(0)
            || self.related_matter.as_ref().is_some_and(|record| {
                record.owner_context() != ContextId::Legal
                    || record.record_type().as_str() != "legal.matter"
                    || record.validate().is_err()
            })
        {
            return Err(LegalDomainError::InvalidLegalCase);
        }
        Ok(())
    }
    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        StableRecordRef::try_new(
            self.id,
            ContextId::Legal,
            RecordTypeId::try_new("legal.case")?,
        )
    }
    pub const fn id(&self) -> RecordId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub fn case_reference(&self) -> &str {
        &self.case_reference
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn related_matter(&self) -> Option<&StableRecordRef> {
        self.related_matter.as_ref()
    }
    pub const fn source_version(&self) -> Option<u64> {
        self.source_version
    }
    pub fn evidence_sha256(&self) -> &str {
        &self.evidence_sha256
    }
    pub const fn status(&self) -> LegalCaseStatus {
        self.status
    }
    pub fn owner_ref(&self) -> &str {
        &self.owner_ref
    }
    pub fn closed_by(&self) -> Option<&str> {
        self.closed_by.as_deref()
    }
    pub const fn retention_until(&self) -> DateTime<Utc> {
        self.retention_until
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}
fn valid_digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use uuid::Uuid;
    #[test]
    fn legal_case_is_distinct_from_matter_and_preserves_retention() {
        let at = Utc.with_ymd_and_hms(2026, 8, 16, 9, 0, 0).single().unwrap();
        let matter = StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            ContextId::Legal,
            RecordTypeId::try_new("legal.matter").unwrap(),
        )
        .unwrap();
        let mut case = LegalCase::propose(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            TenantScope::Company {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2),
            },
            "CASE-10".into(),
            "Retention dispute".into(),
            Some(matter),
            Some(3),
            format!("sha256:{}", "a".repeat(64)),
            "legal-owner".into(),
            at + Duration::days(365),
            at,
        )
        .unwrap();
        assert_eq!(
            case.record_ref().unwrap().record_type().as_str(),
            "legal.case"
        );
        case.transition(
            LegalCaseStatus::Open,
            "lawyer".into(),
            at + Duration::minutes(1),
        )
        .unwrap();
        case.transition(
            LegalCaseStatus::Resolved,
            "lawyer".into(),
            at + Duration::minutes(2),
        )
        .unwrap();
        case.transition(
            LegalCaseStatus::Closed,
            "counsel".into(),
            at + Duration::minutes(3),
        )
        .unwrap();
        assert_eq!(case.version(), 4);
        assert_eq!(case.retention_until(), at + Duration::days(365));
    }
}
