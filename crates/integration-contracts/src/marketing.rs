use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    BoundedContext, DimensionSet, DisplayName, MoneyAmount, RecordRef, ReferenceCode,
    customer::CustomerRef,
};

owned_uuid_reference!(CampaignRef);
owned_uuid_reference!(AudienceMemberRef);
owned_uuid_reference!(LeadCandidateRef);
owned_uuid_reference!(TacticRef);
owned_uuid_reference!(ChannelRef);
owned_uuid_reference!(CampaignBudgetRef);
owned_uuid_reference!(BudgetAllocationRef);
owned_uuid_reference!(SpendRequestRef);
owned_uuid_reference!(AttributionRevisionRef);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignBudgetPeriod {
    Annual,
    Quarterly,
    Campaign,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignBudgetApprovedFact {
    pub budget: CampaignBudgetRef,
    pub campaign: CampaignRef,
    pub period: CampaignBudgetPeriod,
    pub approved_amount: MoneyAmount,
    pub dimensions: DimensionSet,
    pub approved_at: DateTime<Utc>,
}

/// Exact approval request emitted by Marketing for worker-only ingestion by
/// Operations. The subject version binds the decision to the submitted draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignBudgetApprovalRequestedFact {
    pub budget: CampaignBudgetRef,
    pub campaign: CampaignRef,
    pub approval: crate::operations::ApprovalRequestFact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignSpendRequestedFact {
    pub request: SpendRequestRef,
    pub budget: CampaignBudgetRef,
    pub campaign: CampaignRef,
    pub allocation: BudgetAllocationRef,
    pub tactic: TacticRef,
    pub channel: ChannelRef,
    pub amount: MoneyAmount,
    pub dimensions: DimensionSet,
    pub expected_outcome: DisplayName,
    pub requested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributionBasis {
    Forecast,
    Booked,
    Recognized,
    Cash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributedCampaignAmount {
    pub campaign: CampaignRef,
    pub amount: MoneyAmount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributionRevisionFact {
    pub revision: AttributionRevisionRef,
    pub basis: AttributionBasis,
    pub revenue_source: RecordRef,
    pub source_total: MoneyAmount,
    pub allocations: Vec<AttributedCampaignAmount>,
    pub published_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    LeadQualified(QualifiedLeadFact),
    EngagementCaptured(EngagementFact),
    CampaignBudgetApprovalRequested(CampaignBudgetApprovalRequestedFact),
    CampaignBudgetApproved(CampaignBudgetApprovedFact),
    CampaignSpendRequested(CampaignSpendRequestedFact),
    AttributionRevisionPublished(AttributionRevisionFact),
}

impl crate::IntegrationEvent for Event {
    fn owner(&self) -> BoundedContext {
        BoundedContext::Marketing
    }

    fn event_type(&self) -> &'static str {
        match self {
            Self::LeadQualified(_) => "marketing.lead_qualified",
            Self::EngagementCaptured(_) => "marketing.engagement_captured",
            Self::CampaignBudgetApprovalRequested(_) => {
                "marketing.campaign_budget_approval_requested"
            }
            Self::CampaignBudgetApproved(_) => "marketing.campaign_budget_approved",
            Self::CampaignSpendRequested(_) => "marketing.campaign_spend_requested",
            Self::AttributionRevisionPublished(_) => "marketing.attribution_revision_published",
        }
    }

    fn schema_version(&self) -> u16 {
        1
    }

    fn validate(&self) -> Result<(), crate::ContractValidationError> {
        match self {
            Self::LeadQualified(fact) if fact.qualification_score_basis_points > 10_000 => {
                return Err(crate::ContractValidationError::OutOfRange(
                    "qualification_score_basis_points",
                ));
            }
            Self::CampaignBudgetApprovalRequested(fact) => {
                if fact.approval.subject.context != BoundedContext::Marketing
                    || fact.approval.subject.aggregate_kind.as_str() != "campaign_budget"
                    || fact.approval.subject.aggregate_id != fact.budget.as_uuid()
                    || fact.approval.amount.is_none()
                    || fact.approval.requested_at.timestamp() < 0
                {
                    return Err(crate::ContractValidationError::OutOfRange(
                        "campaign_budget_approval_request",
                    ));
                }
            }
            Self::CampaignBudgetApproved(fact)
                if fact.approved_amount.minor_units() <= 0 || fact.approved_at.timestamp() < 0 =>
            {
                return Err(crate::ContractValidationError::OutOfRange(
                    "campaign_budget",
                ));
            }
            Self::CampaignSpendRequested(fact)
                if fact.amount.minor_units() <= 0 || fact.requested_at.timestamp() < 0 =>
            {
                return Err(crate::ContractValidationError::OutOfRange(
                    "campaign_spend_request",
                ));
            }
            Self::AttributionRevisionPublished(fact) => {
                if fact.allocations.is_empty() || fact.published_at.timestamp() < 0 {
                    return Err(crate::ContractValidationError::Empty(
                        "attribution.allocations",
                    ));
                }
                let source_currency = fact.source_total.currency();
                let allocated = fact.allocations.iter().try_fold(0_i64, |total, item| {
                    if item.amount.currency() != source_currency || item.amount.minor_units() < 0 {
                        return None;
                    }
                    total.checked_add(item.amount.minor_units())
                });
                if allocated != Some(fact.source_total.minor_units()) {
                    return Err(crate::ContractValidationError::OutOfRange(
                        "attribution.allocated_total",
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }
}
