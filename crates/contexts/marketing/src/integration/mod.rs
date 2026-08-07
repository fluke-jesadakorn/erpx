use erpx_integration_contracts::{ReferenceCode, customer, marketing};

use crate::domain::DomainEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    InvalidId,
    InvalidText,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<marketing::Event>, IntegrationMappingError> {
    events
        .iter()
        .map(|event| match event {
            DomainEvent::LeadQualified {
                candidate_id,
                campaign_id,
                audience_member_id,
                existing_customer_id,
                source,
                qualification_score_basis_points,
            } => Ok(marketing::Event::LeadQualified(
                marketing::QualifiedLeadFact {
                    candidate: marketing::LeadCandidateRef::try_from_uuid(candidate_id.as_uuid())
                        .map_err(|_| IntegrationMappingError::InvalidId)?,
                    campaign: marketing::CampaignRef::try_from_uuid(campaign_id.as_uuid())
                        .map_err(|_| IntegrationMappingError::InvalidId)?,
                    audience_member: marketing::AudienceMemberRef::try_from_uuid(
                        audience_member_id.as_uuid(),
                    )
                    .map_err(|_| IntegrationMappingError::InvalidId)?,
                    existing_customer: existing_customer_id
                        .map(customer::CustomerRef::try_from_uuid)
                        .transpose()
                        .map_err(|_| IntegrationMappingError::InvalidId)?,
                    source: ReferenceCode::try_new(source.clone())
                        .map_err(|_| IntegrationMappingError::InvalidText)?,
                    qualification_score_basis_points: *qualification_score_basis_points,
                },
            )),
            DomainEvent::EngagementCaptured {
                campaign_id,
                audience_member_id,
                channel,
                action,
            } => Ok(marketing::Event::EngagementCaptured(
                marketing::EngagementFact {
                    campaign: marketing::CampaignRef::try_from_uuid(campaign_id.as_uuid())
                        .map_err(|_| IntegrationMappingError::InvalidId)?,
                    audience_member: marketing::AudienceMemberRef::try_from_uuid(
                        audience_member_id.as_uuid(),
                    )
                    .map_err(|_| IntegrationMappingError::InvalidId)?,
                    channel: ReferenceCode::try_new(channel.clone())
                        .map_err(|_| IntegrationMappingError::InvalidText)?,
                    action: ReferenceCode::try_new(action.clone())
                        .map_err(|_| IntegrationMappingError::InvalidText)?,
                },
            )),
        })
        .collect()
}
