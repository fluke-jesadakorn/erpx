use erpx_integration_contracts::{DisplayName, ReferenceCode, customer, marketing};

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
            } => Ok(Some(marketing::Event::LeadQualified(
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
            ))),
            DomainEvent::EngagementCaptured {
                campaign_id,
                audience_member_id,
                channel,
                action,
            } => Ok(Some(marketing::Event::EngagementCaptured(
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
            ))),
            DomainEvent::CampaignBudgetApprovalRequested {
                budget_id,
                campaign_id,
                approval,
            } => Ok(Some(marketing::Event::CampaignBudgetApprovalRequested(
                marketing::CampaignBudgetApprovalRequestedFact {
                    budget: marketing::CampaignBudgetRef::try_from_uuid(budget_id.as_uuid())
                        .map_err(|_| IntegrationMappingError::InvalidId)?,
                    campaign: marketing::CampaignRef::try_from_uuid(campaign_id.as_uuid())
                        .map_err(|_| IntegrationMappingError::InvalidId)?,
                    approval: approval.clone(),
                },
            ))),
            DomainEvent::CampaignBudgetApproved {
                budget_id,
                campaign_id,
                period,
                approved_amount,
                dimensions,
                approved_at,
            } => Ok(Some(marketing::Event::CampaignBudgetApproved(
                marketing::CampaignBudgetApprovedFact {
                    budget: marketing::CampaignBudgetRef::try_from_uuid(budget_id.as_uuid())
                        .map_err(|_| IntegrationMappingError::InvalidId)?,
                    campaign: marketing::CampaignRef::try_from_uuid(campaign_id.as_uuid())
                        .map_err(|_| IntegrationMappingError::InvalidId)?,
                    period: *period,
                    approved_amount: *approved_amount,
                    dimensions: dimensions.clone(),
                    approved_at: *approved_at,
                },
            ))),
            DomainEvent::SpendRequested {
                budget_id,
                request_id,
                campaign_id,
                allocation_id,
                tactic,
                channel,
                amount,
                dimensions,
                expected_outcome,
                requested_at,
            } => Ok(Some(marketing::Event::CampaignSpendRequested(
                marketing::CampaignSpendRequestedFact {
                    request: marketing::SpendRequestRef::try_from_uuid(request_id.as_uuid())
                        .map_err(|_| IntegrationMappingError::InvalidId)?,
                    budget: marketing::CampaignBudgetRef::try_from_uuid(budget_id.as_uuid())
                        .map_err(|_| IntegrationMappingError::InvalidId)?,
                    campaign: marketing::CampaignRef::try_from_uuid(campaign_id.as_uuid())
                        .map_err(|_| IntegrationMappingError::InvalidId)?,
                    allocation: marketing::BudgetAllocationRef::try_from_uuid(
                        allocation_id.as_uuid(),
                    )
                    .map_err(|_| IntegrationMappingError::InvalidId)?,
                    tactic: *tactic,
                    channel: *channel,
                    amount: *amount,
                    dimensions: dimensions.clone(),
                    expected_outcome: DisplayName::try_new(expected_outcome.clone())
                        .map_err(|_| IntegrationMappingError::InvalidText)?,
                    requested_at: *requested_at,
                },
            ))),
            DomainEvent::CampaignBudgetChanged => Ok(None),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|events| events.into_iter().flatten().collect())
}
