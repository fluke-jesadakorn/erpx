use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, AuthorityScope, CommandId, CorrelationId, EventId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    DependencyAgreement, DependencyEndpoint, DependencyId, HandoffId, Project, ProjectEvent,
    ProjectHandoff, ProjectId, ProjectNode, ProjectNodeId, ProjectsAggregate, WorkflowTemplate,
    WorkflowTemplateId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectAction {
    CreateProject,
    ChangeProject,
    ChangeWorkGraph,
    PublishWorkflow,
    ProposeDependency,
    DecideDependency,
    ProposeHandoff,
    ApproveHandoff,
    ReconcileHandoff,
    View,
    List,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: ProjectAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_projects_enabled(
        &self,
        scope: &AuthorityScope,
        write: bool,
    ) -> Result<(), PortError>;
}

#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load_project(
        &self,
        authority: &AuthorityScope,
        id: ProjectId,
    ) -> Result<Option<Project>, PortError>;
    async fn load_workflow(
        &self,
        authority: &AuthorityScope,
        id: WorkflowTemplateId,
    ) -> Result<Option<WorkflowTemplate>, PortError>;
    async fn load_dependency(
        &self,
        authority: &AuthorityScope,
        id: DependencyId,
    ) -> Result<Option<DependencyAgreement>, PortError>;
    async fn load_handoff(
        &self,
        authority: &AuthorityScope,
        id: HandoffId,
    ) -> Result<Option<ProjectHandoff>, PortError>;
}

#[async_trait]
pub trait DependencyGraphPort: Send + Sync {
    /// Checks the accepted dependency read model and cannot mutate either project.
    async fn would_create_cycle(
        &self,
        source: &DependencyEndpoint,
        target: &DependencyEndpoint,
    ) -> Result<bool, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "value", rename_all = "snake_case")]
pub enum CommandFingerprint {
    CreateProject {
        project_id: ProjectId,
        request_digest: String,
    },
    ChangeProject {
        project_id: ProjectId,
        request_digest: String,
    },
    PublishWorkflow {
        workflow_id: WorkflowTemplateId,
        request_digest: String,
    },
    ProposeDependency {
        dependency_id: DependencyId,
        request_digest: String,
    },
    DecideDependency {
        dependency_id: DependencyId,
        request_digest: String,
    },
    ProposeHandoff {
        handoff_id: HandoffId,
        request_digest: String,
    },
    ChangeHandoff {
        handoff_id: HandoffId,
        request_digest: String,
    },
}

impl CommandFingerprint {
    pub fn aggregate_id(&self) -> AggregateId {
        let id = match self {
            Self::CreateProject { project_id, .. } | Self::ChangeProject { project_id, .. } => {
                project_id.as_uuid()
            }
            Self::PublishWorkflow { workflow_id, .. } => workflow_id.as_uuid(),
            Self::ProposeDependency { dependency_id, .. }
            | Self::DecideDependency { dependency_id, .. } => dependency_id.as_uuid(),
            Self::ProposeHandoff { handoff_id, .. } | Self::ChangeHandoff { handoff_id, .. } => {
                handoff_id.as_uuid()
            }
        };
        AggregateId::try_from_uuid(id).expect("validated Projects identifier")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: CommandFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectsCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: ProjectsAggregate,
    pub events: Vec<ProjectEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: ProjectsAggregate,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

impl StoredCommit {
    pub fn into_receipt(self) -> CommandReceipt<ProjectsAggregate> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "projects".into(),
                aggregate_type: self.aggregate.aggregate_type().into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id_uuid())
                    .expect("validated Projects identifier"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response: self.aggregate,
        }
    }
}

#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError>;
    /// Normalized authority, receipt, version marker, audit, and outbox are
    /// committed atomically.
    async fn commit(&self, request: ProjectsCommit) -> Result<StoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectView {
    pub project: Project,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HierarchyRow {
    pub node: ProjectNode,
    pub ancestor_ids: Vec<ProjectNodeId>,
    pub depth: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectGraphView {
    pub project: Project,
    pub hierarchy: Vec<HierarchyRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRecordKind {
    Phase,
    Item,
    WorkflowTemplate,
    Handoff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record_kind", content = "record", rename_all = "snake_case")]
pub enum ProjectRecordView {
    Node {
        project_id: ProjectId,
        node: ProjectNode,
    },
    WorkflowTemplate(WorkflowTemplate),
    Handoff(ProjectHandoff),
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProjectListFilter {
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectList {
    pub projects: Vec<Project>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get_project(
        &self,
        authority: &AuthorityScope,
        id: ProjectId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ProjectView>>, PortError>;
    async fn get_graph(
        &self,
        authority: &AuthorityScope,
        id: ProjectId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ProjectGraphView>>, PortError>;
    async fn get_record(
        &self,
        authority: &AuthorityScope,
        id: uuid::Uuid,
        kind: ProjectRecordKind,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ProjectRecordView>>, PortError>;
    async fn list_projects(
        &self,
        authority: &AuthorityScope,
        filter: &ProjectListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<ProjectList>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("Projects action is denied")]
    Denied,
    #[error("Projects entitlement is unavailable")]
    EntitlementDenied,
    #[error("Projects aggregate was not found")]
    NotFound,
    #[error("Projects scope is outside authority")]
    ScopeOutsideAuthority,
    #[error("Projects version conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("Projects command identity was reused with different input")]
    IdempotencyConflict,
    #[error("dependency would create a cross-project cycle")]
    DependencyCycle,
    #[error("Projects storage is temporarily unavailable")]
    Unavailable,
    #[error("Projects projection has not reached the requested command")]
    NotReady(ProjectionNotReady),
    #[error("Projects adapter failed")]
    Internal,
}
