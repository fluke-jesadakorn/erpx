//! Typed, storage-agnostic gRPC inbound adapter for Marketing CQRS.

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey, QueryConsistency,
    QueryEnvelope, QueryMetadata, QueryResult, ReadYourWrites,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
    ErrorCategory, EventId, QueryId, SessionId,
};
use erpx_marketing::{
    AudienceMemberId, CampaignId, CaptureEngagement, CreateLeadCandidate, GetLeadCandidate,
    LeadCandidateId, LeadCandidateListFilter, LeadCandidateStatus, LeadCandidateView,
    ListLeadCandidates, MarketingCommand, MarketingMutationResult, MarketingQuery,
    MarketingQueryResponse, QualifyLeadCandidate, RejectLeadCandidate,
};
use erpx_proto::{common, erpx::marketing as platform};
use std::sync::Arc;
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

const MAX_ROUTE_LENGTH: usize = 255;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingHoldingRoute {
    pub holding_id: String,
    pub data_profile_key: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketingAccessMode {
    Read,
    Write,
}
pub struct MarketingAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: AuthorityScope,
    presented_route: Option<MarketingHoldingRoute>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: MarketingAccessMode,
}
impl MarketingAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &AuthorityScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> Option<&MarketingHoldingRoute> {
        self.presented_route.as_ref()
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> MarketingAccessMode {
        self.access_mode
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedMarketingRequest {
    actor: ActorAttribution,
    active_scope: AuthorityScope,
    route: MarketingHoldingRoute,
}
impl AuthenticatedMarketingRequest {
    pub fn try_new(
        actor: ActorAttribution,
        active_scope: AuthorityScope,
        route: MarketingHoldingRoute,
    ) -> Result<Self, MarketingAuthenticationError> {
        actor
            .validate()
            .map_err(|_| MarketingAuthenticationError::Internal)?;
        active_scope
            .validate()
            .map_err(|_| MarketingAuthenticationError::Internal)?;
        validate_route(&route).map_err(|_| MarketingAuthenticationError::Internal)?;
        validate_legacy_uuid_route_scope(&route, &active_scope)
            .map_err(|_| MarketingAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            active_scope,
            route,
        })
    }
    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn active_scope(&self) -> &AuthorityScope {
        &self.active_scope
    }
    pub const fn route(&self) -> &MarketingHoldingRoute {
        &self.route
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketingAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}
#[async_trait]
pub trait MarketingRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: MarketingAuthenticationRequest,
    ) -> Result<AuthenticatedMarketingRequest, MarketingAuthenticationError>;
}
#[async_trait]
pub trait MarketingCommandApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: MarketingHoldingRoute,
        command: CommandEnvelope<MarketingCommand>,
    ) -> Result<CommandReceipt<MarketingMutationResult>, CqrsError>;
}
#[async_trait]
pub trait MarketingQueryApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: MarketingHoldingRoute,
        query: QueryEnvelope<MarketingQuery>,
    ) -> Result<QueryResult<MarketingQueryResponse>, CqrsError>;
}
#[derive(Debug, Clone)]
pub struct MarketingCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> MarketingCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}
#[derive(Debug, Clone)]
pub struct MarketingQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> MarketingQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

impl<A: MarketingCommandApplication, G: MarketingRequestAuthenticator> MarketingCommandRpc<A, G> {
    pub async fn create_lead_candidate(
        &self,
        request: Request<platform::CreateLeadCandidateRequest>,
    ) -> Result<Response<platform::MarketingLeadCandidateMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let command = MarketingCommand::Create(CreateLeadCandidate {
            lead_id: lead_id(&input.lead_id)?,
            campaign_id: CampaignId::try_from_uuid(parse_uuid(&input.campaign_id, "campaign_id")?)
                .map_err(|_| invalid("campaign_id is invalid"))?,
            audience_member_id: AudienceMemberId::try_from_uuid(parse_uuid(
                &input.audience_member_id,
                "audience_member_id",
            )?)
            .map_err(|_| invalid("audience_member_id is invalid"))?,
            source: input.source,
            existing_customer_id: input
                .existing_customer_id
                .as_deref()
                .map(|v| parse_uuid(v, "existing_customer_id"))
                .transpose()?,
        });
        self.command(transport, input.metadata, command).await
    }
    pub async fn capture_engagement(
        &self,
        request: Request<platform::CaptureMarketingEngagementRequest>,
    ) -> Result<Response<platform::MarketingLeadCandidateMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        self.command(
            transport,
            input.metadata,
            MarketingCommand::CaptureEngagement(CaptureEngagement {
                lead_id: lead_id(&input.lead_id)?,
                channel: input.channel,
                action: input.action,
            }),
        )
        .await
    }
    pub async fn qualify_lead_candidate(
        &self,
        request: Request<platform::QualifyLeadCandidateRequest>,
    ) -> Result<Response<platform::MarketingLeadCandidateMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let score = u16::try_from(input.score_basis_points)
            .map_err(|_| invalid("score_basis_points is invalid"))?;
        self.command(
            transport,
            input.metadata,
            MarketingCommand::Qualify(QualifyLeadCandidate {
                lead_id: lead_id(&input.lead_id)?,
                score_basis_points: score,
            }),
        )
        .await
    }
    pub async fn reject_lead_candidate(
        &self,
        request: Request<platform::RejectLeadCandidateRequest>,
    ) -> Result<Response<platform::MarketingLeadCandidateMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        self.command(
            transport,
            input.metadata,
            MarketingCommand::Reject(RejectLeadCandidate {
                lead_id: lead_id(&input.lead_id)?,
            }),
        )
        .await
    }
}
impl<A: MarketingCommandApplication, G: MarketingRequestAuthenticator> MarketingCommandRpc<A, G> {
    async fn command(
        &self,
        transport: MetadataMap,
        metadata: Option<common::CommandRequestMetadata>,
        command: MarketingCommand,
    ) -> Result<Response<platform::MarketingLeadCandidateMutationResponse>, Status> {
        let decoded = decode_command(metadata, command)?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        let MarketingMutationResult::Candidate(candidate) = &receipt.response else {
            return Err(Status::internal("Marketing command response mismatch"));
        };
        Ok(Response::new(
            platform::MarketingLeadCandidateMutationResponse {
                receipt: Some(encode_receipt(&receipt)),
                candidate: Some(encode_candidate(candidate)),
            },
        ))
    }
}

impl<A: MarketingQueryApplication, G: MarketingRequestAuthenticator> MarketingQueryRpc<A, G> {
    pub async fn get_lead_candidate(
        &self,
        request: Request<platform::GetLeadCandidateRequest>,
    ) -> Result<Response<platform::GetLeadCandidateResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            MarketingQuery::Get(GetLeadCandidate {
                lead_id: lead_id(&input.lead_id)?,
            }),
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        let result = self
            .application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        let MarketingQueryResponse::Candidate(candidate) = result.value else {
            return Err(Status::internal("Marketing query response mismatch"));
        };
        Ok(Response::new(platform::GetLeadCandidateResponse {
            candidate: candidate.as_ref().map(encode_candidate),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
    pub async fn list_lead_candidates(
        &self,
        request: Request<platform::ListLeadCandidatesRequest>,
    ) -> Result<Response<platform::ListLeadCandidatesResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let status = input.status.map(decode_status).transpose()?;
        let page_size =
            u16::try_from(input.page_size).map_err(|_| invalid("page_size is invalid"))?;
        let decoded = decode_query(
            input.metadata,
            MarketingQuery::List(ListLeadCandidates {
                filter: LeadCandidateListFilter {
                    status,
                    page_size,
                    cursor: input.cursor,
                },
            }),
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        let result = self
            .application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        let MarketingQueryResponse::Candidates(list) = result.value else {
            return Err(Status::internal("Marketing query response mismatch"));
        };
        Ok(Response::new(platform::ListLeadCandidatesResponse {
            candidates: list.candidates.iter().map(encode_candidate).collect(),
            next_cursor: list.next_cursor,
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
}

struct DecodedCommand<C> {
    route: Option<MarketingHoldingRoute>,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}
struct DecodedQuery<Q> {
    route: Option<MarketingHoldingRoute>,
    envelope: QueryEnvelope<Q>,
}
async fn authenticate_command<C, G: MarketingRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(MarketingHoldingRoute, CommandEnvelope<C>), Status> {
    let authenticated = auth
        .authenticate(MarketingAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.active_scope.clone(),
            presented_route: decoded.route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: MarketingAccessMode::Write,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.active_scope = authenticated.active_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Marketing authority is invalid"))?;
    Ok((authenticated.route().clone(), envelope))
}
async fn authenticate_query<Q, G: MarketingRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(MarketingHoldingRoute, QueryEnvelope<Q>), Status> {
    let authenticated = auth
        .authenticate(MarketingAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.active_scope.clone(),
            presented_route: decoded.route,
            subscription_id: None,
            mfa_code: None,
            access_mode: MarketingAccessMode::Read,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.active_scope = authenticated.active_scope().clone();
    Ok((authenticated.route().clone(), envelope))
}
fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(metadata.holding_route)?;
    let correlation_id = correlation(&metadata.correlation_id)?;
    let envelope = CommandEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        command_id: CommandId::try_from_uuid(parse_uuid(&metadata.command_id, "command_id")?)
            .map_err(|_| invalid("command_id is invalid"))?,
        idempotency_key: IdempotencyKey::try_new(metadata.idempotency_key)
            .map_err(|_| invalid("idempotency_key is invalid"))?,
        correlation_id,
        causation_id: metadata.causation_id.as_deref().map(event_id).transpose()?,
        actor: decode_actor(metadata.actor)?,
        active_scope: decode_scope(
            metadata
                .active_scope
                .ok_or_else(|| invalid("active_scope is required"))?,
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
    let route = decode_route(metadata.holding_route)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&metadata.query_id, "query_id")?)
            .map_err(|_| invalid("query_id is invalid"))?,
        correlation_id: correlation(&metadata.correlation_id)?,
        actor: decode_actor(metadata.actor)?,
        active_scope: decode_scope(
            metadata
                .active_scope
                .ok_or_else(|| invalid("active_scope is required"))?,
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

fn encode_candidate(v: &LeadCandidateView) -> platform::MarketingLeadCandidate {
    platform::MarketingLeadCandidate {
        lead_id: v.id.as_uuid().to_string(),
        source_version: v.version,
        scope: Some(encode_scope(&v.scope)),
        campaign_id: v.campaign_id.as_uuid().to_string(),
        audience_member_id: v.audience_member_id.as_uuid().to_string(),
        source: v.source.clone(),
        existing_customer_id: v.existing_customer_id.map(|id| id.to_string()),
        score_basis_points: v.score_basis_points.map(u32::from),
        status: encode_status(v.status),
        engagement_count: v.engagement_count,
        created_at: v.created_at.to_rfc3339(),
        updated_at: v.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn decode_status(v: i32) -> Result<LeadCandidateStatus, Status> {
    match platform::MarketingLeadCandidateStatus::try_from(v).ok() {
        Some(platform::MarketingLeadCandidateStatus::Open) => Ok(LeadCandidateStatus::Captured),
        Some(platform::MarketingLeadCandidateStatus::Qualified) => {
            Ok(LeadCandidateStatus::Qualified)
        }
        Some(platform::MarketingLeadCandidateStatus::Rejected) => Ok(LeadCandidateStatus::Rejected),
        _ => Err(invalid("Marketing status is invalid")),
    }
}
fn encode_status(v: LeadCandidateStatus) -> i32 {
    match v {
        LeadCandidateStatus::Captured => platform::MarketingLeadCandidateStatus::Open as i32,
        LeadCandidateStatus::Qualified => platform::MarketingLeadCandidateStatus::Qualified as i32,
        LeadCandidateStatus::Rejected => platform::MarketingLeadCandidateStatus::Rejected as i32,
    }
}
fn lead_id(v: &str) -> Result<LeadCandidateId, Status> {
    LeadCandidateId::try_from_uuid(parse_uuid(v, "lead_id")?)
        .map_err(|_| invalid("lead_id is invalid"))
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
fn decode_route(v: Option<common::HoldingRoute>) -> Result<Option<MarketingHoldingRoute>, Status> {
    let Some(v) = v else {
        return Ok(None);
    };
    let route = MarketingHoldingRoute {
        holding_id: v.holding_id.trim().into(),
        data_profile_key: v.data_profile_key.trim().into(),
    };
    validate_route(&route)?;
    Ok(Some(route))
}
fn validate_route(v: &MarketingHoldingRoute) -> Result<(), Status> {
    if [&v.holding_id, &v.data_profile_key]
        .iter()
        .any(|v| v.is_empty() || v.len() > MAX_ROUTE_LENGTH)
    {
        Err(invalid("holding route is invalid"))
    } else {
        Ok(())
    }
}
fn validate_legacy_uuid_route_scope(
    route: &MarketingHoldingRoute,
    scope: &AuthorityScope,
) -> Result<(), Status> {
    let Ok(presented_holding) = Uuid::parse_str(&route.data_profile_key) else {
        return Ok(());
    };
    let holding = match scope {
        AuthorityScope::Platform => return Ok(()),
        AuthorityScope::Holding { holding_id }
        | AuthorityScope::Company { holding_id, .. }
        | AuthorityScope::Branch { holding_id, .. }
        | AuthorityScope::Subtree { holding_id, .. } => holding_id,
    };
    if presented_holding == *holding {
        Ok(())
    } else {
        Err(invalid("holding route does not match Marketing scope"))
    }
}
fn decode_actor(v: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let v = v.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(v.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::HoldingUser) => ActorKind::HoldingUser,
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
            .map_err(|_| invalid("session_id is invalid"))?,
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
fn decode_scope(v: common::AuthorityScope) -> Result<AuthorityScope, Status> {
    use common::authority_scope::Scope;
    match v.scope.ok_or_else(|| invalid("scope is required"))? {
        Scope::Platform(_) => Ok(AuthorityScope::Platform),
        Scope::Holding(v) => Ok(AuthorityScope::Holding {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
        }),
        Scope::Company(v) => Ok(AuthorityScope::Company {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_id: parse_uuid(&v.company_id, "company_id")?,
        }),
        Scope::Branch(v) => Ok(AuthorityScope::Branch {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_id: parse_uuid(&v.company_id, "company_id")?,
            branch_id: parse_uuid(&v.branch_id, "branch_id")?,
        }),
        Scope::Subtree(v) => Ok(AuthorityScope::Subtree {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            organization_node_id: parse_uuid(&v.organization_node_id, "organization_node_id")?,
        }),
    }
}
fn encode_scope(v: &AuthorityScope) -> common::AuthorityScope {
    use common::authority_scope::Scope;
    common::AuthorityScope {
        scope: Some(match v {
            AuthorityScope::Platform => Scope::Platform(common::PlatformScope {}),
            AuthorityScope::Holding { holding_id } => Scope::Holding(common::HoldingScope {
                holding_id: holding_id.to_string(),
            }),
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Scope::Company(common::CompanyScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
            }),
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Scope::Branch(common::BranchScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
                branch_id: branch_id.to_string(),
            }),
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Scope::Subtree(common::SubtreeScope {
                holding_id: holding_id.to_string(),
                organization_node_id: organization_node_id.to_string(),
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
        .map_err(|_| invalid("correlation_id is invalid"))
}
fn event_id(v: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(v, "event_id")?).map_err(|_| invalid("event_id is invalid"))
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
fn status_from_auth(v: MarketingAuthenticationError) -> Status {
    match v {
        MarketingAuthenticationError::Unauthenticated => {
            Status::unauthenticated("authentication required")
        }
        MarketingAuthenticationError::Denied => {
            Status::permission_denied("Marketing access denied")
        }
        MarketingAuthenticationError::Unavailable => {
            Status::unavailable("authentication unavailable")
        }
        MarketingAuthenticationError::Internal => Status::internal("authentication failed"),
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
            principal_ref: "marketing-route-test".into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }
    #[test]
    fn stage_and_scope_round_trip() {
        assert_eq!(
            decode_status(encode_status(LeadCandidateStatus::Qualified)).unwrap(),
            LeadCandidateStatus::Qualified
        );
        let scope = AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        assert_eq!(decode_scope(encode_scope(&scope)).unwrap(), scope);
    }
    #[test]
    fn secret_bearing_auth_request_has_no_debug() {
        let source = include_str!("marketing_cqrs.rs");
        let declaration = source
            .find(concat!("pub struct Marketing", "AuthenticationRequest"))
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
        let holding = Uuid::from_u128(11);
        let scope = AuthorityScope::Company {
            holding_id: holding,
            company_id: Uuid::from_u128(12),
        };
        let matching = MarketingHoldingRoute {
            holding_id: "holding-a".into(),
            data_profile_key: holding.to_string(),
        };
        assert!(AuthenticatedMarketingRequest::try_new(actor(), scope.clone(), matching).is_ok());

        let mismatched = MarketingHoldingRoute {
            holding_id: "holding-a".into(),
            data_profile_key: Uuid::from_u128(99).to_string(),
        };
        assert_eq!(
            AuthenticatedMarketingRequest::try_new(actor(), scope, mismatched),
            Err(MarketingAuthenticationError::Internal)
        );
    }

    #[test]
    fn holding_route_may_be_omitted_for_server_resolution() {
        assert_eq!(decode_route(None).expect("optional route"), None);
    }
}
