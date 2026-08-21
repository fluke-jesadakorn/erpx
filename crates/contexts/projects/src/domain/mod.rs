use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::{
    MoneyAmount, StableRecordRef,
    catalog::ContextId,
    reference::{RecordTypeId, ReferenceValidationError},
};
use erpx_kernel::{AuthorityScope, CorrelationId, HumanDecisionProof, RecordId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_NODES: usize = 10_000;
pub const MAX_LINKS: usize = 256;

macro_rules! project_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);
        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, ProjectDomainError> {
                require_id(value, $field)?;
                Ok(Self(value))
            }
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }
    };
}

project_id!(ProjectId, "project_id");
project_id!(ProjectNodeId, "project_node_id");
project_id!(WorkflowTemplateId, "workflow_template_id");
project_id!(DependencyId, "dependency_id");
project_id!(HandoffId, "handoff_id");
project_id!(TimeFactId, "time_fact_id");
project_id!(CostFactId, "cost_fact_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectClass {
    ClientServices,
    Internal,
    Capital,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Proposed,
    Approved,
    Active,
    Paused,
    Completed,
    Closed,
    Cancelled,
    Archived,
}

impl ProjectStatus {
    const fn permits(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Proposed, Self::Approved | Self::Cancelled)
                | (Self::Approved, Self::Active | Self::Cancelled)
                | (
                    Self::Active,
                    Self::Paused | Self::Completed | Self::Cancelled
                )
                | (Self::Paused, Self::Active | Self::Cancelled)
                | (Self::Completed, Self::Active | Self::Closed)
                | (Self::Closed | Self::Cancelled, Self::Archived)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreWorkState {
    Proposed,
    Open,
    InProgress,
    Waiting,
    Resolved,
    Closed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Phase,
    Epic,
    Task,
    Milestone,
    Deliverable,
    Risk,
    Issue,
    Decision,
    ChangeRequest,
}

impl NodeKind {
    pub const fn is_container(self) -> bool {
        matches!(self, Self::Phase | Self::Epic)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowStateDefinition {
    pub code: String,
    pub core_state: CoreWorkState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowTransitionDefinition {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTemplateStatus {
    Active,
    Withdrawn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowTemplateRef {
    pub template_id: WorkflowTemplateId,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowTemplate {
    id: WorkflowTemplateId,
    version: u64,
    scope: AuthorityScope,
    name: String,
    states: Vec<WorkflowStateDefinition>,
    transitions: Vec<WorkflowTransitionDefinition>,
    status: WorkflowTemplateStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl WorkflowTemplate {
    pub fn publish(
        id: WorkflowTemplateId,
        scope: AuthorityScope,
        name: impl Into<String>,
        states: Vec<WorkflowStateDefinition>,
        transitions: Vec<WorkflowTransitionDefinition>,
        at: DateTime<Utc>,
    ) -> Result<Self, ProjectDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            name: name.into(),
            states,
            transitions,
            status: WorkflowTemplateStatus::Active,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn revise(
        &self,
        name: impl Into<String>,
        states: Vec<WorkflowStateDefinition>,
        transitions: Vec<WorkflowTransitionDefinition>,
        at: DateTime<Utc>,
    ) -> Result<Self, ProjectDomainError> {
        if self.status == WorkflowTemplateStatus::Withdrawn || at < self.updated_at {
            return Err(ProjectDomainError::InvalidTransition);
        }
        let value = Self {
            id: self.id,
            version: next_version(self.version)?,
            scope: self.scope.clone(),
            name: name.into(),
            states,
            transitions,
            status: WorkflowTemplateStatus::Active,
            created_at: self.created_at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn withdraw(&mut self, at: DateTime<Utc>) -> Result<(), ProjectDomainError> {
        if self.status != WorkflowTemplateStatus::Active || at < self.updated_at {
            return Err(ProjectDomainError::InvalidTransition);
        }
        self.status = WorkflowTemplateStatus::Withdrawn;
        self.version = next_version(self.version)?;
        self.updated_at = at;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), ProjectDomainError> {
        validate_scope(&self.scope)?;
        require_text(&self.name, "workflow_name", 255)?;
        if self.version == 0 || self.states.is_empty() || self.updated_at < self.created_at {
            return Err(ProjectDomainError::InvalidWorkflow);
        }
        let mut codes = BTreeSet::new();
        for state in &self.states {
            require_text(&state.code, "workflow_state", 64)?;
            if !codes.insert(state.code.as_str()) {
                return Err(ProjectDomainError::InvalidWorkflow);
            }
        }
        if !self
            .states
            .iter()
            .any(|value| value.core_state == CoreWorkState::Proposed)
            || !self
                .states
                .iter()
                .any(|value| value.core_state == CoreWorkState::Closed)
            || self.transitions.iter().any(|value| {
                value.from == value.to
                    || !codes.contains(value.from.as_str())
                    || !codes.contains(value.to.as_str())
            })
        {
            return Err(ProjectDomainError::InvalidWorkflow);
        }
        Ok(())
    }

    pub fn state(&self, code: &str) -> Result<CoreWorkState, ProjectDomainError> {
        self.states
            .iter()
            .find(|value| value.code == code)
            .map(|value| value.core_state)
            .ok_or(ProjectDomainError::UnknownWorkflowState)
    }

    pub fn permits(&self, pinned: WorkflowTemplateRef, from: &str, to: &str) -> bool {
        self.status == WorkflowTemplateStatus::Active
            && pinned == self.reference()
            && self
                .transitions
                .iter()
                .any(|value| value.from == from && value.to == to)
    }

    pub const fn reference(&self) -> WorkflowTemplateRef {
        WorkflowTemplateRef {
            template_id: self.id,
            version: self.version,
        }
    }

    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn id(&self) -> WorkflowTemplateId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn status(&self) -> WorkflowTemplateStatus {
        self.status
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn states(&self) -> &[WorkflowStateDefinition] {
        &self.states
    }
    pub fn transitions(&self) -> &[WorkflowTransitionDefinition] {
        &self.transitions
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentRevisionRef {
    pub thread_id: Uuid,
    pub comment_id: Uuid,
    pub revision: u64,
    pub content_digest: String,
}

impl CommentRevisionRef {
    pub fn validate(&self) -> Result<(), ProjectDomainError> {
        require_id(self.thread_id, "thread_id")?;
        require_id(self.comment_id, "comment_id")?;
        if self.revision == 0 || !valid_sha256(&self.content_digest) {
            return Err(ProjectDomainError::InvalidPromotionSource);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentRef {
    pub attachment_id: Uuid,
    pub object_ref: String,
    pub content_digest: String,
}

impl AttachmentRef {
    fn validate(&self) -> Result<(), ProjectDomainError> {
        require_id(self.attachment_id, "attachment_id")?;
        require_text(&self.object_ref, "object_ref", 2_048)?;
        if valid_sha256(&self.content_digest) {
            Ok(())
        } else {
            Err(ProjectDomainError::InvalidEvidence)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeFact {
    pub id: TimeFactId,
    pub worker_id: Uuid,
    pub work_date: NaiveDate,
    pub minutes: u32,
    pub source: StableRecordRef,
    pub source_version: u64,
}

impl TimeFact {
    fn validate(&self) -> Result<(), ProjectDomainError> {
        require_id(self.worker_id, "worker_id")?;
        self.source
            .validate()
            .map_err(|_| ProjectDomainError::InvalidEvidence)?;
        if self.minutes == 0 || self.minutes > 1_440 || self.source_version == 0 {
            return Err(ProjectDomainError::InvalidEvidence);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostFact {
    pub id: CostFactId,
    pub amount: MoneyAmount,
    pub source: StableRecordRef,
    pub source_version: u64,
    pub reversed_fact: Option<CostFactId>,
}

impl CostFact {
    fn validate(&self) -> Result<(), ProjectDomainError> {
        self.source
            .validate()
            .map_err(|_| ProjectDomainError::InvalidEvidence)?;
        if self.amount.minor_units() == 0 || self.source_version == 0 {
            return Err(ProjectDomainError::InvalidEvidence);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectNode {
    id: ProjectNodeId,
    parent_id: Option<ProjectNodeId>,
    kind: NodeKind,
    title: String,
    owner_id: Uuid,
    assignees: Vec<Uuid>,
    planned_start: Option<NaiveDate>,
    planned_end: Option<NaiveDate>,
    actual_start: Option<NaiveDate>,
    actual_end: Option<NaiveDate>,
    workflow_state: String,
    core_state: CoreWorkState,
    promotion_source: Option<CommentRevisionRef>,
    attachments: Vec<AttachmentRef>,
    references: Vec<StableRecordRef>,
    time_facts: Vec<TimeFact>,
    cost_facts: Vec<CostFact>,
    version: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ProjectNode {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ProjectNodeId,
        parent_id: Option<ProjectNodeId>,
        kind: NodeKind,
        title: impl Into<String>,
        owner_id: Uuid,
        assignees: Vec<Uuid>,
        planned_start: Option<NaiveDate>,
        planned_end: Option<NaiveDate>,
        workflow_state: impl Into<String>,
        core_state: CoreWorkState,
        promotion_source: Option<CommentRevisionRef>,
        at: DateTime<Utc>,
    ) -> Result<Self, ProjectDomainError> {
        let value = Self {
            id,
            parent_id,
            kind,
            title: title.into(),
            owner_id,
            assignees,
            planned_start,
            planned_end,
            actual_start: None,
            actual_end: None,
            workflow_state: workflow_state.into(),
            core_state,
            promotion_source,
            attachments: Vec::new(),
            references: Vec::new(),
            time_facts: Vec::new(),
            cost_facts: Vec::new(),
            version: 1,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), ProjectDomainError> {
        require_text(&self.title, "node_title", 255)?;
        require_text(&self.workflow_state, "workflow_state", 64)?;
        require_id(self.owner_id, "owner_id")?;
        if self.version == 0
            || self.updated_at < self.created_at
            || self.assignees.len() > 64
            || has_duplicates(&self.assignees)
            || self
                .assignees
                .iter()
                .any(|value| value.is_nil() || *value == self.owner_id)
            || matches!((self.planned_start, self.planned_end), (Some(a), Some(b)) if b < a)
            || matches!((self.actual_start, self.actual_end), (Some(a), Some(b)) if b < a)
            || self.attachments.len() > MAX_LINKS
            || self.references.len() > MAX_LINKS
            || self.time_facts.len() > MAX_LINKS
            || self.cost_facts.len() > MAX_LINKS
        {
            return Err(ProjectDomainError::InvalidNode);
        }
        if let Some(source) = &self.promotion_source {
            source.validate()?;
        }
        self.attachments
            .iter()
            .try_for_each(AttachmentRef::validate)?;
        self.references.iter().try_for_each(|value| {
            value
                .validate()
                .map_err(|_| ProjectDomainError::InvalidEvidence)
        })?;
        self.time_facts.iter().try_for_each(TimeFact::validate)?;
        self.cost_facts.iter().try_for_each(CostFact::validate)
    }

    pub const fn id(&self) -> ProjectNodeId {
        self.id
    }
    pub const fn parent_id(&self) -> Option<ProjectNodeId> {
        self.parent_id
    }
    pub const fn kind(&self) -> NodeKind {
        self.kind
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn owner_id(&self) -> Uuid {
        self.owner_id
    }
    pub fn assignees(&self) -> &[Uuid] {
        &self.assignees
    }
    pub fn workflow_state(&self) -> &str {
        &self.workflow_state
    }
    pub const fn core_state(&self) -> CoreWorkState {
        self.core_state
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn promotion_source(&self) -> Option<&CommentRevisionRef> {
        self.promotion_source.as_ref()
    }
    pub const fn planned_start(&self) -> Option<NaiveDate> {
        self.planned_start
    }
    pub const fn planned_end(&self) -> Option<NaiveDate> {
        self.planned_end
    }
    pub const fn actual_start(&self) -> Option<NaiveDate> {
        self.actual_start
    }
    pub const fn actual_end(&self) -> Option<NaiveDate> {
        self.actual_end
    }
    pub fn attachments(&self) -> &[AttachmentRef] {
        &self.attachments
    }
    pub fn references(&self) -> &[StableRecordRef] {
        &self.references
    }
    pub fn time_facts(&self) -> &[TimeFact] {
        &self.time_facts
    }
    pub fn cost_facts(&self) -> &[CostFact] {
        &self.cost_facts
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    id: ProjectId,
    version: u64,
    scope: AuthorityScope,
    code: String,
    name: String,
    class: ProjectClass,
    policy_ref: String,
    workflow: WorkflowTemplateRef,
    owner_id: Uuid,
    status: ProjectStatus,
    nodes: BTreeMap<ProjectNodeId, ProjectNode>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    events: Vec<ProjectEvent>,
}

impl Project {
    #[allow(clippy::too_many_arguments)]
    pub fn propose(
        id: ProjectId,
        scope: AuthorityScope,
        code: impl Into<String>,
        name: impl Into<String>,
        class: ProjectClass,
        policy_ref: impl Into<String>,
        workflow: &WorkflowTemplate,
        owner_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Self, ProjectDomainError> {
        validate_scope(&scope)?;
        if !scope.contains(workflow.scope()) || workflow.status != WorkflowTemplateStatus::Active {
            return Err(ProjectDomainError::ScopeMismatch);
        }
        let mut value = Self {
            id,
            version: 1,
            scope,
            code: code.into(),
            name: name.into(),
            class,
            policy_ref: policy_ref.into(),
            workflow: workflow.reference(),
            owner_id,
            status: ProjectStatus::Proposed,
            nodes: BTreeMap::new(),
            created_at: at,
            updated_at: at,
            events: Vec::new(),
        };
        value.validate()?;
        value.record_project_event(at);
        Ok(value)
    }

    pub fn transition(
        &mut self,
        next: ProjectStatus,
        close: Option<ProjectCloseEvidence>,
        at: DateTime<Utc>,
    ) -> Result<(), ProjectDomainError> {
        if !self.status.permits(next) || at < self.updated_at {
            return Err(ProjectDomainError::InvalidTransition);
        }
        if next == ProjectStatus::Completed
            && self.nodes.values().any(|value| {
                !matches!(
                    value.core_state,
                    CoreWorkState::Resolved | CoreWorkState::Closed | CoreWorkState::Cancelled
                )
            })
        {
            return Err(ProjectDomainError::UnresolvedWork);
        }
        if next == ProjectStatus::Closed {
            close
                .ok_or(ProjectDomainError::ReconciliationIncomplete)?
                .validate()?;
        }
        let mut candidate = self.clone();
        candidate.status = next;
        candidate.advance(at)?;
        *self = candidate;
        Ok(())
    }

    pub fn add_node(
        &mut self,
        node: ProjectNode,
        workflow: &WorkflowTemplate,
        at: DateTime<Utc>,
    ) -> Result<(), ProjectDomainError> {
        self.ensure_editable(at)?;
        if self.nodes.len() >= MAX_NODES || self.nodes.contains_key(&node.id) {
            return Err(ProjectDomainError::DuplicateNode);
        }
        self.validate_parent(node.id, node.parent_id, node.kind)?;
        if workflow.reference() != self.workflow
            || workflow.status != WorkflowTemplateStatus::Active
            || workflow.state(&node.workflow_state)? != node.core_state
        {
            return Err(ProjectDomainError::InvalidWorkflow);
        }
        let node_id = node.id;
        let mut candidate = self.clone();
        candidate.nodes.insert(node_id, node);
        candidate.advance(at)?;
        candidate.events.push(ProjectEvent::NodeChanged {
            project_id: self.id,
            node_id,
            at,
        });
        *self = candidate;
        Ok(())
    }

    pub fn move_node(
        &mut self,
        node_id: ProjectNodeId,
        parent_id: Option<ProjectNodeId>,
        expected_node_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), ProjectDomainError> {
        self.ensure_editable(at)?;
        let node = self
            .nodes
            .get(&node_id)
            .ok_or(ProjectDomainError::NodeNotFound)?;
        if node.version != expected_node_version {
            return Err(ProjectDomainError::VersionConflict);
        }
        self.validate_parent(node_id, parent_id, node.kind)?;
        if self.would_cycle(node_id, parent_id) {
            return Err(ProjectDomainError::HierarchyCycle);
        }
        let mut candidate = self.clone();
        let node = candidate.nodes.get_mut(&node_id).expect("node was checked");
        node.parent_id = parent_id;
        node.version = next_version(node.version)?;
        node.updated_at = at;
        candidate.advance(at)?;
        candidate.events.push(ProjectEvent::NodeChanged {
            project_id: self.id,
            node_id,
            at,
        });
        *self = candidate;
        Ok(())
    }

    pub fn transition_node(
        &mut self,
        node_id: ProjectNodeId,
        expected_node_version: u64,
        next_state: impl Into<String>,
        workflow: &WorkflowTemplate,
        at: DateTime<Utc>,
    ) -> Result<(), ProjectDomainError> {
        self.ensure_editable(at)?;
        let next_state = next_state.into();
        let node = self
            .nodes
            .get(&node_id)
            .ok_or(ProjectDomainError::NodeNotFound)?;
        if node.version != expected_node_version {
            return Err(ProjectDomainError::VersionConflict);
        }
        if !workflow.permits(self.workflow, &node.workflow_state, &next_state) {
            return Err(ProjectDomainError::InvalidTransition);
        }
        let core_state = workflow.state(&next_state)?;
        let mut candidate = self.clone();
        let node = candidate.nodes.get_mut(&node_id).expect("node was checked");
        node.workflow_state = next_state;
        node.core_state = core_state;
        node.version = next_version(node.version)?;
        node.updated_at = at;
        candidate.advance(at)?;
        candidate.events.push(ProjectEvent::NodeChanged {
            project_id: self.id,
            node_id,
            at,
        });
        *self = candidate;
        Ok(())
    }

    pub fn attach_reference(
        &mut self,
        node_id: ProjectNodeId,
        reference: StableRecordRef,
        at: DateTime<Utc>,
    ) -> Result<(), ProjectDomainError> {
        reference
            .validate()
            .map_err(|_| ProjectDomainError::InvalidEvidence)?;
        self.mutate_node(node_id, at, |node| {
            if node.references.contains(&reference) || node.references.len() >= MAX_LINKS {
                return Err(ProjectDomainError::DuplicateEvidence);
            }
            node.references.push(reference);
            Ok(())
        })
    }

    pub fn add_attachment(
        &mut self,
        node_id: ProjectNodeId,
        attachment: AttachmentRef,
        at: DateTime<Utc>,
    ) -> Result<(), ProjectDomainError> {
        attachment.validate()?;
        self.mutate_node(node_id, at, |node| {
            if node
                .attachments
                .iter()
                .any(|value| value.attachment_id == attachment.attachment_id)
                || node.attachments.len() >= MAX_LINKS
            {
                return Err(ProjectDomainError::DuplicateEvidence);
            }
            node.attachments.push(attachment);
            Ok(())
        })
    }

    pub fn record_time(
        &mut self,
        node_id: ProjectNodeId,
        fact: TimeFact,
        at: DateTime<Utc>,
    ) -> Result<(), ProjectDomainError> {
        fact.validate()?;
        self.mutate_node(node_id, at, |node| {
            if node.time_facts.iter().any(|value| value.id == fact.id) {
                return Err(ProjectDomainError::DuplicateEvidence);
            }
            node.time_facts.push(fact);
            Ok(())
        })
    }

    pub fn record_cost(
        &mut self,
        node_id: ProjectNodeId,
        fact: CostFact,
        at: DateTime<Utc>,
    ) -> Result<(), ProjectDomainError> {
        fact.validate()?;
        self.mutate_node(node_id, at, |node| {
            if node.cost_facts.iter().any(|value| value.id == fact.id)
                || fact
                    .reversed_fact
                    .is_some_and(|id| !node.cost_facts.iter().any(|value| value.id == id))
            {
                return Err(ProjectDomainError::DuplicateEvidence);
            }
            node.cost_facts.push(fact);
            Ok(())
        })
    }

    fn mutate_node<F>(
        &mut self,
        node_id: ProjectNodeId,
        at: DateTime<Utc>,
        mutation: F,
    ) -> Result<(), ProjectDomainError>
    where
        F: FnOnce(&mut ProjectNode) -> Result<(), ProjectDomainError>,
    {
        self.ensure_editable(at)?;
        let mut candidate = self.clone();
        let node = candidate
            .nodes
            .get_mut(&node_id)
            .ok_or(ProjectDomainError::NodeNotFound)?;
        mutation(node)?;
        node.version = next_version(node.version)?;
        node.updated_at = at;
        node.validate()?;
        candidate.advance(at)?;
        candidate.events.push(ProjectEvent::NodeChanged {
            project_id: self.id,
            node_id,
            at,
        });
        *self = candidate;
        Ok(())
    }

    fn validate_parent(
        &self,
        node_id: ProjectNodeId,
        parent_id: Option<ProjectNodeId>,
        kind: NodeKind,
    ) -> Result<(), ProjectDomainError> {
        if parent_id == Some(node_id) {
            return Err(ProjectDomainError::HierarchyCycle);
        }
        if kind.is_container() {
            return parent_id.map_or(Ok(()), |_| Err(ProjectDomainError::InvalidHierarchy));
        }
        let parent = self
            .nodes
            .get(&parent_id.ok_or(ProjectDomainError::InvalidHierarchy)?)
            .ok_or(ProjectDomainError::NodeNotFound)?;
        if !parent.kind.is_container()
            && parent
                .parent_id
                .and_then(|id| self.nodes.get(&id))
                .is_some_and(|ancestor| !ancestor.kind.is_container())
        {
            return Err(ProjectDomainError::InvalidHierarchy);
        }
        Ok(())
    }

    fn would_cycle(&self, node_id: ProjectNodeId, parent_id: Option<ProjectNodeId>) -> bool {
        let mut cursor = parent_id;
        let mut seen = BTreeSet::new();
        while let Some(id) = cursor {
            if id == node_id || !seen.insert(id) {
                return true;
            }
            cursor = self.nodes.get(&id).and_then(ProjectNode::parent_id);
        }
        false
    }

    fn ensure_editable(&self, at: DateTime<Utc>) -> Result<(), ProjectDomainError> {
        if at < self.updated_at
            || matches!(
                self.status,
                ProjectStatus::Closed | ProjectStatus::Cancelled | ProjectStatus::Archived
            )
        {
            Err(ProjectDomainError::InvalidTransition)
        } else {
            Ok(())
        }
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), ProjectDomainError> {
        if at < self.updated_at {
            return Err(ProjectDomainError::InvalidTimestamp);
        }
        self.version = next_version(self.version)?;
        self.updated_at = at;
        self.validate()?;
        self.record_project_event(at);
        Ok(())
    }

    fn record_project_event(&mut self, at: DateTime<Utc>) {
        self.events.push(ProjectEvent::ProjectChanged {
            project_id: self.id,
            version: self.version,
            at,
        });
    }

    pub fn validate(&self) -> Result<(), ProjectDomainError> {
        validate_scope(&self.scope)?;
        require_text(&self.code, "project_code", 128)?;
        require_text(&self.name, "project_name", 255)?;
        require_text(&self.policy_ref, "project_policy_ref", 255)?;
        require_id(self.owner_id, "project_owner_id")?;
        if self.version == 0 || self.updated_at < self.created_at || self.nodes.len() > MAX_NODES {
            return Err(ProjectDomainError::InvalidProject);
        }
        for node in self.nodes.values() {
            node.validate()?;
            self.validate_parent(node.id, node.parent_id, node.kind)?;
            if self.would_cycle(node.id, node.parent_id) {
                return Err(ProjectDomainError::HierarchyCycle);
            }
        }
        Ok(())
    }

    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        StableRecordRef::try_new(
            RecordId::try_from_uuid(self.id.as_uuid()).expect("validated project identifier"),
            ContextId::Projects,
            RecordTypeId::try_new("project.project")?,
        )
    }

    pub fn node_record_ref(
        &self,
        node_id: ProjectNodeId,
    ) -> Result<StableRecordRef, ProjectDomainError> {
        if !self.nodes.contains_key(&node_id) {
            return Err(ProjectDomainError::NodeNotFound);
        }
        StableRecordRef::try_new(
            RecordId::try_from_uuid(node_id.as_uuid()).expect("validated node identifier"),
            ContextId::Projects,
            RecordTypeId::try_new("project.item")
                .map_err(|_| ProjectDomainError::InvalidEvidence)?,
        )
        .map_err(|_| ProjectDomainError::InvalidEvidence)
    }

    pub const fn id(&self) -> ProjectId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn code(&self) -> &str {
        &self.code
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn policy_ref(&self) -> &str {
        &self.policy_ref
    }
    pub const fn class(&self) -> ProjectClass {
        self.class
    }
    pub const fn status(&self) -> ProjectStatus {
        self.status
    }
    pub const fn owner_id(&self) -> Uuid {
        self.owner_id
    }
    pub const fn workflow(&self) -> WorkflowTemplateRef {
        self.workflow
    }
    pub fn nodes(&self) -> &BTreeMap<ProjectNodeId, ProjectNode> {
        &self.nodes
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub fn take_events(&mut self) -> Vec<ProjectEvent> {
        std::mem::take(&mut self.events)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCloseEvidence {
    pub unresolved_items: u32,
    pub unreconciled_time_cost_facts: u32,
    pub pending_handoffs: u32,
    pub unsettled_dependencies: u32,
    pub legal_hold: bool,
}

impl ProjectCloseEvidence {
    fn validate(self) -> Result<(), ProjectDomainError> {
        if self.unresolved_items == 0
            && self.unreconciled_time_cost_facts == 0
            && self.pending_handoffs == 0
            && self.unsettled_dependencies == 0
            && !self.legal_hold
        {
            Ok(())
        } else {
            Err(ProjectDomainError::ReconciliationIncomplete)
        }
    }
}

// Cross-project dependencies are separate aggregates: no project can write the
// other project's graph while collecting two-sided owner acceptance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyEndpoint {
    pub project_id: ProjectId,
    pub project_version: u64,
    pub node_id: ProjectNodeId,
    pub node_version: u64,
    pub scope: AuthorityScope,
    pub owner_id: Uuid,
}

impl DependencyEndpoint {
    fn validate(&self) -> Result<(), ProjectDomainError> {
        validate_scope(&self.scope)?;
        require_id(self.owner_id, "dependency_owner_id")?;
        if self.project_version == 0 || self.node_version == 0 {
            Err(ProjectDomainError::InvalidDependency)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    FinishToStart,
    StartToStart,
    FinishToFinish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyStatus {
    Proposed,
    Accepted,
    Rejected,
    Revoked,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencySide {
    Source,
    Target,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyAcceptance {
    pub side: DependencySide,
    pub owner_id: Uuid,
    pub terms_version: u64,
    pub accepted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyAgreement {
    id: DependencyId,
    version: u64,
    terms_version: u64,
    source: DependencyEndpoint,
    target: DependencyEndpoint,
    kind: DependencyKind,
    status: DependencyStatus,
    source_acceptance: Option<DependencyAcceptance>,
    target_acceptance: Option<DependencyAcceptance>,
    proposed_by: Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl DependencyAgreement {
    pub fn propose(
        id: DependencyId,
        source: DependencyEndpoint,
        target: DependencyEndpoint,
        kind: DependencyKind,
        proposed_by: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Self, ProjectDomainError> {
        source.validate()?;
        target.validate()?;
        require_id(proposed_by, "dependency_proposed_by")?;
        if source.project_id == target.project_id
            || holding_id(&source.scope) != holding_id(&target.scope)
            || proposed_by != source.owner_id
        {
            return Err(ProjectDomainError::InvalidDependency);
        }
        Ok(Self {
            id,
            version: 1,
            terms_version: 1,
            source,
            target,
            kind,
            status: DependencyStatus::Proposed,
            source_acceptance: None,
            target_acceptance: None,
            proposed_by,
            created_at: at,
            updated_at: at,
        })
    }

    pub fn accept(
        &mut self,
        side: DependencySide,
        terms_version: u64,
        decision: &HumanDecisionProof,
    ) -> Result<(), ProjectDomainError> {
        let at = decision.decided_at();
        if self.status != DependencyStatus::Proposed
            || terms_version != self.terms_version
            || at < self.updated_at
        {
            return Err(ProjectDomainError::InvalidDependencyTransition);
        }
        let (expected, expected_scope) = match side {
            DependencySide::Source => (self.source.owner_id, &self.source.scope),
            DependencySide::Target => (self.target.owner_id, &self.target.scope),
        };
        let owner_id = exact_human_decision_principal(
            decision,
            expected_scope,
            &dependency_acceptance_decision_ref(self.id, side),
        )?;
        let slot = match side {
            DependencySide::Source => &mut self.source_acceptance,
            DependencySide::Target => &mut self.target_acceptance,
        };
        if owner_id != expected || slot.is_some() {
            return Err(ProjectDomainError::DependencyAuthorityDenied);
        }
        *slot = Some(DependencyAcceptance {
            side,
            owner_id,
            terms_version,
            accepted_at: at,
        });
        self.version = next_version(self.version)?;
        self.updated_at = at;
        if self.source_acceptance.is_some() && self.target_acceptance.is_some() {
            self.status = DependencyStatus::Accepted;
        }
        Ok(())
    }

    pub fn reject(&mut self, decision: &HumanDecisionProof) -> Result<(), ProjectDomainError> {
        let owner_id = decision_principal_uuid(decision)?;
        let scope = self
            .owner_scope(owner_id)
            .ok_or(ProjectDomainError::DependencyAuthorityDenied)?;
        exact_human_decision_principal(
            decision,
            scope,
            &dependency_rejection_decision_ref(self.id),
        )?;
        let at = decision.decided_at();
        self.finish_proposal(DependencyStatus::Rejected, owner_id, at)
    }

    pub fn cancel(&mut self, owner_id: Uuid, at: DateTime<Utc>) -> Result<(), ProjectDomainError> {
        if owner_id != self.proposed_by {
            return Err(ProjectDomainError::DependencyAuthorityDenied);
        }
        self.finish_proposal(DependencyStatus::Cancelled, owner_id, at)
    }

    pub fn revoke(&mut self, owner_id: Uuid, at: DateTime<Utc>) -> Result<(), ProjectDomainError> {
        if self.status != DependencyStatus::Accepted
            || (owner_id != self.source.owner_id && owner_id != self.target.owner_id)
            || at < self.updated_at
        {
            return Err(ProjectDomainError::InvalidDependencyTransition);
        }
        self.status = DependencyStatus::Revoked;
        self.version = next_version(self.version)?;
        self.updated_at = at;
        Ok(())
    }

    fn finish_proposal(
        &mut self,
        status: DependencyStatus,
        owner_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), ProjectDomainError> {
        if self.status != DependencyStatus::Proposed
            || (owner_id != self.source.owner_id && owner_id != self.target.owner_id)
            || at < self.updated_at
        {
            return Err(ProjectDomainError::InvalidDependencyTransition);
        }
        self.status = status;
        self.version = next_version(self.version)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> DependencyId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn source(&self) -> &DependencyEndpoint {
        &self.source
    }
    pub const fn target(&self) -> &DependencyEndpoint {
        &self.target
    }
    pub const fn kind(&self) -> DependencyKind {
        self.kind
    }
    pub const fn status(&self) -> DependencyStatus {
        self.status
    }
    pub const fn terms_version(&self) -> u64 {
        self.terms_version
    }
    pub const fn source_acceptance(&self) -> Option<&DependencyAcceptance> {
        self.source_acceptance.as_ref()
    }
    pub const fn target_acceptance(&self) -> Option<&DependencyAcceptance> {
        self.target_acceptance.as_ref()
    }
    pub const fn proposed_by(&self) -> Uuid {
        self.proposed_by
    }
    pub fn owner_scope(&self, owner_id: Uuid) -> Option<&AuthorityScope> {
        if owner_id == self.source.owner_id {
            Some(&self.source.scope)
        } else if owner_id == self.target.owner_id {
            Some(&self.target.scope)
        } else {
            None
        }
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffKind {
    FinanceInvoice,
    FinanceBilling,
    LegalContract,
    OperationsAction,
    AssetCapitalization,
    SalesBilling,
    ProcurementRequisition,
}

impl HandoffKind {
    pub const fn owner_context(self) -> ContextId {
        match self {
            Self::FinanceInvoice | Self::FinanceBilling => ContextId::Finance,
            Self::LegalContract => ContextId::Legal,
            Self::OperationsAction => ContextId::Operations,
            Self::AssetCapitalization => ContextId::Assets,
            Self::SalesBilling => ContextId::Sales,
            Self::ProcurementRequisition => ContextId::Procurement,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffStatus {
    Proposed,
    Approved,
    Ready,
    Accepted,
    Rejected,
    Uncertain,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffOutcome {
    pub owner_context: ContextId,
    pub owner_receipt_id: String,
    pub owner_record: Option<StableRecordRef>,
    pub accepted: bool,
    pub reason_code: String,
    pub reconciled_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectHandoff {
    id: HandoffId,
    version: u64,
    project_id: ProjectId,
    item_id: ProjectNodeId,
    scope: AuthorityScope,
    kind: HandoffKind,
    payload_digest: String,
    idempotency_key: String,
    correlation_id: CorrelationId,
    proposed_by: Uuid,
    approved_by: Option<Uuid>,
    status: HandoffStatus,
    outcome: Option<HandoffOutcome>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ProjectHandoff {
    #[allow(clippy::too_many_arguments)]
    pub fn propose(
        id: HandoffId,
        project_id: ProjectId,
        item_id: ProjectNodeId,
        scope: AuthorityScope,
        kind: HandoffKind,
        payload_digest: impl Into<String>,
        idempotency_key: impl Into<String>,
        correlation_id: CorrelationId,
        proposed_by: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Self, ProjectDomainError> {
        let value = Self {
            id,
            version: 1,
            project_id,
            item_id,
            scope,
            kind,
            payload_digest: payload_digest.into(),
            idempotency_key: idempotency_key.into(),
            correlation_id,
            proposed_by,
            approved_by: None,
            status: HandoffStatus::Proposed,
            outcome: None,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn approve(&mut self, decision: &HumanDecisionProof) -> Result<(), ProjectDomainError> {
        let approved_by = exact_human_decision_principal(
            decision,
            &self.scope,
            &handoff_approval_decision_ref(self.id),
        )?;
        let at = decision.decided_at();
        if self.status != HandoffStatus::Proposed
            || approved_by == self.proposed_by
            || at < self.updated_at
        {
            return Err(ProjectDomainError::HandoffAuthorityDenied);
        }
        self.approved_by = Some(approved_by);
        self.transition(HandoffStatus::Approved, at)
    }

    pub fn mark_ready(&mut self, at: DateTime<Utc>) -> Result<(), ProjectDomainError> {
        self.transition(HandoffStatus::Ready, at)
    }

    pub fn mark_uncertain(&mut self, at: DateTime<Utc>) -> Result<(), ProjectDomainError> {
        self.transition(HandoffStatus::Uncertain, at)
    }

    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), ProjectDomainError> {
        self.transition(HandoffStatus::Cancelled, at)
    }

    pub fn reconcile(&mut self, outcome: HandoffOutcome) -> Result<(), ProjectDomainError> {
        if !matches!(self.status, HandoffStatus::Ready | HandoffStatus::Uncertain)
            || outcome.owner_context != self.kind.owner_context()
            || outcome.reconciled_at < self.updated_at
        {
            return Err(ProjectDomainError::InvalidHandoffOutcome);
        }
        require_text(&outcome.owner_receipt_id, "owner_receipt_id", 255)?;
        require_text(&outcome.reason_code, "reason_code", 255)?;
        if outcome.owner_record.as_ref().is_some_and(|record| {
            record.validate().is_err() || record.owner_context() != outcome.owner_context
        }) {
            return Err(ProjectDomainError::InvalidHandoffOutcome);
        }
        self.status = if outcome.accepted {
            HandoffStatus::Accepted
        } else {
            HandoffStatus::Rejected
        };
        self.version = next_version(self.version)?;
        self.updated_at = outcome.reconciled_at;
        self.outcome = Some(outcome);
        self.validate()
    }

    fn transition(
        &mut self,
        next: HandoffStatus,
        at: DateTime<Utc>,
    ) -> Result<(), ProjectDomainError> {
        let allowed = matches!(
            (self.status, next),
            (
                HandoffStatus::Proposed,
                HandoffStatus::Approved | HandoffStatus::Cancelled
            ) | (
                HandoffStatus::Approved,
                HandoffStatus::Ready | HandoffStatus::Cancelled
            ) | (HandoffStatus::Ready, HandoffStatus::Uncertain)
        );
        if !allowed || at < self.updated_at {
            return Err(ProjectDomainError::InvalidHandoffTransition);
        }
        self.status = next;
        self.version = next_version(self.version)?;
        self.updated_at = at;
        self.validate()
    }

    pub fn validate(&self) -> Result<(), ProjectDomainError> {
        validate_scope(&self.scope)?;
        require_id(self.proposed_by, "handoff_proposed_by")?;
        require_text(&self.idempotency_key, "handoff_idempotency_key", 255)?;
        if !valid_sha256(&self.payload_digest)
            || self.version == 0
            || self.updated_at < self.created_at
            || self.approved_by == Some(self.proposed_by)
            || matches!(
                self.status,
                HandoffStatus::Approved
                    | HandoffStatus::Ready
                    | HandoffStatus::Accepted
                    | HandoffStatus::Rejected
                    | HandoffStatus::Uncertain
            ) && self.approved_by.is_none()
            || matches!(
                self.status,
                HandoffStatus::Accepted | HandoffStatus::Rejected
            ) != self.outcome.is_some()
        {
            return Err(ProjectDomainError::InvalidHandoff);
        }
        Ok(())
    }

    pub const fn id(&self) -> HandoffId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }
    pub const fn item_id(&self) -> ProjectNodeId {
        self.item_id
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn kind(&self) -> HandoffKind {
        self.kind
    }
    pub fn payload_digest(&self) -> &str {
        &self.payload_digest
    }
    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }
    pub const fn correlation_id(&self) -> CorrelationId {
        self.correlation_id
    }
    pub const fn status(&self) -> HandoffStatus {
        self.status
    }
    pub const fn proposed_by(&self) -> Uuid {
        self.proposed_by
    }
    pub const fn approved_by(&self) -> Option<Uuid> {
        self.approved_by
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn outcome(&self) -> Option<&HandoffOutcome> {
        self.outcome.as_ref()
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "state", rename_all = "snake_case")]
pub enum ProjectsAggregate {
    Project(Project),
    Workflow(WorkflowTemplate),
    Dependency(DependencyAgreement),
    Handoff(ProjectHandoff),
}

impl ProjectsAggregate {
    pub const fn id_uuid(&self) -> Uuid {
        match self {
            Self::Project(v) => v.id.as_uuid(),
            Self::Workflow(v) => v.id.as_uuid(),
            Self::Dependency(v) => v.id.as_uuid(),
            Self::Handoff(v) => v.id.as_uuid(),
        }
    }
    pub const fn version(&self) -> u64 {
        match self {
            Self::Project(v) => v.version,
            Self::Workflow(v) => v.version,
            Self::Dependency(v) => v.version,
            Self::Handoff(v) => v.version,
        }
    }
    pub const fn scope(&self) -> &AuthorityScope {
        match self {
            Self::Project(v) => &v.scope,
            Self::Workflow(v) => &v.scope,
            Self::Dependency(v) => &v.source.scope,
            Self::Handoff(v) => &v.scope,
        }
    }
    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Project(_) => "project",
            Self::Workflow(_) => "project_workflow_template",
            Self::Dependency(_) => "project_dependency",
            Self::Handoff(_) => "project_handoff",
        }
    }
    pub const fn occurred_at(&self) -> DateTime<Utc> {
        match self {
            Self::Project(v) => v.updated_at,
            Self::Workflow(v) => v.updated_at,
            Self::Dependency(v) => v.updated_at,
            Self::Handoff(v) => v.updated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProjectEvent {
    ProjectChanged {
        project_id: ProjectId,
        version: u64,
        at: DateTime<Utc>,
    },
    NodeChanged {
        project_id: ProjectId,
        node_id: ProjectNodeId,
        at: DateTime<Utc>,
    },
}

impl ProjectEvent {
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::ProjectChanged { .. } => "projects.project_changed",
            Self::NodeChanged { .. } => "projects.node_changed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProjectDomainError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("{field} is invalid or exceeds its bounded length")]
    InvalidText { field: &'static str },
    #[error("Projects require an exact company or branch scope")]
    InvalidScope,
    #[error("scope authority does not contain the linked record")]
    ScopeMismatch,
    #[error("project state is invalid")]
    InvalidProject,
    #[error("project node is invalid")]
    InvalidNode,
    #[error("workflow or core-state mapping is invalid")]
    InvalidWorkflow,
    #[error("workflow state is unknown")]
    UnknownWorkflowState,
    #[error("lifecycle transition is not allowed")]
    InvalidTransition,
    #[error("aggregate timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("aggregate version overflowed")]
    VersionOverflow,
    #[error("expected version does not match")]
    VersionConflict,
    #[error("project node already exists")]
    DuplicateNode,
    #[error("project node was not found")]
    NodeNotFound,
    #[error("hierarchy must be Project -> Phase/Epic -> Item -> Sub-item")]
    InvalidHierarchy,
    #[error("hierarchy or dependency cycle is not allowed")]
    HierarchyCycle,
    #[error("project contains unresolved work")]
    UnresolvedWork,
    #[error("project close reconciliation is incomplete")]
    ReconciliationIncomplete,
    #[error("comment promotion must pin an exact source revision")]
    InvalidPromotionSource,
    #[error("evidence is invalid")]
    InvalidEvidence,
    #[error("evidence identity was already applied")]
    DuplicateEvidence,
    #[error("dependency proposal is invalid")]
    InvalidDependency,
    #[error("dependency lifecycle transition is invalid")]
    InvalidDependencyTransition,
    #[error("dependency owner authority is denied")]
    DependencyAuthorityDenied,
    #[error("Projects decision requires exact authenticated-human evidence")]
    HumanDecisionRequired,
    #[error("handoff is invalid")]
    InvalidHandoff,
    #[error("handoff transition is invalid")]
    InvalidHandoffTransition,
    #[error("handoff maker-checker authority is denied")]
    HandoffAuthorityDenied,
    #[error("owner-context handoff outcome is invalid")]
    InvalidHandoffOutcome,
}

pub fn dependency_acceptance_decision_ref(
    dependency_id: DependencyId,
    side: DependencySide,
) -> String {
    let side = match side {
        DependencySide::Source => "source",
        DependencySide::Target => "target",
    };
    format!(
        "projects.dependency:{}:accept:{side}",
        dependency_id.as_uuid()
    )
}

pub fn dependency_rejection_decision_ref(dependency_id: DependencyId) -> String {
    format!("projects.dependency:{}:reject", dependency_id.as_uuid())
}

pub fn handoff_approval_decision_ref(handoff_id: HandoffId) -> String {
    format!("projects.handoff:{}:approve", handoff_id.as_uuid())
}

fn decision_principal_uuid(decision: &HumanDecisionProof) -> Result<Uuid, ProjectDomainError> {
    Uuid::parse_str(decision.principal_ref()).map_err(|_| ProjectDomainError::HumanDecisionRequired)
}

fn exact_human_decision_principal(
    decision: &HumanDecisionProof,
    scope: &AuthorityScope,
    decision_ref: &str,
) -> Result<Uuid, ProjectDomainError> {
    if decision.authority_scope() != scope
        || decision.decision_ref() != decision_ref
        || !decision.actor().is_human()
        || decision.actor().session_id.is_none()
    {
        return Err(ProjectDomainError::HumanDecisionRequired);
    }
    decision_principal_uuid(decision)
}

fn validate_scope(scope: &AuthorityScope) -> Result<(), ProjectDomainError> {
    scope
        .validate()
        .map_err(|_| ProjectDomainError::InvalidScope)?;
    if matches!(
        scope,
        AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(ProjectDomainError::InvalidScope)
    }
}

fn require_id(value: Uuid, field: &'static str) -> Result<(), ProjectDomainError> {
    if value.is_nil() {
        Err(ProjectDomainError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn require_text(
    value: &str,
    field: &'static str,
    maximum: usize,
) -> Result<(), ProjectDomainError> {
    if value.trim().is_empty() {
        return Err(ProjectDomainError::Required { field });
    }
    if value.len() > maximum || value.chars().any(char::is_control) {
        return Err(ProjectDomainError::InvalidText { field });
    }
    Ok(())
}

fn next_version(value: u64) -> Result<u64, ProjectDomainError> {
    value
        .checked_add(1)
        .ok_or(ProjectDomainError::VersionOverflow)
}

fn valid_sha256(value: &str) -> bool {
    let value = value.strip_prefix("sha256:").unwrap_or(value);
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn has_duplicates(values: &[Uuid]) -> bool {
    let mut seen = BTreeSet::new();
    values.iter().any(|value| !seen.insert(*value))
}

fn holding_id(scope: &AuthorityScope) -> Option<Uuid> {
    match scope {
        AuthorityScope::Holding { holding_id }
        | AuthorityScope::Company { holding_id, .. }
        | AuthorityScope::Branch { holding_id, .. }
        | AuthorityScope::Subtree { holding_id, .. } => Some(*holding_id),
        AuthorityScope::Platform => None,
    }
}

#[cfg(test)]
mod tests;
