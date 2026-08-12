use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_marketing::{
    AudienceMemberId, CampaignId, LeadCandidateId, LeadCandidateList, LeadCandidateStatus,
    LeadCandidateView, MarketingCommand, MarketingMutationResult, MarketingQuery,
    MarketingQueryResponse,
};
use erpx_proto::platform as wire;
use erpx_ui_foundation::UiError;

use crate::{features::marketing::MarketingProtoTransport, transport::typed_cqrs::GrpcChannel};

use super::common::{
    AuthenticatedRoute, Wave67Route, command_result, decode_scope, invalid_response, query_result,
    status_ui, timestamp, unsupported_ui, uuid,
};

type Commands = wire::marketing_command_service_client::MarketingCommandServiceClient<GrpcChannel>;
type Queries = wire::marketing_query_service_client::MarketingQueryServiceClient<GrpcChannel>;

#[derive(Clone)]
pub(crate) struct MarketingGrpcTransport {
    auth: AuthenticatedRoute,
    commands: Commands,
    queries: Queries,
}

impl MarketingGrpcTransport {
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
impl MarketingProtoTransport for MarketingGrpcTransport {
    async fn command(
        &self,
        envelope: CommandEnvelope<MarketingCommand>,
    ) -> Result<CommandReceipt<MarketingMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let mut client = self.commands.clone();
        let response = match envelope.command {
            MarketingCommand::Create(value) => client
                .create_lead_candidate(self.auth.request(
                    wire::CreateLeadCandidateRequest {
                        metadata: Some(metadata),
                        lead_id: value.lead_id.as_uuid().to_string(),
                        campaign_id: value.campaign_id.as_uuid().to_string(),
                        audience_member_id: value.audience_member_id.as_uuid().to_string(),
                        source: value.source,
                        existing_customer_id: value.existing_customer_id.map(|id| id.to_string()),
                    },
                    &actor,
                )?)
                .await
                .map_err(|status| status_ui("marketing create lead candidate", status))?
                .into_inner(),
            MarketingCommand::CaptureEngagement(value) => client
                .capture_engagement(self.auth.request(
                    wire::CaptureMarketingEngagementRequest {
                        metadata: Some(metadata),
                        lead_id: value.lead_id.as_uuid().to_string(),
                        channel: value.channel,
                        action: value.action,
                    },
                    &actor,
                )?)
                .await
                .map_err(|status| status_ui("marketing capture engagement", status))?
                .into_inner(),
            MarketingCommand::Qualify(value) => client
                .qualify_lead_candidate(self.auth.request(
                    wire::QualifyLeadCandidateRequest {
                        metadata: Some(metadata),
                        lead_id: value.lead_id.as_uuid().to_string(),
                        score_basis_points: u32::from(value.score_basis_points),
                    },
                    &actor,
                )?)
                .await
                .map_err(|status| status_ui("marketing qualify lead candidate", status))?
                .into_inner(),
            MarketingCommand::Reject(value) => client
                .reject_lead_candidate(self.auth.request(
                    wire::RejectLeadCandidateRequest {
                        metadata: Some(metadata),
                        lead_id: value.lead_id.as_uuid().to_string(),
                    },
                    &actor,
                )?)
                .await
                .map_err(|status| status_ui("marketing reject lead candidate", status))?
                .into_inner(),
            MarketingCommand::CreateBudget(_)
            | MarketingCommand::SubmitBudget(_)
            | MarketingCommand::ApplyBudgetDecision(_)
            | MarketingCommand::StartBudget(_)
            | MarketingCommand::CompleteBudget(_)
            | MarketingCommand::CancelBudget(_)
            | MarketingCommand::ReserveSpend(_)
            | MarketingCommand::ReleaseSpend(_) => {
                return Err(unsupported_ui("marketing campaign budget command"));
            }
        };
        let candidate = decode_candidate(
            response
                .candidate
                .ok_or_else(|| invalid_response("decode marketing candidate"))?,
        )?;
        command_result(
            response.receipt,
            MarketingMutationResult::Candidate(candidate),
            command_id,
            correlation,
        )
    }

    async fn query(
        &self,
        envelope: QueryEnvelope<MarketingQuery>,
    ) -> Result<QueryResult<MarketingQueryResponse>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let mut client = self.queries.clone();
        match envelope.query {
            MarketingQuery::Get(value) => {
                let response = client
                    .get_lead_candidate(self.auth.request(
                        wire::GetLeadCandidateRequest {
                            metadata: Some(metadata),
                            lead_id: value.lead_id.as_uuid().to_string(),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|status| status_ui("marketing get lead candidate", status))?
                    .into_inner();
                let value = response.candidate.map(decode_candidate).transpose()?;
                query_result(
                    response.metadata,
                    MarketingQueryResponse::Candidate(value),
                    correlation,
                    consistency,
                )
            }
            MarketingQuery::List(value) => {
                let response = client
                    .list_lead_candidates(self.auth.request(
                        wire::ListLeadCandidatesRequest {
                            metadata: Some(metadata),
                            status: value.filter.status.map(encode_status),
                            page_size: u32::from(value.filter.page_size),
                            cursor: value.filter.cursor,
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|status| status_ui("marketing list lead candidates", status))?
                    .into_inner();
                let value = LeadCandidateList {
                    candidates: response
                        .candidates
                        .into_iter()
                        .map(decode_candidate)
                        .collect::<Result<_, _>>()?,
                    next_cursor: response.next_cursor,
                };
                query_result(
                    response.metadata,
                    MarketingQueryResponse::Candidates(value),
                    correlation,
                    consistency,
                )
            }
            MarketingQuery::GetBudget(_) => Err(unsupported_ui("marketing campaign budget query")),
        }
    }
}

fn decode_candidate(value: wire::MarketingLeadCandidate) -> Result<LeadCandidateView, UiError> {
    Ok(LeadCandidateView {
        id: LeadCandidateId::try_from_uuid(uuid(&value.lead_id, "decode marketing candidate")?)
            .map_err(|_| invalid_response("decode marketing candidate"))?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        campaign_id: CampaignId::try_from_uuid(uuid(
            &value.campaign_id,
            "decode marketing candidate",
        )?)
        .map_err(|_| invalid_response("decode marketing candidate"))?,
        audience_member_id: AudienceMemberId::try_from_uuid(uuid(
            &value.audience_member_id,
            "decode marketing candidate",
        )?)
        .map_err(|_| invalid_response("decode marketing candidate"))?,
        source: value.source,
        existing_customer_id: value
            .existing_customer_id
            .map(|id| uuid(&id, "decode marketing candidate"))
            .transpose()?,
        score_basis_points: value
            .score_basis_points
            .map(|score| {
                u16::try_from(score).map_err(|_| invalid_response("decode marketing candidate"))
            })
            .transpose()?,
        status: decode_status(value.status)?,
        engagement_count: value.engagement_count,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}

fn encode_status(value: LeadCandidateStatus) -> i32 {
    match value {
        LeadCandidateStatus::Captured => wire::MarketingLeadCandidateStatus::Open as i32,
        LeadCandidateStatus::Qualified => wire::MarketingLeadCandidateStatus::Qualified as i32,
        LeadCandidateStatus::Rejected => wire::MarketingLeadCandidateStatus::Rejected as i32,
    }
}

fn decode_status(value: i32) -> Result<LeadCandidateStatus, UiError> {
    match wire::MarketingLeadCandidateStatus::try_from(value).ok() {
        Some(wire::MarketingLeadCandidateStatus::Open) => Ok(LeadCandidateStatus::Captured),
        Some(wire::MarketingLeadCandidateStatus::Qualified) => Ok(LeadCandidateStatus::Qualified),
        Some(wire::MarketingLeadCandidateStatus::Rejected) => Ok(LeadCandidateStatus::Rejected),
        _ => Err(invalid_response("decode marketing status")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_mapping_is_total_and_unspecified_fails_closed() {
        for value in [
            LeadCandidateStatus::Captured,
            LeadCandidateStatus::Qualified,
            LeadCandidateStatus::Rejected,
        ] {
            assert!(decode_status(encode_status(value)).is_ok());
        }
        assert!(decode_status(wire::MarketingLeadCandidateStatus::Unspecified as i32).is_err());
    }
}
