use erpx_integration_contracts::{CurrencyCode, MoneyAmount, ReferenceCode, crm, customer};

use crate::domain::{DomainEvent, OpportunityStage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    InvalidId,
    InvalidValue,
}
pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<crm::Event>, IntegrationMappingError> {
    events
        .iter()
        .map(|event| match event {
            DomainEvent::LeadAccepted {
                lead_id,
                marketing_candidate_id,
                customer_id,
                source,
            } => Ok(crm::Event::LeadAccepted(crm::LeadAcceptedFact {
                lead: crm::LeadRef::try_from_uuid(lead_id.as_uuid())
                    .map_err(|_| IntegrationMappingError::InvalidId)?,
                marketing_candidate:
                    erpx_integration_contracts::marketing::LeadCandidateRef::try_from_uuid(
                        *marketing_candidate_id,
                    )
                    .map_err(|_| IntegrationMappingError::InvalidId)?,
                customer: customer_id
                    .map(customer::CustomerRef::try_from_uuid)
                    .transpose()
                    .map_err(|_| IntegrationMappingError::InvalidId)?,
                source: ReferenceCode::try_new(source.clone())
                    .map_err(|_| IntegrationMappingError::InvalidValue)?,
            })),
            DomainEvent::OpportunityChanged {
                opportunity_id,
                customer_id,
                stage,
                expected_value_minor,
                currency,
            } => Ok(crm::Event::OpportunityChanged(crm::OpportunityFact {
                opportunity: crm::OpportunityRef::try_from_uuid(opportunity_id.as_uuid())
                    .map_err(|_| IntegrationMappingError::InvalidId)?,
                customer: customer::CustomerRef::try_from_uuid(*customer_id)
                    .map_err(|_| IntegrationMappingError::InvalidId)?,
                stage: match stage {
                    OpportunityStage::Qualified => crm::OpportunityStage::Qualified,
                    OpportunityStage::Discovery => crm::OpportunityStage::Discovery,
                    OpportunityStage::Proposal => crm::OpportunityStage::Proposal,
                    OpportunityStage::Negotiation => crm::OpportunityStage::Negotiation,
                    OpportunityStage::Won => crm::OpportunityStage::Won,
                    OpportunityStage::Lost => crm::OpportunityStage::Lost,
                },
                expected_value: MoneyAmount::new(
                    *expected_value_minor,
                    CurrencyCode::try_new(currency)
                        .map_err(|_| IntegrationMappingError::InvalidValue)?,
                ),
            })),
            DomainEvent::OpportunityForecastChanged {
                opportunity_id,
                account,
                customer_id,
                owner,
                stage,
                category,
                probability_basis_points,
                expected_value_minor,
                currency,
                expected_close_on,
                originating_campaign,
            } => Ok(crm::Event::OpportunityForecastChanged(
                crm::OpportunityForecastFact {
                    opportunity: crm::OpportunityRef::try_from_uuid(opportunity_id.as_uuid())
                        .map_err(|_| IntegrationMappingError::InvalidId)?,
                    account: *account,
                    customer: customer::CustomerRef::try_from_uuid(*customer_id)
                        .map_err(|_| IntegrationMappingError::InvalidId)?,
                    owner: ReferenceCode::try_new(owner.clone())
                        .map_err(|_| IntegrationMappingError::InvalidValue)?,
                    stage: match stage {
                        OpportunityStage::Qualified => crm::OpportunityStage::Qualified,
                        OpportunityStage::Discovery => crm::OpportunityStage::Discovery,
                        OpportunityStage::Proposal => crm::OpportunityStage::Proposal,
                        OpportunityStage::Negotiation => crm::OpportunityStage::Negotiation,
                        OpportunityStage::Won => crm::OpportunityStage::Won,
                        OpportunityStage::Lost => crm::OpportunityStage::Lost,
                    },
                    category: *category,
                    probability_basis_points: *probability_basis_points,
                    expected_value: MoneyAmount::new(
                        *expected_value_minor,
                        CurrencyCode::try_new(currency)
                            .map_err(|_| IntegrationMappingError::InvalidValue)?,
                    ),
                    expected_close_on: *expected_close_on,
                    originating_campaign: *originating_campaign,
                },
            )),
        })
        .collect()
}
