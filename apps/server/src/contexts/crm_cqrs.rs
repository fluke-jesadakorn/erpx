//! Typed, storage-agnostic gRPC inbound adapter for CRM CQRS.

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey, QueryConsistency,
    QueryEnvelope, QueryMetadata, QueryResult, ReadYourWrites,
};
use erpx_crm::{
    AcceptMarketingLead, CreateOpportunity, CrmCommand, CrmLeadId, CrmLeadView, CrmMutationResult,
    CrmQuery, CrmQueryResponse, GetCrmLead, GetOpportunity, ListOpportunities, OpportunityId,
    OpportunityListFilter, OpportunityStage, OpportunityView, TransitionOpportunity,
};
use erpx_integration_contracts::{ReferenceCode, customer, marketing};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    QueryId, SessionId, TenantScope,
};
use erpx_proto::{common, erpx::crm::v2 as platform};
use std::sync::Arc;
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

const MAX_ROUTE_LENGTH: usize = 255;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrmTenantRoute {
    pub tenant_id: String,
    pub holding_ref: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrmAccessMode {
    Read,
    Write,
}
pub struct CrmAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: TenantScope,
    presented_route: Option<CrmTenantRoute>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: CrmAccessMode,
}
impl CrmAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &TenantScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> Option<&CrmTenantRoute> {
        self.presented_route.as_ref()
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> CrmAccessMode {
        self.access_mode
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedCrmRequest {
    actor: ActorAttribution,
    scope: TenantScope,
    route: CrmTenantRoute,
}
impl AuthenticatedCrmRequest {
    pub fn try_new(
        actor: ActorAttribution,
        scope: TenantScope,
        route: CrmTenantRoute,
    ) -> Result<Self, CrmAuthenticationError> {
        actor
            .validate()
            .map_err(|_| CrmAuthenticationError::Internal)?;
        scope
            .validate()
            .map_err(|_| CrmAuthenticationError::Internal)?;
        validate_route(&route).map_err(|_| CrmAuthenticationError::Internal)?;
        validate_legacy_uuid_route_scope(&route, &scope)
            .map_err(|_| CrmAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            scope,
            route,
        })
    }
    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn tenant_scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn route(&self) -> &CrmTenantRoute {
        &self.route
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrmAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}
#[async_trait]
pub trait CrmRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: CrmAuthenticationRequest,
    ) -> Result<AuthenticatedCrmRequest, CrmAuthenticationError>;
}
#[async_trait]
pub trait CrmCommandApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: CrmTenantRoute,
        command: CommandEnvelope<CrmCommand>,
    ) -> Result<CommandReceipt<CrmMutationResult>, CqrsError>;
}
#[async_trait]
pub trait CrmQueryApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: CrmTenantRoute,
        query: QueryEnvelope<CrmQuery>,
    ) -> Result<QueryResult<CrmQueryResponse>, CqrsError>;
}
#[derive(Debug, Clone)]
pub struct CrmCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> CrmCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}
#[derive(Debug, Clone)]
pub struct CrmQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> CrmQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

impl<A: CrmCommandApplication, G: CrmRequestAuthenticator> CrmCommandRpc<A, G> {
    pub async fn accept_marketing_lead(
        &self,
        request: Request<platform::AcceptMarketingLeadRequest>,
    ) -> Result<Response<platform::AcceptMarketingLeadResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let q = input
            .qualified
            .ok_or_else(|| invalid("qualified lead fact is required"))?;
        let fact = marketing::QualifiedLeadFact {
            candidate: marketing::LeadCandidateRef::try_from_uuid(parse_uuid(
                &q.candidate_id,
                "candidate_id",
            )?)
            .map_err(|_| invalid("candidate_id is invalid"))?,
            campaign: marketing::CampaignRef::try_from_uuid(parse_uuid(
                &q.campaign_id,
                "campaign_id",
            )?)
            .map_err(|_| invalid("campaign_id is invalid"))?,
            audience_member: marketing::AudienceMemberRef::try_from_uuid(parse_uuid(
                &q.audience_member_id,
                "audience_member_id",
            )?)
            .map_err(|_| invalid("audience_member_id is invalid"))?,
            existing_customer: q
                .existing_customer_id
                .as_deref()
                .map(|v| parse_uuid(v, "existing_customer_id"))
                .transpose()?
                .map(customer::CustomerRef::try_from_uuid)
                .transpose()
                .map_err(|_| invalid("existing_customer_id is invalid"))?,
            source: ReferenceCode::try_new(q.source).map_err(|_| invalid("source is invalid"))?,
            qualification_score_basis_points: u16::try_from(q.qualification_score_basis_points)
                .map_err(|_| invalid("qualification score is invalid"))?,
        };
        let receipt = self
            .command(
                transport,
                input.metadata,
                CrmCommand::AcceptMarketingLead(AcceptMarketingLead {
                    lead_id: crm_lead_id(&input.lead_id)?,
                    qualified: fact,
                }),
            )
            .await?;
        let CrmMutationResult::Lead(lead) = &receipt.response else {
            return Err(Status::internal("CRM command response mismatch"));
        };
        Ok(Response::new(platform::AcceptMarketingLeadResponse {
            receipt: Some(encode_receipt(&receipt)),
            lead: Some(encode_lead(lead)),
        }))
    }
    pub async fn create_opportunity(
        &self,
        request: Request<platform::CreateCrmOpportunityRequest>,
    ) -> Result<Response<platform::CrmOpportunityMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = CrmCommand::CreateOpportunity(CreateOpportunity {
            opportunity_id: opportunity_id(&input.opportunity_id)?,
            accepted_lead_id: crm_lead_id(&input.accepted_lead_id)?,
            customer_id: parse_uuid(&input.customer_id, "customer_id")?,
            name: input.name,
            expected_value_minor: input.expected_value_minor,
            currency: input.currency,
        });
        let receipt = self.command(transport, input.metadata, command).await?;
        opportunity_response(receipt)
    }
    pub async fn transition_opportunity(
        &self,
        request: Request<platform::TransitionCrmOpportunityRequest>,
    ) -> Result<Response<platform::CrmOpportunityMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = CrmCommand::TransitionOpportunity(TransitionOpportunity {
            opportunity_id: opportunity_id(&input.opportunity_id)?,
            stage: decode_stage(input.stage)?,
        });
        let receipt = self.command(transport, input.metadata, command).await?;
        opportunity_response(receipt)
    }
}
impl<A: CrmCommandApplication, G: CrmRequestAuthenticator> CrmCommandRpc<A, G> {
    async fn command(
        &self,
        transport: MetadataMap,
        metadata: Option<common::CommandRequestMetadata>,
        command: CrmCommand,
    ) -> Result<CommandReceipt<CrmMutationResult>, Status> {
        let decoded = decode_command(metadata, command)?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        self.application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)
    }
}
fn opportunity_response(
    receipt: CommandReceipt<CrmMutationResult>,
) -> Result<Response<platform::CrmOpportunityMutationResponse>, Status> {
    let CrmMutationResult::Opportunity(value) = &receipt.response else {
        return Err(Status::internal("CRM command response mismatch"));
    };
    Ok(Response::new(platform::CrmOpportunityMutationResponse {
        receipt: Some(encode_receipt(&receipt)),
        opportunity: Some(encode_opportunity(value)),
    }))
}

impl<A: CrmQueryApplication, G: CrmRequestAuthenticator> CrmQueryRpc<A, G> {
    pub async fn get_crm_lead(
        &self,
        request: Request<platform::GetCrmLeadRequest>,
    ) -> Result<Response<platform::GetCrmLeadResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let result = self
            .query(
                transport,
                input.metadata,
                CrmQuery::GetLead(GetCrmLead {
                    lead_id: crm_lead_id(&input.lead_id)?,
                }),
            )
            .await?;
        let CrmQueryResponse::Lead(value) = result.value else {
            return Err(Status::internal("CRM query response mismatch"));
        };
        Ok(Response::new(platform::GetCrmLeadResponse {
            lead: value.as_ref().map(encode_lead),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
    pub async fn get_opportunity(
        &self,
        request: Request<platform::GetCrmOpportunityRequest>,
    ) -> Result<Response<platform::GetCrmOpportunityResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let result = self
            .query(
                transport,
                input.metadata,
                CrmQuery::GetOpportunity(GetOpportunity {
                    opportunity_id: opportunity_id(&input.opportunity_id)?,
                }),
            )
            .await?;
        let CrmQueryResponse::Opportunity(value) = result.value else {
            return Err(Status::internal("CRM query response mismatch"));
        };
        Ok(Response::new(platform::GetCrmOpportunityResponse {
            opportunity: value.as_ref().map(encode_opportunity),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
    pub async fn list_opportunities(
        &self,
        request: Request<platform::ListCrmOpportunitiesRequest>,
    ) -> Result<Response<platform::ListCrmOpportunitiesResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let filter = OpportunityListFilter {
            stage: input.stage.map(decode_stage).transpose()?,
            customer_id: input
                .customer_id
                .as_deref()
                .map(|v| parse_uuid(v, "customer_id"))
                .transpose()?,
            page_size: u16::try_from(input.page_size)
                .map_err(|_| invalid("page_size is invalid"))?,
            cursor: input.cursor,
        };
        let result = self
            .query(
                transport,
                input.metadata,
                CrmQuery::ListOpportunities(ListOpportunities { filter }),
            )
            .await?;
        let CrmQueryResponse::Opportunities(value) = result.value else {
            return Err(Status::internal("CRM query response mismatch"));
        };
        Ok(Response::new(platform::ListCrmOpportunitiesResponse {
            opportunities: value.opportunities.iter().map(encode_opportunity).collect(),
            next_cursor: value.next_cursor,
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
}
impl<A: CrmQueryApplication, G: CrmRequestAuthenticator> CrmQueryRpc<A, G> {
    async fn query(
        &self,
        transport: MetadataMap,
        metadata: Option<common::QueryRequestMetadata>,
        query: CrmQuery,
    ) -> Result<QueryResult<CrmQueryResponse>, Status> {
        let decoded = decode_query(metadata, query)?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        self.application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)
    }
}

struct DecodedCommand<C> {
    route: Option<CrmTenantRoute>,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}
struct DecodedQuery<Q> {
    route: Option<CrmTenantRoute>,
    envelope: QueryEnvelope<Q>,
}
async fn authenticate_command<C, G: CrmRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(CrmTenantRoute, CommandEnvelope<C>), Status> {
    let authenticated = auth
        .authenticate(CrmAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: CrmAccessMode::Write,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
    Ok((authenticated.route().clone(), envelope))
}
async fn authenticate_query<Q, G: CrmRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(CrmTenantRoute, QueryEnvelope<Q>), Status> {
    let authenticated = auth
        .authenticate(CrmAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.route,
            subscription_id: None,
            mfa_code: None,
            access_mode: CrmAccessMode::Read,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
    Ok((authenticated.route().clone(), envelope))
}
fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
    let correlation_id = correlation(&metadata.correlation_id)?;
    let envelope = CommandEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract version is invalid"))?,
        command_id: CommandId::try_from_uuid(parse_uuid(&metadata.command_id, "command_id")?)
            .map_err(|_| invalid("command_id is invalid"))?,
        idempotency_key: IdempotencyKey::try_new(metadata.idempotency_key)
            .map_err(|_| invalid("idempotency key is invalid"))?,
        correlation_id,
        causation_id: metadata.causation_id.as_deref().map(event_id).transpose()?,
        actor: decode_actor(metadata.actor)?,
        tenant_scope: decode_scope(
            metadata
                .tenant_scope
                .ok_or_else(|| invalid("tenant_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit)?,
        expected_aggregate_version: metadata.expected_aggregate_version,
        command,
    };
    envelope
        .validate()
        .map_err(|_| invalid("command metadata is invalid"))?;
    Ok(DecodedCommand {
        route,
        envelope,
        subscription_id: clean(metadata.subscription_id),
        mfa_code: clean(metadata.mfa_code),
    })
}
fn decode_query<Q>(
    metadata: Option<common::QueryRequestMetadata>,
    query: Q,
) -> Result<DecodedQuery<Q>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("query metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract version is invalid"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&metadata.query_id, "query_id")?)
            .map_err(|_| invalid("query_id is invalid"))?,
        correlation_id: correlation(&metadata.correlation_id)?,
        actor: decode_actor(metadata.actor)?,
        tenant_scope: decode_scope(
            metadata
                .tenant_scope
                .ok_or_else(|| invalid("tenant_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit)?,
        consistency: decode_consistency(metadata.consistency)?,
        query,
    };
    envelope
        .validate()
        .map_err(|_| invalid("query metadata is invalid"))?;
    Ok(DecodedQuery { route, envelope })
}

fn encode_lead(v: &CrmLeadView) -> platform::CrmLeadRecord {
    platform::CrmLeadRecord {
        lead_id: v.id.as_uuid().to_string(),
        source_version: v.version,
        scope: Some(encode_scope(&v.scope)),
        marketing_candidate_id: v.marketing_candidate_id.to_string(),
        customer_id: v.customer_id.map(|id| id.to_string()),
        source: v.source.clone(),
        qualification_score_basis_points: u32::from(v.qualification_score_basis_points),
        accepted_at: v.accepted_at.to_rfc3339(),
        projected_at: String::new(),
        campaign_id: v.campaign_id.to_string(),
        audience_member_id: v.audience_member_id.to_string(),
        marketing_source_version: v.marketing_source_version,
    }
}
fn encode_opportunity(v: &OpportunityView) -> platform::CrmOpportunityRecord {
    platform::CrmOpportunityRecord {
        opportunity_id: v.id.as_uuid().to_string(),
        source_version: v.version,
        scope: Some(encode_scope(&v.scope)),
        accepted_lead_id: v.accepted_lead_id.as_uuid().to_string(),
        customer_id: v.customer_id.to_string(),
        name: v.name.clone(),
        expected_value_minor: v.expected_value_minor,
        currency: v.currency.clone(),
        stage: encode_stage(v.stage),
        created_at: v.created_at.to_rfc3339(),
        updated_at: v.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn decode_stage(v: i32) -> Result<OpportunityStage, Status> {
    match platform::CrmOpportunityStage::try_from(v).ok() {
        Some(platform::CrmOpportunityStage::Qualified) => Ok(OpportunityStage::Qualified),
        Some(platform::CrmOpportunityStage::Discovery) => Ok(OpportunityStage::Discovery),
        Some(platform::CrmOpportunityStage::Proposal) => Ok(OpportunityStage::Proposal),
        Some(platform::CrmOpportunityStage::Negotiation) => Ok(OpportunityStage::Negotiation),
        Some(platform::CrmOpportunityStage::Won) => Ok(OpportunityStage::Won),
        Some(platform::CrmOpportunityStage::Lost) => Ok(OpportunityStage::Lost),
        _ => Err(invalid("CRM opportunity stage is invalid")),
    }
}
fn encode_stage(v: OpportunityStage) -> i32 {
    match v {
        OpportunityStage::Qualified => platform::CrmOpportunityStage::Qualified as i32,
        OpportunityStage::Discovery => platform::CrmOpportunityStage::Discovery as i32,
        OpportunityStage::Proposal => platform::CrmOpportunityStage::Proposal as i32,
        OpportunityStage::Negotiation => platform::CrmOpportunityStage::Negotiation as i32,
        OpportunityStage::Won => platform::CrmOpportunityStage::Won as i32,
        OpportunityStage::Lost => platform::CrmOpportunityStage::Lost as i32,
    }
}
fn crm_lead_id(v: &str) -> Result<CrmLeadId, Status> {
    CrmLeadId::try_from_uuid(parse_uuid(v, "lead_id")?).map_err(|_| invalid("lead_id is invalid"))
}
fn opportunity_id(v: &str) -> Result<OpportunityId, Status> {
    OpportunityId::try_from_uuid(parse_uuid(v, "opportunity_id")?)
        .map_err(|_| invalid("opportunity_id is invalid"))
}
fn encode_receipt<R>(v: &CommandReceipt<R>) -> common::CommandReceipt {
    common::CommandReceipt {
        command_id: v.command_id.to_string(),
        correlation_id: v.correlation_id.to_string(),
        aggregate: Some(common::AggregateVersion {
            context: v.aggregate.context.clone(),
            aggregate_type: v.aggregate.aggregate_type.clone(),
            aggregate_id: v.aggregate.aggregate_id.to_string(),
            version: v.aggregate.version,
        }),
        committed_events: v
            .committed_events
            .iter()
            .map(|p| common::EventPosition {
                event_id: p.event_id.to_string(),
                global_sequence: p.global_sequence,
            })
            .collect(),
        committed_at: v.committed_at.to_rfc3339(),
        replayed: v.replayed,
    }
}
fn encode_query_metadata(v: &QueryMetadata) -> common::QueryMetadata {
    common::QueryMetadata {
        correlation_id: v.correlation_id.to_string(),
        projection: Some(common::ProjectionMetadata {
            checkpoint: v
                .projection
                .checkpoint
                .as_ref()
                .map(|c| common::ProjectionCheckpoint {
                    projection: c.projection.clone(),
                    partition: c.partition.clone(),
                    event_id: c.event_id.to_string(),
                    global_sequence: c.global_sequence,
                    updated_at: c.updated_at.to_rfc3339(),
                }),
            freshness: match v.projection.freshness {
                FreshnessState::Current => common::ProjectionFreshnessState::Current as i32,
                FreshnessState::Stale => common::ProjectionFreshnessState::Stale as i32,
                FreshnessState::Rebuilding => common::ProjectionFreshnessState::Rebuilding as i32,
                FreshnessState::Unavailable => common::ProjectionFreshnessState::Unavailable as i32,
            },
            lag_millis: v.projection.lag_millis,
        }),
        consistency: Some(encode_consistency(&v.consistency)),
    }
}
fn decode_consistency(v: Option<common::QueryConsistency>) -> Result<QueryConsistency, Status> {
    use common::query_consistency::Requirement;
    match v.and_then(|v| v.requirement) {
        None | Some(Requirement::Eventual(_)) => Ok(QueryConsistency::Eventual),
        Some(Requirement::ReadYourWrites(v)) => {
            Ok(QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: v.projection,
                partition: v.partition,
                target: erpx_cqrs::ProjectionTarget {
                    event_id: event_id(&v.event_id)?,
                    global_sequence: v.global_sequence,
                },
                timeout_millis: v.timeout_millis,
            }))
        }
        Some(Requirement::AggregateVersion(_)) => {
            Err(invalid("aggregate-version reads are unsupported"))
        }
    }
}
fn encode_consistency(v: &QueryConsistency) -> common::QueryConsistency {
    use common::query_consistency::Requirement;
    common::QueryConsistency {
        requirement: Some(match v {
            QueryConsistency::Eventual => Requirement::Eventual(common::EventualConsistency {}),
            QueryConsistency::ReadYourWrites(v) => {
                let wire = common::ReadYourWrites {
                    projection: v.projection.clone(),
                    partition: v.partition.clone(),
                    event_id: v.target.event_id.to_string(),
                    global_sequence: v.target.global_sequence,
                    timeout_millis: v.timeout_millis,
                    ..Default::default()
                };
                Requirement::ReadYourWrites(wire)
            }
        }),
    }
}
fn decode_route(v: Option<common::TenantRoute>) -> Result<Option<CrmTenantRoute>, Status> {
    let Some(v) = v else {
        return Ok(None);
    };
    let route = CrmTenantRoute {
        tenant_id: v.tenant_id.trim().into(),
        holding_ref: v.holding_ref.trim().into(),
    };
    validate_route(&route)?;
    Ok(Some(route))
}
fn validate_route(v: &CrmTenantRoute) -> Result<(), Status> {
    if [&v.tenant_id, &v.holding_ref]
        .iter()
        .any(|v| v.is_empty() || v.len() > MAX_ROUTE_LENGTH)
    {
        Err(invalid("tenant route is invalid"))
    } else {
        Ok(())
    }
}
fn validate_legacy_uuid_route_scope(
    route: &CrmTenantRoute,
    scope: &TenantScope,
) -> Result<(), Status> {
    let Ok(presented_holding) = Uuid::parse_str(&route.holding_ref) else {
        return Ok(());
    };
    let holding = match scope {
        TenantScope::Platform => return Ok(()),
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => holding_id,
    };
    if presented_holding == *holding {
        Ok(())
    } else {
        Err(invalid("tenant route does not match CRM scope"))
    }
}
fn decode_actor(v: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let v = v.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(v.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::TenantUser) => ActorKind::TenantUser,
        Some(common::ActorKind::Service) => ActorKind::Service,
        Some(common::ActorKind::SystemWorker) => ActorKind::SystemWorker,
        _ => return Err(invalid("actor kind is invalid")),
    };
    Ok(ActorAttribution {
        principal_ref: v.principal_ref,
        kind,
        session_id: optional_uuid(&v.session_id)?
            .map(SessionId::try_from_uuid)
            .transpose()
            .map_err(|_| invalid("session is invalid"))?,
        represented_principal_ref: nonempty(v.represented_principal_ref),
        support_capability_id: optional_uuid(&v.support_capability_id)?,
    })
}
fn decode_audit(v: Option<common::AuditContext>) -> Result<AuditContext, Status> {
    let v = v.ok_or_else(|| invalid("audit is required"))?;
    Ok(AuditContext {
        purpose: v.purpose,
        customer_visible: v.customer_visible,
        module: nonempty(v.module),
    })
}
fn decode_scope(v: common::TenantScope) -> Result<TenantScope, Status> {
    use common::tenant_scope::Scope;
    match v.scope.ok_or_else(|| invalid("scope is required"))? {
        Scope::Platform(_) => Ok(TenantScope::Platform),
        Scope::Holding(v) => Ok(TenantScope::Holding {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
        }),
        Scope::Company(v) => Ok(TenantScope::Company {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_id: parse_uuid(&v.company_id, "company_id")?,
        }),
        Scope::Branch(v) => Ok(TenantScope::Branch {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_id: parse_uuid(&v.company_id, "company_id")?,
            branch_id: parse_uuid(&v.branch_id, "branch_id")?,
        }),
        Scope::Subtree(v) => Ok(TenantScope::Subtree {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_ids: v
                .company_ids
                .iter()
                .map(|id| parse_uuid(id, "company_id"))
                .collect::<Result<_, _>>()?,
            branch_ids: v
                .branch_ids
                .iter()
                .map(|id| parse_uuid(id, "branch_id"))
                .collect::<Result<_, _>>()?,
        }),
    }
}
fn encode_scope(v: &TenantScope) -> common::TenantScope {
    use common::tenant_scope::Scope;
    common::TenantScope {
        scope: Some(match v {
            TenantScope::Platform => Scope::Platform(common::PlatformScope {}),
            TenantScope::Holding { holding_id } => Scope::Holding(common::HoldingScope {
                holding_id: holding_id.to_string(),
            }),
            TenantScope::Company {
                holding_id,
                company_id,
            } => Scope::Company(common::CompanyScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
            }),
            TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Scope::Branch(common::BranchScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
                branch_id: branch_id.to_string(),
            }),
            TenantScope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => Scope::Subtree(common::SubtreeScope {
                holding_id: holding_id.to_string(),
                company_ids: company_ids.iter().map(ToString::to_string).collect(),
                branch_ids: branch_ids.iter().map(ToString::to_string).collect(),
            }),
        }),
    }
}
fn parse_uuid(v: &str, field: &'static str) -> Result<Uuid, Status> {
    Uuid::parse_str(v)
        .map_err(|_| invalid(&format!("{field} is invalid")))
        .and_then(|id| {
            if id.is_nil() {
                Err(invalid(&format!("{field} is invalid")))
            } else {
                Ok(id)
            }
        })
}
fn correlation(v: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(v, "correlation_id")?)
        .map_err(|_| invalid("correlation is invalid"))
}
fn event_id(v: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(v, "event_id")?).map_err(|_| invalid("event is invalid"))
}
fn optional_uuid(v: &str) -> Result<Option<Uuid>, Status> {
    if v.trim().is_empty() {
        Ok(None)
    } else {
        parse_uuid(v, "optional UUID").map(Some)
    }
}
fn nonempty(v: String) -> Option<String> {
    if v.trim().is_empty() { None } else { Some(v) }
}
fn clean(v: Option<String>) -> Option<String> {
    v.and_then(nonempty)
}
fn status_from_cqrs(v: CqrsError) -> Status {
    match v.category() {
        ErrorCategory::Invalid => Status::invalid_argument(v.to_string()),
        ErrorCategory::Denied => Status::permission_denied(v.to_string()),
        ErrorCategory::NotFound => Status::not_found(v.to_string()),
        ErrorCategory::Conflict => Status::aborted(v.to_string()),
        ErrorCategory::NotReady => Status::failed_precondition(v.to_string()),
        ErrorCategory::Unavailable => Status::unavailable(v.to_string()),
        ErrorCategory::Internal => Status::internal(v.to_string()),
    }
}
fn status_from_auth(v: CrmAuthenticationError) -> Status {
    match v {
        CrmAuthenticationError::Unauthenticated => {
            Status::unauthenticated("authentication required")
        }
        CrmAuthenticationError::Denied => Status::permission_denied("CRM access denied"),
        CrmAuthenticationError::Unavailable => Status::unavailable("authentication unavailable"),
        CrmAuthenticationError::Internal => Status::internal("authentication failed"),
    }
}
fn invalid(v: &str) -> Status {
    Status::invalid_argument(v.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> ActorAttribution {
        ActorAttribution {
            principal_ref: "crm-route-test".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }
    #[test]
    fn discovery_stage_survives_transport() {
        assert_eq!(
            decode_stage(encode_stage(OpportunityStage::Discovery)).unwrap(),
            OpportunityStage::Discovery
        );
    }
    #[test]
    fn secret_bearing_auth_request_has_no_debug() {
        let source = include_str!("crm_cqrs.rs");
        let declaration = source
            .find(concat!("pub struct Crm", "AuthenticationRequest"))
            .expect("authentication request declaration");
        let immediate_attributes = source[..declaration]
            .lines()
            .rev()
            .take_while(|line| line.trim_start().starts_with("#["))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!immediate_attributes.contains("Debug"));
    }

    #[test]
    fn authenticated_route_must_match_authority_holding() {
        let holding = Uuid::from_u128(21);
        let scope = TenantScope::Branch {
            holding_id: holding,
            company_id: Uuid::from_u128(22),
            branch_id: Uuid::from_u128(23),
        };
        let matching = CrmTenantRoute {
            tenant_id: "tenant-a".into(),
            holding_ref: holding.to_string(),
        };
        assert!(AuthenticatedCrmRequest::try_new(actor(), scope.clone(), matching).is_ok());

        let mismatched = CrmTenantRoute {
            tenant_id: "tenant-a".into(),
            holding_ref: Uuid::from_u128(98).to_string(),
        };
        assert_eq!(
            AuthenticatedCrmRequest::try_new(actor(), scope, mismatched),
            Err(CrmAuthenticationError::Internal)
        );
    }

    #[test]
    fn tenant_route_may_be_omitted_for_server_resolution() {
        assert_eq!(decode_route(None).expect("optional route"), None);
    }
}
