use serde::{Deserialize, Serialize};

use crate::{BoundedContext, ReferenceCode, customer::CustomerRef};

owned_uuid_reference!(CampaignRef);
owned_uuid_reference!(AudienceMemberRef);
owned_uuid_reference!(LeadCandidateRef);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualifiedLeadFact {
    pub candidate: LeadCandidateRef,
    pub campaign: CampaignRef,
    pub audience_member: AudienceMemberRef,
    pub existing_customer: Option<CustomerRef>,
    pub source: ReferenceCode,
    pub qualification_score_basis_points: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngagementFact {
    pub campaign: CampaignRef,
    pub audience_member: AudienceMemberRef,
    pub channel: ReferenceCode,
    pub action: ReferenceCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    LeadQualified(QualifiedLeadFact),
    EngagementCaptured(EngagementFact),
}

impl crate::IntegrationEvent for Event {
    fn owner(&self) -> BoundedContext {
        BoundedContext::Marketing
    }

    fn event_type(&self) -> &'static str {
        match self {
            Self::LeadQualified(_) => "marketing.lead_qualified",
            Self::EngagementCaptured(_) => "marketing.engagement_captured",
        }
    }

    fn schema_version(&self) -> u16 {
        1
    }

    fn validate(&self) -> Result<(), crate::ContractValidationError> {
        if let Self::LeadQualified(fact) = self
            && fact.qualification_score_basis_points > 10_000
        {
            return Err(crate::ContractValidationError::OutOfRange(
                "qualification_score_basis_points",
            ));
        }
        Ok(())
    }
}
