use std::collections::BTreeSet;

use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId, UserId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngagementAuthorityWitness {
    pub id: RecordId,
    pub version: i64,
}

/// Minimal People-owned facts for Identity. This is neither an access grant nor a wire DTO.
/// Contact, compensation and payroll fields are deliberately absent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonAuthorityFacts {
    pub company_id: CompanyId,
    pub person_id: RecordId,
    pub user_id: Option<UserId>,
    pub status: String,
    pub merged_into_id: Option<RecordId>,
    pub version: i64,
    pub active_engagements: Vec<EngagementAuthorityWitness>,
}

impl PersonAuthorityFacts {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.version < 1 || !["active", "inactive", "archived"].contains(&self.status.as_str()) {
            return Err(invalid_source());
        }
        let mut ids = BTreeSet::new();
        if self
            .active_engagements
            .iter()
            .any(|witness| witness.version < 1 || !ids.insert(&witness.id))
        {
            return Err(invalid_source());
        }
        if self.merged_into_id.as_ref() == Some(&self.person_id) {
            return Err(invalid_source());
        }
        Ok(())
    }
    /// HR eligibility evidence only; Identity still evaluates current access policy and cutoffs.
    pub fn active_hr_subject(&self) -> bool {
        self.validate().is_ok()
            && self.status == "active"
            && self.merged_into_id.is_none()
            && !self.active_engagements.is_empty()
    }
}

pub(crate) fn invalid_source() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_AUTHORITY_SOURCE_INVALID",
        "Current People authority evidence is invalid.",
    )
}
