use erpx_cqrs::{CommandReceipt, IdempotencyKey};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::{
    ActorAttribution, AuditContext, AuthorityScope, Clock, CommandId, CorrelationId, EventId,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{
        AttachmentRef, CostFact, DependencyAgreement, DependencyEndpoint, DependencyId,
        DependencyKind, DependencySide, HandoffId, HandoffKind, HandoffOutcome, Project,
        ProjectClass, ProjectCloseEvidence, ProjectDomainError, ProjectHandoff, ProjectId,
        ProjectNode, ProjectNodeId, ProjectStatus, ProjectsAggregate, TimeFact, WorkflowTemplateId,
    },
    ports::{
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, DependencyGraphPort, EntitlementPort, PortError, ProjectAction,
        ProjectsCommit, WriteRepository,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandContext {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateProject {
    pub project_id: ProjectId,
    pub scope: AuthorityScope,
    pub code: String,
    pub name: String,
    pub class: ProjectClass,
    pub policy_ref: String,
    pub workflow_id: WorkflowTemplateId,
    pub owner_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddProjectNode {
    pub project_id: ProjectId,
    pub node: ProjectNode,
}

/// Adds a node while deriving its core state from the project's pinned
/// workflow. Transport adapters must never be allowed to assert core state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddProjectNodeInput {
    pub project_id: ProjectId,
    pub node_id: ProjectNodeId,
    pub parent_id: Option<ProjectNodeId>,
    pub kind: crate::domain::NodeKind,
    pub title: String,
    pub owner_id: Uuid,
    pub assignees: Vec<Uuid>,
    pub planned_start: Option<chrono::NaiveDate>,
    pub planned_end: Option<chrono::NaiveDate>,
    pub workflow_state: String,
    pub promotion_source: Option<crate::domain::CommentRevisionRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionProject {
    pub project_id: ProjectId,
    pub target_status: ProjectStatus,
    pub close_evidence: Option<ProjectCloseEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectNodeChange {
    Move { parent_id: Option<ProjectNodeId> },
    Transition { target_workflow_state: String },
    AttachReference { reference: StableRecordRef },
    AddAttachment { attachment: AttachmentRef },
    RecordTime { fact: TimeFact },
    RecordCost { fact: CostFact },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeProjectNode {
    pub project_id: ProjectId,
    pub node_id: ProjectNodeId,
    pub change: ProjectNodeChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyDecision {
    Accept {
        side: DependencySide,
        owner_id: Uuid,
    },
    Reject {
        owner_id: Uuid,
    },
    Cancel {
        owner_id: Uuid,
    },
    Revoke {
        owner_id: Uuid,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecideDependency {
    pub dependency_id: DependencyId,
    pub decision: DependencyDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandoffChange {
    Approve { approved_by: Uuid },
    MarkReady,
    MarkUncertain,
    Cancel,
    Reconcile { outcome: HandoffOutcome },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeHandoff {
    pub handoff_id: HandoffId,
    pub change: HandoffChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposeDependency {
    pub dependency_id: DependencyId,
    pub source: DependencyEndpoint,
    pub target: DependencyEndpoint,
    pub kind: DependencyKind,
    pub proposed_by: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposeHandoff {
    pub handoff_id: HandoffId,
    pub project_id: ProjectId,
    pub item_id: crate::domain::ProjectNodeId,
    pub scope: AuthorityScope,
    pub kind: HandoffKind,
    pub payload_digest: String,
    pub owner_id: Uuid,
}

pub struct ProjectsCommandService<W, U, A, E, G, C> {
    write: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    dependency_graph: G,
    clock: C,
}

impl<W, U, A, E, G, C> ProjectsCommandService<W, U, A, E, G, C> {
    pub const fn new(
        write: W,
        unit_of_work: U,
        authorization: A,
        entitlement: E,
        dependency_graph: G,
        clock: C,
    ) -> Self {
        Self {
            write,
            unit_of_work,
            authorization,
            entitlement,
            dependency_graph,
            clock,
        }
    }
}

impl<W, U, A, E, G, C> ProjectsCommandService<W, U, A, E, G, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    G: DependencyGraphPort,
    C: Clock,
{
    async fn preflight(
        &self,
        context: &CommandContext,
        target: &AuthorityScope,
        action: ProjectAction,
    ) -> Result<(), ProjectApplicationError> {
        if !context.authority_scope.contains(target) {
            return Err(ProjectApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_projects_enabled(target, true)
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &context.actor,
                authority_scope: &context.authority_scope,
                target_scope: target,
                action,
            })
            .await?;
        Ok(())
    }

    async fn replay(
        &self,
        context: &CommandContext,
        identity: &CommandIdentity,
        action: ProjectAction,
    ) -> Result<Option<CommandReceipt<ProjectsAggregate>>, ProjectApplicationError> {
        let Some(mut stored) = self.unit_of_work.find_receipt(identity).await? else {
            return Ok(None);
        };
        self.preflight(context, stored.aggregate.scope(), action)
            .await?;
        stored.replayed = true;
        Ok(Some(stored.into_receipt()))
    }

    async fn persist(
        &self,
        context: CommandContext,
        identity: CommandIdentity,
        expected_version: Option<u64>,
        aggregate: ProjectsAggregate,
        events: Vec<crate::domain::ProjectEvent>,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        let stored = self
            .unit_of_work
            .commit(ProjectsCommit {
                identity,
                correlation_id: context.correlation_id,
                causation_id: context.causation_id,
                actor: context.actor,
                authority_scope: context.authority_scope,
                audit: context.audit,
                expected_version,
                aggregate,
                events,
            })
            .await?;
        Ok(stored.into_receipt())
    }

    pub async fn create_project(
        &self,
        context: CommandContext,
        command: CreateProject,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        let identity = CommandIdentity {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            fingerprint: CommandFingerprint::CreateProject {
                project_id: command.project_id,
                request_digest: digest(&command)?,
            },
        };
        if let Some(receipt) = self
            .replay(&context, &identity, ProjectAction::CreateProject)
            .await?
        {
            return Ok(receipt);
        }
        if context.expected_version.is_some() {
            return Err(ProjectApplicationError::UnexpectedCreateVersion);
        }
        self.preflight(&context, &command.scope, ProjectAction::CreateProject)
            .await?;
        let workflow = self
            .write
            .load_workflow(&context.authority_scope, command.workflow_id)
            .await?
            .ok_or(PortError::NotFound)?;
        let at = self.clock.now();
        let mut project = Project::propose(
            command.project_id,
            command.scope,
            command.code,
            command.name,
            command.class,
            command.policy_ref,
            &workflow,
            command.owner_id,
            at,
        )?;
        let events = project.take_events();
        self.persist(
            context,
            identity,
            None,
            ProjectsAggregate::Project(project),
            events,
        )
        .await
    }

    pub async fn add_node(
        &self,
        context: CommandContext,
        command: AddProjectNode,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        let identity = CommandIdentity {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            fingerprint: CommandFingerprint::ChangeProject {
                project_id: command.project_id,
                request_digest: digest(&command)?,
            },
        };
        if let Some(receipt) = self
            .replay(&context, &identity, ProjectAction::ChangeWorkGraph)
            .await?
        {
            return Ok(receipt);
        }
        let mut project = self
            .write
            .load_project(&context.authority_scope, command.project_id)
            .await?
            .ok_or(PortError::NotFound)?;
        self.preflight(&context, project.scope(), ProjectAction::ChangeWorkGraph)
            .await?;
        let expected = required_version(context.expected_version, project.version())?;
        let workflow = self
            .write
            .load_workflow(&context.authority_scope, project.workflow().template_id)
            .await?
            .ok_or(PortError::NotFound)?;
        project.add_node(command.node, &workflow, self.clock.now())?;
        let events = project.take_events();
        self.persist(
            context,
            identity,
            Some(expected),
            ProjectsAggregate::Project(project),
            events,
        )
        .await
    }

    pub async fn add_node_input(
        &self,
        context: CommandContext,
        command: AddProjectNodeInput,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        let identity = CommandIdentity {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            fingerprint: CommandFingerprint::ChangeProject {
                project_id: command.project_id,
                request_digest: digest(&command)?,
            },
        };
        if let Some(receipt) = self
            .replay(&context, &identity, ProjectAction::ChangeWorkGraph)
            .await?
        {
            return Ok(receipt);
        }
        let mut project = self
            .write
            .load_project(&context.authority_scope, command.project_id)
            .await?
            .ok_or(PortError::NotFound)?;
        self.preflight(&context, project.scope(), ProjectAction::ChangeWorkGraph)
            .await?;
        let expected = required_version(context.expected_version, project.version())?;
        let workflow = self
            .write
            .load_workflow(&context.authority_scope, project.workflow().template_id)
            .await?
            .ok_or(PortError::NotFound)?;
        let at = self.clock.now();
        let core_state = workflow.state(&command.workflow_state)?;
        let node = ProjectNode::new(
            command.node_id,
            command.parent_id,
            command.kind,
            command.title,
            command.owner_id,
            command.assignees,
            command.planned_start,
            command.planned_end,
            command.workflow_state,
            core_state,
            command.promotion_source,
            at,
        )?;
        project.add_node(node, &workflow, at)?;
        let events = project.take_events();
        self.persist(
            context,
            identity,
            Some(expected),
            ProjectsAggregate::Project(project),
            events,
        )
        .await
    }

    pub async fn transition_project(
        &self,
        context: CommandContext,
        command: TransitionProject,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        let identity = CommandIdentity {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            fingerprint: CommandFingerprint::ChangeProject {
                project_id: command.project_id,
                request_digest: digest(&command)?,
            },
        };
        if let Some(receipt) = self
            .replay(&context, &identity, ProjectAction::ChangeProject)
            .await?
        {
            return Ok(receipt);
        }
        let mut project = self
            .write
            .load_project(&context.authority_scope, command.project_id)
            .await?
            .ok_or(PortError::NotFound)?;
        self.preflight(&context, project.scope(), ProjectAction::ChangeProject)
            .await?;
        let expected = required_version(context.expected_version, project.version())?;
        project.transition(
            command.target_status,
            command.close_evidence,
            self.clock.now(),
        )?;
        let events = project.take_events();
        self.persist(
            context,
            identity,
            Some(expected),
            ProjectsAggregate::Project(project),
            events,
        )
        .await
    }

    pub async fn change_node(
        &self,
        context: CommandContext,
        command: ChangeProjectNode,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        let identity = CommandIdentity {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            fingerprint: CommandFingerprint::ChangeProject {
                project_id: command.project_id,
                request_digest: digest(&command)?,
            },
        };
        if let Some(receipt) = self
            .replay(&context, &identity, ProjectAction::ChangeWorkGraph)
            .await?
        {
            return Ok(receipt);
        }
        let mut project = self
            .write
            .load_project(&context.authority_scope, command.project_id)
            .await?
            .ok_or(PortError::NotFound)?;
        self.preflight(&context, project.scope(), ProjectAction::ChangeWorkGraph)
            .await?;
        let expected = required_version(context.expected_version, project.version())?;
        let at = self.clock.now();
        match command.change {
            ProjectNodeChange::Move { parent_id } => {
                let node_version = project
                    .nodes()
                    .get(&command.node_id)
                    .ok_or(ProjectDomainError::NodeNotFound)?
                    .version();
                project.move_node(command.node_id, parent_id, node_version, at)?;
            }
            ProjectNodeChange::Transition {
                target_workflow_state,
            } => {
                let node_version = project
                    .nodes()
                    .get(&command.node_id)
                    .ok_or(ProjectDomainError::NodeNotFound)?
                    .version();
                let workflow = self
                    .write
                    .load_workflow(&context.authority_scope, project.workflow().template_id)
                    .await?
                    .ok_or(PortError::NotFound)?;
                project.transition_node(
                    command.node_id,
                    node_version,
                    target_workflow_state,
                    &workflow,
                    at,
                )?;
            }
            ProjectNodeChange::AttachReference { reference } => {
                project.attach_reference(command.node_id, reference, at)?;
            }
            ProjectNodeChange::AddAttachment { attachment } => {
                project.add_attachment(command.node_id, attachment, at)?;
            }
            ProjectNodeChange::RecordTime { fact } => {
                project.record_time(command.node_id, fact, at)?;
            }
            ProjectNodeChange::RecordCost { fact } => {
                project.record_cost(command.node_id, fact, at)?;
            }
        }
        let events = project.take_events();
        self.persist(
            context,
            identity,
            Some(expected),
            ProjectsAggregate::Project(project),
            events,
        )
        .await
    }

    pub async fn propose_dependency(
        &self,
        context: CommandContext,
        command: ProposeDependency,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        let identity = CommandIdentity {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            fingerprint: CommandFingerprint::ProposeDependency {
                dependency_id: command.dependency_id,
                request_digest: digest(&command)?,
            },
        };
        if let Some(receipt) = self
            .replay(&context, &identity, ProjectAction::ProposeDependency)
            .await?
        {
            return Ok(receipt);
        }
        if context.expected_version.is_some() {
            return Err(ProjectApplicationError::UnexpectedCreateVersion);
        }
        self.preflight(
            &context,
            &command.source.scope,
            ProjectAction::ProposeDependency,
        )
        .await?;
        let source_project = self
            .write
            .load_project(&context.authority_scope, command.source.project_id)
            .await?
            .ok_or(PortError::NotFound)?;
        let target_project = self
            .write
            .load_project(&context.authority_scope, command.target.project_id)
            .await?
            .ok_or(PortError::NotFound)?;
        validate_endpoint(&source_project, &command.source)?;
        validate_endpoint(&target_project, &command.target)?;
        if self
            .dependency_graph
            .would_create_cycle(&command.source, &command.target)
            .await?
        {
            return Err(PortError::DependencyCycle.into());
        }
        let dependency = DependencyAgreement::propose(
            command.dependency_id,
            command.source,
            command.target,
            command.kind,
            command.proposed_by,
            self.clock.now(),
        )?;
        self.persist(
            context,
            identity,
            None,
            ProjectsAggregate::Dependency(dependency),
            Vec::new(),
        )
        .await
    }

    pub async fn decide_dependency(
        &self,
        context: CommandContext,
        command: DecideDependency,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        let identity = CommandIdentity {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            fingerprint: CommandFingerprint::DecideDependency {
                dependency_id: command.dependency_id,
                request_digest: digest(&command)?,
            },
        };
        if let Some(receipt) = self
            .replay(&context, &identity, ProjectAction::DecideDependency)
            .await?
        {
            return Ok(receipt);
        }
        let mut dependency = self
            .write
            .load_dependency(&context.authority_scope, command.dependency_id)
            .await?
            .ok_or(PortError::NotFound)?;
        self.preflight(
            &context,
            &dependency.source().scope,
            ProjectAction::DecideDependency,
        )
        .await?;
        let expected = required_version(context.expected_version, dependency.version())?;
        let at = self.clock.now();
        match command.decision {
            DependencyDecision::Accept { side, owner_id } => {
                dependency.accept(side, owner_id, dependency.terms_version(), at)?;
            }
            DependencyDecision::Reject { owner_id } => dependency.reject(owner_id, at)?,
            DependencyDecision::Cancel { owner_id } => dependency.cancel(owner_id, at)?,
            DependencyDecision::Revoke { owner_id } => dependency.revoke(owner_id, at)?,
        }
        self.persist(
            context,
            identity,
            Some(expected),
            ProjectsAggregate::Dependency(dependency),
            Vec::new(),
        )
        .await
    }

    pub async fn propose_handoff(
        &self,
        context: CommandContext,
        command: ProposeHandoff,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        let identity = CommandIdentity {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            fingerprint: CommandFingerprint::ProposeHandoff {
                handoff_id: command.handoff_id,
                request_digest: digest(&command)?,
            },
        };
        if let Some(receipt) = self
            .replay(&context, &identity, ProjectAction::ProposeHandoff)
            .await?
        {
            return Ok(receipt);
        }
        if context.expected_version.is_some() {
            return Err(ProjectApplicationError::UnexpectedCreateVersion);
        }
        self.preflight(&context, &command.scope, ProjectAction::ProposeHandoff)
            .await?;
        let project = self
            .write
            .load_project(&context.authority_scope, command.project_id)
            .await?
            .ok_or(PortError::NotFound)?;
        if project.scope() != &command.scope || !project.nodes().contains_key(&command.item_id) {
            return Err(ProjectApplicationError::StaleEndpoint);
        }
        let handoff = ProjectHandoff::propose(
            command.handoff_id,
            command.project_id,
            command.item_id,
            command.scope,
            command.kind,
            command.payload_digest,
            context.idempotency_key.as_str(),
            context.correlation_id,
            command.owner_id,
            self.clock.now(),
        )?;
        self.persist(
            context,
            identity,
            None,
            ProjectsAggregate::Handoff(handoff),
            Vec::new(),
        )
        .await
    }

    pub async fn change_handoff(
        &self,
        context: CommandContext,
        command: ChangeHandoff,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        let identity = CommandIdentity {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            fingerprint: CommandFingerprint::ChangeHandoff {
                handoff_id: command.handoff_id,
                request_digest: digest(&command)?,
            },
        };
        let action = match command.change {
            HandoffChange::Reconcile { .. } => ProjectAction::ReconcileHandoff,
            _ => ProjectAction::ApproveHandoff,
        };
        if let Some(receipt) = self.replay(&context, &identity, action).await? {
            return Ok(receipt);
        }
        let mut handoff = self
            .write
            .load_handoff(&context.authority_scope, command.handoff_id)
            .await?
            .ok_or(PortError::NotFound)?;
        self.preflight(&context, handoff.scope(), action).await?;
        let expected = required_version(context.expected_version, handoff.version())?;
        let at = self.clock.now();
        match command.change {
            HandoffChange::Approve { approved_by } => handoff.approve(approved_by, at)?,
            HandoffChange::MarkReady => handoff.mark_ready(at)?,
            HandoffChange::MarkUncertain => handoff.mark_uncertain(at)?,
            HandoffChange::Cancel => handoff.cancel(at)?,
            HandoffChange::Reconcile { mut outcome } => {
                outcome.reconciled_at = at;
                handoff.reconcile(outcome)?;
            }
        }
        self.persist(
            context,
            identity,
            Some(expected),
            ProjectsAggregate::Handoff(handoff),
            Vec::new(),
        )
        .await
    }
}

fn validate_endpoint(
    project: &Project,
    endpoint: &DependencyEndpoint,
) -> Result<(), ProjectApplicationError> {
    let Some(node) = project.nodes().get(&endpoint.node_id) else {
        return Err(ProjectApplicationError::StaleEndpoint);
    };
    if project.scope() != &endpoint.scope
        || project.version() != endpoint.project_version
        || project.owner_id() != endpoint.owner_id
        || node.version() != endpoint.node_version
    {
        return Err(ProjectApplicationError::StaleEndpoint);
    }
    Ok(())
}

fn required_version(expected: Option<u64>, actual: u64) -> Result<u64, ProjectApplicationError> {
    let expected = expected.ok_or(ProjectApplicationError::MissingExpectedVersion)?;
    if expected == actual {
        Ok(expected)
    } else {
        Err(ProjectApplicationError::VersionConflict { expected, actual })
    }
}

fn digest<T: Serialize>(value: &T) -> Result<String, ProjectApplicationError> {
    let encoded = serde_json::to_vec(value).map_err(|_| ProjectApplicationError::Serialization)?;
    Ok(format!("{:x}", Sha256::digest(encoded)))
}

#[derive(Debug, Error)]
pub enum ProjectApplicationError {
    #[error(transparent)]
    Domain(#[from] ProjectDomainError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("create command cannot carry expected version")]
    UnexpectedCreateVersion,
    #[error("mutation requires expected aggregate version")]
    MissingExpectedVersion,
    #[error("version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("scope is outside authority")]
    ScopeOutsideAuthority,
    #[error("project dependency or handoff endpoint is missing or stale")]
    StaleEndpoint,
    #[error("command fingerprint serialization failed")]
    Serialization,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_digest_is_stable_and_payload_sensitive() {
        let base = CreateProject {
            project_id: ProjectId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            scope: AuthorityScope::Company {
                holding_id: Uuid::from_u128(2),
                company_id: Uuid::from_u128(3),
            },
            code: "PRJ-1".into(),
            name: "Project one".into(),
            class: ProjectClass::Internal,
            policy_ref: "internal-v1".into(),
            workflow_id: WorkflowTemplateId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
            owner_id: Uuid::from_u128(5),
        };
        assert_eq!(digest(&base).unwrap(), digest(&base).unwrap());
        let mut changed = base.clone();
        changed.name = "Changed".into();
        assert_ne!(digest(&base).unwrap(), digest(&changed).unwrap());
    }
}
