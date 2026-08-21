//! Authenticated gRPC boundary for the future Agent Orchestration and Projects contracts.
//!
//! This module is intentionally fail-closed. It dispatches only operations for
//! which the owner context currently exposes a complete application/query port
//! and a truthful target response. Missing owner ports return
//! `FAILED_PRECONDITION`; they never return an empty or fabricated success.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use erpx_agent_orchestration::{
    AgentExperienceQueryPort, InvocationEvidenceSummary, PoolRoutingDecisionId, PoolRoutingSummary,
    PortError as AgentPortError, ProviderInvocationId,
};
use erpx_agent_orchestration_sqlx::SqlxAgentExperienceQuery;
use erpx_cqrs::{
    CommandReceipt, FreshnessState, IdempotencyKey, ProjectionMetadata, QueryConsistency,
    ReadYourWrites,
};
use erpx_cqrs_sqlx::{ReadPool, WritePool};
use erpx_infrastructure::RoutedDatabaseSide;
use erpx_integration_contracts::{StableRecordRef, catalog::ContextId, reference::RecordTypeId};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId, EventId,
    HoldingId, QueryId, RecordId, SessionId, SystemClock,
};
use erpx_projects::{
    AddProjectNodeInput, AttachmentRef, ChangeHandoff, ChangeProjectNode,
    CommandContext as ProjectsApplicationCommandContext, CreateProject, DecideDependency,
    DependencyDecision, DependencyEndpoint, DependencyId, DependencyKind, DependencySide,
    HandoffChange, HandoffId, HandoffKind, NodeKind, ProjectApplicationError, ProjectClass,
    ProjectDomainError, ProjectGraphView, ProjectId, ProjectList, ProjectListFilter,
    ProjectNodeChange, ProjectNodeId, ProjectRecordKind, ProjectRecordView, ProjectStatus,
    ProjectView, ProjectsAggregate, ProposeDependency, ProposeHandoff, ReadProjection,
    TransitionProject,
};
use erpx_projects::{AuthorizationPort as _, ReadRepository as _};
use erpx_projects_sqlx::{ProjectsCommandAdapter, ProjectsReadRepository, ProjectsWriteRepository};
use erpx_proto::{
    common,
    erpx::{agent, projects},
};
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

use agent::{
    agent_orchestration_command_service_server::AgentOrchestrationCommandService,
    agent_orchestration_query_service_server::AgentOrchestrationQueryService,
};
use projects::{
    projects_command_service_server::ProjectsCommandService,
    projects_query_service_server::ProjectsQueryService,
};

use crate::holding_workspace::resolve_holding_workspace;
use crate::wave4_composition::authenticate_context_for_holding;
use crate::{AppState, require_actor_match};

const MAX_SECURITY_INPUT_LENGTH: usize = 512;
const MAX_ROUTE_LENGTH: usize = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FutureOwnerContext {
    AgentOrchestration,
    Projects,
}

impl FutureOwnerContext {
    const fn label(self) -> &'static str {
        match self {
            Self::AgentOrchestration => "Agent Orchestration",
            Self::Projects => "Projects",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FutureAccessMode {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FutureHoldingRoute {
    holding_id: String,
    data_profile_key: String,
}

impl FutureHoldingRoute {
    pub fn try_new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, FutureAuthenticationError> {
        let route = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
        };
        validate_route(&route).map_err(|_| FutureAuthenticationError::Internal)?;
        Ok(route)
    }

    pub fn holding_id(&self) -> &str {
        &self.holding_id
    }

    pub fn data_profile_key(&self) -> &str {
        &self.data_profile_key
    }
}

/// Untrusted session hints and security inputs passed to the runtime auth gate.
/// Deliberately not `Debug`: metadata and MFA input may contain secrets.
pub struct FutureAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: AuthorityScope,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    owner_context: FutureOwnerContext,
    access_mode: FutureAccessMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedFutureRequest {
    actor: ActorAttribution,
    active_scope: AuthorityScope,
    holding_route: FutureHoldingRoute,
}

impl AuthenticatedFutureRequest {
    pub fn try_new(
        actor: ActorAttribution,
        active_scope: AuthorityScope,
        holding_route: FutureHoldingRoute,
    ) -> Result<Self, FutureAuthenticationError> {
        actor
            .validate()
            .map_err(|_| FutureAuthenticationError::Internal)?;
        active_scope
            .validate()
            .map_err(|_| FutureAuthenticationError::Internal)?;
        validate_route_scope(&holding_route, &active_scope)
            .map_err(|_| FutureAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            active_scope,
            holding_route,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FutureAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}

#[async_trait]
pub trait FutureAgentProjectsAuthenticator: Send + Sync + 'static {
    /// Resolves the canonical actor, session authority, and physical holding
    /// route. Presented protobuf scope is only an authentication hint.
    async fn authenticate(
        &self,
        request: FutureAuthenticationRequest,
    ) -> Result<AuthenticatedFutureRequest, FutureAuthenticationError>;
}

#[async_trait]
pub trait ProjectsCommandApplication: Send + Sync + 'static {
    async fn create_project(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: CreateProject,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError>;

    async fn transition_project(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: TransitionProject,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError>;

    async fn add_node(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: AddProjectNodeInput,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError>;

    async fn change_node(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: ChangeProjectNode,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError>;

    async fn propose_dependency(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: ProposeDependency,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError>;

    async fn decide_dependency(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: DecideDependency,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError>;

    async fn propose_handoff(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: ProposeHandoff,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError>;

    async fn change_handoff(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: ChangeHandoff,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError>;
}

#[async_trait]
pub trait ProjectsQueryApplication: Send + Sync + 'static {
    /// Implementations must authorize `ProjectAction::View` before invoking the
    /// Projects read repository with the canonical authority scope.
    async fn get_project(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        consistency: QueryConsistency,
        project_id: ProjectId,
    ) -> Result<ReadProjection<Option<ProjectView>>, erpx_projects::PortError>;

    async fn get_project_graph(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        consistency: QueryConsistency,
        project_id: ProjectId,
    ) -> Result<ReadProjection<Option<ProjectGraphView>>, erpx_projects::PortError>;

    async fn get_project_record(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        consistency: QueryConsistency,
        record_id: Uuid,
        record_kind: ProjectRecordKind,
    ) -> Result<ReadProjection<Option<ProjectRecordView>>, erpx_projects::PortError>;

    /// Implementations must authorize `ProjectAction::List` before lookup.
    async fn list_projects(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        consistency: QueryConsistency,
        filter: ProjectListFilter,
    ) -> Result<ReadProjection<ProjectList>, erpx_projects::PortError>;
}

#[async_trait]
pub trait AgentQueryApplication: Send + Sync + 'static {
    /// Wraps `AgentExperienceQueryPort::routing_summary` with owner-context
    /// authorization using the canonical authenticated actor and scope.
    async fn routing_summary(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        id: PoolRoutingDecisionId,
    ) -> Result<Option<PoolRoutingSummary>, AgentPortError>;

    /// Wraps `AgentExperienceQueryPort::invocation_summary` with owner-context
    /// authorization using the canonical authenticated actor and scope.
    async fn invocation_summary(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        id: ProviderInvocationId,
    ) -> Result<Option<InvocationEvidenceSummary>, AgentPortError>;
}

#[derive(Debug, Clone)]
pub struct AgentCommandRpc<G> {
    authenticator: Arc<G>,
}

impl<G> AgentCommandRpc<G> {
    pub const fn new(authenticator: Arc<G>) -> Self {
        Self { authenticator }
    }
}

#[derive(Debug, Clone)]
pub struct AgentQueryRpc<Q, G> {
    application: Arc<Q>,
    authenticator: Arc<G>,
}

impl<Q, G> AgentQueryRpc<Q, G> {
    pub const fn new(application: Arc<Q>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectsCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> ProjectsCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

impl<A, G> ProjectsCommandRpc<A, G>
where
    A: ProjectsCommandApplication,
    G: FutureAgentProjectsAuthenticator,
{
    async fn change_handoff_status(
        &self,
        request: Request<projects::ProjectHandoffActionRequest>,
        change: HandoffChange,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        let receipt = self
            .application
            .change_handoff(
                authenticated.holding_route,
                authenticated.projects_context,
                ChangeHandoff {
                    handoff_id: handoff_id(&input.handoff_id)?,
                    change,
                },
            )
            .await
            .map_err(status_from_projects_application)?;
        Ok(Response::new(encode_projects_mutation(receipt)))
    }
}

#[derive(Debug, Clone)]
pub struct ProjectsQueryRpc<Q, G> {
    application: Arc<Q>,
    authenticator: Arc<G>,
}

impl<Q, G> ProjectsQueryRpc<Q, G> {
    pub const fn new(application: Arc<Q>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

const AGENT_WRITE_PERMISSION: &str = "agent-orchestration.write";
const AGENT_READ_PERMISSION: &str = "agent-orchestration.read";
const PROJECTS_WRITE_PERMISSION: &str = "projects.write";
const PROJECTS_READ_PERMISSION: &str = "projects.read";

#[derive(Clone)]
pub(crate) struct AppStateFutureAuthenticator {
    state: AppState,
}

impl AppStateFutureAuthenticator {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl FutureAgentProjectsAuthenticator for AppStateFutureAuthenticator {
    async fn authenticate(
        &self,
        request: FutureAuthenticationRequest,
    ) -> Result<AuthenticatedFutureRequest, FutureAuthenticationError> {
        let mut transport = Request::new(());
        *transport.metadata_mut() = request.transport_metadata;
        // Token/session authentication is deliberately the first protected
        // operation. Holding routing and owner-context authorization follow it.
        let claims = self
            .state
            .auth
            .authorize(&transport)
            .await
            .map_err(map_future_auth_status)?;
        require_actor_match(
            &claims.principal_ref,
            &request.presented_actor.principal_ref,
            "future-context actor must match the authenticated principal",
        )
        .map_err(map_future_auth_status)?;
        let workspace = resolve_holding_workspace(
            &self.state,
            &request.presented_scope,
            None,
            None,
            request.access_mode == FutureAccessMode::Write,
        )
        .await
        .map_err(map_future_auth_status)?;
        let session_id = SessionId::try_from_uuid(claims.session_id)
            .map_err(|_| FutureAuthenticationError::Internal)?;
        let kind = crate::auth_actor::kernel_actor_kind(&claims);
        let presented_actor = ActorAttribution {
            principal_ref: claims.principal_ref,
            kind,
            session_id: Some(session_id),
            represented_principal_ref: None,
            support_capability_id: None,
        };
        let (product, permission) = match (request.owner_context, request.access_mode) {
            (FutureOwnerContext::AgentOrchestration, FutureAccessMode::Write) => {
                ("agent-orchestration", AGENT_WRITE_PERMISSION)
            }
            (FutureOwnerContext::AgentOrchestration, FutureAccessMode::Read) => {
                ("agent-orchestration", AGENT_READ_PERMISSION)
            }
            (FutureOwnerContext::Projects, FutureAccessMode::Write) => {
                ("projects", PROJECTS_WRITE_PERMISSION)
            }
            (FutureOwnerContext::Projects, FutureAccessMode::Read) => {
                ("projects", PROJECTS_READ_PERMISSION)
            }
        };
        let subscription = workspace.subscription_id.map(|value| value.to_string());
        if request.subscription_id.as_deref().is_some()
            && request.subscription_id.as_deref() != subscription.as_deref()
        {
            return Err(FutureAuthenticationError::Denied);
        }
        let actor = authenticate_context_for_holding(
            &self.state,
            transport.metadata(),
            &presented_actor,
            &request.presented_scope,
            &workspace.customer_id.to_string(),
            &workspace.holding_id,
            permission,
            product,
            request.access_mode == FutureAccessMode::Write,
            subscription.as_deref(),
            request.mfa_code.as_deref(),
        )
        .await
        .map_err(map_future_auth_status)?;
        let route = FutureHoldingRoute::try_new(workspace.holding_id, workspace.data_profile_key)?;
        AuthenticatedFutureRequest::try_new(actor, request.presented_scope, route)
    }
}

fn map_future_auth_status(status: Status) -> FutureAuthenticationError {
    match status.code() {
        tonic::Code::Unauthenticated => FutureAuthenticationError::Unauthenticated,
        tonic::Code::PermissionDenied | tonic::Code::InvalidArgument => {
            FutureAuthenticationError::Denied
        }
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => {
            FutureAuthenticationError::Unavailable
        }
        _ => FutureAuthenticationError::Internal,
    }
}

#[derive(Debug, Clone)]
struct ProjectsBoundaryAccess {
    actor: ActorAttribution,
    authority_scope: AuthorityScope,
    write: bool,
}

#[async_trait]
impl erpx_projects::AuthorizationPort for ProjectsBoundaryAccess {
    async fn authorize(
        &self,
        request: erpx_projects::AuthorizationRequest<'_>,
    ) -> Result<(), erpx_projects::PortError> {
        let action_writes = !matches!(
            request.action,
            erpx_projects::ProjectAction::View | erpx_projects::ProjectAction::List
        );
        if request.actor != &self.actor
            || request.authority_scope != &self.authority_scope
            || action_writes != self.write
            || !self.authority_scope.contains(request.target_scope)
        {
            return Err(erpx_projects::PortError::Denied);
        }
        Ok(())
    }
}

#[async_trait]
impl erpx_projects::EntitlementPort for ProjectsBoundaryAccess {
    async fn ensure_projects_enabled(
        &self,
        scope: &AuthorityScope,
        write: bool,
    ) -> Result<(), erpx_projects::PortError> {
        if write != self.write || !self.authority_scope.contains(scope) {
            Err(erpx_projects::PortError::EntitlementDenied)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
struct SqlxProjectsDependencyGraph {
    pool: ReadPool,
}

#[async_trait]
impl erpx_projects::DependencyGraphPort for SqlxProjectsDependencyGraph {
    async fn would_create_cycle(
        &self,
        source: &DependencyEndpoint,
        target: &DependencyEndpoint,
    ) -> Result<bool, erpx_projects::PortError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| erpx_projects::PortError::Unavailable)?;
        let cycle = sqlx::query_scalar::<_, bool>(
            "WITH RECURSIVE active_edges AS (
                 SELECT d.source_project_id,d.target_project_id
                 FROM erpx_projects_read.dependency_projection d
                 JOIN erpx_cqrs_read.projection_generations g
                   ON g.holding_id=d.holding_id AND g.projection_name=d.projection_name
                  AND g.generation=d.generation AND g.status='active'
                 WHERE d.dependency_status='accepted'
             ), reachable(project_id) AS (
                 SELECT target_project_id FROM active_edges WHERE source_project_id=$1
                 UNION
                 SELECT e.target_project_id FROM active_edges e
                 JOIN reachable r ON e.source_project_id=r.project_id
             )
             SELECT EXISTS(SELECT 1 FROM reachable WHERE project_id=$2)",
        )
        .bind(target.project_id.as_uuid())
        .bind(source.project_id.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| erpx_projects::PortError::Unavailable)?;
        tx.commit()
            .await
            .map_err(|_| erpx_projects::PortError::Unavailable)?;
        Ok(cycle)
    }
}

type ConcreteProjectsCommandService = erpx_projects::ProjectsCommandService<
    ProjectsWriteRepository,
    ProjectsCommandAdapter,
    ProjectsBoundaryAccess,
    ProjectsBoundaryAccess,
    SqlxProjectsDependencyGraph,
    SystemClock,
>;

#[derive(Clone)]
pub(crate) struct AppStateProjectsApplication {
    state: AppState,
}

impl AppStateProjectsApplication {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }

    async fn command_service(
        &self,
        route: &FutureHoldingRoute,
        context: &ProjectsApplicationCommandContext,
    ) -> Result<ConcreteProjectsCommandService, ProjectApplicationError> {
        let holding_uuid = parse_route_holding(route)?;
        let write_pool = self.route(route, RoutedDatabaseSide::Write).await?;
        let read_pool = self.route(route, RoutedDatabaseSide::Read).await?;
        let write = WritePool::for_holding(write_pool, holding_uuid)
            .map_err(|_| erpx_projects::PortError::Unavailable)?;
        let read = ReadPool::for_holding(read_pool, holding_uuid)
            .map_err(|_| erpx_projects::PortError::Unavailable)?;
        let holding_id = HoldingId::try_from_uuid(holding_uuid)
            .map_err(|_| erpx_projects::PortError::Unavailable)?;
        let access = ProjectsBoundaryAccess {
            actor: context.actor.clone(),
            authority_scope: context.authority_scope.clone(),
            write: true,
        };
        Ok(erpx_projects::ProjectsCommandService::new(
            ProjectsWriteRepository::new(write.clone()),
            ProjectsCommandAdapter::new(write, holding_id),
            access.clone(),
            access,
            SqlxProjectsDependencyGraph { pool: read },
            SystemClock,
        ))
    }

    async fn route(
        &self,
        route: &FutureHoldingRoute,
        side: RoutedDatabaseSide,
    ) -> Result<sqlx::PgPool, ProjectApplicationError> {
        let router = self
            .state
            .holding_router
            .as_ref()
            .ok_or(erpx_projects::PortError::Unavailable)?;
        router
            .route(route.holding_id(), route.data_profile_key(), side)
            .await
            .map(|value| value.database_pool().clone())
            .map_err(|_| erpx_projects::PortError::Unavailable.into())
    }
}

fn parse_route_holding(route: &FutureHoldingRoute) -> Result<Uuid, ProjectApplicationError> {
    Uuid::parse_str(route.holding_id()).map_err(|_| erpx_projects::PortError::Unavailable.into())
}

#[async_trait]
impl ProjectsCommandApplication for AppStateProjectsApplication {
    async fn create_project(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: CreateProject,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        self.command_service(&route, &context)
            .await?
            .create_project(context, command)
            .await
    }

    async fn transition_project(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: TransitionProject,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        self.command_service(&route, &context)
            .await?
            .transition_project(context, command)
            .await
    }

    async fn add_node(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: AddProjectNodeInput,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        self.command_service(&route, &context)
            .await?
            .add_node_input(context, command)
            .await
    }

    async fn change_node(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: ChangeProjectNode,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        self.command_service(&route, &context)
            .await?
            .change_node(context, command)
            .await
    }

    async fn propose_dependency(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: ProposeDependency,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        self.command_service(&route, &context)
            .await?
            .propose_dependency(context, command)
            .await
    }

    async fn decide_dependency(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: DecideDependency,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        self.command_service(&route, &context)
            .await?
            .decide_dependency(context, command)
            .await
    }

    async fn propose_handoff(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: ProposeHandoff,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        self.command_service(&route, &context)
            .await?
            .propose_handoff(context, command)
            .await
    }

    async fn change_handoff(
        &self,
        route: FutureHoldingRoute,
        context: ProjectsApplicationCommandContext,
        command: ChangeHandoff,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        self.command_service(&route, &context)
            .await?
            .change_handoff(context, command)
            .await
    }
}

#[async_trait]
impl ProjectsQueryApplication for AppStateProjectsApplication {
    async fn get_project(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        consistency: QueryConsistency,
        project_id: ProjectId,
    ) -> Result<ReadProjection<Option<ProjectView>>, erpx_projects::PortError> {
        let access = ProjectsBoundaryAccess {
            actor: actor.clone(),
            authority_scope: authority_scope.clone(),
            write: false,
        };
        access
            .authorize(erpx_projects::AuthorizationRequest {
                actor: &actor,
                authority_scope: &authority_scope,
                target_scope: &authority_scope,
                action: erpx_projects::ProjectAction::View,
            })
            .await?;
        self.read_repository(&route)
            .await?
            .get_project(&authority_scope, project_id, &consistency)
            .await
    }

    async fn get_project_graph(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        consistency: QueryConsistency,
        project_id: ProjectId,
    ) -> Result<ReadProjection<Option<ProjectGraphView>>, erpx_projects::PortError> {
        let access = ProjectsBoundaryAccess {
            actor: actor.clone(),
            authority_scope: authority_scope.clone(),
            write: false,
        };
        access
            .authorize(erpx_projects::AuthorizationRequest {
                actor: &actor,
                authority_scope: &authority_scope,
                target_scope: &authority_scope,
                action: erpx_projects::ProjectAction::View,
            })
            .await?;
        self.read_repository(&route)
            .await?
            .get_graph(&authority_scope, project_id, &consistency)
            .await
    }

    async fn get_project_record(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        consistency: QueryConsistency,
        record_id: Uuid,
        record_kind: ProjectRecordKind,
    ) -> Result<ReadProjection<Option<ProjectRecordView>>, erpx_projects::PortError> {
        let access = ProjectsBoundaryAccess {
            actor: actor.clone(),
            authority_scope: authority_scope.clone(),
            write: false,
        };
        access
            .authorize(erpx_projects::AuthorizationRequest {
                actor: &actor,
                authority_scope: &authority_scope,
                target_scope: &authority_scope,
                action: erpx_projects::ProjectAction::View,
            })
            .await?;
        self.read_repository(&route)
            .await?
            .get_record(&authority_scope, record_id, record_kind, &consistency)
            .await
    }

    async fn list_projects(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        consistency: QueryConsistency,
        filter: ProjectListFilter,
    ) -> Result<ReadProjection<ProjectList>, erpx_projects::PortError> {
        let access = ProjectsBoundaryAccess {
            actor: actor.clone(),
            authority_scope: authority_scope.clone(),
            write: false,
        };
        access
            .authorize(erpx_projects::AuthorizationRequest {
                actor: &actor,
                authority_scope: &authority_scope,
                target_scope: &authority_scope,
                action: erpx_projects::ProjectAction::List,
            })
            .await?;
        self.read_repository(&route)
            .await?
            .list_projects(&authority_scope, &filter, &consistency)
            .await
    }
}

impl AppStateProjectsApplication {
    async fn read_repository(
        &self,
        route: &FutureHoldingRoute,
    ) -> Result<ProjectsReadRepository, erpx_projects::PortError> {
        let holding_id = Uuid::parse_str(route.holding_id())
            .map_err(|_| erpx_projects::PortError::Unavailable)?;
        let pool = self
            .state
            .holding_router
            .as_ref()
            .ok_or(erpx_projects::PortError::Unavailable)?
            .route(
                route.holding_id(),
                route.data_profile_key(),
                RoutedDatabaseSide::Read,
            )
            .await
            .map_err(|_| erpx_projects::PortError::Unavailable)?
            .database_pool()
            .clone();
        let pool = ReadPool::for_holding(pool, holding_id)
            .map_err(|_| erpx_projects::PortError::Unavailable)?;
        ProjectsReadRepository::new(pool).map_err(|_| erpx_projects::PortError::Unavailable)
    }
}

#[derive(Clone)]
pub(crate) struct AppStateAgentQueryApplication {
    state: AppState,
}

impl AppStateAgentQueryApplication {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }

    async fn query(
        &self,
        route: &FutureHoldingRoute,
    ) -> Result<SqlxAgentExperienceQuery, AgentPortError> {
        let holding_id =
            Uuid::parse_str(route.holding_id()).map_err(|_| AgentPortError::Persistence)?;
        let pool = self
            .state
            .holding_router
            .as_ref()
            .ok_or(AgentPortError::Persistence)?
            .route(
                route.holding_id(),
                route.data_profile_key(),
                RoutedDatabaseSide::Read,
            )
            .await
            .map_err(|_| AgentPortError::Persistence)?
            .database_pool()
            .clone();
        SqlxAgentExperienceQuery::try_new(pool, holding_id)
    }
}

#[async_trait]
impl AgentQueryApplication for AppStateAgentQueryApplication {
    async fn routing_summary(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        id: PoolRoutingDecisionId,
    ) -> Result<Option<PoolRoutingSummary>, AgentPortError> {
        actor.validate().map_err(|_| AgentPortError::Denied)?;
        self.query(&route)
            .await?
            .routing_summary(&authority_scope, id)
            .await
    }

    async fn invocation_summary(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        id: ProviderInvocationId,
    ) -> Result<Option<InvocationEvidenceSummary>, AgentPortError> {
        actor.validate().map_err(|_| AgentPortError::Denied)?;
        self.query(&route)
            .await?
            .invocation_summary(&authority_scope, id)
            .await
    }
}

pub(crate) type AgentCommandTonicService =
    agent::agent_orchestration_command_service_server::AgentOrchestrationCommandServiceServer<
        AgentCommandRpc<AppStateFutureAuthenticator>,
    >;
pub(crate) type AgentQueryTonicService =
    agent::agent_orchestration_query_service_server::AgentOrchestrationQueryServiceServer<
        AgentQueryRpc<AppStateAgentQueryApplication, AppStateFutureAuthenticator>,
    >;
pub(crate) type ProjectsCommandTonicService =
    projects::projects_command_service_server::ProjectsCommandServiceServer<
        ProjectsCommandRpc<AppStateProjectsApplication, AppStateFutureAuthenticator>,
    >;
pub(crate) type ProjectsQueryTonicService =
    projects::projects_query_service_server::ProjectsQueryServiceServer<
        ProjectsQueryRpc<AppStateProjectsApplication, AppStateFutureAuthenticator>,
    >;

pub(crate) struct FutureAgentProjectsTonicServices {
    pub agent_command: AgentCommandTonicService,
    pub agent_query: AgentQueryTonicService,
    pub projects_command: ProjectsCommandTonicService,
    pub projects_query: ProjectsQueryTonicService,
}

pub(crate) fn tonic_services(state: AppState) -> FutureAgentProjectsTonicServices {
    let authenticator = Arc::new(AppStateFutureAuthenticator::new(state.clone()));
    let projects = Arc::new(AppStateProjectsApplication::new(state.clone()));
    let agent_queries = Arc::new(AppStateAgentQueryApplication::new(state));
    FutureAgentProjectsTonicServices {
        agent_command: AgentCommandTonicService::new(AgentCommandRpc::new(authenticator.clone())),
        agent_query: AgentQueryTonicService::new(AgentQueryRpc::new(
            agent_queries,
            authenticator.clone(),
        )),
        projects_command: ProjectsCommandTonicService::new(ProjectsCommandRpc::new(
            projects.clone(),
            authenticator.clone(),
        )),
        projects_query: ProjectsQueryTonicService::new(ProjectsQueryRpc::new(
            projects,
            authenticator,
        )),
    }
}

macro_rules! unsupported_agent_command {
    ($method:ident, $request:ty, $gap:literal) => {
        fn $method<'life0, 'async_trait>(
            &'life0 self,
            request: Request<$request>,
        ) -> core::pin::Pin<
            Box<
                dyn core::future::Future<
                        Output = Result<Response<agent::AgentMutationResponse>, Status>,
                    > + Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move {
                let transport = request.metadata().clone();
                let input = request.into_inner();
                let _authenticated = authenticate_command(
                    self.authenticator.as_ref(),
                    transport,
                    input.context,
                    FutureOwnerContext::AgentOrchestration,
                )
                .await?;
                Err(owner_port_gap($gap))
            })
        }
    };
}

#[async_trait]
impl<G> AgentOrchestrationCommandService for AgentCommandRpc<G>
where
    G: FutureAgentProjectsAuthenticator,
{
    unsupported_agent_command!(
        register_agent,
        agent::RegisterAgentRequest,
        "AGN-CMD-REGISTER-RECEIPT"
    );
    unsupported_agent_command!(
        add_agent_version,
        agent::AddAgentVersionRequest,
        "AGN-CMD-VERSION-RECEIPT"
    );
    unsupported_agent_command!(
        approve_agent_version,
        agent::ApproveAgentVersionRequest,
        "AGN-CMD-APPROVE-RECEIPT"
    );
    unsupported_agent_command!(
        suspend_agent,
        agent::AgentDefinitionIdentityRequest,
        "AGN-CMD-SUSPEND-RECEIPT"
    );
    unsupported_agent_command!(
        retire_agent,
        agent::AgentDefinitionIdentityRequest,
        "AGN-CMD-RETIRE-RECEIPT"
    );
    unsupported_agent_command!(
        start_assignment,
        agent::StartAgentAssignmentRequest,
        "AGN-CMD-START-RECEIPT"
    );
    unsupported_agent_command!(
        delegate_assignment,
        agent::DelegateAgentAssignmentRequest,
        "AGN-CMD-DELEGATE-RECEIPT"
    );
    unsupported_agent_command!(
        retry_assignment,
        agent::RetryAgentAssignmentRequest,
        "AGN-CMD-RETRY-RECEIPT"
    );
    unsupported_agent_command!(
        cancel_assignment,
        agent::CancelAgentAssignmentRequest,
        "AGN-CMD-CANCEL-RECEIPT"
    );
    unsupported_agent_command!(
        register_pool,
        agent::RegisterAgentPoolRequest,
        "AGN-CMD-POOL-REGISTER-RECEIPT"
    );
    unsupported_agent_command!(
        replace_pool_members,
        agent::ReplaceAgentPoolMembersRequest,
        "AGN-CMD-POOL-MEMBERS-RECEIPT"
    );
    unsupported_agent_command!(
        route_pool,
        agent::RouteAgentPoolRequest,
        "AGN-CMD-POOL-ROUTE-RECEIPT"
    );
    unsupported_agent_command!(
        start_private_session,
        agent::StartPrivateAgentSessionRequest,
        "AGN-CMD-SESSION-START-RECEIPT"
    );
    unsupported_agent_command!(
        append_private_session_message,
        agent::AppendPrivateAgentSessionMessageRequest,
        "AGN-CMD-SESSION-APPEND-RECEIPT"
    );
    unsupported_agent_command!(
        publish_private_session_message,
        agent::PublishPrivateAgentSessionMessageRequest,
        "AGN-CMD-SESSION-PUBLISH-RECEIPT"
    );
    unsupported_agent_command!(
        set_private_session_legal_hold,
        agent::SetPrivateAgentSessionLegalHoldRequest,
        "AGN-CMD-SESSION-HOLD-RECEIPT"
    );
    unsupported_agent_command!(
        expire_private_session,
        agent::PrivateAgentSessionIdentityRequest,
        "AGN-CMD-SESSION-EXPIRE-RECEIPT"
    );
}

macro_rules! unsupported_agent_query {
    ($method:ident, $response:ty, $gap:literal) => {
        fn $method<'life0, 'async_trait>(
            &'life0 self,
            request: Request<agent::GetAgentRecordRequest>,
        ) -> core::pin::Pin<
            Box<
                dyn core::future::Future<Output = Result<Response<$response>, Status>>
                    + Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move {
                let transport = request.metadata().clone();
                let input = request.into_inner();
                let _authenticated = authenticate_query(
                    self.authenticator.as_ref(),
                    transport,
                    input.context,
                    FutureOwnerContext::AgentOrchestration,
                )
                .await?;
                Err(owner_port_gap($gap))
            })
        }
    };
}

#[async_trait]
impl<Q, G> AgentOrchestrationQueryService for AgentQueryRpc<Q, G>
where
    Q: AgentQueryApplication,
    G: FutureAgentProjectsAuthenticator,
{
    unsupported_agent_query!(
        get_agent_definition,
        agent::GetAgentDefinitionResponse,
        "AGN-QRY-DEFINITION-PROJECTION"
    );
    unsupported_agent_query!(
        get_assignment,
        agent::GetAgentAssignmentResponse,
        "AGN-QRY-ASSIGNMENT-PROJECTION"
    );
    unsupported_agent_query!(
        get_pool,
        agent::GetAgentPoolResponse,
        "AGN-QRY-POOL-PROJECTION"
    );
    unsupported_agent_query!(
        get_private_session,
        agent::GetPrivateAgentSessionResponse,
        "AGN-QRY-SESSION-PROJECTION"
    );

    async fn get_routing_decision(
        &self,
        request: Request<agent::GetAgentRecordRequest>,
    ) -> Result<Response<agent::GetAgentRoutingDecisionResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_query(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::AgentOrchestration,
        )
        .await?;
        let id = PoolRoutingDecisionId::try_from_uuid(parse_uuid(
            &input.record_id,
            "routing_decision_id",
        )?)
        .map_err(|_| invalid("routing_decision_id is invalid"))?;
        let summary = self
            .application
            .routing_summary(
                authenticated.holding_route,
                authenticated.actor,
                authenticated.active_scope,
                id,
            )
            .await
            .map_err(status_from_agent_port)?;
        Ok(Response::new(agent::GetAgentRoutingDecisionResponse {
            decision: summary.as_ref().map(encode_routing_summary),
            // The current owner query port does not expose a checkpoint. The
            // field remains absent rather than fabricating freshness.
            projection: None,
        }))
    }

    async fn get_invocation_evidence(
        &self,
        request: Request<agent::GetAgentRecordRequest>,
    ) -> Result<Response<agent::GetAgentInvocationEvidenceResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_query(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::AgentOrchestration,
        )
        .await?;
        let id = ProviderInvocationId::try_from_uuid(parse_uuid(
            &input.record_id,
            "provider_invocation_id",
        )?)
        .map_err(|_| invalid("provider_invocation_id is invalid"))?;
        let summary = self
            .application
            .invocation_summary(
                authenticated.holding_route,
                authenticated.actor,
                authenticated.active_scope,
                id,
            )
            .await
            .map_err(status_from_agent_port)?;
        Ok(Response::new(agent::GetAgentInvocationEvidenceResponse {
            evidence: summary.as_ref().map(encode_invocation_summary),
            projection: None,
        }))
    }
}

macro_rules! unsupported_projects_command {
    ($method:ident, $request:ty, $gap:literal) => {
        fn $method<'life0, 'async_trait>(
            &'life0 self,
            request: Request<$request>,
        ) -> core::pin::Pin<
            Box<
                dyn core::future::Future<
                        Output = Result<Response<projects::ProjectsMutationResponse>, Status>,
                    > + Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move {
                let transport = request.metadata().clone();
                let input = request.into_inner();
                let _authenticated = authenticate_command(
                    self.authenticator.as_ref(),
                    transport,
                    input.context,
                    FutureOwnerContext::Projects,
                )
                .await?;
                Err(owner_port_gap($gap))
            })
        }
    };
}

#[async_trait]
impl<A, G> ProjectsCommandService for ProjectsCommandRpc<A, G>
where
    A: ProjectsCommandApplication,
    G: FutureAgentProjectsAuthenticator,
{
    async fn create_project(
        &self,
        request: Request<projects::CreateProjectRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        let command = CreateProject {
            project_id: project_id(&input.project_id)?,
            scope: decode_target_scope(input.scope, &authenticated.active_scope)?,
            code: input.code,
            name: input.name,
            class: decode_project_class(input.project_class)?,
            policy_ref: input.policy_ref,
            workflow_id: workflow_id(&input.workflow_template_id)?,
            owner_id: parse_uuid(&input.owner_principal_id, "owner_principal_id")?,
        };
        let receipt = self
            .application
            .create_project(
                authenticated.holding_route,
                authenticated.projects_context,
                command,
            )
            .await
            .map_err(status_from_projects_application)?;
        Ok(Response::new(encode_projects_mutation(receipt)))
    }

    async fn propose_dependency(
        &self,
        request: Request<projects::ProposeProjectDependencyRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        let command = ProposeDependency {
            dependency_id: dependency_id(&input.dependency_id)?,
            source: decode_dependency_endpoint(
                input.source,
                &authenticated.active_scope,
                "source",
            )?,
            target: decode_dependency_endpoint(
                input.target,
                &authenticated.active_scope,
                "target",
            )?,
            kind: decode_dependency_kind(input.kind)?,
            proposed_by: parse_uuid(&input.proposed_by_principal_id, "proposed_by_principal_id")?,
        };
        let receipt = self
            .application
            .propose_dependency(
                authenticated.holding_route,
                authenticated.projects_context,
                command,
            )
            .await
            .map_err(status_from_projects_application)?;
        Ok(Response::new(encode_projects_mutation(receipt)))
    }

    async fn propose_handoff(
        &self,
        request: Request<projects::ProposeProjectHandoffRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        let command = ProposeHandoff {
            handoff_id: handoff_id(&input.handoff_id)?,
            project_id: project_id(&input.project_id)?,
            item_id: project_node_id(&input.node_id)?,
            scope: decode_target_scope(input.scope, &authenticated.active_scope)?,
            kind: decode_handoff_kind(input.kind)?,
            payload_digest: input.payload_digest,
            owner_id: parse_uuid(&input.owner_principal_id, "owner_principal_id")?,
        };
        let receipt = self
            .application
            .propose_handoff(
                authenticated.holding_route,
                authenticated.projects_context,
                command,
            )
            .await
            .map_err(status_from_projects_application)?;
        Ok(Response::new(encode_projects_mutation(receipt)))
    }

    async fn transition_project(
        &self,
        request: Request<projects::TransitionProjectRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        let target_status = decode_project_status(input.target_status)?;
        if target_status == ProjectStatus::Closed {
            return Err(owner_port_gap("PRJ-CMD-CLOSE-EVIDENCE-CONTRACT"));
        }
        if input.close_evidence_digest.is_some() {
            return Err(invalid(
                "close evidence is only valid for a closed transition",
            ));
        }
        let receipt = self
            .application
            .transition_project(
                authenticated.holding_route,
                authenticated.projects_context,
                TransitionProject {
                    project_id: project_id(&input.project_id)?,
                    target_status,
                    close_evidence: None,
                },
            )
            .await
            .map_err(status_from_projects_application)?;
        Ok(Response::new(encode_projects_mutation(receipt)))
    }

    async fn add_node(
        &self,
        request: Request<projects::AddProjectNodeRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        if input.source_thread_id.is_some()
            || input.source_comment_id.is_some()
            || input.source_comment_revision.is_some()
        {
            return Err(owner_port_gap("PRJ-CMD-ADD-NODE-PROMOTION-DIGEST"));
        }
        let receipt = self
            .application
            .add_node(
                authenticated.holding_route,
                authenticated.projects_context,
                AddProjectNodeInput {
                    project_id: project_id(&input.project_id)?,
                    node_id: project_node_id(&input.node_id)?,
                    parent_id: input
                        .parent_node_id
                        .as_deref()
                        .map(project_node_id)
                        .transpose()?,
                    kind: decode_node_kind(input.kind)?,
                    title: input.title,
                    owner_id: parse_uuid(&input.owner_principal_id, "owner_principal_id")?,
                    assignees: input
                        .assignee_principal_ids
                        .iter()
                        .map(|value| parse_uuid(value, "assignee_principal_id"))
                        .collect::<Result<_, _>>()?,
                    planned_start: parse_optional_date(
                        input.planned_start_date,
                        "planned_start_date",
                    )?,
                    planned_end: parse_optional_date(input.planned_end_date, "planned_end_date")?,
                    workflow_state: input.workflow_state,
                    promotion_source: None,
                },
            )
            .await
            .map_err(status_from_projects_application)?;
        Ok(Response::new(encode_projects_mutation(receipt)))
    }

    async fn move_node(
        &self,
        request: Request<projects::MoveProjectNodeRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        let receipt = self
            .application
            .change_node(
                authenticated.holding_route,
                authenticated.projects_context,
                ChangeProjectNode {
                    project_id: project_id(&input.project_id)?,
                    node_id: project_node_id(&input.node_id)?,
                    change: ProjectNodeChange::Move {
                        parent_id: input
                            .new_parent_node_id
                            .as_deref()
                            .map(project_node_id)
                            .transpose()?,
                    },
                },
            )
            .await
            .map_err(status_from_projects_application)?;
        Ok(Response::new(encode_projects_mutation(receipt)))
    }

    async fn transition_node(
        &self,
        request: Request<projects::TransitionProjectNodeRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        let receipt = self
            .application
            .change_node(
                authenticated.holding_route,
                authenticated.projects_context,
                ChangeProjectNode {
                    project_id: project_id(&input.project_id)?,
                    node_id: project_node_id(&input.node_id)?,
                    change: ProjectNodeChange::Transition {
                        target_workflow_state: input.target_workflow_state,
                    },
                },
            )
            .await
            .map_err(status_from_projects_application)?;
        Ok(Response::new(encode_projects_mutation(receipt)))
    }

    async fn attach_node_reference(
        &self,
        request: Request<projects::AttachProjectNodeReferenceRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        let reference = StableRecordRef::try_new(
            RecordId::try_from_uuid(parse_uuid(
                &input.referenced_record_id,
                "referenced_record_id",
            )?)
            .map_err(|_| invalid("referenced_record_id is invalid"))?,
            decode_context_id(&input.referenced_context)?,
            RecordTypeId::try_new(input.referenced_record_type)
                .map_err(|_| invalid("referenced_record_type is invalid"))?,
        )
        .map_err(|_| invalid("referenced record identity is invalid"))?;
        let receipt = self
            .application
            .change_node(
                authenticated.holding_route,
                authenticated.projects_context,
                ChangeProjectNode {
                    project_id: project_id(&input.project_id)?,
                    node_id: project_node_id(&input.node_id)?,
                    change: ProjectNodeChange::AttachReference { reference },
                },
            )
            .await
            .map_err(status_from_projects_application)?;
        Ok(Response::new(encode_projects_mutation(receipt)))
    }

    async fn add_node_attachment(
        &self,
        request: Request<projects::AddProjectNodeAttachmentRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        let receipt = self
            .application
            .change_node(
                authenticated.holding_route,
                authenticated.projects_context,
                ChangeProjectNode {
                    project_id: project_id(&input.project_id)?,
                    node_id: project_node_id(&input.node_id)?,
                    change: ProjectNodeChange::AddAttachment {
                        attachment: AttachmentRef {
                            attachment_id: parse_uuid(&input.attachment_id, "attachment_id")?,
                            object_ref: input.object_ref,
                            content_digest: input.content_digest,
                        },
                    },
                },
            )
            .await
            .map_err(status_from_projects_application)?;
        Ok(Response::new(encode_projects_mutation(receipt)))
    }
    unsupported_projects_command!(
        record_node_time,
        projects::RecordProjectNodeTimeRequest,
        "PRJ-CMD-RECORD-TIME-EVIDENCE-TYPE"
    );
    unsupported_projects_command!(
        record_node_cost,
        projects::RecordProjectNodeCostRequest,
        "PRJ-CMD-RECORD-COST-EVIDENCE-TYPE"
    );
    async fn decide_dependency(
        &self,
        request: Request<projects::DecideProjectDependencyRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        require_authenticated_human_decision(&authenticated.projects_context.actor)?;
        // `owner_principal_id` is a legacy, untrusted hint. Human decision
        // authority is derived only from the authenticated actor above.
        let decision = if input.accept {
            DependencyDecision::Accept {
                side: decode_dependency_side(input.side)?,
            }
        } else {
            DependencyDecision::Reject
        };
        let receipt = self
            .application
            .decide_dependency(
                authenticated.holding_route,
                authenticated.projects_context,
                DecideDependency {
                    dependency_id: dependency_id(&input.dependency_id)?,
                    decision,
                },
            )
            .await
            .map_err(status_from_projects_application)?;
        Ok(Response::new(encode_projects_mutation(receipt)))
    }

    async fn cancel_dependency(
        &self,
        request: Request<projects::EndProjectDependencyRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        let receipt = self
            .application
            .decide_dependency(
                authenticated.holding_route,
                authenticated.projects_context,
                DecideDependency {
                    dependency_id: dependency_id(&input.dependency_id)?,
                    decision: DependencyDecision::Cancel {
                        owner_id: parse_uuid(&input.owner_principal_id, "owner_principal_id")?,
                    },
                },
            )
            .await
            .map_err(status_from_projects_application)?;
        Ok(Response::new(encode_projects_mutation(receipt)))
    }

    async fn revoke_dependency(
        &self,
        request: Request<projects::EndProjectDependencyRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        let receipt = self
            .application
            .decide_dependency(
                authenticated.holding_route,
                authenticated.projects_context,
                DecideDependency {
                    dependency_id: dependency_id(&input.dependency_id)?,
                    decision: DependencyDecision::Revoke {
                        owner_id: parse_uuid(&input.owner_principal_id, "owner_principal_id")?,
                    },
                },
            )
            .await
            .map_err(status_from_projects_application)?;
        Ok(Response::new(encode_projects_mutation(receipt)))
    }

    async fn approve_handoff(
        &self,
        request: Request<projects::ProjectHandoffActionRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        require_authenticated_human_decision(&authenticated.projects_context.actor)?;
        let receipt = self
            .application
            .change_handoff(
                authenticated.holding_route,
                authenticated.projects_context,
                ChangeHandoff {
                    handoff_id: handoff_id(&input.handoff_id)?,
                    change: HandoffChange::Approve,
                },
            )
            .await
            .map_err(status_from_projects_application)?;
        Ok(Response::new(encode_projects_mutation(receipt)))
    }

    async fn mark_handoff_ready(
        &self,
        request: Request<projects::ProjectHandoffActionRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        self.change_handoff_status(request, HandoffChange::MarkReady)
            .await
    }

    async fn mark_handoff_uncertain(
        &self,
        request: Request<projects::ProjectHandoffActionRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        self.change_handoff_status(request, HandoffChange::MarkUncertain)
            .await
    }

    async fn cancel_handoff(
        &self,
        request: Request<projects::ProjectHandoffActionRequest>,
    ) -> Result<Response<projects::ProjectsMutationResponse>, Status> {
        self.change_handoff_status(request, HandoffChange::Cancel)
            .await
    }
    unsupported_projects_command!(
        reconcile_handoff,
        projects::ProjectHandoffActionRequest,
        "PRJ-CMD-RECONCILE-HANDOFF-OUTCOME"
    );
}

#[async_trait]
impl<Q, G> ProjectsQueryService for ProjectsQueryRpc<Q, G>
where
    Q: ProjectsQueryApplication,
    G: FutureAgentProjectsAuthenticator,
{
    async fn get_project(
        &self,
        request: Request<projects::GetProjectRequest>,
    ) -> Result<Response<projects::GetProjectResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_query(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        let result = self
            .application
            .get_project(
                authenticated.holding_route,
                authenticated.actor,
                authenticated.active_scope,
                authenticated.consistency,
                project_id(&input.project_id)?,
            )
            .await
            .map_err(status_from_projects_port)?;
        Ok(Response::new(projects::GetProjectResponse {
            project: result
                .value
                .as_ref()
                .map(|value| encode_project(&value.project)),
            projection: Some(encode_projection_metadata(&result.metadata)),
        }))
    }

    async fn get_project_graph(
        &self,
        request: Request<projects::GetProjectRequest>,
    ) -> Result<Response<projects::GetProjectGraphResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_query(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        let result = self
            .application
            .get_project_graph(
                authenticated.holding_route,
                authenticated.actor,
                authenticated.active_scope,
                authenticated.consistency,
                project_id(&input.project_id)?,
            )
            .await
            .map_err(status_from_projects_port)?;
        let (project, nodes) = result.value.map_or_else(
            || (None, Vec::new()),
            |value| {
                let project = Some(encode_project(&value.project));
                let nodes = value.hierarchy.iter().map(encode_hierarchy_row).collect();
                (project, nodes)
            },
        );
        Ok(Response::new(projects::GetProjectGraphResponse {
            project,
            nodes,
            projection: Some(encode_projection_metadata(&result.metadata)),
        }))
    }

    async fn get_record(
        &self,
        request: Request<projects::GetProjectRecordRequest>,
    ) -> Result<Response<projects::GetProjectRecordResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let record_kind = decode_project_record_kind(input.record_kind)?;
        let record_id = parse_uuid(&input.record_id, "record_id")?;
        let authenticated = authenticate_query(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        let result = self
            .application
            .get_project_record(
                authenticated.holding_route,
                authenticated.actor,
                authenticated.active_scope,
                authenticated.consistency,
                record_id,
                record_kind,
            )
            .await
            .map_err(status_from_projects_port)?;
        Ok(Response::new(projects::GetProjectRecordResponse {
            record: result.value.as_ref().map(encode_project_record),
            projection: Some(encode_projection_metadata(&result.metadata)),
        }))
    }

    async fn list_projects(
        &self,
        request: Request<projects::ListProjectsRequest>,
    ) -> Result<Response<projects::ListProjectsResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_query(
            self.authenticator.as_ref(),
            transport,
            input.context,
            FutureOwnerContext::Projects,
        )
        .await?;
        let page_size = u16::try_from(input.page_size)
            .ok()
            .filter(|value| *value > 0 && *value <= 500)
            .ok_or_else(|| invalid("page_size is invalid"))?;
        let result = self
            .application
            .list_projects(
                authenticated.holding_route,
                authenticated.actor,
                authenticated.active_scope,
                authenticated.consistency,
                ProjectListFilter {
                    page_size,
                    cursor: optional_text(input.cursor),
                },
            )
            .await
            .map_err(status_from_projects_port)?;
        Ok(Response::new(projects::ListProjectsResponse {
            projects: result.value.projects.iter().map(encode_project).collect(),
            next_cursor: result.value.next_cursor,
            projection: Some(encode_projection_metadata(&result.metadata)),
        }))
    }
}

struct AuthenticatedCommand {
    holding_route: FutureHoldingRoute,
    active_scope: AuthorityScope,
    projects_context: ProjectsApplicationCommandContext,
}

struct AuthenticatedQuery {
    holding_route: FutureHoldingRoute,
    actor: ActorAttribution,
    active_scope: AuthorityScope,
    consistency: QueryConsistency,
}

async fn authenticate_command<G: FutureAgentProjectsAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    context: Option<common::CommandContext>,
    owner_context: FutureOwnerContext,
) -> Result<AuthenticatedCommand, Status> {
    let context = context.ok_or_else(|| invalid("command context is required"))?;
    let presented_actor = decode_actor(context.actor.clone())?;
    let presented_scope = decode_scope(
        context
            .active_scope
            .clone()
            .ok_or_else(|| invalid("active_scope is required"))?,
    )?;
    let subscription_id = normalize_security_input(context.subscription_id.clone())?;
    let mfa_code = normalize_security_input(context.mfa_code.clone())?;
    let authenticated = authenticator
        .authenticate(FutureAuthenticationRequest {
            transport_metadata,
            presented_actor,
            presented_scope,
            subscription_id,
            mfa_code,
            owner_context,
            access_mode: FutureAccessMode::Write,
        })
        .await
        .map_err(|error| status_from_authentication(error, owner_context))?;
    let canonical_actor = authenticated.actor.clone();
    let canonical_scope = authenticated.active_scope.clone();
    let projects_context =
        decode_projects_command_context(context, canonical_actor, canonical_scope.clone())?;
    Ok(AuthenticatedCommand {
        holding_route: authenticated.holding_route,
        active_scope: canonical_scope,
        projects_context,
    })
}

async fn authenticate_query<G: FutureAgentProjectsAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    context: Option<common::QueryContext>,
    owner_context: FutureOwnerContext,
) -> Result<AuthenticatedQuery, Status> {
    let context = context.ok_or_else(|| invalid("query context is required"))?;
    let presented_actor = decode_actor(context.actor.clone())?;
    let presented_scope = decode_scope(
        context
            .active_scope
            .clone()
            .ok_or_else(|| invalid("active_scope is required"))?,
    )?;
    let authenticated = authenticator
        .authenticate(FutureAuthenticationRequest {
            transport_metadata,
            presented_actor,
            presented_scope,
            subscription_id: None,
            mfa_code: None,
            owner_context,
            access_mode: FutureAccessMode::Read,
        })
        .await
        .map_err(|error| status_from_authentication(error, owner_context))?;
    validate_query_context(&context)?;
    Ok(AuthenticatedQuery {
        holding_route: authenticated.holding_route,
        actor: authenticated.actor,
        active_scope: authenticated.active_scope,
        consistency: decode_consistency(context.consistency)?,
    })
}

fn decode_projects_command_context(
    context: common::CommandContext,
    actor: ActorAttribution,
    authority_scope: AuthorityScope,
) -> Result<ProjectsApplicationCommandContext, Status> {
    validate_contract_version(context.contract_version)?;
    let correlation_id = correlation_id(&context.correlation_id)?;
    Ok(ProjectsApplicationCommandContext {
        command_id: CommandId::try_from_uuid(parse_uuid(&context.command_id, "command_id")?)
            .map_err(|_| invalid("command_id is invalid"))?,
        idempotency_key: IdempotencyKey::try_new(context.idempotency_key)
            .map_err(|_| invalid("idempotency_key is invalid"))?,
        correlation_id,
        causation_id: parse_optional_uuid(&context.causation_id, "causation_id")?
            .map(EventId::try_from_uuid)
            .transpose()
            .map_err(|_| invalid("causation_id is invalid"))?,
        actor,
        authority_scope,
        audit: decode_audit(context.audit, correlation_id)?,
        expected_version: (context.expected_aggregate_version != 0)
            .then_some(context.expected_aggregate_version),
    })
}

fn validate_query_context(context: &common::QueryContext) -> Result<(), Status> {
    validate_contract_version(context.contract_version)?;
    QueryId::try_from_uuid(parse_uuid(&context.request_id, "request_id")?)
        .map_err(|_| invalid("request_id is invalid"))?;
    let correlation_id = correlation_id(&context.correlation_id)?;
    decode_audit(context.audit.clone(), correlation_id)?;
    Ok(())
}

fn validate_contract_version(value: u32) -> Result<(), Status> {
    if value == u32::from(erpx_cqrs::CONTRACT_VERSION) {
        Ok(())
    } else {
        Err(invalid("contract_version is unsupported"))
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
    expected_correlation_id: CorrelationId,
) -> Result<AuditContext, Status> {
    let audit = audit.ok_or_else(|| invalid("audit context is required"))?;
    if !audit.correlation_id.trim().is_empty()
        && correlation_id(&audit.correlation_id)? != expected_correlation_id
    {
        return Err(invalid("audit correlation does not match request"));
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
        Scope::Holding(value) => Ok(AuthorityScope::Holding {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
        }),
        Scope::Company(value) => Ok(AuthorityScope::Company {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
        }),
        Scope::Branch(value) => Ok(AuthorityScope::Branch {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
            branch_id: parse_uuid(&value.branch_id, "branch_id")?,
        }),
        Scope::Subtree(value) => Ok(AuthorityScope::Subtree {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            organization_node_id: parse_uuid(&value.organization_node_id, "organization_node_id")?,
        }),
    }
}

fn decode_target_scope(
    scope: Option<common::AuthorityScope>,
    authority_scope: &AuthorityScope,
) -> Result<AuthorityScope, Status> {
    let scope = decode_scope(scope.ok_or_else(|| invalid("target scope is required"))?)?;
    if authority_scope.contains(&scope) {
        Ok(scope)
    } else {
        Err(Status::permission_denied(
            "target scope is outside authenticated authority",
        ))
    }
}

fn decode_consistency(
    consistency: Option<common::QueryConsistency>,
) -> Result<QueryConsistency, Status> {
    use common::query_consistency::Requirement;
    match consistency.and_then(|value| value.requirement) {
        None | Some(Requirement::Eventual(_)) => Ok(QueryConsistency::Eventual),
        Some(Requirement::ReadYourWrites(value)) => {
            Ok(QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: value.projection,
                partition: value.partition,
                target: erpx_cqrs::ProjectionTarget {
                    event_id: EventId::try_from_uuid(parse_uuid(&value.event_id, "event_id")?)
                        .map_err(|_| invalid("event_id is invalid"))?,
                    global_sequence: value.global_sequence,
                },
                timeout_millis: value.timeout_millis,
            }))
        }
        // The owner response carries its exact source aggregate version.  The
        // authenticated Console compares that value with this requirement and
        // reports `Stale`; projection-position callers use the stronger waiter
        // above.  Treating an aggregate-version hint as eventual here avoids
        // fabricating an event-position mapping that the write receipt did not
        // supply.
        Some(Requirement::AggregateVersion(_)) => Ok(QueryConsistency::Eventual),
    }
}

fn decode_dependency_endpoint(
    endpoint: Option<projects::ProjectDependencyEndpoint>,
    authority_scope: &AuthorityScope,
    label: &'static str,
) -> Result<DependencyEndpoint, Status> {
    let endpoint = endpoint.ok_or_else(|| invalid("dependency endpoint is required"))?;
    Ok(DependencyEndpoint {
        project_id: project_id(&endpoint.project_id)?,
        project_version: endpoint.project_version,
        node_id: project_node_id(&endpoint.node_id)?,
        node_version: endpoint.node_version,
        scope: decode_target_scope(endpoint.scope, authority_scope)?,
        owner_id: parse_uuid(&endpoint.owner_principal_id, label)?,
    })
}

fn decode_project_class(value: i32) -> Result<ProjectClass, Status> {
    match projects::ProjectClass::try_from(value).ok() {
        Some(projects::ProjectClass::ClientServices) => Ok(ProjectClass::ClientServices),
        Some(projects::ProjectClass::Internal) => Ok(ProjectClass::Internal),
        Some(projects::ProjectClass::Capital) => Ok(ProjectClass::Capital),
        _ => Err(invalid("project_class is invalid")),
    }
}

fn decode_project_status(value: i32) -> Result<ProjectStatus, Status> {
    match projects::ProjectStatus::try_from(value).ok() {
        Some(projects::ProjectStatus::Proposed) => Ok(ProjectStatus::Proposed),
        Some(projects::ProjectStatus::Approved) => Ok(ProjectStatus::Approved),
        Some(projects::ProjectStatus::Active) => Ok(ProjectStatus::Active),
        Some(projects::ProjectStatus::Paused) => Ok(ProjectStatus::Paused),
        Some(projects::ProjectStatus::Completed) => Ok(ProjectStatus::Completed),
        Some(projects::ProjectStatus::Closed) => Ok(ProjectStatus::Closed),
        Some(projects::ProjectStatus::Cancelled) => Ok(ProjectStatus::Cancelled),
        Some(projects::ProjectStatus::Archived) => Ok(ProjectStatus::Archived),
        _ => Err(invalid("target_status is invalid")),
    }
}

fn decode_node_kind(value: i32) -> Result<NodeKind, Status> {
    match projects::ProjectNodeKind::try_from(value).ok() {
        Some(projects::ProjectNodeKind::Phase) => Ok(NodeKind::Phase),
        Some(projects::ProjectNodeKind::Epic) => Ok(NodeKind::Epic),
        Some(projects::ProjectNodeKind::Task) => Ok(NodeKind::Task),
        Some(projects::ProjectNodeKind::Milestone) => Ok(NodeKind::Milestone),
        Some(projects::ProjectNodeKind::Deliverable) => Ok(NodeKind::Deliverable),
        Some(projects::ProjectNodeKind::Risk) => Ok(NodeKind::Risk),
        Some(projects::ProjectNodeKind::Issue) => Ok(NodeKind::Issue),
        Some(projects::ProjectNodeKind::Decision) => Ok(NodeKind::Decision),
        Some(projects::ProjectNodeKind::ChangeRequest) => Ok(NodeKind::ChangeRequest),
        _ => Err(invalid("project node kind is invalid")),
    }
}

fn decode_dependency_side(value: i32) -> Result<DependencySide, Status> {
    match projects::ProjectDependencySide::try_from(value).ok() {
        Some(projects::ProjectDependencySide::Source) => Ok(DependencySide::Source),
        Some(projects::ProjectDependencySide::Target) => Ok(DependencySide::Target),
        _ => Err(invalid("dependency side is invalid")),
    }
}

fn decode_context_id(value: &str) -> Result<ContextId, Status> {
    let value = value.trim();
    ContextId::ALL
        .into_iter()
        .find(|context| context.slug() == value)
        .ok_or_else(|| invalid("referenced_context is invalid"))
}

fn parse_optional_date(
    value: Option<String>,
    field: &'static str,
) -> Result<Option<NaiveDate>, Status> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").map_err(|_| invalid(field))
        })
        .transpose()
}

fn decode_dependency_kind(value: i32) -> Result<DependencyKind, Status> {
    match projects::ProjectDependencyKind::try_from(value).ok() {
        Some(projects::ProjectDependencyKind::FinishToStart) => Ok(DependencyKind::FinishToStart),
        Some(projects::ProjectDependencyKind::StartToStart) => Ok(DependencyKind::StartToStart),
        Some(projects::ProjectDependencyKind::FinishToFinish) => Ok(DependencyKind::FinishToFinish),
        _ => Err(invalid(
            "dependency kind is unsupported by the owner domain",
        )),
    }
}

fn decode_handoff_kind(value: i32) -> Result<HandoffKind, Status> {
    match projects::ProjectHandoffKind::try_from(value).ok() {
        Some(projects::ProjectHandoffKind::FinanceInvoice) => Ok(HandoffKind::FinanceInvoice),
        Some(projects::ProjectHandoffKind::FinanceBilling) => Ok(HandoffKind::FinanceBilling),
        Some(projects::ProjectHandoffKind::LegalContract) => Ok(HandoffKind::LegalContract),
        Some(projects::ProjectHandoffKind::OperationsAction) => Ok(HandoffKind::OperationsAction),
        Some(projects::ProjectHandoffKind::AssetCapitalization) => {
            Ok(HandoffKind::AssetCapitalization)
        }
        Some(projects::ProjectHandoffKind::SalesBilling) => Ok(HandoffKind::SalesBilling),
        Some(projects::ProjectHandoffKind::ProcurementRequisition) => {
            Ok(HandoffKind::ProcurementRequisition)
        }
        _ => Err(invalid("handoff kind is invalid")),
    }
}

fn encode_projects_mutation(
    receipt: CommandReceipt<ProjectsAggregate>,
) -> projects::ProjectsMutationResponse {
    let record_type = receipt.response.aggregate_type().to_owned();
    let record_id = receipt.response.id_uuid().to_string();
    projects::ProjectsMutationResponse {
        receipt: Some(encode_receipt(&receipt)),
        record_type,
        record_id,
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
            .map(|event| common::EventPosition {
                event_id: event.event_id.to_string(),
                global_sequence: event.global_sequence,
            })
            .collect(),
        committed_at: receipt.committed_at.to_rfc3339(),
        replayed: receipt.replayed,
    }
}

fn encode_project(value: &erpx_projects::Project) -> projects::ProjectSummary {
    projects::ProjectSummary {
        project_id: value.id().as_uuid().to_string(),
        source_version: value.version(),
        scope: Some(encode_scope(value.scope())),
        code: value.code().to_owned(),
        name: value.name().to_owned(),
        project_class: match value.class() {
            ProjectClass::ClientServices => projects::ProjectClass::ClientServices as i32,
            ProjectClass::Internal => projects::ProjectClass::Internal as i32,
            ProjectClass::Capital => projects::ProjectClass::Capital as i32,
        },
        status: match value.status() {
            erpx_projects::ProjectStatus::Proposed => projects::ProjectStatus::Proposed as i32,
            erpx_projects::ProjectStatus::Approved => projects::ProjectStatus::Approved as i32,
            erpx_projects::ProjectStatus::Active => projects::ProjectStatus::Active as i32,
            erpx_projects::ProjectStatus::Paused => projects::ProjectStatus::Paused as i32,
            erpx_projects::ProjectStatus::Completed => projects::ProjectStatus::Completed as i32,
            erpx_projects::ProjectStatus::Closed => projects::ProjectStatus::Closed as i32,
            erpx_projects::ProjectStatus::Cancelled => projects::ProjectStatus::Cancelled as i32,
            erpx_projects::ProjectStatus::Archived => projects::ProjectStatus::Archived as i32,
        },
        owner_principal_id: value.owner_id().to_string(),
        workflow_template_id: value.workflow().template_id.as_uuid().to_string(),
        workflow_template_version: value.workflow().version,
        updated_at: value.updated_at().to_rfc3339(),
    }
}

fn encode_hierarchy_row(value: &erpx_projects::HierarchyRow) -> projects::ProjectNodeSummary {
    projects::ProjectNodeSummary {
        node_id: value.node.id().as_uuid().to_string(),
        source_version: value.node.version(),
        parent_node_id: value.node.parent_id().map(|id| id.as_uuid().to_string()),
        kind: match value.node.kind() {
            erpx_projects::NodeKind::Phase => projects::ProjectNodeKind::Phase as i32,
            erpx_projects::NodeKind::Epic => projects::ProjectNodeKind::Epic as i32,
            erpx_projects::NodeKind::Task => projects::ProjectNodeKind::Task as i32,
            erpx_projects::NodeKind::Milestone => projects::ProjectNodeKind::Milestone as i32,
            erpx_projects::NodeKind::Deliverable => projects::ProjectNodeKind::Deliverable as i32,
            erpx_projects::NodeKind::Risk => projects::ProjectNodeKind::Risk as i32,
            erpx_projects::NodeKind::Issue => projects::ProjectNodeKind::Issue as i32,
            erpx_projects::NodeKind::Decision => projects::ProjectNodeKind::Decision as i32,
            erpx_projects::NodeKind::ChangeRequest => {
                projects::ProjectNodeKind::ChangeRequest as i32
            }
        },
        title: value.node.title().to_owned(),
        owner_principal_id: value.node.owner_id().to_string(),
        assignee_principal_ids: value
            .node
            .assignees()
            .iter()
            .map(ToString::to_string)
            .collect(),
        workflow_state: value.node.workflow_state().to_owned(),
        core_state: format!("{:?}", value.node.core_state()).to_ascii_lowercase(),
        depth: u32::from(value.depth),
    }
}

fn decode_project_record_kind(value: i32) -> Result<ProjectRecordKind, Status> {
    match projects::ProjectRecordKind::try_from(value).ok() {
        Some(projects::ProjectRecordKind::Phase) => Ok(ProjectRecordKind::Phase),
        Some(projects::ProjectRecordKind::Item) => Ok(ProjectRecordKind::Item),
        Some(projects::ProjectRecordKind::WorkflowTemplate) => {
            Ok(ProjectRecordKind::WorkflowTemplate)
        }
        Some(projects::ProjectRecordKind::Handoff) => Ok(ProjectRecordKind::Handoff),
        _ => Err(invalid("project record kind is invalid")),
    }
}

fn enum_token<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}

fn encode_project_record(value: &ProjectRecordView) -> projects::ProjectRecordSummary {
    match value {
        ProjectRecordView::Node { project_id, node } => projects::ProjectRecordSummary {
            record_id: node.id().as_uuid().to_string(),
            source_version: node.version(),
            record_kind: if node.kind().is_container() {
                projects::ProjectRecordKind::Phase as i32
            } else {
                projects::ProjectRecordKind::Item as i32
            },
            title: node.title().to_owned(),
            lifecycle_status: enum_token(node.core_state()),
            project_id: Some(project_id.as_uuid().to_string()),
            updated_at: node.updated_at().to_rfc3339(),
        },
        ProjectRecordView::WorkflowTemplate(value) => projects::ProjectRecordSummary {
            record_id: value.id().as_uuid().to_string(),
            source_version: value.version(),
            record_kind: projects::ProjectRecordKind::WorkflowTemplate as i32,
            title: value.name().to_owned(),
            lifecycle_status: enum_token(value.status()),
            project_id: None,
            updated_at: value.updated_at().to_rfc3339(),
        },
        ProjectRecordView::Handoff(value) => projects::ProjectRecordSummary {
            record_id: value.id().as_uuid().to_string(),
            source_version: value.version(),
            record_kind: projects::ProjectRecordKind::Handoff as i32,
            title: value.id().as_uuid().to_string(),
            lifecycle_status: enum_token(value.status()),
            project_id: Some(value.project_id().as_uuid().to_string()),
            updated_at: value.updated_at().to_rfc3339(),
        },
    }
}

fn encode_routing_summary(value: &PoolRoutingSummary) -> agent::AgentRoutingDecisionProjection {
    agent::AgentRoutingDecisionProjection {
        routing_decision_id: value.id.as_uuid().to_string(),
        pool_id: value.pool_id.as_uuid().to_string(),
        selected_agent_version_ids: value
            .selected_versions
            .iter()
            .map(|id| id.as_uuid().to_string())
            .collect(),
        comparison_requested: value.compare_requested,
        decision_digest: value.decision_digest.clone(),
        decided_at: value.decided_at.to_rfc3339(),
    }
}

fn encode_invocation_summary(
    value: &InvocationEvidenceSummary,
) -> agent::AgentInvocationEvidenceProjection {
    agent::AgentInvocationEvidenceProjection {
        invocation_id: value.id.as_uuid().to_string(),
        assignment_id: value.assignment_id.as_uuid().to_string(),
        attempt: value.attempt,
        provider_ref: value.provider_ref.clone(),
        model_ref: value.model_ref.clone(),
        policy_version: value.policy_version,
        policy_digest: value.policy_digest.clone(),
        payload_digest: value.payload_digest.clone(),
        output_digest: value.output_digest.clone(),
        provider_units: value.provider_units,
        provider_cost_micros: value.provider_cost_micros,
        outcome: value.outcome.clone(),
        invoked_at: value.invoked_at.to_rfc3339(),
        completed_at: value.completed_at.map(|at| at.to_rfc3339()),
    }
}

fn encode_projection_metadata(metadata: &ProjectionMetadata) -> common::ProjectionMetadata {
    common::ProjectionMetadata {
        checkpoint: metadata
            .checkpoint
            .as_ref()
            .map(|checkpoint| common::ProjectionCheckpoint {
                projection: checkpoint.projection.clone(),
                partition: checkpoint.partition.clone(),
                event_id: checkpoint.event_id.to_string(),
                global_sequence: checkpoint.global_sequence,
                updated_at: checkpoint.updated_at.to_rfc3339(),
            }),
        freshness: match metadata.freshness {
            FreshnessState::Current => common::ProjectionFreshnessState::Current as i32,
            FreshnessState::Stale => common::ProjectionFreshnessState::Stale as i32,
            FreshnessState::Rebuilding => common::ProjectionFreshnessState::Rebuilding as i32,
            FreshnessState::Unavailable => common::ProjectionFreshnessState::Unavailable as i32,
        },
        lag_millis: metadata.lag_millis,
    }
}

fn encode_scope(scope: &AuthorityScope) -> common::AuthorityScope {
    use common::authority_scope::Scope;
    let scope = match scope {
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
    };
    common::AuthorityScope { scope: Some(scope) }
}

fn project_id(value: &str) -> Result<ProjectId, Status> {
    ProjectId::try_from_uuid(parse_uuid(value, "project_id")?)
        .map_err(|_| invalid("project_id is invalid"))
}

fn project_node_id(value: &str) -> Result<ProjectNodeId, Status> {
    ProjectNodeId::try_from_uuid(parse_uuid(value, "node_id")?)
        .map_err(|_| invalid("node_id is invalid"))
}

fn workflow_id(value: &str) -> Result<erpx_projects::WorkflowTemplateId, Status> {
    erpx_projects::WorkflowTemplateId::try_from_uuid(parse_uuid(value, "workflow_template_id")?)
        .map_err(|_| invalid("workflow_template_id is invalid"))
}

fn dependency_id(value: &str) -> Result<DependencyId, Status> {
    DependencyId::try_from_uuid(parse_uuid(value, "dependency_id")?)
        .map_err(|_| invalid("dependency_id is invalid"))
}

fn handoff_id(value: &str) -> Result<HandoffId, Status> {
    HandoffId::try_from_uuid(parse_uuid(value, "handoff_id")?)
        .map_err(|_| invalid("handoff_id is invalid"))
}

fn correlation_id(value: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(value, "correlation_id")?)
        .map_err(|_| invalid("correlation_id is invalid"))
}

fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    let value = Uuid::parse_str(value.trim()).map_err(|_| invalid(field))?;
    if value.is_nil() {
        return Err(invalid(field));
    }
    Ok(value)
}

fn parse_optional_uuid(value: &str, field: &'static str) -> Result<Option<Uuid>, Status> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        parse_uuid(value, field).map(Some)
    }
}

fn optional_nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn optional_text(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn normalize_security_input(value: String) -> Result<Option<String>, Status> {
    let value = optional_nonempty(value);
    if value
        .as_ref()
        .is_some_and(|value| value.len() > MAX_SECURITY_INPUT_LENGTH)
    {
        Err(invalid("security metadata is invalid"))
    } else {
        Ok(value)
    }
}

fn validate_route(route: &FutureHoldingRoute) -> Result<(), Status> {
    for value in [&route.holding_id, &route.data_profile_key] {
        if value.trim().is_empty() || value.len() > MAX_ROUTE_LENGTH {
            return Err(invalid("holding route is invalid"));
        }
    }
    Ok(())
}

fn validate_route_scope(route: &FutureHoldingRoute, scope: &AuthorityScope) -> Result<(), Status> {
    let expected_holding = match scope {
        AuthorityScope::Platform => return Ok(()),
        AuthorityScope::Holding { holding_id }
        | AuthorityScope::Company { holding_id, .. }
        | AuthorityScope::Branch { holding_id, .. }
        | AuthorityScope::Subtree { holding_id, .. } => holding_id,
    };
    let route_holding = parse_uuid(&route.data_profile_key, "holding route data_profile_key")?;
    if &route_holding == expected_holding {
        Ok(())
    } else {
        Err(invalid("holding route does not match authenticated scope"))
    }
}

fn owner_port_gap(id: &'static str) -> Status {
    Status::failed_precondition(format!(
        "owner application port is not available for target contract operation {id}"
    ))
}

fn invalid(message: &'static str) -> Status {
    Status::invalid_argument(message)
}

fn require_authenticated_human_decision(actor: &ActorAttribution) -> Result<(), Status> {
    if actor.is_human()
        && actor.session_id.is_some()
        && actor.represented_principal_ref.is_none()
        && actor.support_capability_id.is_none()
    {
        Ok(())
    } else {
        Err(Status::permission_denied(
            "Projects decision requires a directly authenticated human session",
        ))
    }
}

fn status_from_authentication(
    error: FutureAuthenticationError,
    owner: FutureOwnerContext,
) -> Status {
    match error {
        FutureAuthenticationError::Unauthenticated => {
            Status::unauthenticated(format!("{} authentication is required", owner.label()))
        }
        FutureAuthenticationError::Denied => {
            Status::permission_denied(format!("{} authority is denied", owner.label()))
        }
        FutureAuthenticationError::Unavailable => {
            Status::unavailable(format!("{} authentication is unavailable", owner.label()))
        }
        FutureAuthenticationError::Internal => {
            Status::internal(format!("{} authentication failed", owner.label()))
        }
    }
}

fn status_from_projects_application(error: ProjectApplicationError) -> Status {
    match error {
        ProjectApplicationError::HumanDecisionRequired
        | ProjectApplicationError::Domain(
            ProjectDomainError::HumanDecisionRequired
            | ProjectDomainError::DependencyAuthorityDenied
            | ProjectDomainError::HandoffAuthorityDenied,
        ) => Status::permission_denied(
            "Projects decision requires an authorized authenticated human",
        ),
        ProjectApplicationError::UnexpectedCreateVersion
        | ProjectApplicationError::MissingExpectedVersion
        | ProjectApplicationError::ScopeOutsideAuthority
        | ProjectApplicationError::StaleEndpoint
        | ProjectApplicationError::Serialization
        | ProjectApplicationError::Domain(_) => {
            Status::invalid_argument("Projects request is invalid")
        }
        ProjectApplicationError::VersionConflict { .. } => {
            Status::aborted("Projects record has changed")
        }
        ProjectApplicationError::Port(error) => status_from_projects_port(error),
    }
}

fn status_from_projects_port(error: erpx_projects::PortError) -> Status {
    use erpx_projects::PortError;
    match error {
        PortError::Denied | PortError::ScopeOutsideAuthority | PortError::EntitlementDenied => {
            Status::permission_denied("Projects authority is denied")
        }
        PortError::NotFound => Status::not_found("Projects record was not found"),
        PortError::Conflict { .. }
        | PortError::IdempotencyConflict
        | PortError::DependencyCycle => Status::aborted("Projects record has changed"),
        PortError::NotReady(_) => Status::unavailable("Projects projection is not ready"),
        PortError::Unavailable => Status::unavailable("Projects is unavailable"),
        PortError::Internal => Status::internal("Projects operation failed"),
    }
}

fn status_from_agent_port(error: AgentPortError) -> Status {
    match error {
        AgentPortError::Denied => Status::permission_denied("Agent authority is denied"),
        AgentPortError::NotFound => Status::not_found("Agent record was not found"),
        AgentPortError::VersionConflict | AgentPortError::IdempotencyConflict => {
            Status::aborted("Agent record has changed")
        }
        AgentPortError::BudgetUnavailable => {
            Status::resource_exhausted("Agent budget is unavailable")
        }
        AgentPortError::ProviderDenied => {
            Status::failed_precondition("Agent provider policy denied the request")
        }
        AgentPortError::ProviderRetryable | AgentPortError::ProtectedMaterialUnavailable => {
            Status::unavailable("Agent provider material is unavailable")
        }
        AgentPortError::ProviderPermanent => {
            Status::failed_precondition("Agent provider rejected the request permanently")
        }
        AgentPortError::Persistence | AgentPortError::Internal => {
            Status::internal("Agent query failed")
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerPortGap {
    pub service: &'static str,
    pub rpc: &'static str,
    pub gap_id: &'static str,
    pub reason: &'static str,
}

#[cfg(test)]
pub const OWNER_PORT_GAPS: &[OwnerPortGap] = &[
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "RegisterAgent",
        gap_id: "AGN-CMD-REGISTER-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "AddAgentVersion",
        gap_id: "AGN-CMD-VERSION-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "ApproveAgentVersion",
        gap_id: "AGN-CMD-APPROVE-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "SuspendAgent",
        gap_id: "AGN-CMD-SUSPEND-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "RetireAgent",
        gap_id: "AGN-CMD-RETIRE-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "StartAssignment",
        gap_id: "AGN-CMD-START-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "DelegateAssignment",
        gap_id: "AGN-CMD-DELEGATE-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "RetryAssignment",
        gap_id: "AGN-CMD-RETRY-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "CancelAssignment",
        gap_id: "AGN-CMD-CANCEL-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "RegisterPool",
        gap_id: "AGN-CMD-POOL-REGISTER-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "ReplacePoolMembers",
        gap_id: "AGN-CMD-POOL-MEMBERS-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "RoutePool",
        gap_id: "AGN-CMD-POOL-ROUTE-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "StartPrivateSession",
        gap_id: "AGN-CMD-SESSION-START-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "AppendPrivateSessionMessage",
        gap_id: "AGN-CMD-SESSION-APPEND-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "PublishPrivateSessionMessage",
        gap_id: "AGN-CMD-SESSION-PUBLISH-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "SetPrivateSessionLegalHold",
        gap_id: "AGN-CMD-SESSION-HOLD-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationCommandService",
        rpc: "ExpirePrivateSession",
        gap_id: "AGN-CMD-SESSION-EXPIRE-RECEIPT",
        reason: "owner UoW does not return a target command receipt or replay fact",
    },
    OwnerPortGap {
        service: "AgentOrchestrationQueryService",
        rpc: "GetAgentDefinition",
        gap_id: "AGN-QRY-DEFINITION-PROJECTION",
        reason: "no authorization-aware definition projection query port",
    },
    OwnerPortGap {
        service: "AgentOrchestrationQueryService",
        rpc: "GetAssignment",
        gap_id: "AGN-QRY-ASSIGNMENT-PROJECTION",
        reason: "no authorization-aware assignment projection query port",
    },
    OwnerPortGap {
        service: "AgentOrchestrationQueryService",
        rpc: "GetPool",
        gap_id: "AGN-QRY-POOL-PROJECTION",
        reason: "no authorization-aware pool projection query port",
    },
    OwnerPortGap {
        service: "AgentOrchestrationQueryService",
        rpc: "GetPrivateSession",
        gap_id: "AGN-QRY-SESSION-PROJECTION",
        reason: "no authorization-aware session projection query port and private content must not leak",
    },
    OwnerPortGap {
        service: "ProjectsCommandService",
        rpc: "TransitionProject",
        gap_id: "PRJ-CMD-CLOSE-EVIDENCE-CONTRACT",
        reason: "closed transition carries only a digest and cannot prove the reconciliation counters required by the owner domain",
    },
    OwnerPortGap {
        service: "ProjectsCommandService",
        rpc: "AddNode",
        gap_id: "PRJ-CMD-ADD-NODE-PROMOTION-DIGEST",
        reason: "comment promotion fields omit the exact source revision content digest required by the owner domain",
    },
    OwnerPortGap {
        service: "ProjectsCommandService",
        rpc: "RecordNodeTime",
        gap_id: "PRJ-CMD-RECORD-TIME-EVIDENCE-TYPE",
        reason: "target request omits the evidence owner context and record type required for a stable record reference",
    },
    OwnerPortGap {
        service: "ProjectsCommandService",
        rpc: "RecordNodeCost",
        gap_id: "PRJ-CMD-RECORD-COST-EVIDENCE-TYPE",
        reason: "target request omits the evidence owner context and record type and the owner domain has no category or occurrence-date fields",
    },
    OwnerPortGap {
        service: "ProjectsCommandService",
        rpc: "ReconcileHandoff",
        gap_id: "PRJ-CMD-RECONCILE-HANDOFF-OUTCOME",
        reason: "target request omits owner context, owner record identity, accepted outcome, and reason code required by the owner domain",
    },
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn decision_actor(kind: ActorKind, with_session: bool) -> ActorAttribution {
        ActorAttribution {
            principal_ref: Uuid::from_u128(7).to_string(),
            kind,
            session_id: with_session.then(|| SessionId::try_from_uuid(Uuid::from_u128(8)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }

    #[test]
    fn projects_decision_boundary_accepts_only_direct_authenticated_humans() {
        for kind in [
            ActorKind::Agent,
            ActorKind::Service,
            ActorKind::SystemWorker,
        ] {
            let status = require_authenticated_human_decision(&decision_actor(kind, true))
                .expect_err("non-human actor must be denied");
            assert_eq!(status.code(), tonic::Code::PermissionDenied);
        }
        let no_session = decision_actor(ActorKind::HoldingUser, false);
        assert!(require_authenticated_human_decision(&no_session).is_err());

        let mut represented = decision_actor(ActorKind::PlatformUser, true);
        represented.represented_principal_ref = Some("principal:represented".into());
        assert!(require_authenticated_human_decision(&represented).is_err());

        assert!(
            require_authenticated_human_decision(&decision_actor(ActorKind::HoldingUser, true))
                .is_ok()
        );
        assert_eq!(
            status_from_projects_application(ProjectApplicationError::Domain(
                ProjectDomainError::HandoffAuthorityDenied,
            ))
            .code(),
            tonic::Code::PermissionDenied
        );
    }

    #[test]
    fn owner_port_gap_inventory_is_unique_and_never_describes_success() {
        let mut ids = BTreeSet::new();
        let mut operations = BTreeSet::new();
        for gap in OWNER_PORT_GAPS {
            assert!(ids.insert(gap.gap_id), "duplicate gap id {}", gap.gap_id);
            assert!(
                operations.insert((gap.service, gap.rpc)),
                "duplicate operation {}.{}",
                gap.service,
                gap.rpc
            );
            assert!(!gap.reason.trim().is_empty());
            assert!(!gap.reason.contains("success"));
        }
        assert_eq!(OWNER_PORT_GAPS.len(), 26);
    }

    #[test]
    fn target_parity_is_explicitly_partial_until_owner_ports_exist() {
        let supported = [
            "ProjectsCommandService.CreateProject",
            "ProjectsCommandService.MoveNode",
            "ProjectsCommandService.TransitionNode",
            "ProjectsCommandService.AttachNodeReference",
            "ProjectsCommandService.AddNodeAttachment",
            "ProjectsCommandService.ProposeDependency",
            "ProjectsCommandService.DecideDependency",
            "ProjectsCommandService.CancelDependency",
            "ProjectsCommandService.RevokeDependency",
            "ProjectsCommandService.ProposeHandoff",
            "ProjectsCommandService.ApproveHandoff",
            "ProjectsCommandService.MarkHandoffReady",
            "ProjectsCommandService.MarkHandoffUncertain",
            "ProjectsCommandService.CancelHandoff",
            "ProjectsQueryService.GetProject",
            "ProjectsQueryService.GetProjectGraph",
            "ProjectsQueryService.ListProjects",
            "AgentOrchestrationQueryService.GetRoutingDecision",
            "AgentOrchestrationQueryService.GetInvocationEvidence",
        ];
        assert_eq!(supported.len(), 19);
        assert!(
            OWNER_PORT_GAPS
                .iter()
                .all(|gap| !supported.contains(&format!("{}.{}", gap.service, gap.rpc).as_str()))
        );
    }

    #[test]
    fn owner_port_gap_is_failed_precondition() {
        let status = owner_port_gap("AGN-CMD-REGISTER-RECEIPT");
        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
        assert!(status.message().contains("AGN-CMD-REGISTER-RECEIPT"));
    }
}
