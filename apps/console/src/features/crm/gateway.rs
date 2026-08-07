use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_crm::{
    CrmCommand, CrmLeadView, CrmMutationResult, CrmQuery, CrmQueryResponse, OpportunityList,
    OpportunityStage, OpportunityView,
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

use super::{
    models::{CrmLeadItem, CrmOpportunityItem, CrmOpportunityItems},
    presenter::CrmContent,
};

pub fn present_mutation(value: &CrmMutationResult) -> CrmContent {
    match value {
        CrmMutationResult::Lead(value) => CrmContent::Lead(CrmLeadItem::from(value.clone())),
        CrmMutationResult::Opportunity(value) => {
            CrmContent::Opportunity(CrmOpportunityItem::from(value.clone()))
        }
    }
}

pub fn present_query(value: QueryResult<CrmQueryResponse>) -> QueryResult<Option<CrmContent>> {
    let metadata = value.metadata;
    let value = match value.value {
        CrmQueryResponse::Lead(value) => value.map(CrmLeadItem::from).map(CrmContent::Lead),
        CrmQueryResponse::Opportunity(value) => value
            .map(CrmOpportunityItem::from)
            .map(CrmContent::Opportunity),
        CrmQueryResponse::Opportunities(value) => {
            let items = CrmOpportunityItems::from(value);
            (!items.opportunities.is_empty())
                .then_some(items)
                .map(CrmContent::Opportunities)
        }
    };
    QueryResult { value, metadata }
}

impl From<CrmLeadView> for CrmLeadItem {
    fn from(value: CrmLeadView) -> Self {
        Self {
            lead_id: value.id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            marketing_candidate_id: value.marketing_candidate_id.to_string(),
            customer_id: value.customer_id.map(|id| id.to_string()),
            source: value.source,
            qualification_score_basis_points: value.qualification_score_basis_points,
            accepted_at: value.accepted_at.to_rfc3339(),
        }
    }
}

impl From<OpportunityView> for CrmOpportunityItem {
    fn from(value: OpportunityView) -> Self {
        Self {
            opportunity_id: value.id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            accepted_lead_id: value.accepted_lead_id.as_uuid().to_string(),
            customer_id: value.customer_id.to_string(),
            name: value.name,
            expected_value_minor: value.expected_value_minor,
            currency: value.currency,
            stage: match value.stage {
                OpportunityStage::Qualified => "qualified",
                OpportunityStage::Discovery => "discovery",
                OpportunityStage::Proposal => "proposal",
                OpportunityStage::Negotiation => "negotiation",
                OpportunityStage::Won => "won",
                OpportunityStage::Lost => "lost",
            }
            .into(),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<OpportunityList> for CrmOpportunityItems {
    fn from(value: OpportunityList) -> Self {
        Self {
            opportunities: value
                .opportunities
                .into_iter()
                .map(CrmOpportunityItem::from)
                .collect(),
            next_cursor: value.next_cursor,
        }
    }
}
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait CrmProtoTransport: Send + Sync {
    async fn command(
        &self,
        envelope: CommandEnvelope<CrmCommand>,
    ) -> Result<CommandReceipt<CrmMutationResult>, UiError>;
    async fn query(
        &self,
        envelope: QueryEnvelope<CrmQuery>,
    ) -> Result<QueryResult<CrmQueryResponse>, UiError>;
}
#[derive(Debug, Clone)]
pub struct CrmGateway<T> {
    transport: T,
}
impl<T> CrmGateway<T> {
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
impl<T: CrmProtoTransport> CommandGateway<CrmCommand> for CrmGateway<T> {
    type Response = CrmMutationResult;
    async fn execute(
        &self,
        envelope: CommandEnvelope<CrmCommand>,
    ) -> Result<CommandReceipt<Self::Response>, UiError> {
        self.transport.command(envelope).await
    }
}
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: CrmProtoTransport> QueryGateway<CrmQuery> for CrmGateway<T> {
    type Response = CrmQueryResponse;
    async fn execute(
        &self,
        envelope: QueryEnvelope<CrmQuery>,
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
    impl CrmProtoTransport for Fake {
        async fn command(
            &self,
            _: CommandEnvelope<CrmCommand>,
        ) -> Result<CommandReceipt<CrmMutationResult>, UiError> {
            unreachable!()
        }
        async fn query(
            &self,
            _: QueryEnvelope<CrmQuery>,
        ) -> Result<QueryResult<CrmQueryResponse>, UiError> {
            unreachable!()
        }
    }
    #[test]
    fn gateway_is_typed() {
        fn command<G: CommandGateway<CrmCommand, Response = CrmMutationResult>>() {}
        fn query<G: QueryGateway<CrmQuery, Response = CrmQueryResponse>>() {}
        command::<CrmGateway<Fake>>();
        query::<CrmGateway<Fake>>();
    }
    #[test]
    fn protobuf_cannot_reach_feature() {
        let source = include_str!("gateway.rs");
        assert!(!source.contains(concat!("erpx_", "proto")));
    }

    #[test]
    fn gateway_is_the_only_core_to_ui_mapping_boundary() {
        let source = include_str!("gateway.rs");
        assert!(source.contains("impl From<OpportunityView> for CrmOpportunityItem"));
        for source in [
            include_str!("models.rs"),
            include_str!("presenter.rs"),
            include_str!("view.rs"),
        ] {
            assert!(!source.contains(concat!("erpx_", "crm")));
        }
    }
}
