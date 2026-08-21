//! Authenticated gRPC boundary for the future Agent Orchestration and Projects contracts.
//!
//! This module is intentionally fail-closed. It dispatches only operations for
//! which the owner context currently exposes a complete application/query port
//! and a truthful target response. Missing owner ports return
//! `FAILED_PRECONDITION`; they never return an empty or fabricated success.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use erpx_agent_orchestration::{
    AgentAuthorizationPort, AgentDefinitionId, AgentExperienceQueryPort, AgentExperienceService,
    AgentOrchestrationError, AgentOrchestrationService, AgentSessionId, AgentVersionId,
    AppendPrivateSessionMessage, AssignmentId, AssignmentState,
    AuthorizationRequest as AgentAuthorizationRequest, BudgetRequest, BudgetReservationId,
    BudgetSubjects, InvocationEvidenceSummary, InvocationOrigin, OrchestrationAction,
    PayloadManifest, PoolRoutingDecisionId, PoolRoutingSummary, PortError as AgentPortError,
    PrincipalKind as AgentPrincipalKind, ProviderInvocationId, PublishPrivateSessionMessage,
    RootAssignmentReceipt, RootInvocation, SessionContext, SessionMessageId,
    SessionMutationReceipt, StartAgentWork, StartPrivateSession, SuggestionApplicationError,
    SuggestionAuthorizationPort, SuggestionCapability, SuggestionCommand, SuggestionCommandRequest,
    SuggestionEnvelope, SuggestionId, SuggestionOwner, SuggestionProvenance,
    SuggestionProviderKind, SuggestionReceipt, SuggestionRepository, SuggestionService,
    SuggestionStatus, agent_assignment_decision_ref,
};
use erpx_agent_orchestration_sqlx::{
    SqlxAgentExperienceQuery, SqlxAgentExperienceStore, SqlxAgentRegistry, SqlxAgentRuntime,
    SqlxSuggestionRepository,
};
use erpx_collaboration_sqlx::{DirectoryAccess, DirectoryLookup, SqlxItemDirectory};
use erpx_cqrs::{
    CommandReceipt, FreshnessState, IdempotencyKey, ProjectionMetadata, QueryConsistency,
    ReadYourWrites,
};
use erpx_cqrs_sqlx::{ReadPool, WritePool};
use erpx_infrastructure::RoutedDatabaseSide;
use erpx_integration_contracts::{
    FieldPatchConflict, FieldPatchConflictReason, FieldSuggestionEvidence, RecordFieldPatch,
    RecordVersionRef, StableFieldRef, StableRecordRef, SuggestedFieldChange, SuggestedFieldValue,
    catalog::ContextId, record_editable_field_schema, reference::RecordTypeId,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
    DataClassification, EventId, HoldingId, HumanDecisionProof, PrincipalId, QueryId, RecordId,
    SessionId, SystemClock,
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
    erpx::{agent, interface, projects},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FutureOwnerContext {
    AgentOrchestration,
    Projects,
    SalesQuoteSuggestionApply,
    Customer360NoteSuggestionApply,
    ProcurementRequisitionSuggestionApply,
    MaintenanceRequestSuggestionApply,
    MarketingContentSuggestionApply,
}

const SUGGESTION_APPLY_CONTEXTS: [FutureOwnerContext; 5] = [
    FutureOwnerContext::SalesQuoteSuggestionApply,
    FutureOwnerContext::Customer360NoteSuggestionApply,
    FutureOwnerContext::ProcurementRequisitionSuggestionApply,
    FutureOwnerContext::MaintenanceRequestSuggestionApply,
    FutureOwnerContext::MarketingContentSuggestionApply,
];

fn suggestion_apply_context(record_type: &str) -> Option<FutureOwnerContext> {
    match record_type {
        "sales.quote" => Some(FutureOwnerContext::SalesQuoteSuggestionApply),
        "customer-360.note" => Some(FutureOwnerContext::Customer360NoteSuggestionApply),
        "procurement.requisition" => {
            Some(FutureOwnerContext::ProcurementRequisitionSuggestionApply)
        }
        "maintenance.request" => Some(FutureOwnerContext::MaintenanceRequestSuggestionApply),
        "marketing.content" => Some(FutureOwnerContext::MarketingContentSuggestionApply),
        _ => None,
    }
}

impl FutureOwnerContext {
    const fn label(self) -> &'static str {
        match self {
            Self::AgentOrchestration => "Agent Orchestration",
            Self::Projects => "Projects",
            Self::SalesQuoteSuggestionApply => "Sales Quote suggestion apply",
            Self::Customer360NoteSuggestionApply => "Customer 360 Note suggestion apply",
            Self::ProcurementRequisitionSuggestionApply => {
                "Procurement Requisition suggestion apply"
            }
            Self::MaintenanceRequestSuggestionApply => "Maintenance Request suggestion apply",
            Self::MarketingContentSuggestionApply => "Marketing Content suggestion apply",
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

    async fn get_field_suggestion(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        id: SuggestionId,
    ) -> Result<Option<SuggestionEnvelope>, SuggestionApplicationError>;

    async fn list_field_suggestions_by_record(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        source: StableRecordRef,
        page_size: u16,
    ) -> Result<Vec<SuggestionEnvelope>, SuggestionApplicationError>;
}

#[derive(Debug, Clone)]
pub struct AgentApplicationCommandContext {
    command_id: CommandId,
    correlation_id: CorrelationId,
    idempotency_key: String,
    actor: ActorAttribution,
    authority_scope: AuthorityScope,
    audit: AuditContext,
    expected_version: Option<u64>,
}

#[async_trait]
pub trait AgentCommandApplication: Send + Sync + 'static {
    async fn start_assignment(
        &self,
        route: FutureHoldingRoute,
        context: AgentApplicationCommandContext,
        command: StartAgentWork,
    ) -> Result<RootAssignmentReceipt, AgentOrchestrationError>;

    async fn start_private_session(
        &self,
        route: FutureHoldingRoute,
        context: AgentApplicationCommandContext,
        command: StartPrivateSession,
    ) -> Result<SessionMutationReceipt, AgentOrchestrationError>;

    async fn append_private_session_message(
        &self,
        route: FutureHoldingRoute,
        context: AgentApplicationCommandContext,
        command: AppendPrivateSessionMessage,
    ) -> Result<SessionMutationReceipt, AgentOrchestrationError>;

    async fn publish_private_session_message(
        &self,
        route: FutureHoldingRoute,
        context: AgentApplicationCommandContext,
        command: PublishPrivateSessionMessage,
    ) -> Result<SessionMutationReceipt, AgentOrchestrationError>;

    async fn execute_field_suggestion(
        &self,
        route: FutureHoldingRoute,
        context: AgentApplicationCommandContext,
        expected_version: u64,
        command: SuggestionCommand,
    ) -> Result<SuggestionReceipt, SuggestionApplicationError>;
}

#[derive(Debug, Clone)]
pub struct AgentCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> AgentCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
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
            (FutureOwnerContext::SalesQuoteSuggestionApply, FutureAccessMode::Write) => {
                ("sales", "sales.quote.suggestion.apply")
            }
            (FutureOwnerContext::Customer360NoteSuggestionApply, FutureAccessMode::Write) => {
                ("customer-360", "customer_360.note.suggestion.apply")
            }
            (
                FutureOwnerContext::ProcurementRequisitionSuggestionApply,
                FutureAccessMode::Write,
            ) => ("scm", "procurement.requisition.suggestion.apply"),
            (FutureOwnerContext::MaintenanceRequestSuggestionApply, FutureAccessMode::Write) => {
                ("maintenance", "maintenance.request.suggestion.apply")
            }
            (FutureOwnerContext::MarketingContentSuggestionApply, FutureAccessMode::Write) => {
                ("marketing", "marketing.content.suggestion.apply")
            }
            (
                FutureOwnerContext::SalesQuoteSuggestionApply
                | FutureOwnerContext::Customer360NoteSuggestionApply
                | FutureOwnerContext::ProcurementRequisitionSuggestionApply
                | FutureOwnerContext::MaintenanceRequestSuggestionApply
                | FutureOwnerContext::MarketingContentSuggestionApply,
                FutureAccessMode::Read,
            ) => return Err(FutureAuthenticationError::Denied),
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

    async fn suggestion_repository(
        &self,
        route: &FutureHoldingRoute,
    ) -> Result<SqlxSuggestionRepository, SuggestionApplicationError> {
        suggestion_repository(&self.state, route).await
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

    async fn get_field_suggestion(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        id: SuggestionId,
    ) -> Result<Option<SuggestionEnvelope>, SuggestionApplicationError> {
        actor
            .validate()
            .map_err(|_| SuggestionApplicationError::Denied)?;
        let repository = self.suggestion_repository(&route).await?;
        let envelope = repository.load(&authority_scope, id).await?;
        if let Some(envelope) = envelope.as_ref() {
            authorize_suggestion_record(
                &self.state,
                &route,
                &actor,
                &authority_scope,
                &suggestion_source_record(envelope)?,
            )
            .await?;
        }
        Ok(envelope)
    }

    async fn list_field_suggestions_by_record(
        &self,
        route: FutureHoldingRoute,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        source: StableRecordRef,
        page_size: u16,
    ) -> Result<Vec<SuggestionEnvelope>, SuggestionApplicationError> {
        actor
            .validate()
            .map_err(|_| SuggestionApplicationError::Denied)?;
        authorize_suggestion_record(&self.state, &route, &actor, &authority_scope, &source).await?;
        self.suggestion_repository(&route)
            .await?
            .list_by_record(
                &authority_scope,
                source.record_type().as_str(),
                source.id().as_uuid(),
                page_size,
            )
            .await
    }
}

async fn suggestion_repository(
    state: &AppState,
    route: &FutureHoldingRoute,
) -> Result<SqlxSuggestionRepository, SuggestionApplicationError> {
    let holding_id =
        Uuid::parse_str(route.holding_id()).map_err(|_| SuggestionApplicationError::Unavailable)?;
    let pool = state
        .holding_router
        .as_ref()
        .ok_or(SuggestionApplicationError::Unavailable)?
        .route(
            route.holding_id(),
            route.data_profile_key(),
            RoutedDatabaseSide::Write,
        )
        .await
        .map_err(|_| SuggestionApplicationError::Unavailable)?
        .database_pool()
        .clone();
    SqlxSuggestionRepository::try_new(pool, holding_id)
}

fn suggestion_source_record(
    envelope: &SuggestionEnvelope,
) -> Result<StableRecordRef, SuggestionApplicationError> {
    let source = envelope.source();
    let record_type = RecordTypeId::try_new(source.record_kind.clone())
        .map_err(|_| SuggestionApplicationError::Unavailable)?;
    StableRecordRef::try_new(
        RecordId::try_from_uuid(source.record_id)
            .map_err(|_| SuggestionApplicationError::Unavailable)?,
        record_type.descriptor().owner,
        record_type,
    )
    .map_err(|_| SuggestionApplicationError::Unavailable)
}

async fn authorize_suggestion_record(
    state: &AppState,
    route: &FutureHoldingRoute,
    actor: &ActorAttribution,
    authority_scope: &AuthorityScope,
    source: &StableRecordRef,
) -> Result<(), SuggestionApplicationError> {
    let holding_uuid =
        Uuid::parse_str(route.holding_id()).map_err(|_| SuggestionApplicationError::Unavailable)?;
    let holding_id = HoldingId::try_from_uuid(holding_uuid)
        .map_err(|_| SuggestionApplicationError::Unavailable)?;
    let principal_id = Uuid::parse_str(&actor.principal_ref)
        .ok()
        .and_then(|value| PrincipalId::try_from_uuid(value).ok())
        .ok_or(SuggestionApplicationError::Denied)?;
    let pool = state
        .holding_router
        .as_ref()
        .ok_or(SuggestionApplicationError::Unavailable)?
        .route(
            route.holding_id(),
            route.data_profile_key(),
            RoutedDatabaseSide::Read,
        )
        .await
        .map_err(|_| SuggestionApplicationError::Unavailable)?
        .database_pool()
        .clone();
    let access =
        DirectoryAccess::try_from_authorization(holding_id, principal_id, authority_scope.clone())
            .map_err(|_| SuggestionApplicationError::Denied)?;
    match SqlxItemDirectory::new(pool)
        .resolve(&access, source.id(), Utc::now())
        .await
        .map_err(|_| SuggestionApplicationError::Unavailable)?
    {
        DirectoryLookup::Visible { value, .. } if value.record() == source => Ok(()),
        DirectoryLookup::Neutral => Err(SuggestionApplicationError::Denied),
        DirectoryLookup::Visible { .. } => Err(SuggestionApplicationError::Denied),
        DirectoryLookup::Degraded(_) | DirectoryLookup::Unavailable => {
            Err(SuggestionApplicationError::Unavailable)
        }
    }
}

#[derive(Debug, Clone)]
struct AgentBoundaryAccess {
    actor: ActorAttribution,
    authority_scope: AuthorityScope,
}

#[async_trait]
impl AgentAuthorizationPort for AgentBoundaryAccess {
    async fn authorize(
        &self,
        request: AgentAuthorizationRequest<'_>,
    ) -> Result<(), AgentPortError> {
        if request.actor != &self.actor
            || request.authority_scope != &self.authority_scope
            || !self.authority_scope.contains(request.target_scope)
            || !matches!(
                request.action,
                OrchestrationAction::StartAssignment
                    | OrchestrationAction::StartSession
                    | OrchestrationAction::AppendSessionMessage
                    | OrchestrationAction::PublishSession
            )
        {
            return Err(AgentPortError::Denied);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct SuggestionBoundaryAccess {
    actor: ActorAttribution,
    authority_scope: AuthorityScope,
}

#[async_trait]
impl SuggestionAuthorizationPort for SuggestionBoundaryAccess {
    async fn authorize(
        &self,
        scope: &AuthorityScope,
        actor: &ActorAttribution,
        command: &SuggestionCommand,
    ) -> Result<bool, SuggestionApplicationError> {
        if actor != &self.actor
            || scope != &self.authority_scope
            || !self.authority_scope.contains(scope)
        {
            return Ok(false);
        }
        let authenticated_human =
            matches!(actor.kind, ActorKind::PlatformUser | ActorKind::HoldingUser)
                && actor.session_id.is_some()
                && actor.represented_principal_ref.is_none();
        let trusted_producer = matches!(actor.kind, ActorKind::Agent | ActorKind::SystemWorker);
        Ok(match command {
            SuggestionCommand::Request { .. }
            | SuggestionCommand::ApplyFieldPatch { .. }
            | SuggestionCommand::Reject { .. } => authenticated_human,
            SuggestionCommand::Start { .. } | SuggestionCommand::CompleteFieldPatch { .. } => {
                trusted_producer
            }
            SuggestionCommand::Complete { .. }
            | SuggestionCommand::Apply { .. }
            | SuggestionCommand::Supersede { .. }
            | SuggestionCommand::Fail { .. } => false,
        })
    }
}

#[derive(Clone)]
pub(crate) struct AppStateAgentCommandApplication {
    state: AppState,
}

impl AppStateAgentCommandApplication {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }

    async fn write_pool(
        &self,
        route: &FutureHoldingRoute,
    ) -> Result<(sqlx::PgPool, Uuid), AgentOrchestrationError> {
        let holding_id = Uuid::parse_str(route.holding_id())
            .map_err(|_| AgentOrchestrationError::Port(AgentPortError::Persistence))?;
        let pool = self
            .state
            .holding_router
            .as_ref()
            .ok_or(AgentOrchestrationError::Port(AgentPortError::Persistence))?
            .route(
                route.holding_id(),
                route.data_profile_key(),
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| AgentOrchestrationError::Port(AgentPortError::Persistence))?
            .database_pool()
            .clone();
        Ok((pool, holding_id))
    }

    async fn experience_service(
        &self,
        route: &FutureHoldingRoute,
        context: &AgentApplicationCommandContext,
    ) -> Result<ConcreteAgentExperienceService, AgentOrchestrationError> {
        let (pool, holding_id) = self.write_pool(route).await?;
        let store = SqlxAgentExperienceStore::try_new(pool, holding_id)?;
        let access = AgentBoundaryAccess {
            actor: context.actor.clone(),
            authority_scope: context.authority_scope.clone(),
        };
        Ok(AgentExperienceService::new(
            store.clone(),
            store.clone(),
            store,
            access,
            SystemClock,
        ))
    }
}

type ConcreteAgentExperienceService = AgentExperienceService<
    SqlxAgentExperienceStore,
    SqlxAgentExperienceStore,
    SqlxAgentExperienceStore,
    AgentBoundaryAccess,
    SystemClock,
>;

#[async_trait]
impl AgentCommandApplication for AppStateAgentCommandApplication {
    async fn start_assignment(
        &self,
        route: FutureHoldingRoute,
        context: AgentApplicationCommandContext,
        command: StartAgentWork,
    ) -> Result<RootAssignmentReceipt, AgentOrchestrationError> {
        if command.command_id != context.command_id
            || command.correlation_id != context.correlation_id
            || command.invocation.idempotency_key != context.idempotency_key
            || command.budget_subjects.data_profile_key != route.data_profile_key()
            || command.budget_subjects.principal_ref != context.actor.principal_ref
            || command.budget_subjects.root_assignment_ref
                != command.assignment_id.as_uuid().to_string()
            || command.budget_subjects.agent_ref
                != command.agent_definition_id.as_uuid().to_string()
        {
            return Err(AgentOrchestrationError::Port(AgentPortError::Denied));
        }
        let (pool, holding_id) = self.write_pool(&route).await?;
        let registry = SqlxAgentRegistry::try_new(pool.clone(), holding_id)?;
        let runtime = SqlxAgentRuntime::try_new(pool, holding_id)?;
        let access = AgentBoundaryAccess {
            actor: context.actor.clone(),
            authority_scope: context.authority_scope.clone(),
        };
        AgentOrchestrationService::new(
            registry,
            runtime.clone(),
            runtime.clone(),
            runtime,
            access,
            SystemClock,
        )
        .start_work(
            context.actor,
            context.authority_scope,
            context.audit,
            command,
        )
        .await
    }

    async fn start_private_session(
        &self,
        route: FutureHoldingRoute,
        context: AgentApplicationCommandContext,
        command: StartPrivateSession,
    ) -> Result<SessionMutationReceipt, AgentOrchestrationError> {
        validate_session_command_context(
            &context,
            command.command_id,
            command.correlation_id,
            &command.idempotency_key,
        )?;
        self.experience_service(&route, &context)
            .await?
            .start_session(
                context.actor,
                context.authority_scope,
                context.audit,
                command,
            )
            .await
    }

    async fn append_private_session_message(
        &self,
        route: FutureHoldingRoute,
        context: AgentApplicationCommandContext,
        command: AppendPrivateSessionMessage,
    ) -> Result<SessionMutationReceipt, AgentOrchestrationError> {
        validate_session_command_context(
            &context,
            command.command_id,
            command.correlation_id,
            &command.idempotency_key,
        )?;
        self.experience_service(&route, &context)
            .await?
            .append_session_message(
                context.actor,
                context.authority_scope,
                context.audit,
                command,
            )
            .await
    }

    async fn publish_private_session_message(
        &self,
        route: FutureHoldingRoute,
        context: AgentApplicationCommandContext,
        command: PublishPrivateSessionMessage,
    ) -> Result<SessionMutationReceipt, AgentOrchestrationError> {
        validate_session_command_context(
            &context,
            command.command_id,
            command.correlation_id,
            &command.idempotency_key,
        )?;
        self.experience_service(&route, &context)
            .await?
            .publish_session_message(
                context.actor,
                context.authority_scope,
                context.audit,
                command,
            )
            .await
    }

    async fn execute_field_suggestion(
        &self,
        route: FutureHoldingRoute,
        context: AgentApplicationCommandContext,
        expected_version: u64,
        command: SuggestionCommand,
    ) -> Result<SuggestionReceipt, SuggestionApplicationError> {
        if context
            .expected_version
            .is_some_and(|value| value != expected_version)
            || (expected_version == 0 && context.expected_version.is_some())
        {
            return Err(SuggestionApplicationError::VersionConflict);
        }
        let repository = suggestion_repository(&self.state, &route).await?;
        let source = match &command {
            SuggestionCommand::Request { source, .. } => {
                let record_type = RecordTypeId::try_new(source.record_kind.clone())
                    .map_err(|_| SuggestionApplicationError::InvalidRequest)?;
                StableRecordRef::try_new(
                    RecordId::try_from_uuid(source.record_id)
                        .map_err(|_| SuggestionApplicationError::InvalidRequest)?,
                    record_type.descriptor().owner,
                    record_type,
                )
                .map_err(|_| SuggestionApplicationError::InvalidRequest)?
            }
            _ => {
                let envelope = repository
                    .load(&context.authority_scope, command.suggestion_id())
                    .await?
                    .ok_or(SuggestionApplicationError::NotFound)?;
                suggestion_source_record(&envelope)?
            }
        };
        authorize_suggestion_record(
            &self.state,
            &route,
            &context.actor,
            &context.authority_scope,
            &source,
        )
        .await?;
        SuggestionService::new(
            repository,
            SuggestionBoundaryAccess {
                actor: context.actor.clone(),
                authority_scope: context.authority_scope.clone(),
            },
        )
        .execute(SuggestionCommandRequest {
            command_id: context.command_id.as_uuid(),
            idempotency_key: context.idempotency_key,
            scope: context.authority_scope,
            actor: context.actor,
            expected_version,
            command,
            at: Utc::now(),
        })
        .await
    }
}

fn validate_session_command_context(
    context: &AgentApplicationCommandContext,
    command_id: CommandId,
    correlation_id: CorrelationId,
    idempotency_key: &str,
) -> Result<(), AgentOrchestrationError> {
    if command_id == context.command_id
        && correlation_id == context.correlation_id
        && idempotency_key == context.idempotency_key
    {
        Ok(())
    } else {
        Err(AgentOrchestrationError::Port(AgentPortError::Denied))
    }
}

pub(crate) type AgentCommandTonicService =
    agent::agent_orchestration_command_service_server::AgentOrchestrationCommandServiceServer<
        AgentCommandRpc<AppStateAgentCommandApplication, AppStateFutureAuthenticator>,
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
    let agent_commands = Arc::new(AppStateAgentCommandApplication::new(state.clone()));
    let agent_queries = Arc::new(AppStateAgentQueryApplication::new(state));
    FutureAgentProjectsTonicServices {
        agent_command: AgentCommandTonicService::new(AgentCommandRpc::new(
            agent_commands,
            authenticator.clone(),
        )),
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
impl<A, G> AgentOrchestrationCommandService for AgentCommandRpc<A, G>
where
    A: AgentCommandApplication,
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
    async fn start_assignment(
        &self,
        request: Request<agent::StartAgentAssignmentRequest>,
    ) -> Result<Response<agent::AgentMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        if !input.human_confirmed {
            return Err(Status::failed_precondition(
                "Agent assignment requires an explicit human confirmation",
            ));
        }
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context.clone(),
            FutureOwnerContext::AgentOrchestration,
        )
        .await?;
        let command = decode_start_agent_work(&input, &authenticated.agent_context)?;
        let receipt = self
            .application
            .start_assignment(
                authenticated.holding_route,
                authenticated.agent_context,
                command,
            )
            .await
            .map_err(status_from_agent_application)?;
        Ok(Response::new(encode_agent_assignment_mutation(receipt)))
    }
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
    async fn start_private_session(
        &self,
        request: Request<agent::StartPrivateAgentSessionRequest>,
    ) -> Result<Response<agent::AgentMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context.clone(),
            FutureOwnerContext::AgentOrchestration,
        )
        .await?;
        let command = decode_start_private_session(&input, &authenticated.agent_context)?;
        let receipt = self
            .application
            .start_private_session(
                authenticated.holding_route,
                authenticated.agent_context,
                command,
            )
            .await
            .map_err(status_from_agent_application)?;
        Ok(Response::new(encode_agent_session_mutation(receipt)))
    }

    async fn append_private_session_message(
        &self,
        request: Request<agent::AppendPrivateAgentSessionMessageRequest>,
    ) -> Result<Response<agent::AgentMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context.clone(),
            FutureOwnerContext::AgentOrchestration,
        )
        .await?;
        let command = decode_append_private_session_message(&input, &authenticated.agent_context)?;
        let receipt = self
            .application
            .append_private_session_message(
                authenticated.holding_route,
                authenticated.agent_context,
                command,
            )
            .await
            .map_err(status_from_agent_application)?;
        Ok(Response::new(encode_agent_session_mutation(receipt)))
    }

    async fn publish_private_session_message(
        &self,
        request: Request<agent::PublishPrivateAgentSessionMessageRequest>,
    ) -> Result<Response<agent::AgentMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context.clone(),
            FutureOwnerContext::AgentOrchestration,
        )
        .await?;
        let command = decode_publish_private_session_message(&input, &authenticated.agent_context)?;
        let receipt = self
            .application
            .publish_private_session_message(
                authenticated.holding_route,
                authenticated.agent_context,
                command,
            )
            .await
            .map_err(status_from_agent_application)?;
        Ok(Response::new(encode_agent_session_mutation(receipt)))
    }

    async fn request_field_suggestion(
        &self,
        request: Request<agent::RequestFieldSuggestionRequest>,
    ) -> Result<Response<agent::FieldSuggestionMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context.clone(),
            FutureOwnerContext::AgentOrchestration,
        )
        .await?;
        let source = decode_suggestion_record_ref(input.source)?;
        let suggestion_id = decode_suggestion_id(&input.suggestion_id)?;
        let provenance = input
            .provenance
            .ok_or_else(|| invalid("provenance is required"))?;
        let local_development = provenance.local_development;
        let command = SuggestionCommand::Request {
            suggestion_id,
            capability: SuggestionCapability::RecordFieldPatch {
                owner: source.owner_context(),
            },
            source: erpx_agent_orchestration::SuggestionSource {
                record_kind: source.record_type().as_str().to_owned(),
                record_id: source.id().as_uuid(),
                record_version: input.source_version,
            },
            policy_version: input.policy_version,
            payload_digest: decode_sha256(&input.payload_digest_sha256, "payload_digest_sha256")?,
            provenance: decode_suggestion_provenance(provenance)?,
            local_development,
        };
        let context = authenticated.agent_context;
        let receipt = self
            .application
            .execute_field_suggestion(authenticated.holding_route, context.clone(), 0, command)
            .await
            .map_err(status_from_suggestion_application)?;
        Ok(Response::new(encode_field_suggestion_mutation(
            &context, receipt,
        )))
    }

    async fn start_field_suggestion(
        &self,
        request: Request<agent::StartFieldSuggestionRequest>,
    ) -> Result<Response<agent::FieldSuggestionMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context.clone(),
            FutureOwnerContext::AgentOrchestration,
        )
        .await?;
        let command = SuggestionCommand::Start {
            suggestion_id: decode_suggestion_id(&input.suggestion_id)?,
        };
        let context = authenticated.agent_context;
        let receipt = self
            .application
            .execute_field_suggestion(
                authenticated.holding_route,
                context.clone(),
                input.expected_version,
                command,
            )
            .await
            .map_err(status_from_suggestion_application)?;
        Ok(Response::new(encode_field_suggestion_mutation(
            &context, receipt,
        )))
    }

    async fn complete_field_suggestion(
        &self,
        request: Request<agent::CompleteFieldSuggestionRequest>,
    ) -> Result<Response<agent::FieldSuggestionMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context.clone(),
            FutureOwnerContext::AgentOrchestration,
        )
        .await?;
        let command = SuggestionCommand::CompleteFieldPatch {
            suggestion_id: decode_suggestion_id(&input.suggestion_id)?,
            patch: decode_record_field_patch(input.patch)?,
            output_digest: decode_sha256(&input.output_digest_sha256, "output_digest_sha256")?,
        };
        let context = authenticated.agent_context;
        let receipt = self
            .application
            .execute_field_suggestion(
                authenticated.holding_route,
                context.clone(),
                input.expected_version,
                command,
            )
            .await
            .map_err(status_from_suggestion_application)?;
        Ok(Response::new(encode_field_suggestion_mutation(
            &context, receipt,
        )))
    }

    async fn apply_field_suggestion(
        &self,
        request: Request<agent::ApplyFieldSuggestionRequest>,
    ) -> Result<Response<agent::FieldSuggestionMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let owner_context = suggestion_apply_context_from_wire_patch(input.edited_patch.as_ref())?;
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context.clone(),
            owner_context,
        )
        .await?;
        let command = SuggestionCommand::ApplyFieldPatch {
            suggestion_id: decode_suggestion_id(&input.suggestion_id)?,
            edited_patch: decode_record_field_patch(input.edited_patch)?,
            current_source_version: input.current_source_version,
            current_payload_digest: decode_sha256(
                &input.current_payload_digest_sha256,
                "current_payload_digest_sha256",
            )?,
            conflicts: input
                .conflicts
                .into_iter()
                .map(decode_field_patch_conflict)
                .collect::<Result<_, _>>()?,
        };
        let context = authenticated.agent_context;
        let receipt = self
            .application
            .execute_field_suggestion(
                authenticated.holding_route,
                context.clone(),
                input.expected_version,
                command,
            )
            .await
            .map_err(status_from_suggestion_application)?;
        Ok(Response::new(encode_field_suggestion_mutation(
            &context, receipt,
        )))
    }

    async fn reject_field_suggestion(
        &self,
        request: Request<agent::RejectFieldSuggestionRequest>,
    ) -> Result<Response<agent::FieldSuggestionMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let authenticated = authenticate_command(
            self.authenticator.as_ref(),
            transport,
            input.context.clone(),
            FutureOwnerContext::AgentOrchestration,
        )
        .await?;
        let command = SuggestionCommand::Reject {
            suggestion_id: decode_suggestion_id(&input.suggestion_id)?,
        };
        let context = authenticated.agent_context;
        let receipt = self
            .application
            .execute_field_suggestion(
                authenticated.holding_route,
                context.clone(),
                input.expected_version,
                command,
            )
            .await
            .map_err(status_from_suggestion_application)?;
        Ok(Response::new(encode_field_suggestion_mutation(
            &context, receipt,
        )))
    }
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

    async fn get_field_suggestion(
        &self,
        request: Request<agent::GetFieldSuggestionRequest>,
    ) -> Result<Response<agent::GetFieldSuggestionResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let query_context = input
            .context
            .clone()
            .ok_or_else(|| invalid("query context is required"))?;
        let authenticated = authenticate_query(
            self.authenticator.as_ref(),
            transport.clone(),
            input.context,
            FutureOwnerContext::AgentOrchestration,
        )
        .await?;
        let value = self
            .application
            .get_field_suggestion(
                authenticated.holding_route,
                authenticated.actor,
                authenticated.active_scope,
                decode_suggestion_id(&input.suggestion_id)?,
            )
            .await
            .map_err(status_from_suggestion_application)?;
        let can_reject = authenticate_query_write_capability(
            self.authenticator.as_ref(),
            &transport,
            &query_context,
            FutureOwnerContext::AgentOrchestration,
        )
        .await;
        let can_apply = match value
            .as_ref()
            .and_then(|suggestion| suggestion.field_patch())
            .and_then(|patch| {
                suggestion_apply_context(patch.source.record().record_type().as_str())
            }) {
            Some(owner_context) => {
                authenticate_query_write_capability(
                    self.authenticator.as_ref(),
                    &transport,
                    &query_context,
                    owner_context,
                )
                .await
            }
            None => false,
        };
        Ok(Response::new(agent::GetFieldSuggestionResponse {
            suggestion: value
                .as_ref()
                .map(|value| encode_field_suggestion(value, can_apply, can_reject)),
            // This authority-side read has no rebuildable read checkpoint. It
            // is therefore never advertised as a current projection.
            projection: None,
            authority_current: true,
        }))
    }

    async fn list_field_suggestions_by_record(
        &self,
        request: Request<agent::ListFieldSuggestionsByRecordRequest>,
    ) -> Result<Response<agent::ListFieldSuggestionsByRecordResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let query_context = input
            .context
            .clone()
            .ok_or_else(|| invalid("query context is required"))?;
        let authenticated = authenticate_query(
            self.authenticator.as_ref(),
            transport.clone(),
            input.context,
            FutureOwnerContext::AgentOrchestration,
        )
        .await?;
        let source = decode_suggestion_record_ref(input.source)?;
        let page_size = u16::try_from(input.page_size)
            .ok()
            .filter(|value| (1..=100).contains(value))
            .ok_or_else(|| invalid("page_size must be between 1 and 100"))?;
        let values = self
            .application
            .list_field_suggestions_by_record(
                authenticated.holding_route,
                authenticated.actor,
                authenticated.active_scope,
                source,
                page_size,
            )
            .await
            .map_err(status_from_suggestion_application)?;
        let can_reject = authenticate_query_write_capability(
            self.authenticator.as_ref(),
            &transport,
            &query_context,
            FutureOwnerContext::AgentOrchestration,
        )
        .await;
        let mut apply_contexts = BTreeSet::new();
        for owner_context in SUGGESTION_APPLY_CONTEXTS {
            if values.iter().any(|suggestion| {
                suggestion.field_patch().and_then(|patch| {
                    suggestion_apply_context(patch.source.record().record_type().as_str())
                }) == Some(owner_context)
            }) && authenticate_query_write_capability(
                self.authenticator.as_ref(),
                &transport,
                &query_context,
                owner_context,
            )
            .await
            {
                apply_contexts.insert(owner_context);
            }
        }
        Ok(Response::new(agent::ListFieldSuggestionsByRecordResponse {
            suggestions: values
                .iter()
                .map(|value| {
                    let can_apply = value
                        .field_patch()
                        .and_then(|patch| {
                            suggestion_apply_context(patch.source.record().record_type().as_str())
                        })
                        .is_some_and(|context| apply_contexts.contains(&context));
                    encode_field_suggestion(value, can_apply, can_reject)
                })
                .collect(),
            projection: None,
            authority_current: true,
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
    agent_context: AgentApplicationCommandContext,
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
    let agent_context = AgentApplicationCommandContext {
        command_id: projects_context.command_id,
        correlation_id: projects_context.correlation_id,
        idempotency_key: projects_context.idempotency_key.as_str().to_owned(),
        actor: projects_context.actor.clone(),
        authority_scope: projects_context.authority_scope.clone(),
        audit: projects_context.audit.clone(),
        expected_version: projects_context.expected_version,
    };
    Ok(AuthenticatedCommand {
        holding_route: authenticated.holding_route,
        active_scope: canonical_scope,
        agent_context,
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

async fn authenticate_query_write_capability<G: FutureAgentProjectsAuthenticator>(
    authenticator: &G,
    transport_metadata: &MetadataMap,
    context: &common::QueryContext,
    owner_context: FutureOwnerContext,
) -> bool {
    let Ok(presented_actor) = decode_actor(context.actor.clone()) else {
        return false;
    };
    let Some(scope) = context.active_scope.clone() else {
        return false;
    };
    let Ok(presented_scope) = decode_scope(scope) else {
        return false;
    };
    authenticator
        .authenticate(FutureAuthenticationRequest {
            transport_metadata: transport_metadata.clone(),
            presented_actor,
            presented_scope,
            subscription_id: None,
            mfa_code: None,
            owner_context,
            access_mode: FutureAccessMode::Write,
        })
        .await
        .is_ok()
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

fn decode_suggestion_id(value: &str) -> Result<SuggestionId, Status> {
    SuggestionId::try_from_uuid(parse_uuid(value, "suggestion_id")?)
        .map_err(|_| invalid("suggestion_id is invalid"))
}

fn decode_suggestion_record_ref(
    value: Option<interface::RecordRef>,
) -> Result<StableRecordRef, Status> {
    let value = value.ok_or_else(|| invalid("record reference is required"))?;
    let record_type =
        RecordTypeId::try_new(value.record_type).map_err(|_| invalid("record_type is invalid"))?;
    let owner = decode_context_id(&value.owner_context)?;
    StableRecordRef::try_new(
        RecordId::try_from_uuid(parse_uuid(&value.id, "record_id")?)
            .map_err(|_| invalid("record_id is invalid"))?,
        owner,
        record_type,
    )
    .map_err(|_| invalid("record reference is invalid"))
}

fn encode_suggestion_record_ref(value: &StableRecordRef) -> interface::RecordRef {
    interface::RecordRef {
        id: value.id().to_string(),
        owner_context: value.owner_context().slug().to_owned(),
        record_type: value.record_type().as_str().to_owned(),
    }
}

fn decode_sha256(value: &[u8], field: &'static str) -> Result<String, Status> {
    if value.len() != 32 {
        return Err(invalid(field));
    }
    Ok(hex::encode(value))
}

fn encode_sha256(value: &str) -> Vec<u8> {
    hex::decode(value).unwrap_or_default()
}

fn decode_suggestion_provenance(
    value: agent::FieldSuggestionProvenance,
) -> Result<SuggestionProvenance, Status> {
    let provider_kind = match value.provider_kind.trim() {
        "fixture" => SuggestionProviderKind::Fixture,
        "ollama" => SuggestionProviderKind::Ollama,
        "open_ai_compatible" => SuggestionProviderKind::OpenAiCompatible,
        _ => return Err(invalid("provider_kind is invalid")),
    };
    Ok(SuggestionProvenance {
        provider_kind,
        provider_ref: value.provider_ref,
        model_ref: value.model_ref,
        prompt_digest: decode_sha256(&value.prompt_digest_sha256, "prompt_digest_sha256")?,
        invocation_evidence_ref: value.invocation_evidence_ref,
    })
}

fn suggestion_apply_context_from_wire_patch(
    value: Option<&agent::RecordFieldPatch>,
) -> Result<FutureOwnerContext, Status> {
    let value = value.ok_or_else(|| invalid("edited_patch is required"))?;
    let source = value
        .source
        .as_ref()
        .ok_or_else(|| invalid("field patch source is required"))?;
    let schema = record_editable_field_schema(&source.record_type)
        .filter(|schema| {
            schema.supports_agent_patches()
                && u64::from(schema.version) == value.schema_version
                && schema.owner.slug() == source.owner_context
        })
        .ok_or_else(|| invalid("field patch owner schema is unavailable"))?;
    if value.changes.is_empty()
        || value.changes.iter().any(|change| {
            schema
                .field(&change.field_id)
                .is_none_or(|field| field.human_apply_permission.trim().is_empty())
        })
    {
        return Err(invalid("field patch apply permission is unavailable"));
    }
    let context = suggestion_apply_context(&source.record_type)
        .ok_or_else(|| invalid("field patch owner apply route is unavailable"))?;
    let expected_permission = match context {
        FutureOwnerContext::SalesQuoteSuggestionApply => "sales.quote.suggestion.apply",
        FutureOwnerContext::Customer360NoteSuggestionApply => "customer_360.note.suggestion.apply",
        FutureOwnerContext::ProcurementRequisitionSuggestionApply => {
            "procurement.requisition.suggestion.apply"
        }
        FutureOwnerContext::MaintenanceRequestSuggestionApply => {
            "maintenance.request.suggestion.apply"
        }
        FutureOwnerContext::MarketingContentSuggestionApply => "marketing.content.suggestion.apply",
        FutureOwnerContext::AgentOrchestration | FutureOwnerContext::Projects => {
            return Err(invalid("field patch owner apply route is unavailable"));
        }
    };
    if value.changes.iter().any(|change| {
        schema
            .field(&change.field_id)
            .is_none_or(|field| field.human_apply_permission != expected_permission)
    }) {
        return Err(invalid("field patch apply permission is inconsistent"));
    }
    Ok(context)
}

fn decode_record_field_patch(
    value: Option<agent::RecordFieldPatch>,
) -> Result<RecordFieldPatch, Status> {
    let value = value.ok_or_else(|| invalid("record field patch is required"))?;
    let source_record = decode_suggestion_record_ref(value.source)?;
    let source = RecordVersionRef::try_new(source_record, value.source_version)
        .map_err(|_| invalid("source_version is invalid"))?;
    let changes = value
        .changes
        .into_iter()
        .map(decode_suggested_field_change)
        .collect::<Result<Vec<_>, _>>()?;
    let patch = RecordFieldPatch {
        source,
        schema_version: value.schema_version,
        changes,
    };
    patch
        .validate()
        .map_err(|_| invalid("record field patch is invalid"))?;
    Ok(patch)
}

fn decode_suggested_field_change(
    value: agent::SuggestedFieldChange,
) -> Result<SuggestedFieldChange, Status> {
    let record = decode_suggestion_record_ref(value.record)?;
    let field = StableFieldRef::try_new(record, value.field_id, value.field_schema_version)
        .map_err(|_| invalid("field reference is invalid"))?;
    let proposed_value = value
        .proposed_value
        .and_then(|value| value.value)
        .ok_or_else(|| invalid("proposed_value is required"))?;
    let proposed_value = match proposed_value {
        agent::suggested_field_value::Value::Unset(_) => SuggestedFieldValue::Unset,
        agent::suggested_field_value::Value::Text(value) => SuggestedFieldValue::Text(value),
        agent::suggested_field_value::Value::Integer(value) => SuggestedFieldValue::Integer(value),
        agent::suggested_field_value::Value::Boolean(value) => SuggestedFieldValue::Boolean(value),
        agent::suggested_field_value::Value::Date(value) => SuggestedFieldValue::Date(
            NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
                .map_err(|_| invalid("suggested date is invalid"))?,
        ),
        agent::suggested_field_value::Value::DateTime(value) => SuggestedFieldValue::DateTime(
            DateTime::parse_from_rfc3339(value.trim())
                .map_err(|_| invalid("suggested date_time is invalid"))?
                .with_timezone(&Utc),
        ),
        agent::suggested_field_value::Value::Choice(value) => SuggestedFieldValue::Choice(value),
        agent::suggested_field_value::Value::Money(value) => SuggestedFieldValue::Money {
            amount_minor: value.amount_minor,
            currency: value.currency,
        },
    };
    let confidence_basis_points = u16::try_from(value.confidence_basis_points)
        .map_err(|_| invalid("confidence_basis_points is invalid"))?;
    let evidence = value
        .evidence
        .into_iter()
        .map(decode_field_suggestion_evidence)
        .collect::<Result<_, _>>()?;
    Ok(SuggestedFieldChange {
        field,
        previous_value_digest: decode_sha256(
            &value.previous_value_digest_sha256,
            "previous_value_digest_sha256",
        )?,
        proposed_value,
        confidence_basis_points,
        evidence,
    })
}

fn decode_field_suggestion_evidence(
    value: agent::FieldSuggestionEvidence,
) -> Result<FieldSuggestionEvidence, Status> {
    match value
        .evidence
        .ok_or_else(|| invalid("field suggestion evidence is required"))?
    {
        agent::field_suggestion_evidence::Evidence::Record(value) => {
            Ok(FieldSuggestionEvidence::Record {
                record: RecordVersionRef::try_new(
                    decode_suggestion_record_ref(value.record)?,
                    value.version,
                )
                .map_err(|_| invalid("evidence record version is invalid"))?,
            })
        }
        agent::field_suggestion_evidence::Evidence::ProtectedMaterial(value) => {
            Ok(FieldSuggestionEvidence::ProtectedMaterial {
                evidence_ref: value.evidence_ref,
                digest: decode_sha256(&value.digest_sha256, "evidence digest is invalid")?,
            })
        }
    }
}

fn decode_field_patch_conflict(
    value: agent::FieldPatchConflict,
) -> Result<FieldPatchConflict, Status> {
    let field = StableFieldRef::try_new(
        decode_suggestion_record_ref(value.record)?,
        value.field_id,
        value.field_schema_version,
    )
    .map_err(|_| invalid("conflict field is invalid"))?;
    let reason = match agent::field_patch_conflict::Reason::try_from(value.reason).ok() {
        Some(agent::field_patch_conflict::Reason::RecordVersionChanged) => {
            FieldPatchConflictReason::RecordVersionChanged
        }
        Some(agent::field_patch_conflict::Reason::FieldValueChanged) => {
            FieldPatchConflictReason::FieldValueChanged
        }
        Some(agent::field_patch_conflict::Reason::SchemaChanged) => {
            FieldPatchConflictReason::SchemaChanged
        }
        Some(agent::field_patch_conflict::Reason::AuthorizationChanged) => {
            FieldPatchConflictReason::AuthorizationChanged
        }
        _ => return Err(invalid("conflict reason is invalid")),
    };
    Ok(FieldPatchConflict {
        field,
        reason,
        current_record_version: value.current_record_version,
    })
}

fn encode_field_suggestion_mutation(
    context: &AgentApplicationCommandContext,
    receipt: SuggestionReceipt,
) -> agent::FieldSuggestionMutationResponse {
    agent::FieldSuggestionMutationResponse {
        receipt: Some(common::CommandReceipt {
            command_id: context.command_id.to_string(),
            correlation_id: context.correlation_id.to_string(),
            aggregate: Some(common::AggregateVersion {
                context: "agent_orchestration".into(),
                aggregate_type: "suggestion".into(),
                aggregate_id: receipt.envelope.id().as_uuid().to_string(),
                version: receipt.envelope.version(),
            }),
            committed_events: vec![common::EventPosition {
                event_id: receipt.event_id.to_string(),
                global_sequence: receipt.global_sequence,
            }],
            committed_at: receipt.committed_at.to_rfc3339(),
            replayed: receipt.replayed,
        }),
        suggestion: Some(encode_field_suggestion(&receipt.envelope, false, false)),
    }
}

fn encode_field_suggestion(
    value: &SuggestionEnvelope,
    can_apply: bool,
    can_reject: bool,
) -> agent::FieldSuggestionProjection {
    agent::FieldSuggestionProjection {
        suggestion_id: value.id().as_uuid().to_string(),
        aggregate_version: value.version(),
        status: match value.status() {
            SuggestionStatus::Queued => agent::FieldSuggestionStatus::Queued,
            SuggestionStatus::Running => agent::FieldSuggestionStatus::Running,
            SuggestionStatus::Ready => agent::FieldSuggestionStatus::Ready,
            SuggestionStatus::Conflicted => agent::FieldSuggestionStatus::Conflicted,
            SuggestionStatus::Applied => agent::FieldSuggestionStatus::Applied,
            SuggestionStatus::Rejected => agent::FieldSuggestionStatus::Rejected,
            SuggestionStatus::Superseded => agent::FieldSuggestionStatus::Superseded,
            SuggestionStatus::Failed => agent::FieldSuggestionStatus::Failed,
        } as i32,
        owner_context: match value.owner() {
            SuggestionOwner::Sales => "sales".into(),
            SuggestionOwner::Finance => "finance".into(),
            SuggestionOwner::RecordOwner(owner) => owner.slug().into(),
        },
        patch: value.field_patch().map(encode_record_field_patch),
        conflicts: value
            .field_patch_conflicts()
            .iter()
            .map(encode_field_patch_conflict)
            .collect(),
        policy_version: value.policy_version(),
        source_payload_digest_sha256: encode_sha256(value.payload_digest()),
        output_digest_sha256: value.output_digest().map(encode_sha256),
        provider_label: value.provenance().display_label().into(),
        human_review: value
            .human_field_patch_review()
            .map(|review| agent::HumanFieldPatchReview {
                reviewer_principal_ref: review.reviewer_principal_ref.clone(),
                reviewer_session_id: review.reviewer_session_id.to_string(),
                reviewed_at: review.reviewed_at.to_rfc3339(),
                edited_patch_digest_sha256: encode_sha256(&review.edited_patch_digest),
            }),
        requested_by: value.requested_by().into(),
        created_at: value.created_at().to_rfc3339(),
        updated_at: value.updated_at().to_rfc3339(),
        capabilities: Some(agent::FieldSuggestionCapabilities {
            can_apply: can_apply && value.status() == SuggestionStatus::Ready,
            can_reject: can_reject && value.status() == SuggestionStatus::Ready,
        }),
    }
}

fn encode_record_field_patch(value: &RecordFieldPatch) -> agent::RecordFieldPatch {
    agent::RecordFieldPatch {
        source: Some(encode_suggestion_record_ref(value.source.record())),
        source_version: value.source.version(),
        schema_version: value.schema_version,
        changes: value
            .changes
            .iter()
            .map(encode_suggested_field_change)
            .collect(),
    }
}

fn encode_suggested_field_change(value: &SuggestedFieldChange) -> agent::SuggestedFieldChange {
    use agent::suggested_field_value::Value;
    let proposed_value = match &value.proposed_value {
        SuggestedFieldValue::Unset => Value::Unset(agent::suggested_field_value::Unset {}),
        SuggestedFieldValue::Text(value) => Value::Text(value.clone()),
        SuggestedFieldValue::Integer(value) => Value::Integer(*value),
        SuggestedFieldValue::Boolean(value) => Value::Boolean(*value),
        SuggestedFieldValue::Date(value) => Value::Date(value.format("%Y-%m-%d").to_string()),
        SuggestedFieldValue::DateTime(value) => Value::DateTime(value.to_rfc3339()),
        SuggestedFieldValue::Choice(value) => Value::Choice(value.clone()),
        SuggestedFieldValue::Money {
            amount_minor,
            currency,
        } => Value::Money(agent::MoneyFieldValue {
            amount_minor: *amount_minor,
            currency: currency.clone(),
        }),
    };
    agent::SuggestedFieldChange {
        record: Some(encode_suggestion_record_ref(value.field.record())),
        field_id: value.field.field_id().into(),
        field_schema_version: value.field.schema_version(),
        previous_value_digest_sha256: encode_sha256(&value.previous_value_digest),
        proposed_value: Some(agent::SuggestedFieldValue {
            value: Some(proposed_value),
        }),
        confidence_basis_points: u32::from(value.confidence_basis_points),
        evidence: value
            .evidence
            .iter()
            .map(encode_field_suggestion_evidence)
            .collect(),
    }
}

fn encode_field_suggestion_evidence(
    value: &FieldSuggestionEvidence,
) -> agent::FieldSuggestionEvidence {
    use agent::field_suggestion_evidence::Evidence;
    let evidence = match value {
        FieldSuggestionEvidence::Record { record } => {
            Evidence::Record(agent::field_suggestion_evidence::RecordEvidence {
                record: Some(encode_suggestion_record_ref(record.record())),
                version: record.version(),
            })
        }
        FieldSuggestionEvidence::ProtectedMaterial {
            evidence_ref,
            digest,
        } => Evidence::ProtectedMaterial(
            agent::field_suggestion_evidence::ProtectedMaterialEvidence {
                evidence_ref: evidence_ref.clone(),
                digest_sha256: encode_sha256(digest),
            },
        ),
    };
    agent::FieldSuggestionEvidence {
        evidence: Some(evidence),
    }
}

fn encode_field_patch_conflict(value: &FieldPatchConflict) -> agent::FieldPatchConflict {
    agent::FieldPatchConflict {
        record: Some(encode_suggestion_record_ref(value.field.record())),
        field_id: value.field.field_id().into(),
        field_schema_version: value.field.schema_version(),
        reason: match value.reason {
            FieldPatchConflictReason::RecordVersionChanged => {
                agent::field_patch_conflict::Reason::RecordVersionChanged
            }
            FieldPatchConflictReason::FieldValueChanged => {
                agent::field_patch_conflict::Reason::FieldValueChanged
            }
            FieldPatchConflictReason::SchemaChanged => {
                agent::field_patch_conflict::Reason::SchemaChanged
            }
            FieldPatchConflictReason::AuthorizationChanged => {
                agent::field_patch_conflict::Reason::AuthorizationChanged
            }
        } as i32,
        current_record_version: value.current_record_version,
    }
}

fn decode_start_agent_work(
    input: &agent::StartAgentAssignmentRequest,
    context: &AgentApplicationCommandContext,
) -> Result<StartAgentWork, Status> {
    if !context.actor.is_human()
        || context.actor.session_id.is_none()
        || context.actor.represented_principal_ref.is_some()
        || context.actor.support_capability_id.is_some()
        || input.initiating_human_ref != context.actor.principal_ref
    {
        return Err(Status::permission_denied(
            "Agent assignment requires the authenticated human actor",
        ));
    }
    let assignment_id =
        AssignmentId::try_from_uuid(parse_uuid(&input.assignment_id, "assignment_id")?)
            .map_err(|_| invalid("assignment_id is invalid"))?;
    let agent_definition_id = AgentDefinitionId::try_from_uuid(parse_uuid(
        &input.agent_definition_id,
        "agent_definition_id",
    )?)
    .map_err(|_| invalid("agent_definition_id is invalid"))?;
    let agent_version_id =
        AgentVersionId::try_from_uuid(parse_uuid(&input.agent_version_id, "agent_version_id")?)
            .map_err(|_| invalid("agent_version_id is invalid"))?;
    let assignment_scope =
        decode_target_scope(input.assignment_scope.clone(), &context.authority_scope)?;
    validate_agent_text(&input.source_action_ref, "source_action_ref", 256)?;
    validate_agent_text(&input.prompt_ref, "prompt_ref", 512)?;
    let origin = match agent::AgentInvocationOrigin::try_from(input.origin).ok() {
        Some(agent::AgentInvocationOrigin::Mention) => InvocationOrigin::Mention,
        Some(agent::AgentInvocationOrigin::Assign) => InvocationOrigin::Assign,
        Some(agent::AgentInvocationOrigin::Run) => InvocationOrigin::Run,
        Some(agent::AgentInvocationOrigin::Compare) => InvocationOrigin::Compare,
        _ => return Err(invalid("origin is invalid")),
    };
    let budget_reservation_id = BudgetReservationId::try_from_uuid(parse_uuid(
        &input.budget_reservation_id,
        "budget_reservation_id",
    )?)
    .map_err(|_| invalid("budget_reservation_id is invalid"))?;
    let subjects = input
        .budget_subjects
        .as_ref()
        .ok_or_else(|| invalid("budget_subjects are required"))?;
    for (value, field) in [
        (&subjects.data_profile_key, "data_profile_key"),
        (&subjects.workspace_ref, "workspace_ref"),
        (&subjects.agent_ref, "agent_ref"),
        (&subjects.principal_ref, "principal_ref"),
        (&subjects.root_assignment_ref, "root_assignment_ref"),
    ] {
        validate_agent_text(value, field, 256)?;
    }
    let budget = input
        .budget_request
        .as_ref()
        .ok_or_else(|| invalid("budget_request is required"))?;
    let queue_priority =
        u16::try_from(budget.queue_priority).map_err(|_| invalid("queue_priority is invalid"))?;
    if budget.estimated_units == 0
        || budget.estimated_cost_micros == 0
        || budget.stop_after_units < budget.estimated_units
    {
        return Err(invalid("budget_request is invalid"));
    }
    let payload = input
        .payload
        .as_ref()
        .ok_or_else(|| invalid("payload is required"))?;
    validate_sha256_hex(&payload.payload_digest, "payload_digest")?;
    if payload.contains_secret {
        return Err(Status::permission_denied(
            "secret-bearing Agent payloads are denied",
        ));
    }
    validate_agent_text(&payload.residency, "residency", 128)?;
    let classification = match agent::DataClassification::try_from(payload.classification).ok() {
        Some(agent::DataClassification::Public) => DataClassification::Public,
        Some(agent::DataClassification::Internal) => DataClassification::Internal,
        Some(agent::DataClassification::Confidential) => DataClassification::Confidential,
        Some(agent::DataClassification::Restricted) => DataClassification::Restricted,
        _ => return Err(invalid("payload classification is invalid")),
    };
    let authorization_checked_at = DateTime::parse_from_rfc3339(&payload.authorization_checked_at)
        .map_err(|_| invalid("authorization_checked_at is invalid"))?
        .with_timezone(&Utc);
    let now = Utc::now();
    if authorization_checked_at > now
        || authorization_checked_at < now - chrono::Duration::minutes(5)
    {
        return Err(Status::failed_precondition(
            "Agent payload authorization is stale",
        ));
    }
    if payload.field_refs.len() > 256 || payload.document_record_ids.len() > 256 {
        return Err(invalid("payload references exceed the limit"));
    }
    let field_refs = payload
        .field_refs
        .iter()
        .map(|value| {
            validate_agent_text(value, "field_ref", 256)?;
            Ok(value.clone())
        })
        .collect::<Result<BTreeSet<_>, Status>>()?;
    if field_refs.len() != payload.field_refs.len() {
        return Err(invalid("field_refs contain duplicates"));
    }
    let document_refs = payload
        .document_record_ids
        .iter()
        .map(|value| {
            RecordId::try_from_uuid(parse_uuid(value, "document_record_id")?)
                .map_err(|_| invalid("document_record_id is invalid"))
        })
        .collect::<Result<BTreeSet<_>, Status>>()?;
    if document_refs.len() != payload.document_record_ids.len() {
        return Err(invalid("document_record_ids contain duplicates"));
    }
    let human_decision = HumanDecisionProof::try_new(
        context.actor.clone(),
        assignment_scope.clone(),
        agent_assignment_decision_ref(assignment_id, context.command_id),
        now,
    )
    .map_err(|_| Status::permission_denied("Agent assignment confirmation is invalid"))?;
    Ok(StartAgentWork {
        command_id: context.command_id,
        correlation_id: context.correlation_id,
        assignment_id,
        agent_definition_id,
        agent_version_id,
        scope: assignment_scope,
        invocation: RootInvocation {
            initiating_principal_ref: context.actor.principal_ref.clone(),
            initiating_principal_kind: AgentPrincipalKind::Human,
            origin,
            source_action_ref: input.source_action_ref.clone(),
            idempotency_key: context.idempotency_key.clone(),
        },
        budget_reservation_id,
        budget_subjects: BudgetSubjects {
            data_profile_key: subjects.data_profile_key.clone(),
            workspace_ref: subjects.workspace_ref.clone(),
            agent_ref: subjects.agent_ref.clone(),
            principal_ref: subjects.principal_ref.clone(),
            root_assignment_ref: subjects.root_assignment_ref.clone(),
        },
        budget_request: BudgetRequest {
            estimated_units: budget.estimated_units,
            estimated_cost_micros: budget.estimated_cost_micros,
            queue_priority,
            stop_after_units: budget.stop_after_units,
        },
        prompt_ref: input.prompt_ref.clone(),
        payload_manifest: PayloadManifest {
            payload_digest: payload.payload_digest.clone(),
            classification,
            field_refs,
            document_refs,
            authorization_checked_at,
            contains_secret: false,
            residency: payload.residency.clone(),
        },
        human_decision,
    })
}

fn validate_agent_text(value: &str, field: &'static str, maximum: usize) -> Result<(), Status> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        Err(invalid(field))
    } else {
        Ok(())
    }
}

fn validate_sha256_hex(value: &str, field: &'static str) -> Result<(), Status> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(invalid(field))
    }
}

fn decode_start_private_session(
    input: &agent::StartPrivateAgentSessionRequest,
    context: &AgentApplicationCommandContext,
) -> Result<StartPrivateSession, Status> {
    if context.expected_version.is_some()
        || !context.actor.is_human()
        || context.actor.session_id.is_none()
        || context.actor.represented_principal_ref.is_some()
        || context.actor.support_capability_id.is_some()
        || input.owner_human_ref != context.actor.principal_ref
    {
        return Err(Status::permission_denied(
            "Private Agent sessions require their authenticated human owner",
        ));
    }
    let session_id = AgentSessionId::try_from_uuid(parse_uuid(&input.session_id, "session_id")?)
        .map_err(|_| invalid("session_id is invalid"))?;
    let scope = decode_target_scope(input.scope.clone(), &context.authority_scope)?;
    let expires_at = DateTime::parse_from_rfc3339(&input.expires_at)
        .map_err(|_| invalid("expires_at is invalid"))?
        .with_timezone(&Utc);
    let record_id = input
        .context_record_id
        .as_deref()
        .map(|value| {
            RecordId::try_from_uuid(parse_uuid(value, "context_record_id")?)
                .map_err(|_| invalid("context_record_id is invalid"))
        })
        .transpose()?;
    let session_context = match (
        record_id,
        input.context_descriptor_version,
        input.context_source_event_position,
    ) {
        (None, None, None) => None,
        (Some(record_id), Some(descriptor_version), Some(source_event_position))
            if descriptor_version > 0 && source_event_position > 0 =>
        {
            Some(SessionContext {
                record_id,
                descriptor_version,
                source_event_position,
            })
        }
        _ => return Err(invalid("private session context is incomplete")),
    };
    Ok(StartPrivateSession {
        command_id: context.command_id,
        correlation_id: context.correlation_id,
        session_id,
        scope,
        owner_human_ref: context.actor.principal_ref.clone(),
        context: session_context,
        expires_at,
        idempotency_key: context.idempotency_key.clone(),
    })
}

fn decode_append_private_session_message(
    input: &agent::AppendPrivateAgentSessionMessageRequest,
    context: &AgentApplicationCommandContext,
) -> Result<AppendPrivateSessionMessage, Status> {
    let expected_version = context
        .expected_version
        .filter(|version| *version > 0)
        .ok_or_else(|| invalid("expected_aggregate_version is required"))?;
    if input.author_ref != context.actor.principal_ref {
        return Err(Status::permission_denied(
            "Private session message author does not match the authenticated actor",
        ));
    }
    let author_kind = match agent::SessionMessageAuthorKind::try_from(input.author_kind).ok() {
        Some(agent::SessionMessageAuthorKind::Human) if context.actor.is_human() => {
            AgentPrincipalKind::Human
        }
        Some(agent::SessionMessageAuthorKind::Agent) if context.actor.kind == ActorKind::Agent => {
            AgentPrincipalKind::Agent
        }
        _ => {
            return Err(Status::permission_denied(
                "Private session message actor kind is invalid",
            ));
        }
    };
    validate_agent_text(&input.encrypted_payload_ref, "encrypted_payload_ref", 512)?;
    validate_sha256_hex(&input.content_digest, "content_digest")?;
    Ok(AppendPrivateSessionMessage {
        command_id: context.command_id,
        correlation_id: context.correlation_id,
        session_id: AgentSessionId::try_from_uuid(parse_uuid(&input.session_id, "session_id")?)
            .map_err(|_| invalid("session_id is invalid"))?,
        expected_version,
        message_id: SessionMessageId::try_from_uuid(parse_uuid(&input.message_id, "message_id")?)
            .map_err(|_| invalid("message_id is invalid"))?,
        author_kind,
        author_ref: context.actor.principal_ref.clone(),
        encrypted_payload_ref: input.encrypted_payload_ref.clone(),
        content_digest: input.content_digest.clone(),
        idempotency_key: context.idempotency_key.clone(),
    })
}

fn decode_publish_private_session_message(
    input: &agent::PublishPrivateAgentSessionMessageRequest,
    context: &AgentApplicationCommandContext,
) -> Result<PublishPrivateSessionMessage, Status> {
    let expected_version = context
        .expected_version
        .filter(|version| *version > 0)
        .ok_or_else(|| invalid("expected_aggregate_version is required"))?;
    if !context.actor.is_human()
        || context.actor.session_id.is_none()
        || context.actor.represented_principal_ref.is_some()
        || context.actor.support_capability_id.is_some()
        || input.approving_human_ref != context.actor.principal_ref
        || !input.audience_reauthorized
    {
        return Err(Status::permission_denied(
            "Publishing Agent output requires direct human review and audience reauthorization",
        ));
    }
    Ok(PublishPrivateSessionMessage {
        command_id: context.command_id,
        correlation_id: context.correlation_id,
        session_id: AgentSessionId::try_from_uuid(parse_uuid(&input.session_id, "session_id")?)
            .map_err(|_| invalid("session_id is invalid"))?,
        expected_version,
        message_id: SessionMessageId::try_from_uuid(parse_uuid(&input.message_id, "message_id")?)
            .map_err(|_| invalid("message_id is invalid"))?,
        approving_human_ref: context.actor.principal_ref.clone(),
        audience_reauthorized: true,
        publication_anchor_ref: parse_uuid(
            &input.publication_anchor_ref,
            "publication_anchor_ref",
        )?,
        idempotency_key: context.idempotency_key.clone(),
    })
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

fn encode_agent_assignment_mutation(
    receipt: RootAssignmentReceipt,
) -> agent::AgentMutationResponse {
    let assignment = &receipt.assignment;
    agent::AgentMutationResponse {
        receipt: Some(common::CommandReceipt {
            command_id: receipt.command_id.to_string(),
            correlation_id: receipt.correlation_id.to_string(),
            aggregate: Some(common::AggregateVersion {
                context: "agent_orchestration".into(),
                aggregate_type: "assignment".into(),
                aggregate_id: assignment.id().as_uuid().to_string(),
                version: assignment.version(),
            }),
            committed_events: vec![common::EventPosition {
                event_id: receipt.event_id.to_string(),
                global_sequence: receipt.global_sequence,
            }],
            committed_at: receipt.committed_at.to_rfc3339(),
            replayed: receipt.replayed,
        }),
        record_type: "agent.assignment".into(),
        record_id: assignment.id().as_uuid().to_string(),
        lifecycle_status: match assignment.state() {
            AssignmentState::Queued => "queued",
            AssignmentState::Running => "running",
            AssignmentState::Waiting => "waiting",
            AssignmentState::Completed => "completed",
            AssignmentState::Failed => "failed",
            AssignmentState::Cancelled => "cancelled",
        }
        .into(),
    }
}

fn encode_agent_session_mutation(receipt: SessionMutationReceipt) -> agent::AgentMutationResponse {
    let session = &receipt.session;
    agent::AgentMutationResponse {
        receipt: Some(common::CommandReceipt {
            command_id: receipt.command_id.to_string(),
            correlation_id: receipt.correlation_id.to_string(),
            aggregate: Some(common::AggregateVersion {
                context: "agent_orchestration".into(),
                aggregate_type: "agent_session".into(),
                aggregate_id: session.id().as_uuid().to_string(),
                version: session.version(),
            }),
            committed_events: vec![common::EventPosition {
                event_id: receipt.event_id.to_string(),
                global_sequence: receipt.global_sequence,
            }],
            committed_at: receipt.committed_at.to_rfc3339(),
            replayed: receipt.replayed,
        }),
        record_type: "agent.private-session".into(),
        record_id: session.id().as_uuid().to_string(),
        lifecycle_status: if session.expired_at().is_some() {
            "expired"
        } else {
            "private-review"
        }
        .into(),
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

fn status_from_agent_application(error: AgentOrchestrationError) -> Status {
    match error {
        AgentOrchestrationError::Port(error) => status_from_agent_port(error),
        AgentOrchestrationError::NotFound => {
            Status::not_found("Approved Agent version was not found")
        }
        AgentOrchestrationError::VersionConflict => Status::aborted("Agent assignment has changed"),
        AgentOrchestrationError::BudgetApprovalRequired => {
            Status::resource_exhausted("Agent budget requires explicit approval")
        }
        AgentOrchestrationError::HumanDecision(_) => {
            Status::permission_denied("Agent assignment confirmation is invalid")
        }
        AgentOrchestrationError::Domain(_) => {
            Status::failed_precondition("Agent assignment is invalid")
        }
    }
}

fn status_from_suggestion_application(error: SuggestionApplicationError) -> Status {
    match error {
        SuggestionApplicationError::InvalidRequest => {
            Status::invalid_argument("Field suggestion request is invalid")
        }
        SuggestionApplicationError::Denied => {
            Status::permission_denied("Field suggestion authority is denied")
        }
        SuggestionApplicationError::NotFound => Status::not_found("Field suggestion was not found"),
        SuggestionApplicationError::VersionConflict => {
            Status::aborted("Field suggestion has changed")
        }
        SuggestionApplicationError::Unavailable => {
            Status::unavailable("Field suggestion service is unavailable")
        }
        SuggestionApplicationError::HumanDecisionRequired => Status::permission_denied(
            "Applying a field suggestion requires a direct authenticated human decision",
        ),
        SuggestionApplicationError::Domain(_) => {
            Status::failed_precondition("Field suggestion transition is invalid")
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
        assert_eq!(OWNER_PORT_GAPS.len(), 22);
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
            "AgentOrchestrationCommandService.StartAssignment",
            "AgentOrchestrationCommandService.StartPrivateSession",
            "AgentOrchestrationCommandService.AppendPrivateSessionMessage",
            "AgentOrchestrationCommandService.PublishPrivateSessionMessage",
            "AgentOrchestrationQueryService.GetRoutingDecision",
            "AgentOrchestrationQueryService.GetInvocationEvidence",
        ];
        assert_eq!(supported.len(), 23);
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

    #[test]
    fn field_suggestion_apply_routes_are_closed_and_owner_exact() {
        let expected = [
            (
                "sales.quote",
                FutureOwnerContext::SalesQuoteSuggestionApply,
                "sales.quote.suggestion.apply",
            ),
            (
                "customer-360.note",
                FutureOwnerContext::Customer360NoteSuggestionApply,
                "customer_360.note.suggestion.apply",
            ),
            (
                "procurement.requisition",
                FutureOwnerContext::ProcurementRequisitionSuggestionApply,
                "procurement.requisition.suggestion.apply",
            ),
            (
                "maintenance.request",
                FutureOwnerContext::MaintenanceRequestSuggestionApply,
                "maintenance.request.suggestion.apply",
            ),
            (
                "marketing.content",
                FutureOwnerContext::MarketingContentSuggestionApply,
                "marketing.content.suggestion.apply",
            ),
        ];
        assert_eq!(SUGGESTION_APPLY_CONTEXTS.len(), expected.len());
        for (record_type, context, permission) in expected {
            assert_eq!(suggestion_apply_context(record_type), Some(context));
            assert!(SUGGESTION_APPLY_CONTEXTS.contains(&context));
            let schema = record_editable_field_schema(record_type).expect("current owner schema");
            assert!(schema.supports_agent_patches());
            assert!(
                schema
                    .fields
                    .iter()
                    .all(|field| field.human_apply_permission == permission)
            );
        }
        for denied in [
            "finance.invoice",
            "planning.plan",
            "platform.holding",
            "sales.quote.future",
            "",
        ] {
            assert_eq!(suggestion_apply_context(denied), None);
        }
    }

    #[test]
    fn assignment_decoder_binds_confirmation_scope_identity_and_fresh_payload() {
        let holding_id = Uuid::new_v4();
        let actor = ActorAttribution {
            principal_ref: Uuid::new_v4().to_string(),
            kind: ActorKind::HoldingUser,
            session_id: Some(SessionId::try_from_uuid(Uuid::new_v4()).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        };
        let scope = AuthorityScope::Holding { holding_id };
        let command_id = CommandId::try_from_uuid(Uuid::new_v4()).unwrap();
        let context = AgentApplicationCommandContext {
            command_id,
            correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4()).unwrap(),
            idempotency_key: Uuid::new_v4().to_string(),
            actor: actor.clone(),
            authority_scope: scope.clone(),
            audit: AuditContext {
                purpose: "test assignment".into(),
                customer_visible: false,
                module: Some("agent-orchestration".into()),
            },
            expected_version: None,
        };
        let assignment_id = Uuid::new_v4();
        let definition_id = Uuid::new_v4();
        let input = agent::StartAgentAssignmentRequest {
            context: None,
            assignment_id: assignment_id.to_string(),
            agent_definition_id: definition_id.to_string(),
            agent_version_id: Uuid::new_v4().to_string(),
            assignment_scope: Some(encode_scope(&scope)),
            origin: agent::AgentInvocationOrigin::Assign as i32,
            initiating_human_ref: actor.principal_ref.clone(),
            source_action_ref: format!("assign:{assignment_id}"),
            budget_reservation_id: Uuid::new_v4().to_string(),
            budget_subjects: Some(agent::AgentBudgetSubjects {
                data_profile_key: holding_id.to_string(),
                workspace_ref: format!("holding:{holding_id}"),
                agent_ref: definition_id.to_string(),
                principal_ref: actor.principal_ref.clone(),
                root_assignment_ref: assignment_id.to_string(),
            }),
            budget_request: Some(agent::AgentBudgetRequest {
                estimated_units: 10,
                estimated_cost_micros: 100,
                queue_priority: 1,
                stop_after_units: 10,
            }),
            prompt_ref: "vault:prompt/1".into(),
            payload: Some(agent::AgentPayloadManifest {
                payload_digest: "ab".repeat(32),
                classification: agent::DataClassification::Confidential as i32,
                field_refs: vec!["finance.invoice.memo".into()],
                document_record_ids: vec![Uuid::new_v4().to_string()],
                authorization_checked_at: Utc::now().to_rfc3339(),
                contains_secret: false,
                residency: "th".into(),
            }),
            human_confirmed: true,
        };
        let decoded = decode_start_agent_work(&input, &context).unwrap();
        assert_eq!(decoded.command_id, command_id);
        assert_eq!(decoded.scope, scope);
        assert_eq!(
            decoded.invocation.initiating_principal_ref,
            actor.principal_ref
        );
        assert_eq!(
            decoded.human_decision.decision_ref(),
            agent_assignment_decision_ref(decoded.assignment_id, command_id)
        );
        assert_eq!(decoded.human_decision.actor(), &actor);

        let mut stale = input;
        stale.payload.as_mut().unwrap().authorization_checked_at =
            (Utc::now() - chrono::Duration::minutes(6)).to_rfc3339();
        assert_eq!(
            decode_start_agent_work(&stale, &context)
                .unwrap_err()
                .code(),
            tonic::Code::FailedPrecondition
        );
    }
}
