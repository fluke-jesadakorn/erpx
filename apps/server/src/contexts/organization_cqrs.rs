//! Typed gRPC inbound adapter for the Organization bounded context.
//!
//! Runtime composition supplies authorization and application ports. This
//! module performs no pool construction or persistence access.

use std::sync::Arc;

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey,
    ProjectionCheckpoint, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryMetadata,
    QueryResult, ReadYourWrites,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
    ErrorCategory, EventId, QueryId, SessionId,
};
use erpx_organization::{
    ActivateOrganizationNode, ArchiveOrganizationNode, ChangeOrganizationCode,
    CreateOrganizationNode, CreateOrganizationNodeResult, GetOrganizationNode,
    GetOrganizationNodeResult, ListOrganizationNodes, NodeList, NodeListFilter, NodeView,
    OrganizationMutationResult, OrganizationNodeId, OrganizationNodeKind, OrganizationNodeStatus,
    OrganizationPlacement, RenameOrganizationNode, ReparentOrganizationNode,
    SuspendOrganizationNode,
};
use erpx_proto::{common, erpx::organization as platform};
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

use platform::organization_command_service_server::OrganizationCommandService;
use platform::organization_query_service_server::OrganizationQueryService;

const MAX_ROUTE_LENGTH: usize = 255;
const MAX_SECURITY_INPUT_LENGTH: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationHoldingRoute {
    pub holding_id: String,
    pub data_profile_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganizationAccessMode {
    Read,
    Write,
}

pub struct OrganizationAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: AuthorityScope,
    presented_route: Option<OrganizationHoldingRoute>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: OrganizationAccessMode,
}

impl OrganizationAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &AuthorityScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> Option<&OrganizationHoldingRoute> {
        self.presented_route.as_ref()
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> OrganizationAccessMode {
        self.access_mode
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedOrganizationRequest {
    actor: ActorAttribution,
    active_scope: AuthorityScope,
    holding_route: OrganizationHoldingRoute,
}

impl AuthenticatedOrganizationRequest {
    pub fn try_new(
        actor: ActorAttribution,
        active_scope: AuthorityScope,
        holding_route: OrganizationHoldingRoute,
    ) -> Result<Self, OrganizationAuthenticationError> {
        actor
            .validate()
            .map_err(|_| OrganizationAuthenticationError::Internal)?;
        active_scope
            .validate()
            .map_err(|_| OrganizationAuthenticationError::Internal)?;
        validate_route(&holding_route).map_err(|_| OrganizationAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            active_scope,
            holding_route,
        })
    }

    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn active_scope(&self) -> &AuthorityScope {
        &self.active_scope
    }
    pub const fn holding_route(&self) -> &OrganizationHoldingRoute {
        &self.holding_route
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganizationAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}

#[async_trait]
pub trait OrganizationRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: OrganizationAuthenticationRequest,
    ) -> Result<AuthenticatedOrganizationRequest, OrganizationAuthenticationError>;
}

#[async_trait]
pub trait OrganizationCommandApplication: Send + Sync + 'static {
    async fn create(
        &self,
        route: OrganizationHoldingRoute,
        command: CommandEnvelope<CreateOrganizationNode>,
    ) -> Result<CommandReceipt<CreateOrganizationNodeResult>, CqrsError>;
    async fn rename(
        &self,
        route: OrganizationHoldingRoute,
        command: CommandEnvelope<RenameOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError>;
    async fn change_code(
        &self,
        route: OrganizationHoldingRoute,
        command: CommandEnvelope<ChangeOrganizationCode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError>;
    async fn reparent(
        &self,
        route: OrganizationHoldingRoute,
        command: CommandEnvelope<ReparentOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError>;
    async fn suspend(
        &self,
        route: OrganizationHoldingRoute,
        command: CommandEnvelope<SuspendOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError>;
    async fn activate(
        &self,
        route: OrganizationHoldingRoute,
        command: CommandEnvelope<ActivateOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError>;
    async fn archive(
        &self,
        route: OrganizationHoldingRoute,
        command: CommandEnvelope<ArchiveOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError>;
}

#[async_trait]
pub trait OrganizationQueryApplication: Send + Sync + 'static {
    async fn get(
        &self,
        route: OrganizationHoldingRoute,
        query: QueryEnvelope<GetOrganizationNode>,
    ) -> Result<QueryResult<GetOrganizationNodeResult>, CqrsError>;
    async fn list(
        &self,
        route: OrganizationHoldingRoute,
        query: QueryEnvelope<ListOrganizationNodes>,
    ) -> Result<QueryResult<NodeList>, CqrsError>;
}

#[derive(Debug, Clone)]
pub struct OrganizationCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> OrganizationCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OrganizationQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> OrganizationQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[async_trait]
impl<A, G> OrganizationCommandService for OrganizationCommandRpc<A, G>
where
    A: OrganizationCommandApplication,
    G: OrganizationRequestAuthenticator,
{
    async fn create_node(
        &self,
        request: Request<platform::CreateOrganizationNodeCommandRequest>,
    ) -> Result<Response<platform::OrganizationNodeCommandResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let node_id = decode_node_id(&input.node_id)?;
        let placement = decode_placement(input.placement, node_id)?;
        let decoded = decode_command(
            input.metadata,
            CreateOrganizationNode {
                node_id,
                placement,
                code: input.code,
                name: input.display_name,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .create(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(command_response(
            &receipt,
            &receipt.response.node,
        )))
    }

    async fn rename_node(
        &self,
        request: Request<platform::RenameOrganizationNodeRequest>,
    ) -> Result<Response<platform::OrganizationNodeCommandResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            RenameOrganizationNode {
                node_id: decode_node_id(&input.node_id)?,
                name: input.display_name,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .rename(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(command_response(
            &receipt,
            &receipt.response.node,
        )))
    }

    async fn change_node_code(
        &self,
        request: Request<platform::ChangeOrganizationNodeCodeRequest>,
    ) -> Result<Response<platform::OrganizationNodeCommandResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            ChangeOrganizationCode {
                node_id: decode_node_id(&input.node_id)?,
                code: input.code,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .change_code(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(command_response(
            &receipt,
            &receipt.response.node,
        )))
    }

    async fn reparent_node(
        &self,
        request: Request<platform::ReparentOrganizationNodeRequest>,
    ) -> Result<Response<platform::OrganizationNodeCommandResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let node_id = decode_node_id(&input.node_id)?;
        let decoded = decode_command(
            input.metadata,
            ReparentOrganizationNode {
                node_id,
                placement: decode_placement(input.placement, node_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .reparent(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(command_response(
            &receipt,
            &receipt.response.node,
        )))
    }

    async fn suspend_node(
        &self,
        request: Request<platform::OrganizationNodeLifecycleRequest>,
    ) -> Result<Response<platform::OrganizationNodeCommandResponse>, Status> {
        self.lifecycle(request, Lifecycle::Suspend).await
    }

    async fn activate_node(
        &self,
        request: Request<platform::OrganizationNodeLifecycleRequest>,
    ) -> Result<Response<platform::OrganizationNodeCommandResponse>, Status> {
        self.lifecycle(request, Lifecycle::Activate).await
    }

    async fn archive_node(
        &self,
        request: Request<platform::OrganizationNodeLifecycleRequest>,
    ) -> Result<Response<platform::OrganizationNodeCommandResponse>, Status> {
        self.lifecycle(request, Lifecycle::Archive).await
    }
}

enum Lifecycle {
    Suspend,
    Activate,
    Archive,
}

impl<A, G> OrganizationCommandRpc<A, G>
where
    A: OrganizationCommandApplication,
    G: OrganizationRequestAuthenticator,
{
    async fn lifecycle(
        &self,
        request: Request<platform::OrganizationNodeLifecycleRequest>,
        action: Lifecycle,
    ) -> Result<Response<platform::OrganizationNodeCommandResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let node_id = decode_node_id(&input.node_id)?;
        let (route, metadata) = (decode_command(input.metadata, ())?, transport);
        let (holding_route, envelope) =
            authenticate_command(self.authenticator.as_ref(), metadata, route).await?;
        let receipt = match action {
            Lifecycle::Suspend => {
                self.application
                    .suspend(
                        holding_route,
                        envelope.map(|_| SuspendOrganizationNode { node_id }),
                    )
                    .await
            }
            Lifecycle::Activate => {
                self.application
                    .activate(
                        holding_route,
                        envelope.map(|_| ActivateOrganizationNode { node_id }),
                    )
                    .await
            }
            Lifecycle::Archive => {
                self.application
                    .archive(
                        holding_route,
                        envelope.map(|_| ArchiveOrganizationNode { node_id }),
                    )
                    .await
            }
        }
        .map_err(status_from_cqrs)?;
        Ok(Response::new(command_response(
            &receipt,
            &receipt.response.node,
        )))
    }
}

trait MapCommandEnvelope<C> {
    fn map<D>(self, map: impl FnOnce(C) -> D) -> CommandEnvelope<D>;
}

impl<C> MapCommandEnvelope<C> for CommandEnvelope<C> {
    fn map<D>(self, map: impl FnOnce(C) -> D) -> CommandEnvelope<D> {
        CommandEnvelope {
            contract_version: self.contract_version,
            command_id: self.command_id,
            idempotency_key: self.idempotency_key,
            correlation_id: self.correlation_id,
            causation_id: self.causation_id,
            actor: self.actor,
            active_scope: self.active_scope,
            audit: self.audit,
            expected_aggregate_version: self.expected_aggregate_version,
            command: map(self.command),
        }
    }
}

#[async_trait]
impl<A, G> OrganizationQueryService for OrganizationQueryRpc<A, G>
where
    A: OrganizationQueryApplication,
    G: OrganizationRequestAuthenticator,
{
    async fn get_node(
        &self,
        request: Request<platform::GetOrganizationNodeRequest>,
    ) -> Result<Response<platform::GetOrganizationNodeResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            GetOrganizationNode {
                node_id: decode_node_id(&input.node_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        let result = self
            .application
            .get(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::GetOrganizationNodeResponse {
            node: result.value.node.as_ref().map(encode_node),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn list_nodes(
        &self,
        request: Request<platform::ListOrganizationNodeRecordsRequest>,
    ) -> Result<Response<platform::ListOrganizationNodeRecordsResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            ListOrganizationNodes {
                filter: NodeListFilter {
                    kind: input.kind.map(decode_kind).transpose()?,
                    status: input.status.map(decode_status).transpose()?,
                    parent_id: input
                        .parent_node_id
                        .as_deref()
                        .map(decode_node_id)
                        .transpose()?,
                    page_size: u16::try_from(input.page_size)
                        .map_err(|_| invalid("page_size is invalid"))?,
                    cursor: input.cursor,
                },
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        let result = self
            .application
            .list(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::ListOrganizationNodeRecordsResponse {
                nodes: result.value.nodes.iter().map(encode_node).collect(),
                next_cursor: result.value.next_cursor,
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }
}

struct DecodedCommand<C> {
    presented_route: Option<OrganizationHoldingRoute>,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}

struct DecodedQuery<Q> {
    presented_route: Option<OrganizationHoldingRoute>,
    envelope: QueryEnvelope<Q>,
}

async fn authenticate_command<C, G: OrganizationRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(OrganizationHoldingRoute, CommandEnvelope<C>), Status> {
    let authenticated = authenticator
        .authenticate(OrganizationAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.active_scope.clone(),
            presented_route: decoded.presented_route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: OrganizationAccessMode::Write,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.active_scope = authenticated.active_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Organization authority is invalid"))?;
    Ok((authenticated.holding_route().clone(), envelope))
}

async fn authenticate_query<Q, G: OrganizationRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(OrganizationHoldingRoute, QueryEnvelope<Q>), Status> {
    let authenticated = authenticator
        .authenticate(OrganizationAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.active_scope.clone(),
            presented_route: decoded.presented_route,
            subscription_id: None,
            mfa_code: None,
            access_mode: OrganizationAccessMode::Read,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.active_scope = authenticated.active_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Organization authority is invalid"))?;
    Ok((authenticated.holding_route().clone(), envelope))
}

fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(metadata.holding_route)?;
    let correlation_id = parse_correlation_id(&metadata.correlation_id)?;
    let subscription_id = normalize_security_input(metadata.subscription_id)?;
    let mfa_code = normalize_security_input(metadata.mfa_code)?;
    let envelope = CommandEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        command_id: CommandId::try_from_uuid(parse_uuid(&metadata.command_id, "command_id")?)
            .map_err(|_| invalid("command_id is invalid"))?,
        idempotency_key: IdempotencyKey::try_new(metadata.idempotency_key)
            .map_err(|_| invalid("idempotency_key is invalid"))?,
        correlation_id,
        causation_id: metadata
            .causation_id
            .map(|value| parse_event_id(&value))
            .transpose()?,
        actor: decode_actor(metadata.actor)?,
        active_scope: decode_scope(
            metadata
                .active_scope
                .ok_or_else(|| invalid("active_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit, correlation_id)?,
        expected_aggregate_version: metadata.expected_aggregate_version,
        command,
    };
    envelope
        .validate()
        .map_err(|_| invalid("command metadata is invalid"))?;
    Ok(DecodedCommand {
        presented_route: route,
        envelope,
        subscription_id,
        mfa_code,
    })
}

fn decode_query<Q>(
    metadata: Option<common::QueryRequestMetadata>,
    query: Q,
) -> Result<DecodedQuery<Q>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("query metadata is required"))?;
    let route = decode_route(metadata.holding_route)?;
    let correlation_id = parse_correlation_id(&metadata.correlation_id)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&metadata.query_id, "query_id")?)
            .map_err(|_| invalid("query_id is invalid"))?,
        correlation_id,
        actor: decode_actor(metadata.actor)?,
        active_scope: decode_scope(
            metadata
                .active_scope
                .ok_or_else(|| invalid("active_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit, correlation_id)?,
        consistency: decode_consistency(metadata.consistency)?,
        query,
    };
    envelope
        .validate()
        .map_err(|_| invalid("query metadata is invalid"))?;
    Ok(DecodedQuery {
        presented_route: route,
        envelope,
    })
}

fn decode_placement(
    value: Option<platform::OrganizationPlacement>,
    node_id: OrganizationNodeId,
) -> Result<OrganizationPlacement, Status> {
    use platform::organization_placement::Placement;
    match value
        .and_then(|value| value.placement)
        .ok_or_else(|| invalid("placement is required"))?
    {
        Placement::Holding(_) => Ok(OrganizationPlacement::Holding),
        Placement::Company(value) => Ok(OrganizationPlacement::Company {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
        }),
        Placement::Headquarters(value) => Ok(OrganizationPlacement::Headquarters {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
        }),
        Placement::Branch(value) => Ok(OrganizationPlacement::Branch {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
        }),
        Placement::Department(value) => Ok(OrganizationPlacement::Department {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
            branch_id: value
                .branch_id
                .as_deref()
                .map(|id| parse_uuid(id, "branch_id"))
                .transpose()?,
        }),
        Placement::Team(value) => {
            if !value.team_id.trim().is_empty()
                && parse_uuid(&value.team_id, "team_id")? != node_id.as_uuid()
            {
                return Err(invalid("team_id does not match node_id"));
            }
            Ok(OrganizationPlacement::Team {
                holding_id: parse_uuid(&value.holding_id, "holding_id")?,
                company_id: parse_uuid(&value.company_id, "company_id")?,
                branch_id: value
                    .branch_id
                    .as_deref()
                    .map(|id| parse_uuid(id, "branch_id"))
                    .transpose()?,
                department_id: parse_uuid(&value.department_id, "department_id")?,
            })
        }
    }
}

fn encode_placement(
    value: &OrganizationPlacement,
    node_id: OrganizationNodeId,
) -> platform::OrganizationPlacement {
    use platform::organization_placement::Placement;
    let placement = match value {
        OrganizationPlacement::Holding => {
            Placement::Holding(platform::OrganizationHoldingPlacement {})
        }
        OrganizationPlacement::Company { holding_id } => {
            Placement::Company(platform::OrganizationCompanyPlacement {
                holding_id: holding_id.to_string(),
            })
        }
        OrganizationPlacement::Headquarters {
            holding_id,
            company_id,
        } => Placement::Headquarters(platform::OrganizationHeadquartersPlacement {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
        }),
        OrganizationPlacement::Branch {
            holding_id,
            company_id,
        } => Placement::Branch(platform::OrganizationBranchPlacement {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
        }),
        OrganizationPlacement::Department {
            holding_id,
            company_id,
            branch_id,
        } => Placement::Department(platform::OrganizationDepartmentPlacement {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
            branch_id: branch_id.map(|id| id.to_string()),
        }),
        OrganizationPlacement::Team {
            holding_id,
            company_id,
            branch_id,
            department_id,
        } => Placement::Team(platform::OrganizationTeamPlacement {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
            branch_id: branch_id.map(|id| id.to_string()),
            department_id: department_id.to_string(),
            team_id: node_id.as_uuid().to_string(),
        }),
    };
    platform::OrganizationPlacement {
        placement: Some(placement),
    }
}

fn command_response<R>(
    receipt: &CommandReceipt<R>,
    node: &NodeView,
) -> platform::OrganizationNodeCommandResponse {
    platform::OrganizationNodeCommandResponse {
        receipt: Some(encode_receipt(receipt)),
        node: Some(encode_node(node)),
    }
}

fn encode_node(view: &NodeView) -> platform::OrganizationNodeRecord {
    platform::OrganizationNodeRecord {
        node_id: view.node_id.as_uuid().to_string(),
        source_version: view.version,
        kind: encode_kind(view.kind),
        status: encode_status(view.status),
        placement: Some(encode_placement(&view.placement, view.node_id)),
        parent_node_id: view.parent_id.map(|id| id.as_uuid().to_string()),
        scope: Some(encode_scope(&view.scope)),
        code: view.code.clone(),
        display_name: view.name.clone(),
        created_at: view.created_at.to_rfc3339(),
        updated_at: view.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}

fn decode_kind(value: i32) -> Result<OrganizationNodeKind, Status> {
    match platform::OrganizationNodeKind::try_from(value).ok() {
        Some(platform::OrganizationNodeKind::Holding) => Ok(OrganizationNodeKind::Holding),
        Some(platform::OrganizationNodeKind::Company) => Ok(OrganizationNodeKind::Company),
        Some(platform::OrganizationNodeKind::Headquarters) => {
            Ok(OrganizationNodeKind::Headquarters)
        }
        Some(platform::OrganizationNodeKind::Branch) => Ok(OrganizationNodeKind::Branch),
        Some(platform::OrganizationNodeKind::Department) => Ok(OrganizationNodeKind::Department),
        Some(platform::OrganizationNodeKind::Team) => Ok(OrganizationNodeKind::Team),
        _ => Err(invalid("organization node kind is invalid")),
    }
}

fn decode_status(value: i32) -> Result<OrganizationNodeStatus, Status> {
    match platform::OrganizationNodeStatus::try_from(value).ok() {
        Some(platform::OrganizationNodeStatus::Active) => Ok(OrganizationNodeStatus::Active),
        Some(platform::OrganizationNodeStatus::Suspended) => Ok(OrganizationNodeStatus::Suspended),
        Some(platform::OrganizationNodeStatus::Archived) => Ok(OrganizationNodeStatus::Archived),
        _ => Err(invalid("organization node status is invalid")),
    }
}

const fn encode_kind(value: OrganizationNodeKind) -> i32 {
    match value {
        OrganizationNodeKind::Holding => platform::OrganizationNodeKind::Holding as i32,
        OrganizationNodeKind::Company => platform::OrganizationNodeKind::Company as i32,
        OrganizationNodeKind::Headquarters => platform::OrganizationNodeKind::Headquarters as i32,
        OrganizationNodeKind::Branch => platform::OrganizationNodeKind::Branch as i32,
        OrganizationNodeKind::Department => platform::OrganizationNodeKind::Department as i32,
        OrganizationNodeKind::Team => platform::OrganizationNodeKind::Team as i32,
    }
}
const fn encode_status(value: OrganizationNodeStatus) -> i32 {
    match value {
        OrganizationNodeStatus::Active => platform::OrganizationNodeStatus::Active as i32,
        OrganizationNodeStatus::Suspended => platform::OrganizationNodeStatus::Suspended as i32,
        OrganizationNodeStatus::Archived => platform::OrganizationNodeStatus::Archived as i32,
    }
}

fn decode_route(
    route: Option<common::HoldingRoute>,
) -> Result<Option<OrganizationHoldingRoute>, Status> {
    let Some(route) = route else {
        return Ok(None);
    };
    let route = OrganizationHoldingRoute {
        holding_id: route.holding_id.trim().to_owned(),
        data_profile_key: route.data_profile_key.trim().to_owned(),
    };
    validate_route(&route)?;
    Ok(Some(route))
}
fn validate_route(route: &OrganizationHoldingRoute) -> Result<(), Status> {
    if [&route.holding_id, &route.data_profile_key]
        .iter()
        .any(|value| value.trim().is_empty() || value.len() > MAX_ROUTE_LENGTH)
    {
        Err(invalid("holding route is invalid"))
    } else {
        Ok(())
    }
}
fn decode_actor(actor: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let actor = actor.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(actor.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::HoldingUser) => ActorKind::HoldingUser,
        Some(common::ActorKind::Service) => ActorKind::Service,
        Some(common::ActorKind::SystemWorker) => ActorKind::SystemWorker,
        _ => return Err(invalid("actor kind is invalid")),
    };
    Ok(ActorAttribution {
        principal_ref: actor.principal_ref,
        kind,
        session_id: parse_optional_uuid(&actor.session_id, "session_id")?
            .map(SessionId::try_from_uuid)
            .transpose()
            .map_err(|_| invalid("session_id is invalid"))?,
        represented_principal_ref: optional_nonempty(actor.represented_principal_ref),
        support_capability_id: parse_optional_uuid(
            &actor.support_capability_id,
            "support_capability_id",
        )?,
    })
}
fn decode_audit(
    audit: Option<common::AuditContext>,
    correlation_id: CorrelationId,
) -> Result<AuditContext, Status> {
    let audit = audit.ok_or_else(|| invalid("audit context is required"))?;
    if !audit.correlation_id.trim().is_empty()
        && parse_correlation_id(&audit.correlation_id)? != correlation_id
    {
        return Err(invalid("audit correlation mismatch"));
    }
    Ok(AuditContext {
        purpose: audit.purpose,
        customer_visible: audit.customer_visible,
        module: optional_nonempty(audit.module),
    })
}
fn decode_scope(scope: common::AuthorityScope) -> Result<AuthorityScope, Status> {
    use common::authority_scope::Scope;
    match scope
        .scope
        .ok_or_else(|| invalid("holding scope is empty"))?
    {
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
fn encode_scope(scope: &AuthorityScope) -> common::AuthorityScope {
    use common::authority_scope::Scope;
    common::AuthorityScope {
        scope: Some(match scope {
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

fn decode_consistency(value: Option<common::QueryConsistency>) -> Result<QueryConsistency, Status> {
    use common::query_consistency::Requirement;
    match value.and_then(|v| v.requirement) {
        None | Some(Requirement::Eventual(_)) => Ok(QueryConsistency::Eventual),
        Some(Requirement::ReadYourWrites(v)) => {
            Ok(QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: v.projection,
                partition: v.partition,
                target: erpx_cqrs::ProjectionTarget {
                    event_id: parse_event_id(&v.event_id)?,
                    global_sequence: v.global_sequence,
                },
                timeout_millis: v.timeout_millis,
            }))
        }
        Some(Requirement::AggregateVersion(_)) => {
            Err(invalid("aggregate consistency is unsupported"))
        }
    }
}
fn encode_receipt<R>(receipt: &CommandReceipt<R>) -> common::CommandReceipt {
    common::CommandReceipt {
        command_id: receipt.command_id.to_string(),
        correlation_id: receipt.correlation_id.to_string(),
        aggregate: Some(common::AggregateVersion {
            context: receipt.aggregate.context.clone(),
            aggregate_type: receipt.aggregate.aggregate_type.clone(),
            aggregate_id: receipt.aggregate.aggregate_id.to_string(),
            version: receipt.aggregate.version,
        }),
        committed_events: receipt
            .committed_events
            .iter()
            .map(|v| common::EventPosition {
                event_id: v.event_id.to_string(),
                global_sequence: v.global_sequence,
            })
            .collect(),
        committed_at: receipt.committed_at.to_rfc3339(),
        replayed: receipt.replayed,
    }
}
fn encode_query_metadata(value: &QueryMetadata) -> common::QueryMetadata {
    common::QueryMetadata {
        correlation_id: value.correlation_id.to_string(),
        projection: Some(encode_projection_metadata(&value.projection)),
        consistency: Some(encode_consistency(&value.consistency)),
    }
}
fn encode_projection_metadata(value: &ProjectionMetadata) -> common::ProjectionMetadata {
    common::ProjectionMetadata {
        checkpoint: value.checkpoint.as_ref().map(encode_checkpoint),
        freshness: match value.freshness {
            FreshnessState::Current => common::ProjectionFreshnessState::Current as i32,
            FreshnessState::Stale => common::ProjectionFreshnessState::Stale as i32,
            FreshnessState::Rebuilding => common::ProjectionFreshnessState::Rebuilding as i32,
            FreshnessState::Unavailable => common::ProjectionFreshnessState::Unavailable as i32,
        },
        lag_millis: value.lag_millis,
    }
}
fn encode_checkpoint(value: &ProjectionCheckpoint) -> common::ProjectionCheckpoint {
    common::ProjectionCheckpoint {
        projection: value.projection.clone(),
        partition: value.partition.clone(),
        event_id: value.event_id.to_string(),
        global_sequence: value.global_sequence,
        updated_at: value.updated_at.to_rfc3339(),
    }
}
fn encode_consistency(value: &QueryConsistency) -> common::QueryConsistency {
    use common::query_consistency::Requirement;
    common::QueryConsistency {
        requirement: Some(match value {
            QueryConsistency::Eventual => Requirement::Eventual(common::EventualConsistency {}),
            QueryConsistency::ReadYourWrites(v) =>
            {
                #[allow(deprecated)]
                Requirement::ReadYourWrites(common::ReadYourWrites {
                    command_id: String::new(),
                    projection: v.projection.clone(),
                    partition: v.partition.clone(),
                    event_id: v.target.event_id.to_string(),
                    global_sequence: v.target.global_sequence,
                    timeout_millis: v.timeout_millis,
                })
            }
        }),
    }
}

fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    let value = Uuid::parse_str(value.trim()).map_err(|_| invalid(field))?;
    if value.is_nil() {
        Err(invalid(field))
    } else {
        Ok(value)
    }
}
fn parse_optional_uuid(value: &str, field: &'static str) -> Result<Option<Uuid>, Status> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        parse_uuid(value, field).map(Some)
    }
}
fn parse_event_id(value: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(value, "event_id")?)
        .map_err(|_| invalid("event_id is invalid"))
}
fn parse_correlation_id(value: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(value, "correlation_id")?)
        .map_err(|_| invalid("correlation_id is invalid"))
}
fn decode_node_id(value: &str) -> Result<OrganizationNodeId, Status> {
    OrganizationNodeId::try_from_uuid(parse_uuid(value, "node_id")?)
        .map_err(|_| invalid("node_id is invalid"))
}
fn optional_nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}
fn normalize_security_input(value: Option<String>) -> Result<Option<String>, Status> {
    let value = value.filter(|v| !v.trim().is_empty());
    if value
        .as_ref()
        .is_some_and(|v| v.len() > MAX_SECURITY_INPUT_LENGTH)
    {
        Err(invalid("security metadata is invalid"))
    } else {
        Ok(value)
    }
}
fn invalid(message: &'static str) -> Status {
    Status::invalid_argument(message)
}
fn status_from_cqrs(error: CqrsError) -> Status {
    match error.category() {
        ErrorCategory::Invalid => Status::invalid_argument("Organization request is invalid"),
        ErrorCategory::Denied => Status::permission_denied("Organization request is denied"),
        ErrorCategory::NotFound => Status::not_found("Organization node was not found"),
        ErrorCategory::Conflict => Status::aborted("Organization node has changed"),
        ErrorCategory::NotReady => Status::unavailable("Organization projection is not ready"),
        ErrorCategory::Unavailable => {
            Status::unavailable("Organization is temporarily unavailable")
        }
        ErrorCategory::Internal => Status::internal("Organization operation failed"),
    }
}
fn status_from_authentication(error: OrganizationAuthenticationError) -> Status {
    match error {
        OrganizationAuthenticationError::Unauthenticated => {
            Status::unauthenticated("Organization authentication is required")
        }
        OrganizationAuthenticationError::Denied => {
            Status::permission_denied("Organization authority is denied")
        }
        OrganizationAuthenticationError::Unavailable => {
            Status::unavailable("Organization authentication is unavailable")
        }
        OrganizationAuthenticationError::Internal => {
            Status::internal("Organization authentication failed")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_placement_round_trips_without_json() {
        let node_id = OrganizationNodeId::try_from_uuid(Uuid::from_u128(5)).unwrap();
        let placement = OrganizationPlacement::Team {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: Some(Uuid::from_u128(3)),
            department_id: Uuid::from_u128(4),
        };
        assert_eq!(
            decode_placement(Some(encode_placement(&placement, node_id)), node_id).unwrap(),
            placement
        );
    }

    #[test]
    fn presenter_transport_never_accepts_generic_json_mutations() {
        let source = include_str!("organization_cqrs.rs");
        assert!(!source.contains(concat!("payload_", "json")));
        assert!(source.contains("OrganizationCommandService"));
        assert!(source.contains("OrganizationQueryService"));
    }
}
