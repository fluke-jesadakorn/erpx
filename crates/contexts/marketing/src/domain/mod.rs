use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_MARKETING_TEXT_BYTES: usize = 255;

macro_rules! marketing_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, MarketingDomainError> {
                if value.is_nil() {
                    return Err(MarketingDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = MarketingDomainError;
            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

marketing_id!(LeadCandidateId, "lead_candidate_id");
marketing_id!(CampaignId, "campaign_id");
marketing_id!(AudienceMemberId, "audience_member_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeadCandidateStatus {
    Captured,
    Qualified,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MarketingDomainError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds {maximum} bytes")]
    TooLong { field: &'static str, maximum: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("Marketing records require an exact company or branch scope")]
    InvalidScope,
    #[error("qualification score must be between 1 and 10,000 basis points")]
    InvalidQualificationScore,
    #[error("lead candidate lifecycle transition is not allowed")]
    InvalidTransition,
    #[error("Marketing timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("Marketing aggregate version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadCandidate {
    id: LeadCandidateId,
    version: u64,
    scope: TenantScope,
    campaign_id: CampaignId,
    audience_member_id: AudienceMemberId,
    source: String,
    existing_customer_id: Option<Uuid>,
    qualification_score_basis_points: Option<u16>,
    status: LeadCandidateStatus,
    engagement_count: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl LeadCandidate {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: LeadCandidateId,
        scope: TenantScope,
        campaign_id: CampaignId,
        audience_member_id: AudienceMemberId,
        source: String,
        existing_customer_id: Option<Uuid>,
        at: DateTime<Utc>,
    ) -> Result<Self, MarketingDomainError> {
        exact_scope(&scope)?;
        validate_text(&source, "source")?;
        if existing_customer_id.is_some_and(|id| id.is_nil()) {
            return Err(MarketingDomainError::NilIdentifier {
                field: "existing_customer_id",
            });
        }
        let value = Self {
            id,
            version: 1,
            scope,
            campaign_id,
            audience_member_id,
            source,
            existing_customer_id,
            qualification_score_basis_points: None,
            status: LeadCandidateStatus::Captured,
            engagement_count: 0,
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn capture_engagement(
        &mut self,
        channel: String,
        action: String,
        at: DateTime<Utc>,
    ) -> Result<(), MarketingDomainError> {
        if self.status != LeadCandidateStatus::Captured {
            return Err(MarketingDomainError::InvalidTransition);
        }
        validate_text(&channel, "channel")?;
        validate_text(&action, "action")?;
        self.advance(at)?;
        self.engagement_count = self
            .engagement_count
            .checked_add(1)
            .ok_or(MarketingDomainError::VersionOverflow)?;
        self.uncommitted_events
            .push(DomainEvent::EngagementCaptured {
                campaign_id: self.campaign_id,
                audience_member_id: self.audience_member_id,
                channel,
                action,
            });
        Ok(())
    }

    pub fn qualify(
        &mut self,
        score_basis_points: u16,
        at: DateTime<Utc>,
    ) -> Result<(), MarketingDomainError> {
        if self.status != LeadCandidateStatus::Captured {
            return Err(MarketingDomainError::InvalidTransition);
        }
        if score_basis_points == 0 || score_basis_points > 10_000 {
            return Err(MarketingDomainError::InvalidQualificationScore);
        }
        self.advance(at)?;
        self.qualification_score_basis_points = Some(score_basis_points);
        self.status = LeadCandidateStatus::Qualified;
        self.uncommitted_events.push(DomainEvent::LeadQualified {
            candidate_id: self.id,
            campaign_id: self.campaign_id,
            audience_member_id: self.audience_member_id,
            existing_customer_id: self.existing_customer_id,
            source: self.source.clone(),
            qualification_score_basis_points: score_basis_points,
        });
        Ok(())
    }

    pub fn reject(&mut self, at: DateTime<Utc>) -> Result<(), MarketingDomainError> {
        if self.status != LeadCandidateStatus::Captured {
            return Err(MarketingDomainError::InvalidTransition);
        }
        self.advance(at)?;
        self.status = LeadCandidateStatus::Rejected;
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), MarketingDomainError> {
        if at < self.updated_at {
            return Err(MarketingDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(MarketingDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), MarketingDomainError> {
        exact_scope(&self.scope)?;
        validate_text(&self.source, "source")?;
        if self.version == 0 {
            return Err(MarketingDomainError::VersionOverflow);
        }
        if self.updated_at < self.created_at {
            return Err(MarketingDomainError::InvalidTimestamp);
        }
        if self
            .qualification_score_basis_points
            .is_some_and(|score| score == 0 || score > 10_000)
        {
            return Err(MarketingDomainError::InvalidQualificationScore);
        }
        if (self.status == LeadCandidateStatus::Qualified)
            != self.qualification_score_basis_points.is_some()
        {
            return Err(MarketingDomainError::InvalidTransition);
        }
        Ok(())
    }

    pub const fn id(&self) -> LeadCandidateId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn campaign_id(&self) -> CampaignId {
        self.campaign_id
    }
    pub const fn audience_member_id(&self) -> AudienceMemberId {
        self.audience_member_id
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub const fn existing_customer_id(&self) -> Option<Uuid> {
        self.existing_customer_id
    }
    pub const fn score(&self) -> Option<u16> {
        self.qualification_score_basis_points
    }
    pub const fn status(&self) -> LeadCandidateStatus {
        self.status
    }
    pub const fn engagement_count(&self) -> u64 {
        self.engagement_count
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    LeadQualified {
        candidate_id: LeadCandidateId,
        campaign_id: CampaignId,
        audience_member_id: AudienceMemberId,
        existing_customer_id: Option<Uuid>,
        source: String,
        qualification_score_basis_points: u16,
    },
    EngagementCaptured {
        campaign_id: CampaignId,
        audience_member_id: AudienceMemberId,
        channel: String,
        action: String,
    },
}

fn exact_scope(scope: &TenantScope) -> Result<(), MarketingDomainError> {
    match scope {
        TenantScope::Company {
            holding_id,
            company_id,
        } if !holding_id.is_nil() && !company_id.is_nil() => Ok(()),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } if !holding_id.is_nil() && !company_id.is_nil() && !branch_id.is_nil() => Ok(()),
        _ => Err(MarketingDomainError::InvalidScope),
    }
}

fn validate_text(value: &str, field: &'static str) -> Result<(), MarketingDomainError> {
    if value.trim().is_empty() {
        return Err(MarketingDomainError::Required { field });
    }
    if value.len() > MAX_MARKETING_TEXT_BYTES {
        return Err(MarketingDomainError::TooLong {
            field,
            maximum: MAX_MARKETING_TEXT_BYTES,
        });
    }
    if value.chars().any(char::is_control) {
        return Err(MarketingDomainError::ControlCharacter { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn scope() -> TenantScope {
        TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }
    fn lead() -> LeadCandidate {
        LeadCandidate::create(
            LeadCandidateId::try_from_uuid(Uuid::from_u128(3)).unwrap(),
            scope(),
            CampaignId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
            AudienceMemberId::try_from_uuid(Uuid::from_u128(5)).unwrap(),
            "organic".into(),
            None,
            Utc::now(),
        )
        .unwrap()
    }

    #[test]
    fn qualification_is_terminal_and_bounded() {
        let mut lead = lead();
        let at = lead.created_at();
        assert_eq!(
            lead.qualify(0, at),
            Err(MarketingDomainError::InvalidQualificationScore)
        );
        lead.qualify(7_500, at).unwrap();
        assert_eq!(lead.status(), LeadCandidateStatus::Qualified);
        assert_eq!(
            lead.reject(at),
            Err(MarketingDomainError::InvalidTransition)
        );
    }

    #[test]
    fn engagement_is_only_owned_before_handoff() {
        let mut lead = lead();
        let at = lead.created_at();
        lead.capture_engagement("email".into(), "opened".into(), at)
            .unwrap();
        lead.qualify(8_000, at).unwrap();
        assert_eq!(
            lead.capture_engagement("web".into(), "visit".into(), at),
            Err(MarketingDomainError::InvalidTransition)
        );
    }

    #[test]
    fn timestamps_and_scopes_are_authoritative() {
        let mut lead = lead();
        assert_eq!(
            lead.reject(lead.created_at() - Duration::seconds(1)),
            Err(MarketingDomainError::InvalidTimestamp)
        );
        assert!(matches!(
            LeadCandidate::create(
                LeadCandidateId::try_from_uuid(Uuid::from_u128(9)).unwrap(),
                TenantScope::Platform,
                CampaignId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
                AudienceMemberId::try_from_uuid(Uuid::from_u128(5)).unwrap(),
                "x".into(),
                None,
                Utc::now()
            ),
            Err(MarketingDomainError::InvalidScope)
        ));
    }
}
