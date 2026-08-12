use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_crm::{
    CrmCommand, CrmLeadId, CrmLeadView, CrmMutationResult, CrmQuery, CrmQueryResponse,
    OpportunityId, OpportunityList, OpportunityStage, OpportunityView,
};
use erpx_proto::platform as wire;
use erpx_ui_foundation::UiError;

use crate::{features::crm::CrmProtoTransport, transport::typed_cqrs::GrpcChannel};

use super::common::{
    AuthenticatedRoute, Wave67Route, command_result, decode_scope, invalid_response, query_result,
    status_ui, timestamp, unsupported_ui, uuid,
};

type Commands = wire::crm_command_service_client::CrmCommandServiceClient<GrpcChannel>;
type Queries = wire::crm_query_service_client::CrmQueryServiceClient<GrpcChannel>;

#[derive(Clone)]
pub(crate) struct CrmGrpcTransport {
    auth: AuthenticatedRoute,
    commands: Commands,
    queries: Queries,
}

impl CrmGrpcTransport {
    pub(crate) fn try_new(
        channel: GrpcChannel,
        token: impl Into<String>,
        route: Wave67Route,
    ) -> Result<Self, UiError> {
        Ok(Self {
            auth: AuthenticatedRoute::try_new(token, route)?,
            commands: Commands::new(channel.clone()),
            queries: Queries::new(channel),
        })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl CrmProtoTransport for CrmGrpcTransport {
    async fn command(
        &self,
        envelope: CommandEnvelope<CrmCommand>,
    ) -> Result<CommandReceipt<CrmMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let mut client = self.commands.clone();
        match envelope.command {
            CrmCommand::AcceptMarketingLead(value) => {
                let fact = value.qualified;
                let response = client
                    .accept_marketing_lead(self.auth.request(
                        wire::AcceptMarketingLeadRequest {
                            metadata: Some(metadata),
                            lead_id: value.lead_id.as_uuid().to_string(),
                            qualified: Some(wire::MarketingQualifiedLeadFact {
                                candidate_id: fact.candidate.as_uuid().to_string(),
                                campaign_id: fact.campaign.as_uuid().to_string(),
                                audience_member_id: fact.audience_member.as_uuid().to_string(),
                                existing_customer_id:
                                    fact.existing_customer.map(|id| id.as_uuid().to_string()),
                                source: fact.source.into_inner(),
                                qualification_score_basis_points: u32::from(
                                    fact.qualification_score_basis_points,
                                ),
                            }),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|status| status_ui("crm accept marketing lead", status))?
                    .into_inner();
                let value = decode_lead(
                    response
                        .lead
                        .ok_or_else(|| invalid_response("decode crm lead"))?,
                )?;
                command_result(
                    response.receipt,
                    CrmMutationResult::Lead(value),
                    command_id,
                    correlation,
                )
            }
            CrmCommand::CreateOpportunity(value) => {
                let response = client
                    .create_opportunity(self.auth.request(
                        wire::CreateCrmOpportunityRequest {
                            metadata: Some(metadata),
                            opportunity_id: value.opportunity_id.as_uuid().to_string(),
                            accepted_lead_id: value.accepted_lead_id.as_uuid().to_string(),
                            customer_id: value.customer_id.to_string(),
                            name: value.name,
                            expected_value_minor: value.expected_value_minor,
                            currency: value.currency,
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|status| status_ui("crm create opportunity", status))?
                    .into_inner();
                let value = decode_opportunity(
                    response
                        .opportunity
                        .ok_or_else(|| invalid_response("decode crm opportunity"))?,
                )?;
                command_result(
                    response.receipt,
                    CrmMutationResult::Opportunity(value),
                    command_id,
                    correlation,
                )
            }
            CrmCommand::TransitionOpportunity(value) => {
                let response = client
                    .transition_opportunity(self.auth.request(
                        wire::TransitionCrmOpportunityRequest {
                            metadata: Some(metadata),
                            opportunity_id: value.opportunity_id.as_uuid().to_string(),
                            stage: encode_stage(value.stage),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|status| status_ui("crm transition opportunity", status))?
                    .into_inner();
                let value = decode_opportunity(
                    response
                        .opportunity
                        .ok_or_else(|| invalid_response("decode crm opportunity"))?,
                )?;
                command_result(
                    response.receipt,
                    CrmMutationResult::Opportunity(value),
                    command_id,
                    correlation,
                )
            }
            CrmCommand::UpdateOpportunityForecast(_) => {
                Err(unsupported_ui("crm opportunity forecast update"))
            }
        }
    }

    async fn query(
        &self,
        envelope: QueryEnvelope<CrmQuery>,
    ) -> Result<QueryResult<CrmQueryResponse>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let mut client = self.queries.clone();
        match envelope.query {
            CrmQuery::GetLead(value) => {
                let response = client
                    .get_crm_lead(self.auth.request(
                        wire::GetCrmLeadRequest {
                            metadata: Some(metadata),
                            lead_id: value.lead_id.as_uuid().to_string(),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|status| status_ui("crm get lead", status))?
                    .into_inner();
                query_result(
                    response.metadata,
                    CrmQueryResponse::Lead(response.lead.map(decode_lead).transpose()?),
                    correlation,
                    consistency,
                )
            }
            CrmQuery::GetOpportunity(value) => {
                let response = client
                    .get_opportunity(self.auth.request(
                        wire::GetCrmOpportunityRequest {
                            metadata: Some(metadata),
                            opportunity_id: value.opportunity_id.as_uuid().to_string(),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|status| status_ui("crm get opportunity", status))?
                    .into_inner();
                query_result(
                    response.metadata,
                    CrmQueryResponse::Opportunity(
                        response.opportunity.map(decode_opportunity).transpose()?,
                    ),
                    correlation,
                    consistency,
                )
            }
            CrmQuery::ListOpportunities(value) => {
                let response = client
                    .list_opportunities(self.auth.request(
                        wire::ListCrmOpportunitiesRequest {
                            metadata: Some(metadata),
                            stage: value.filter.stage.map(encode_stage),
                            customer_id: value.filter.customer_id.map(|id| id.to_string()),
                            page_size: u32::from(value.filter.page_size),
                            cursor: value.filter.cursor,
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|status| status_ui("crm list opportunities", status))?
                    .into_inner();
                let value = OpportunityList {
                    opportunities: response
                        .opportunities
                        .into_iter()
                        .map(decode_opportunity)
                        .collect::<Result<_, _>>()?,
                    next_cursor: response.next_cursor,
                };
                query_result(
                    response.metadata,
                    CrmQueryResponse::Opportunities(value),
                    correlation,
                    consistency,
                )
            }
        }
    }
}

fn decode_lead(value: wire::CrmLeadRecord) -> Result<CrmLeadView, UiError> {
    Ok(CrmLeadView {
        id: CrmLeadId::try_from_uuid(uuid(&value.lead_id, "decode crm lead")?)
            .map_err(|_| invalid_response("decode crm lead"))?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        marketing_candidate_id: uuid(&value.marketing_candidate_id, "decode crm lead")?,
        customer_id: value
            .customer_id
            .map(|id| uuid(&id, "decode crm lead"))
            .transpose()?,
        source: value.source,
        qualification_score_basis_points: u16::try_from(value.qualification_score_basis_points)
            .map_err(|_| invalid_response("decode crm lead"))?,
        accepted_at: timestamp(&value.accepted_at)?,
    })
}

fn decode_opportunity(value: wire::CrmOpportunityRecord) -> Result<OpportunityView, UiError> {
    Ok(OpportunityView {
        id: OpportunityId::try_from_uuid(uuid(&value.opportunity_id, "decode crm opportunity")?)
            .map_err(|_| invalid_response("decode crm opportunity"))?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        accepted_lead_id: CrmLeadId::try_from_uuid(uuid(
            &value.accepted_lead_id,
            "decode crm opportunity",
        )?)
        .map_err(|_| invalid_response("decode crm opportunity"))?,
        customer_id: uuid(&value.customer_id, "decode crm opportunity")?,
        name: value.name,
        expected_value_minor: value.expected_value_minor,
        currency: value.currency,
        stage: decode_stage(value.stage)?,
        account: None,
        owner: None,
        pipeline_category: None,
        probability_basis_points: None,
        expected_close_on: None,
        originating_campaign: None,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}

fn encode_stage(value: OpportunityStage) -> i32 {
    match value {
        OpportunityStage::Qualified => wire::CrmOpportunityStage::Qualified as i32,
        OpportunityStage::Discovery => wire::CrmOpportunityStage::Discovery as i32,
        OpportunityStage::Proposal => wire::CrmOpportunityStage::Proposal as i32,
        OpportunityStage::Negotiation => wire::CrmOpportunityStage::Negotiation as i32,
        OpportunityStage::Won => wire::CrmOpportunityStage::Won as i32,
        OpportunityStage::Lost => wire::CrmOpportunityStage::Lost as i32,
    }
}

fn decode_stage(value: i32) -> Result<OpportunityStage, UiError> {
    match wire::CrmOpportunityStage::try_from(value).ok() {
        Some(wire::CrmOpportunityStage::Qualified) => Ok(OpportunityStage::Qualified),
        Some(wire::CrmOpportunityStage::Discovery) => Ok(OpportunityStage::Discovery),
        Some(wire::CrmOpportunityStage::Proposal) => Ok(OpportunityStage::Proposal),
        Some(wire::CrmOpportunityStage::Negotiation) => Ok(OpportunityStage::Negotiation),
        Some(wire::CrmOpportunityStage::Won) => Ok(OpportunityStage::Won),
        Some(wire::CrmOpportunityStage::Lost) => Ok(OpportunityStage::Lost),
        _ => Err(invalid_response("decode crm stage")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stage_mapping_is_total_and_unspecified_fails_closed() {
        for value in [
            OpportunityStage::Qualified,
            OpportunityStage::Discovery,
            OpportunityStage::Proposal,
            OpportunityStage::Negotiation,
            OpportunityStage::Won,
            OpportunityStage::Lost,
        ] {
            assert!(decode_stage(encode_stage(value)).is_ok());
        }
        assert!(decode_stage(wire::CrmOpportunityStage::Unspecified as i32).is_err());
    }
}
