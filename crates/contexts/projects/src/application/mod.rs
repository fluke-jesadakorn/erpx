use erpx_cqrs::{CommandReceipt, IdempotencyKey};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::{
    ActorAttribution, AuditContext, AuthorityScope, Clock, CommandId, CorrelationId, EventId,
    HumanDecisionProof,
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
        dependency_acceptance_decision_ref, dependency_rejection_decision_ref,
        handoff_approval_decision_ref, project_node_transition_decision_ref,
        project_transition_decision_ref,
    },
    ports::{
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, DependencyDecisionAction, DependencyGraphPort, EntitlementPort,
        HandoffChangeAction, PortError, ProjectAction, ProjectsCommit, WriteRepository,
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
    Accept { side: DependencySide },
    Reject,
    Cancel { owner_id: Uuid },
    Revoke { owner_id: Uuid },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecideDependency {
    pub dependency_id: DependencyId,
    pub decision: DependencyDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandoffChange {
    Approve,
    MarkReady,
    MarkUncertain,
    Cancel,
    Reconcile { outcome: HandoffOutcome },
}

impl DependencyDecision {
    const fn action(&self) -> DependencyDecisionAction {
        match self {
            Self::Accept {
                side: DependencySide::Source,
            } => DependencyDecisionAction::AcceptSource,
            Self::Accept {
                side: DependencySide::Target,
            } => DependencyDecisionAction::AcceptTarget,
            Self::Reject => DependencyDecisionAction::Reject,
            Self::Cancel { .. } => DependencyDecisionAction::Cancel,
            Self::Revoke { .. } => DependencyDecisionAction::Revoke,
        }
    }

    const fn requires_human_decision(&self) -> bool {
        matches!(self, Self::Accept { .. } | Self::Reject)
    }
}

impl HandoffChange {
    const fn action(&self) -> HandoffChangeAction {
        match self {
            Self::Approve => HandoffChangeAction::Approve,
            Self::MarkReady => HandoffChangeAction::MarkReady,
            Self::MarkUncertain => HandoffChangeAction::MarkUncertain,
            Self::Cancel => HandoffChangeAction::Cancel,
            Self::Reconcile { .. } => HandoffChangeAction::Reconcile,
        }
    }
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
        self.persist_with_human_decision(
            context,
            identity,
            expected_version,
            aggregate,
            events,
            None,
        )
        .await
    }

    async fn persist_with_human_decision(
        &self,
        context: CommandContext,
        identity: CommandIdentity,
        expected_version: Option<u64>,
        aggregate: ProjectsAggregate,
        events: Vec<crate::domain::ProjectEvent>,
        human_decision: Option<HumanDecisionProof>,
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
                human_decision,
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
        require_authenticated_human_decision(&context.actor)?;
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
        let human_decision = bind_human_decision(
            &context.actor,
            project.scope().clone(),
            project_transition_decision_ref(
                project.id(),
                expected,
                command.target_status,
                context.command_id,
            ),
            self.clock.now(),
        )?;
        project.transition(
            command.target_status,
            command.close_evidence,
            context.command_id,
            &human_decision,
        )?;
        let events = project.take_events();
        self.persist_with_human_decision(
            context,
            identity,
            Some(expected),
            ProjectsAggregate::Project(project),
            events,
            Some(human_decision),
        )
        .await
    }

    pub async fn change_node(
        &self,
        context: CommandContext,
        command: ChangeProjectNode,
    ) -> Result<CommandReceipt<ProjectsAggregate>, ProjectApplicationError> {
        if matches!(command.change, ProjectNodeChange::Transition { .. }) {
            require_authenticated_human_decision(&context.actor)?;
        }
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
        let mut human_decision = None;
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
                let decision = bind_human_decision(
                    &context.actor,
                    project.scope().clone(),
                    project_node_transition_decision_ref(
                        project.id(),
                        command.node_id,
                        expected,
                        node_version,
                        &target_workflow_state,
                        context.command_id,
                    ),
                    at,
                )?;
                project.transition_node(
                    command.node_id,
                    node_version,
                    target_workflow_state,
                    &workflow,
                    context.command_id,
                    &decision,
                )?;
                human_decision = Some(decision);
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
        self.persist_with_human_decision(
            context,
            identity,
            Some(expected),
            ProjectsAggregate::Project(project),
            events,
            human_decision,
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
        if command.decision.requires_human_decision() {
            require_authenticated_human_decision(&context.actor)?;
            decision_actor_uuid(&context.actor)?;
        }
        let decision_action = command.decision.action();
        let identity = CommandIdentity {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            fingerprint: CommandFingerprint::DecideDependency {
                dependency_id: command.dependency_id,
                decision: decision_action,
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
        let decision_scope = match &command.decision {
            DependencyDecision::Accept { side } => match side {
                DependencySide::Source => dependency.source().scope.clone(),
                DependencySide::Target => dependency.target().scope.clone(),
            },
            DependencyDecision::Reject => {
                let owner_id = decision_actor_uuid(&context.actor)?;
                dependency
                    .owner_scope(owner_id)
                    .cloned()
                    .ok_or(ProjectApplicationError::HumanDecisionRequired)?
            }
            DependencyDecision::Cancel { .. } | DependencyDecision::Revoke { .. } => {
                dependency.source().scope.clone()
            }
        };
        self.preflight(&context, &decision_scope, ProjectAction::DecideDependency)
            .await?;
        let expected = required_version(context.expected_version, dependency.version())?;
        let at = self.clock.now();
        let mut human_decision = None;
        match command.decision {
            DependencyDecision::Accept { side } => {
                let decision = bind_human_decision(
                    &context.actor,
                    decision_scope,
                    dependency_acceptance_decision_ref(command.dependency_id, side),
                    at,
                )?;
                dependency.accept(side, dependency.terms_version(), &decision)?;
                human_decision = Some(decision);
            }
            DependencyDecision::Reject => {
                let decision = bind_human_decision(
                    &context.actor,
                    decision_scope,
                    dependency_rejection_decision_ref(command.dependency_id),
                    at,
                )?;
                dependency.reject(&decision)?;
                human_decision = Some(decision);
            }
            DependencyDecision::Cancel { owner_id } => dependency.cancel(owner_id, at)?,
            DependencyDecision::Revoke { owner_id } => dependency.revoke(owner_id, at)?,
        }
        self.persist_with_human_decision(
            context,
            identity,
            Some(expected),
            ProjectsAggregate::Dependency(dependency),
            Vec::new(),
            human_decision,
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
        if matches!(command.change, HandoffChange::Approve) {
            require_authenticated_human_decision(&context.actor)?;
            decision_actor_uuid(&context.actor)?;
        }
        let change_action = command.change.action();
        let identity = CommandIdentity {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            fingerprint: CommandFingerprint::ChangeHandoff {
                handoff_id: command.handoff_id,
                change: change_action,
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
        let mut human_decision = None;
        match command.change {
            HandoffChange::Approve => {
                let decision = bind_human_decision(
                    &context.actor,
                    handoff.scope().clone(),
                    handoff_approval_decision_ref(command.handoff_id),
                    at,
                )?;
                handoff.approve(&decision)?;
                human_decision = Some(decision);
            }
            HandoffChange::MarkReady => handoff.mark_ready(at)?,
            HandoffChange::MarkUncertain => handoff.mark_uncertain(at)?,
            HandoffChange::Cancel => handoff.cancel(at)?,
            HandoffChange::Reconcile { mut outcome } => {
                outcome.reconciled_at = at;
                handoff.reconcile(outcome)?;
            }
        }
        self.persist_with_human_decision(
            context,
            identity,
            Some(expected),
            ProjectsAggregate::Handoff(handoff),
            Vec::new(),
            human_decision,
        )
        .await
    }
}

fn require_authenticated_human_decision(
    actor: &ActorAttribution,
) -> Result<(), ProjectApplicationError> {
    if actor.is_human()
        && actor.session_id.is_some()
        && actor.represented_principal_ref.is_none()
        && actor.support_capability_id.is_none()
    {
        Ok(())
    } else {
        Err(ProjectApplicationError::HumanDecisionRequired)
    }
}

fn decision_actor_uuid(actor: &ActorAttribution) -> Result<Uuid, ProjectApplicationError> {
    Uuid::parse_str(&actor.principal_ref)
        .map_err(|_| ProjectApplicationError::HumanDecisionRequired)
}

fn bind_human_decision(
    actor: &ActorAttribution,
    scope: AuthorityScope,
    decision_ref: String,
    decided_at: chrono::DateTime<chrono::Utc>,
) -> Result<HumanDecisionProof, ProjectApplicationError> {
    require_authenticated_human_decision(actor)?;
    HumanDecisionProof::try_new(actor.clone(), scope, decision_ref, decided_at)
        .map_err(|_| ProjectApplicationError::HumanDecisionRequired)
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
    #[error("Projects decision requires a directly authenticated human session")]
    HumanDecisionRequired,
    #[error("command fingerprint serialization failed")]
    Serialization,
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;
    use chrono::{Duration, TimeZone, Utc};
    use erpx_kernel::{ActorKind, FixedClock, SessionId};

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn test_at(minute: i64) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 21, 9, 0, 0).single().unwrap() + Duration::minutes(minute)
    }

    fn branch_scope(branch_id: u128) -> AuthorityScope {
        AuthorityScope::Branch {
            holding_id: id(1),
            company_id: id(2),
            branch_id: id(branch_id),
        }
    }

    fn command_context(
        kind: ActorKind,
        principal_id: u128,
        expected_version: u64,
    ) -> CommandContext {
        CommandContext {
            command_id: CommandId::try_from_uuid(id(500)).unwrap(),
            idempotency_key: IdempotencyKey::try_new("projects-human-decision-test").unwrap(),
            correlation_id: CorrelationId::try_from_uuid(id(501)).unwrap(),
            causation_id: None,
            actor: ActorAttribution {
                principal_ref: id(principal_id).to_string(),
                kind,
                session_id: Some(SessionId::try_from_uuid(id(502)).unwrap()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            authority_scope: AuthorityScope::Holding { holding_id: id(1) },
            audit: AuditContext {
                purpose: "projects human decision test".into(),
                customer_visible: false,
                module: Some("projects".into()),
            },
            expected_version: Some(expected_version),
        }
    }

    fn dependency_endpoint(
        project_id: u128,
        node_id: u128,
        branch_id: u128,
        owner_id: u128,
    ) -> DependencyEndpoint {
        DependencyEndpoint {
            project_id: ProjectId::try_from_uuid(id(project_id)).unwrap(),
            project_version: 1,
            node_id: ProjectNodeId::try_from_uuid(id(node_id)).unwrap(),
            node_version: 1,
            scope: branch_scope(branch_id),
            owner_id: id(owner_id),
        }
    }

    fn proposed_dependency() -> DependencyAgreement {
        DependencyAgreement::propose(
            DependencyId::try_from_uuid(id(520)).unwrap(),
            dependency_endpoint(100, 101, 3, 110),
            dependency_endpoint(102, 103, 4, 111),
            DependencyKind::FinishToStart,
            id(110),
            test_at(0),
        )
        .unwrap()
    }

    fn proposed_handoff() -> ProjectHandoff {
        ProjectHandoff::propose(
            HandoffId::try_from_uuid(id(530)).unwrap(),
            ProjectId::try_from_uuid(id(100)).unwrap(),
            ProjectNodeId::try_from_uuid(id(101)).unwrap(),
            branch_scope(3),
            HandoffKind::AssetCapitalization,
            "a".repeat(64),
            "projects-handoff-test",
            CorrelationId::try_from_uuid(id(531)).unwrap(),
            id(132),
            test_at(0),
        )
        .unwrap()
    }

    #[derive(Clone, Default)]
    struct TestWrite {
        dependency: Option<DependencyAgreement>,
        handoff: Option<ProjectHandoff>,
        loads: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl WriteRepository for TestWrite {
        async fn load_project(
            &self,
            _: &AuthorityScope,
            _: ProjectId,
        ) -> Result<Option<Project>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn load_workflow(
            &self,
            _: &AuthorityScope,
            _: WorkflowTemplateId,
        ) -> Result<Option<crate::domain::WorkflowTemplate>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn load_dependency(
            &self,
            _: &AuthorityScope,
            _: DependencyId,
        ) -> Result<Option<DependencyAgreement>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(self.dependency.clone())
        }

        async fn load_handoff(
            &self,
            _: &AuthorityScope,
            _: HandoffId,
        ) -> Result<Option<ProjectHandoff>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(self.handoff.clone())
        }
    }

    #[derive(Clone, Default)]
    struct TestUnitOfWork {
        receipt_reads: Arc<AtomicUsize>,
        commits: Arc<Mutex<Vec<ProjectsCommit>>>,
    }

    #[async_trait]
    impl CommandUnitOfWork for TestUnitOfWork {
        async fn find_receipt(
            &self,
            _: &CommandIdentity,
        ) -> Result<Option<crate::ports::StoredCommit>, PortError> {
            self.receipt_reads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn commit(
            &self,
            request: ProjectsCommit,
        ) -> Result<crate::ports::StoredCommit, PortError> {
            let stored = crate::ports::StoredCommit {
                command_id: request.identity.command_id,
                correlation_id: request.correlation_id,
                aggregate: request.aggregate.clone(),
                committed_events: Vec::new(),
                committed_at: request.aggregate.occurred_at(),
                replayed: false,
            };
            self.commits.lock().unwrap().push(request);
            Ok(stored)
        }
    }

    #[derive(Clone, Copy)]
    struct Allow;

    #[async_trait]
    impl AuthorizationPort for Allow {
        async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError> {
            assert!(request.authority_scope.contains(request.target_scope));
            Ok(())
        }
    }

    #[async_trait]
    impl EntitlementPort for Allow {
        async fn ensure_projects_enabled(
            &self,
            _: &AuthorityScope,
            _: bool,
        ) -> Result<(), PortError> {
            Ok(())
        }
    }

    #[async_trait]
    impl DependencyGraphPort for Allow {
        async fn would_create_cycle(
            &self,
            _: &DependencyEndpoint,
            _: &DependencyEndpoint,
        ) -> Result<bool, PortError> {
            Ok(false)
        }
    }

    fn service(
        write: TestWrite,
        unit_of_work: TestUnitOfWork,
        minute: i64,
    ) -> ProjectsCommandService<TestWrite, TestUnitOfWork, Allow, Allow, Allow, FixedClock> {
        ProjectsCommandService::new(
            write,
            unit_of_work,
            Allow,
            Allow,
            Allow,
            FixedClock::new(test_at(minute)),
        )
    }

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

    #[tokio::test]
    async fn non_humans_are_denied_before_projects_receipt_or_owner_load() {
        for kind in [
            ActorKind::Agent,
            ActorKind::Service,
            ActorKind::SystemWorker,
        ] {
            let write = TestWrite {
                dependency: Some(proposed_dependency()),
                handoff: Some(proposed_handoff()),
                ..Default::default()
            };
            let loads = write.loads.clone();
            let uow = TestUnitOfWork::default();
            let application = service(write, uow.clone(), 1);
            let error = application
                .decide_dependency(
                    command_context(kind, 110, 1),
                    DecideDependency {
                        dependency_id: DependencyId::try_from_uuid(id(520)).unwrap(),
                        decision: DependencyDecision::Accept {
                            side: DependencySide::Source,
                        },
                    },
                )
                .await
                .unwrap_err();
            assert!(matches!(
                error,
                ProjectApplicationError::HumanDecisionRequired
            ));
            assert_eq!(loads.load(Ordering::SeqCst), 0);
            assert_eq!(uow.receipt_reads.load(Ordering::SeqCst), 0);
            assert!(uow.commits.lock().unwrap().is_empty());

            let write = TestWrite {
                handoff: Some(proposed_handoff()),
                ..Default::default()
            };
            let loads = write.loads.clone();
            let uow = TestUnitOfWork::default();
            let application = service(write, uow.clone(), 1);
            let error = application
                .change_handoff(
                    command_context(kind, 133, 1),
                    ChangeHandoff {
                        handoff_id: HandoffId::try_from_uuid(id(530)).unwrap(),
                        change: HandoffChange::Approve,
                    },
                )
                .await
                .unwrap_err();
            assert!(matches!(
                error,
                ProjectApplicationError::HumanDecisionRequired
            ));
            assert_eq!(loads.load(Ordering::SeqCst), 0);
            assert_eq!(uow.receipt_reads.load(Ordering::SeqCst), 0);
            assert!(uow.commits.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn project_and_node_transitions_reject_invalid_humans_before_replay_or_load() {
        let mut actors = vec![
            command_context(ActorKind::Agent, 140, 1),
            command_context(ActorKind::Service, 141, 1),
            command_context(ActorKind::SystemWorker, 142, 1),
        ];
        let mut sessionless = command_context(ActorKind::HoldingUser, 143, 1);
        sessionless.actor.session_id = None;
        actors.push(sessionless);
        let mut represented = command_context(ActorKind::HoldingUser, 144, 1);
        represented.actor.represented_principal_ref = Some(id(145).to_string());
        actors.push(represented);

        for context in actors {
            let write = TestWrite::default();
            let loads = write.loads.clone();
            let uow = TestUnitOfWork::default();
            let application = service(write, uow.clone(), 1);
            let error = application
                .transition_project(
                    context.clone(),
                    TransitionProject {
                        project_id: ProjectId::try_from_uuid(id(100)).unwrap(),
                        target_status: ProjectStatus::Approved,
                        close_evidence: None,
                    },
                )
                .await
                .unwrap_err();
            assert!(matches!(
                error,
                ProjectApplicationError::HumanDecisionRequired
            ));
            assert_eq!(loads.load(Ordering::SeqCst), 0);
            assert_eq!(uow.receipt_reads.load(Ordering::SeqCst), 0);
            assert!(uow.commits.lock().unwrap().is_empty());

            let write = TestWrite::default();
            let loads = write.loads.clone();
            let uow = TestUnitOfWork::default();
            let error = service(write, uow.clone(), 1)
                .change_node(
                    context,
                    ChangeProjectNode {
                        project_id: ProjectId::try_from_uuid(id(100)).unwrap(),
                        node_id: ProjectNodeId::try_from_uuid(id(101)).unwrap(),
                        change: ProjectNodeChange::Transition {
                            target_workflow_state: "working".into(),
                        },
                    },
                )
                .await
                .unwrap_err();
            assert!(matches!(
                error,
                ProjectApplicationError::HumanDecisionRequired
            ));
            assert_eq!(loads.load(Ordering::SeqCst), 0);
            assert_eq!(uow.receipt_reads.load(Ordering::SeqCst), 0);
            assert!(uow.commits.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn dependency_decisions_derive_owner_and_commit_exact_human_evidence() {
        for (principal_id, decision, expected_scope, expected_ref) in [
            (
                110,
                DependencyDecision::Accept {
                    side: DependencySide::Source,
                },
                branch_scope(3),
                dependency_acceptance_decision_ref(
                    DependencyId::try_from_uuid(id(520)).unwrap(),
                    DependencySide::Source,
                ),
            ),
            (
                111,
                DependencyDecision::Reject,
                branch_scope(4),
                dependency_rejection_decision_ref(DependencyId::try_from_uuid(id(520)).unwrap()),
            ),
        ] {
            let write = TestWrite {
                dependency: Some(proposed_dependency()),
                ..Default::default()
            };
            let uow = TestUnitOfWork::default();
            service(write, uow.clone(), 1)
                .decide_dependency(
                    command_context(ActorKind::HoldingUser, principal_id, 1),
                    DecideDependency {
                        dependency_id: DependencyId::try_from_uuid(id(520)).unwrap(),
                        decision,
                    },
                )
                .await
                .unwrap();
            let commits = uow.commits.lock().unwrap();
            let commit = commits.first().expect("one dependency decision commit");
            let proof = commit
                .human_decision
                .as_ref()
                .expect("dependency decision carries human evidence");
            assert_eq!(proof.actor(), &commit.actor);
            assert_eq!(proof.principal_ref(), id(principal_id).to_string());
            assert_eq!(proof.authority_scope(), &expected_scope);
            assert_eq!(proof.decision_ref(), expected_ref);
            assert_eq!(proof.decided_at(), test_at(1));
            let ProjectsAggregate::Dependency(dependency) = &commit.aggregate else {
                panic!("dependency decision must commit a dependency");
            };
            assert_eq!(dependency.updated_at(), proof.decided_at());
        }
    }

    #[tokio::test]
    async fn handoff_approval_derives_checker_and_commits_exact_human_evidence() {
        let write = TestWrite {
            handoff: Some(proposed_handoff()),
            ..Default::default()
        };
        let uow = TestUnitOfWork::default();
        service(write, uow.clone(), 1)
            .change_handoff(
                command_context(ActorKind::HoldingUser, 133, 1),
                ChangeHandoff {
                    handoff_id: HandoffId::try_from_uuid(id(530)).unwrap(),
                    change: HandoffChange::Approve,
                },
            )
            .await
            .unwrap();
        let commits = uow.commits.lock().unwrap();
        let commit = commits.first().expect("one handoff approval commit");
        let proof = commit
            .human_decision
            .as_ref()
            .expect("handoff approval carries human evidence");
        assert_eq!(proof.actor(), &commit.actor);
        assert_eq!(proof.principal_ref(), id(133).to_string());
        assert_eq!(proof.authority_scope(), &branch_scope(3));
        assert_eq!(
            proof.decision_ref(),
            handoff_approval_decision_ref(HandoffId::try_from_uuid(id(530)).unwrap())
        );
        assert_eq!(proof.decided_at(), test_at(1));
        let ProjectsAggregate::Handoff(handoff) = &commit.aggregate else {
            panic!("handoff approval must commit a handoff");
        };
        assert_eq!(handoff.approved_by(), Some(id(133)));
        assert_eq!(handoff.updated_at(), proof.decided_at());
    }

    #[tokio::test]
    async fn owner_cancel_and_reconcile_policies_do_not_require_human_proof() {
        let write = TestWrite {
            dependency: Some(proposed_dependency()),
            ..Default::default()
        };
        let uow = TestUnitOfWork::default();
        service(write, uow.clone(), 1)
            .decide_dependency(
                command_context(ActorKind::Service, 999, 1),
                DecideDependency {
                    dependency_id: DependencyId::try_from_uuid(id(520)).unwrap(),
                    decision: DependencyDecision::Cancel { owner_id: id(110) },
                },
            )
            .await
            .unwrap();
        assert!(uow.commits.lock().unwrap()[0].human_decision.is_none());

        let mut handoff = proposed_handoff();
        let checker = HumanDecisionProof::try_new(
            command_context(ActorKind::HoldingUser, 133, 1).actor,
            branch_scope(3),
            handoff_approval_decision_ref(handoff.id()),
            test_at(1),
        )
        .unwrap();
        handoff.approve(&checker).unwrap();
        handoff.mark_ready(test_at(2)).unwrap();
        let write = TestWrite {
            handoff: Some(handoff),
            ..Default::default()
        };
        let uow = TestUnitOfWork::default();
        service(write, uow.clone(), 3)
            .change_handoff(
                command_context(ActorKind::Service, 999, 3),
                ChangeHandoff {
                    handoff_id: HandoffId::try_from_uuid(id(530)).unwrap(),
                    change: HandoffChange::Reconcile {
                        outcome: HandoffOutcome {
                            owner_context: erpx_integration_contracts::catalog::ContextId::Assets,
                            owner_receipt_id: "asset-receipt-530".into(),
                            owner_record: None,
                            accepted: true,
                            reason_code: "capitalized".into(),
                            reconciled_at: test_at(0),
                        },
                    },
                },
            )
            .await
            .unwrap();
        assert!(uow.commits.lock().unwrap()[0].human_decision.is_none());
    }
}
