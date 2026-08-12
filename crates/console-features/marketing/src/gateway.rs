use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_marketing::{
    CampaignBudgetStatus, CampaignBudgetView, LeadCandidateList, LeadCandidateStatus,
    LeadCandidateView, MarketingCommand, MarketingMutationResult, MarketingQuery,
    MarketingQueryResponse, SpendReservationStatus,
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

use super::{
    models::{MarketingCampaignBudgetItem, MarketingLeadItem, MarketingLeadItems},
    presenter::MarketingContent,
};

pub fn present_mutation(value: &MarketingMutationResult) -> MarketingContent {
    match value {
        MarketingMutationResult::Candidate(value) => {
            MarketingContent::Candidate(MarketingLeadItem::from(value.clone()))
        }
        MarketingMutationResult::CampaignBudget(value) => MarketingContent::CampaignBudget(
            MarketingCampaignBudgetItem::from(value.as_ref().clone()),
        ),
    }
}

pub fn present_query(
    value: QueryResult<MarketingQueryResponse>,
) -> QueryResult<Option<MarketingContent>> {
    let metadata = value.metadata;
    let value = match value.value {
        MarketingQueryResponse::Candidate(value) => value
            .map(MarketingLeadItem::from)
            .map(MarketingContent::Candidate),
        MarketingQueryResponse::Candidates(value) => {
            let items = MarketingLeadItems::from(value);
            (!items.candidates.is_empty())
                .then_some(items)
                .map(MarketingContent::Candidates)
        }
        MarketingQueryResponse::CampaignBudget(value) => value
            .map(|value| MarketingCampaignBudgetItem::from(*value))
            .map(MarketingContent::CampaignBudget),
    };
    QueryResult { value, metadata }
}

impl From<LeadCandidateView> for MarketingLeadItem {
    fn from(value: LeadCandidateView) -> Self {
        Self {
            lead_id: value.id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            campaign_id: value.campaign_id.as_uuid().to_string(),
            audience_member_id: value.audience_member_id.as_uuid().to_string(),
            source: value.source,
            existing_customer_id: value.existing_customer_id.map(|id| id.to_string()),
            score_basis_points: value.score_basis_points,
            status: match value.status {
                LeadCandidateStatus::Captured => "open",
                LeadCandidateStatus::Qualified => "qualified",
                LeadCandidateStatus::Rejected => "rejected",
            }
            .into(),
            engagement_count: value.engagement_count,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<LeadCandidateList> for MarketingLeadItems {
    fn from(value: LeadCandidateList) -> Self {
        Self {
            candidates: value
                .candidates
                .into_iter()
                .map(MarketingLeadItem::from)
                .collect(),
            next_cursor: value.next_cursor,
        }
    }
}

impl From<CampaignBudgetView> for MarketingCampaignBudgetItem {
    fn from(value: CampaignBudgetView) -> Self {
        Self {
            budget_id: value.id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            campaign_id: value.campaign_id.as_uuid().to_string(),
            code: value.code,
            name: value.name,
            period: format!("{:?}", value.period),
            currency: value.requested_amount.currency().as_str().into(),
            requested_minor: value.requested_amount.minor_units(),
            approved_minor: value.approved_amount.map(|amount| amount.minor_units()),
            reserved_minor: value.reserved_minor,
            available_minor: value.available_minor,
            allocation_count: value.allocations.len(),
            active_spend_request_count: value
                .reservations
                .iter()
                .filter(|reservation| reservation.status == SpendReservationStatus::Active)
                .count(),
            approval_request_id: value
                .approval_request
                .map(|request| request.as_uuid().to_string()),
            status: match value.status {
                CampaignBudgetStatus::Draft => "draft",
                CampaignBudgetStatus::Review => "review",
                CampaignBudgetStatus::Approved => "approved",
                CampaignBudgetStatus::Running => "running",
                CampaignBudgetStatus::Completed => "completed",
                CampaignBudgetStatus::Cancelled => "cancelled",
            }
            .into(),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

/// Generated protobuf and channel details stop behind this boundary.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait MarketingProtoTransport: Send + Sync {
    async fn command(
        &self,
        envelope: CommandEnvelope<MarketingCommand>,
    ) -> Result<CommandReceipt<MarketingMutationResult>, UiError>;
    async fn query(
        &self,
        envelope: QueryEnvelope<MarketingQuery>,
    ) -> Result<QueryResult<MarketingQueryResponse>, UiError>;
}
#[derive(Debug, Clone)]
pub struct MarketingGateway<T> {
    transport: T,
}
impl<T> MarketingGateway<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }
    pub const fn transport(&self) -> &T {
        &self.transport
    }
    pub fn into_transport(self) -> T {
        self.transport
    }
}
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: MarketingProtoTransport> CommandGateway<MarketingCommand> for MarketingGateway<T> {
    type Response = MarketingMutationResult;
    async fn execute(
        &self,
        envelope: CommandEnvelope<MarketingCommand>,
    ) -> Result<CommandReceipt<Self::Response>, UiError> {
        self.transport.command(envelope).await
    }
}
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: MarketingProtoTransport> QueryGateway<MarketingQuery> for MarketingGateway<T> {
    type Response = MarketingQueryResponse;
    async fn execute(
        &self,
        envelope: QueryEnvelope<MarketingQuery>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.query(envelope).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake;
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl MarketingProtoTransport for Fake {
        async fn command(
            &self,
            _: CommandEnvelope<MarketingCommand>,
        ) -> Result<CommandReceipt<MarketingMutationResult>, UiError> {
            unreachable!()
        }
        async fn query(
            &self,
            _: QueryEnvelope<MarketingQuery>,
        ) -> Result<QueryResult<MarketingQueryResponse>, UiError> {
            unreachable!()
        }
    }
    #[test]
    fn gateway_is_typed_for_both_directions() {
        fn command<G: CommandGateway<MarketingCommand, Response = MarketingMutationResult>>() {}
        fn query<G: QueryGateway<MarketingQuery, Response = MarketingQueryResponse>>() {}
        command::<MarketingGateway<Fake>>();
        query::<MarketingGateway<Fake>>();
    }
    #[test]
    fn transport_details_do_not_leak() {
        let source = include_str!("gateway.rs");
        assert!(!source.contains(concat!("erpx_", "proto")));
        assert!(!source.contains(concat!("tonic", "::")));
    }

    #[test]
    fn gateway_is_the_only_core_to_ui_mapping_boundary() {
        let source = include_str!("gateway.rs");
        assert!(source.contains("impl From<LeadCandidateView> for MarketingLeadItem"));
        for source in [
            include_str!("models.rs"),
            include_str!("presenter.rs"),
            include_str!("view.rs"),
        ] {
            assert!(!source.contains(concat!("erpx_", "marketing")));
        }
    }
}
