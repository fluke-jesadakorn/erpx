use chrono::{DateTime, Utc};
use erpx_integration_contracts::{
    StableRecordRef,
    catalog::ContextId,
    reference::{RecordTypeId, ReferenceValidationError},
};
use erpx_kernel::{AuthorityScope, RecordId};
use serde::{Deserialize, Serialize};

use super::SupportDomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformCaseStatus {
    Open,
    Investigating,
    Waiting,
    Resolved,
    Closed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformSupportCase {
    id: RecordId,
    version: u64,
    scope: AuthorityScope,
    case_code: String,
    title: String,
    source_record: Option<StableRecordRef>,
    source_version: Option<u64>,
    evidence_sha256: String,
    status: PlatformCaseStatus,
    opened_by: String,
    resolved_by: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl PlatformSupportCase {
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        id: RecordId,
        scope: AuthorityScope,
        case_code: String,
        title: String,
        source_record: Option<StableRecordRef>,
        source_version: Option<u64>,
        evidence_sha256: String,
        opened_by: String,
        at: DateTime<Utc>,
    ) -> Result<Self, SupportDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            case_code,
            title,
            source_record,
            source_version,
            evidence_sha256,
            status: PlatformCaseStatus::Open,
            opened_by,
            resolved_by: None,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn transition(
        &mut self,
        next: PlatformCaseStatus,
        actor: String,
        at: DateTime<Utc>,
    ) -> Result<(), SupportDomainError> {
        let allowed = matches!(
            (self.status, next),
            (
                PlatformCaseStatus::Open,
                PlatformCaseStatus::Investigating | PlatformCaseStatus::Cancelled
            ) | (
                PlatformCaseStatus::Investigating,
                PlatformCaseStatus::Waiting
                    | PlatformCaseStatus::Resolved
                    | PlatformCaseStatus::Cancelled
            ) | (
                PlatformCaseStatus::Waiting,
                PlatformCaseStatus::Investigating
                    | PlatformCaseStatus::Resolved
                    | PlatformCaseStatus::Cancelled
            ) | (
                PlatformCaseStatus::Resolved,
                PlatformCaseStatus::Closed | PlatformCaseStatus::Investigating
            )
        );
        if !allowed || actor.trim().is_empty() || at < self.updated_at {
            return Err(SupportDomainError::InvalidPlatformCase);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(SupportDomainError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        self.resolved_by = matches!(
            next,
            PlatformCaseStatus::Resolved | PlatformCaseStatus::Closed
        )
        .then_some(actor);
        Ok(())
    }

    pub fn validate(&self) -> Result<(), SupportDomainError> {
        if self.version == 0
            || self.scope.validate().is_err()
            || !matches!(
                self.scope,
                AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
            )
            || self.case_code.trim().is_empty()
            || self.case_code.len() > 128
            || self.title.trim().is_empty()
            || self.title.len() > 255
            || self.opened_by.trim().is_empty()
            || self.updated_at < self.created_at
            || !valid_digest(&self.evidence_sha256)
            || self.source_record.is_some() != self.source_version.is_some()
            || self.source_version == Some(0)
            || self
                .source_record
                .as_ref()
                .is_some_and(|record| record.validate().is_err())
        {
            return Err(SupportDomainError::InvalidPlatformCase);
        }
        Ok(())
    }

    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        StableRecordRef::try_new(
            self.id,
            ContextId::Support,
            RecordTypeId::try_new("support.platform-case")?,
        )
    }
    pub const fn id(&self) -> RecordId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn case_code(&self) -> &str {
        &self.case_code
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn source_record(&self) -> Option<&StableRecordRef> {
        self.source_record.as_ref()
    }
    pub const fn source_version(&self) -> Option<u64> {
        self.source_version
    }
    pub fn evidence_sha256(&self) -> &str {
        &self.evidence_sha256
    }
    pub const fn status(&self) -> PlatformCaseStatus {
        self.status
    }
    pub fn opened_by(&self) -> &str {
        &self.opened_by
    }
    pub fn resolved_by(&self) -> Option<&str> {
        self.resolved_by.as_deref()
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
    fn platform_case_enforces_governed_transition_order() {
        let at = Utc.with_ymd_and_hms(2026, 8, 16, 9, 0, 0).single().unwrap();
        let mut case = PlatformSupportCase::open(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            AuthorityScope::Company {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2),
            },
            "PLT-10".into(),
            "Holding incident".into(),
            None,
            None,
            format!("sha256:{}", "a".repeat(64)),
            "operator".into(),
            at,
        )
        .unwrap();
        assert!(
            case.transition(
                PlatformCaseStatus::Closed,
                "operator".into(),
                at + Duration::minutes(1)
            )
            .is_err()
        );
        case.transition(
            PlatformCaseStatus::Investigating,
            "operator".into(),
            at + Duration::minutes(1),
        )
        .unwrap();
        case.transition(
            PlatformCaseStatus::Resolved,
            "resolver".into(),
            at + Duration::minutes(2),
        )
        .unwrap();
        case.transition(
            PlatformCaseStatus::Closed,
            "closer".into(),
            at + Duration::minutes(3),
        )
        .unwrap();
        assert_eq!(case.version(), 4);
    }
}
