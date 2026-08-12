use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{BoundedContext, MoneyAmount, ReferenceCode, customer::CustomerRef};

owned_uuid_reference!(LeadRef);
owned_uuid_reference!(OpportunityRef);
owned_uuid_reference!(AccountRelationshipRef);
owned_uuid_reference!(CrmAccountRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpportunityStage {
    Qualified,
    Discovery,
    Proposal,
    Negotiation,
    Won,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadAcceptedFact {
    pub lead: LeadRef,
    pub marketing_candidate: crate::marketing::LeadCandidateRef,
    pub customer: Option<CustomerRef>,
    pub source: ReferenceCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpportunityFact {
    pub opportunity: OpportunityRef,
    pub customer: CustomerRef,
    pub stage: OpportunityStage,
    pub expected_value: MoneyAmount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineCategory {
    Pipeline,
    BestCase,
    Commit,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpportunityForecastFact {
    pub opportunity: OpportunityRef,
    pub account: CrmAccountRef,
    pub customer: CustomerRef,
    pub owner: ReferenceCode,
    pub stage: OpportunityStage,
    pub category: PipelineCategory,
    pub probability_basis_points: u16,
    pub expected_value: MoneyAmount,
    pub expected_close_on: NaiveDate,
    pub originating_campaign: Option<crate::marketing::CampaignRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    LeadAccepted(LeadAcceptedFact),
    OpportunityChanged(OpportunityFact),
    OpportunityForecastChanged(OpportunityForecastFact),
}

impl_integration_event!(Event, BoundedContext::Crm, {
    Self::LeadAccepted(_) => "crm.lead_accepted",
    Self::OpportunityChanged(_) => "crm.opportunity_changed",
    Self::OpportunityForecastChanged(_) => "crm.opportunity_forecast_changed",
});
