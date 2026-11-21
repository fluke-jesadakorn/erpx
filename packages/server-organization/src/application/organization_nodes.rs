//! Organization node, business-unit, sharing and lifecycle use cases.
//!
//! The repository supplies current sources and owns the transaction; this
//! layer keeps the retained command order, optimistic version checks,
//! independent review and relocation fences. It never opens a transaction.
use crate::domain::organization_nodes::{
    ChangeFilters, NodeChangeAction, NodeChangeInput, NodeChangeState, NodeDecisionInput,
    NodeImpact, NodeLifecycleKind, NodeReviewDecision, NodeStatus, OrganizationActor,
    OrganizationScopeKind, VisibleNodeScope, change_not_found, company_authority_required,
    validate_cancellable, validate_change_owner, validate_editable, validate_lifecycle_impact,
    validate_preview_snapshot, validate_relocation_children, validate_relocation_slug_conflict,
    validate_reserved_draft, validate_review_before_apply, validate_review_state,
    validate_target_facts,
};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, HoldingId, RecordId, UserId};
use serde::Serialize;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type OrganizationFuture<'a, T, E = BusinessError> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Portable command identity owned by this layer. The adapter converts it to
/// the shared Workflow actor on the transaction; this layer has no effectful
/// dependency on that crate.
#[derive(Clone, Debug)]
pub struct OrganizationCommand {
    pub user_id: UserId,
    pub assignment_id: erpx_server_shared::public::AssignmentId,
    pub role: String,
    pub company_id: CompanyId,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub authority: Option<crate::domain::organization_nodes::OrganizationAuthority>,
    /// Canonical ordered IAM JSON from the retained projection.
    pub iam_json: String,
    pub assignment_scope_kind: String,
    pub assignment_scope_node_id: Option<String>,
}

/// Current authority supplied by the composition root. It is not
/// request-deserializable and grants no work by itself.
#[derive(Clone, Debug)]
pub struct OrganizationCommandActor {
    pub holding_id: HoldingId,
    pub command: OrganizationCommand,
    pub modules: Vec<String>,
    pub permissions: Vec<String>,
}

impl OrganizationCommandActor {
    pub fn projection(&self) -> OrganizationActor {
        OrganizationActor {
            holding_id: self.holding_id.clone(),
            company_id: self.command.company_id.clone(),
            user_id: self.command.user_id.clone(),
            assignment_id: self.command.assignment_id.clone(),
            role: self.command.role.clone(),
            branch_id: self
                .command
                .branch_id
                .as_ref()
                .and_then(|value| RecordId::new(value.clone()).ok()),
            department_id: self
                .command
                .department_id
                .as_ref()
                .and_then(|value| RecordId::new(value.clone()).ok()),
            scope_kind: match self.command.assignment_scope_kind.as_str() {
                "branch" => OrganizationScopeKind::Branch,
                "department" => OrganizationScopeKind::Department,
                _ => OrganizationScopeKind::Holding,
            },
            assignment_scope_kind: match self.command.assignment_scope_kind.as_str() {
                "branch" => OrganizationScopeKind::Branch,
                "department" => OrganizationScopeKind::Department,
                _ => OrganizationScopeKind::Holding,
            },
            permissions: self.permissions.clone(),
            authority: self.command.authority.clone(),
        }
    }

    pub fn actor(&self) -> Result<OrganizationActor, BusinessError> {
        let actor = self.projection();
        actor.company_authority()?;
        Ok(actor)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceNode {
    pub id: String,
    pub kind: String,
    pub slug: String,
    pub display_name: String,
    pub parent_id: Option<String>,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub status: String,
    pub hierarchy_revision: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkspaceEvent {
    pub action: String,
    pub actor_name: String,
    pub version: i64,
    pub reason: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkspaceChange {
    pub id: String,
    pub target_node_id: Option<String>,
    pub action: String,
    pub node_kind: String,
    pub slug: Option<String>,
    pub display_name: String,
    pub branch_id: Option<String>,
    pub source_branch_id: Option<String>,
    pub expected_node_version: Option<i64>,
    pub state: String,
    pub version: i64,
    pub requested_by: String,
    pub reviewed_by: Option<String>,
    pub reason: String,
    pub review_reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub reviewed_at: Option<i64>,
    pub applied_at: Option<i64>,
    #[serde(rename = "canReview")]
    pub can_review: bool,
    pub events: Vec<WorkspaceEvent>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeQueue {
    pub record_id: Option<String>,
    pub state: String,
    pub query: String,
    pub page: i64,
    pub page_size: i64,
    pub total: i64,
    pub pages: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SharedChannel {
    pub node_id: String,
    pub room_id: String,
    pub name: String,
    pub kind: String,
    pub status: String,
}

/// Organization-owned position catalog row. The page read carries the same
/// placement/level facts the retained overview projects, never a roster row.
#[derive(Clone, Debug, Serialize)]
pub struct WorkspacePosition {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub title: String,
    pub level_id: String,
    pub level_number: i64,
    pub reports_to_position_id: Option<String>,
    pub requirements: String,
    pub is_ceo: i64,
    pub roster_required: i64,
    pub active: i64,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkspaceLevel {
    pub id: String,
    pub name: String,
    pub level_number: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkspaceTemplate {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub positions: Vec<Value>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationWorkspace {
    pub nodes: Vec<WorkspaceNode>,
    pub positions: Vec<WorkspacePosition>,
    pub levels: Vec<WorkspaceLevel>,
    pub templates: Vec<WorkspaceTemplate>,
    pub node_changes: Vec<WorkspaceChange>,
    pub change_queue: ChangeQueue,
    pub authorized_channel_nodes: Vec<String>,
    pub channels: Vec<SharedChannel>,
    pub actor_id: String,
    pub can_manage: bool,
    pub can_manage_units: bool,
    pub can_manage_spending: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResult {
    pub impact: NodeImpact,
    pub preview_snapshot: String,
    pub source_branch_id: Option<String>,
    pub destination_branch_id: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SaveTargetFacts {
    pub node: Option<NodeTarget>,
    pub branch_active: bool,
    pub reserved_department: bool,
    pub has_children: bool,
    pub slug_conflict: bool,
    pub relocation_snapshot: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NodeTarget {
    pub kind: NodeLifecycleKind,
    pub branch_id: Option<String>,
    pub slug: String,
    pub status: NodeStatus,
    pub hierarchy_revision: i64,
}

#[derive(Clone, Debug)]
pub struct ChangeRow {
    pub id: String,
    pub target_node_id: Option<String>,
    pub action: NodeChangeAction,
    pub node_kind: NodeLifecycleKind,
    pub slug: Option<String>,
    pub display_name: String,
    pub branch_id: Option<String>,
    pub source_branch_id: Option<String>,
    pub expected_node_version: Option<i64>,
    pub state: NodeChangeState,
    pub version: i64,
    pub requested_by: UserId,
    pub reviewed_by: Option<UserId>,
    pub reason: String,
    pub review_reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub reviewed_at: Option<i64>,
    pub applied_at: Option<i64>,
}

impl ChangeRow {
    /// Exact retained re-validation of a submitted change before apply.
    pub fn as_input(&self) -> NodeChangeInput {
        NodeChangeInput {
            action: self.action,
            target_node_id: self
                .target_node_id
                .as_ref()
                .and_then(|id| RecordId::new(id.clone()).ok()),
            kind: self.node_kind,
            slug: self.slug.clone(),
            display_name: self.display_name.clone(),
            branch_id: self
                .branch_id
                .as_ref()
                .and_then(|id| RecordId::new(id.clone()).ok()),
            expected_node_version: self.expected_node_version,
            expected_version: self.version,
            reason: self.reason.clone(),
            preview_snapshot: None,
        }
    }
}

pub struct SaveChangeCommand {
    pub actor: OrganizationCommandActor,
    pub key: String,
    pub record_id: String,
    pub input: NodeChangeInput,
    pub submit: bool,
    pub existing: bool,
    pub source_branch_id: Option<String>,
    pub slug: Option<String>,
}

pub struct ReviewChangeCommand {
    pub actor: OrganizationCommandActor,
    pub key: String,
    pub change: ChangeRow,
    pub input: NodeDecisionInput,
    pub state: NodeChangeState,
    pub node_id: Option<String>,
}

pub struct CancelChangeCommand {
    pub actor: OrganizationCommandActor,
    pub key: String,
    pub change: ChangeRow,
    pub version: i64,
}

pub trait OrganizationNodeRepository: Send {
    type Error: From<BusinessError> + Send;

    fn workspace<'a>(
        &'a mut self,
        actor: &'a OrganizationActor,
        filters: &'a ChangeFilters,
    ) -> OrganizationFuture<'a, OrganizationWorkspace, Self::Error>;

    fn replay<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        key: &'a str,
        operation: &'a str,
        record_id: &'a str,
        input: &'a Value,
    ) -> OrganizationFuture<'a, Option<Value>, Self::Error>;

    fn change<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> OrganizationFuture<'a, Option<ChangeRow>, Self::Error>;

    fn target<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> OrganizationFuture<'a, Option<(Option<String>, String)>, Self::Error>;

    fn save_target_facts<'a>(
        &'a mut self,
        actor: &'a OrganizationActor,
        input: &'a NodeChangeInput,
        current: Option<&'a ChangeRow>,
    ) -> OrganizationFuture<'a, SaveTargetFacts, Self::Error>;

    fn preview<'a>(
        &'a mut self,
        company: &'a CompanyId,
        node_id: &'a str,
    ) -> OrganizationFuture<'a, (NodeImpact, String), Self::Error>;

    fn independent_reviewer<'a>(
        &'a mut self,
        actor: &'a OrganizationActor,
        requester: &'a UserId,
    ) -> OrganizationFuture<'a, bool, Self::Error>;

    fn submitted_snapshot<'a>(
        &'a mut self,
        change_id: &'a str,
    ) -> OrganizationFuture<'a, Option<String>, Self::Error>;

    fn relocation_snapshot<'a>(
        &'a mut self,
        company: &'a CompanyId,
        node_id: &'a str,
    ) -> OrganizationFuture<'a, String, Self::Error>;

    fn persist_save<'a>(
        &'a mut self,
        command: &'a SaveChangeCommand,
        now: i64,
    ) -> OrganizationFuture<'a, Value, Self::Error>;

    fn persist_review<'a>(
        &'a mut self,
        command: &'a ReviewChangeCommand,
        now: i64,
    ) -> OrganizationFuture<'a, Value, Self::Error>;

    fn persist_cancel<'a>(
        &'a mut self,
        command: &'a CancelChangeCommand,
        now: i64,
    ) -> OrganizationFuture<'a, Value, Self::Error>;
}

fn save_operation(submit: bool) -> &'static str {
    if submit {
        "organization.node-change.submit"
    } else {
        "organization.node-change.save"
    }
}

fn record_id_for(key: &str, existing: Option<&str>) -> String {
    existing
        .map(str::to_owned)
        .unwrap_or_else(|| format!("org-change-{key}"))
}

fn input_json(input: &NodeChangeInput) -> Result<Value, BusinessError> {
    let mut object = serde_json::Map::new();
    object.insert("action".into(), Value::String(input.action.as_str().into()));
    object.insert(
        "targetNodeId".into(),
        input
            .target_node_id
            .as_ref()
            .map(|id| Value::String(id.as_str().into()))
            .unwrap_or(Value::Null),
    );
    object.insert("kind".into(), Value::String(input.kind.as_str().into()));
    object.insert(
        "slug".into(),
        input
            .slug
            .as_ref()
            .map(|slug| Value::String(slug.clone()))
            .unwrap_or(Value::Null),
    );
    object.insert(
        "displayName".into(),
        Value::String(input.display_name.clone()),
    );
    object.insert(
        "branchId".into(),
        input
            .branch_id
            .as_ref()
            .map(|id| Value::String(id.as_str().into()))
            .unwrap_or(Value::Null),
    );
    object.insert(
        "expectedNodeVersion".into(),
        input
            .expected_node_version
            .map(Value::from)
            .unwrap_or(Value::Null),
    );
    object.insert(
        "expectedVersion".into(),
        Value::from(input.expected_version),
    );
    object.insert("reason".into(), Value::String(input.reason.clone()));
    if let Some(snapshot) = &input.preview_snapshot {
        object.insert("previewSnapshot".into(), Value::String(snapshot.clone()));
    }
    Ok(Value::Object(object))
}

fn decision_json(input: &NodeDecisionInput) -> Value {
    let mut object = serde_json::Map::new();
    object.insert(
        "expectedVersion".into(),
        Value::from(input.expected_version),
    );
    object.insert(
        "decision".into(),
        Value::String(
            match input.decision {
                NodeReviewDecision::Approve => "approve",
                NodeReviewDecision::Reject => "reject",
            }
            .into(),
        ),
    );
    object.insert("reason".into(), Value::String(input.reason.clone()));
    if !input.attachment_set_ids.is_empty() {
        object.insert(
            "attachmentSetIds".into(),
            Value::Array(
                input
                    .attachment_set_ids
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }
    Value::Object(object)
}

/// Retained `saveChange` ordering with command replay before validation.
pub async fn save_change<R: OrganizationNodeRepository>(
    repository: &mut R,
    actor: OrganizationCommandActor,
    key: &str,
    id: Option<&str>,
    input: NodeChangeInput,
    submit: bool,
    clock: &dyn ClockSource,
) -> Result<Value, R::Error> {
    let projection = actor.actor()?;
    let record_id = record_id_for(key, id);
    let operation = save_operation(submit);
    let receipt_input = input_json(&input)?;
    if let Some(prior) = repository
        .replay(&actor, key, operation, &record_id, &receipt_input)
        .await?
    {
        return Ok(prior);
    }
    input.validate_structure()?;
    let current = match id {
        Some(id) => {
            let current = repository
                .change(&actor.command.company_id, id)
                .await?
                .ok_or_else(change_not_found)?;
            validate_change_owner(&current.requested_by, &projection)?;
            validate_editable(current.state)?;
            Some(current)
        }
        None => None,
    };
    if input.is_relocation() {
        let facts = repository
            .save_target_facts(&projection, &input, current.as_ref())
            .await?;
        if input.expected_version == 0 {
            validate_reserved_draft(facts.reserved_department)?;
        }
        let branch_id = input
            .branch_id
            .as_ref()
            .map(RecordId::as_str)
            .map(str::to_owned);
        let _ = branch_id;
        validate_target_facts(
            &input,
            facts
                .node
                .as_ref()
                .map(|node| crate::domain::organization_nodes::NodeFacts {
                    id_kind: node.kind,
                    branch_id: node
                        .branch_id
                        .as_ref()
                        .and_then(|id| RecordId::new(id.clone()).ok()),
                    status: node.status,
                    hierarchy_revision: node.hierarchy_revision,
                }),
            facts.branch_active,
            input.branch_id.as_ref(),
        )?;
        validate_relocation_children(facts.has_children)?;
        validate_relocation_slug_conflict(facts.slug_conflict)?;
        if let Some(snapshot) = &input.preview_snapshot {
            let current = facts.relocation_snapshot.as_deref().unwrap_or_default();
            validate_preview_snapshot(Some(snapshot), current)?;
        }
    } else {
        let facts = repository
            .save_target_facts(&projection, &input, current.as_ref())
            .await?;
        validate_target_facts(
            &input,
            facts
                .node
                .as_ref()
                .map(|node| crate::domain::organization_nodes::NodeFacts {
                    id_kind: node.kind,
                    branch_id: node
                        .branch_id
                        .as_ref()
                        .and_then(|id| RecordId::new(id.clone()).ok()),
                    status: node.status,
                    hierarchy_revision: node.hierarchy_revision,
                }),
            facts.branch_active,
            input.branch_id.as_ref(),
        )?;
    }
    let target = match &input.target_node_id {
        Some(id) => {
            repository
                .target(&actor.command.company_id, id.as_str())
                .await?
        }
        None => None,
    };
    let source_branch_id = if input.is_relocation() {
        target
            .as_ref()
            .and_then(|(branch, _)| branch.clone())
            .or_else(|| {
                current
                    .as_ref()
                    .and_then(|change| change.source_branch_id.clone())
            })
    } else {
        None
    };
    let slug = if input.is_relocation() {
        target
            .as_ref()
            .map(|(_, slug)| slug.clone())
            .or_else(|| current.as_ref().and_then(|change| change.slug.clone()))
    } else {
        input.slug.clone()
    };
    let command = SaveChangeCommand {
        actor,
        key: key.to_owned(),
        record_id,
        input,
        submit,
        existing: id.is_some(),
        source_branch_id,
        slug,
    };
    repository.persist_save(&command, clock.now().get()).await
}

/// Retained `previewChange`: structural checks and current impact only.
pub async fn preview_change<R: OrganizationNodeRepository>(
    repository: &mut R,
    actor: OrganizationCommandActor,
    input: NodeChangeInput,
) -> Result<PreviewResult, R::Error> {
    let projection = actor.actor()?;
    input.validate_structure()?;
    if !input.is_relocation() || input.kind != NodeLifecycleKind::Department {
        return Err(BusinessError::invalid(
            "RELOCATION_PREVIEW_REQUIRED",
            "Choose a department relocation.",
        )
        .into());
    }
    let facts = repository
        .save_target_facts(&projection, &input, None)
        .await?;
    validate_target_facts(
        &input,
        facts
            .node
            .as_ref()
            .map(|node| crate::domain::organization_nodes::NodeFacts {
                id_kind: node.kind,
                branch_id: node
                    .branch_id
                    .as_ref()
                    .and_then(|id| RecordId::new(id.clone()).ok()),
                status: node.status,
                hierarchy_revision: node.hierarchy_revision,
            }),
        facts.branch_active,
        input.branch_id.as_ref(),
    )?;
    validate_relocation_children(facts.has_children)?;
    validate_relocation_slug_conflict(facts.slug_conflict)?;
    let node_id = input.target_node_id.as_ref().ok_or_else(|| {
        BusinessError::invalid("NODE_VERSION_REQUIRED", "Choose the current department.")
    })?;
    let (impact, preview_snapshot) = repository
        .preview(&actor.command.company_id, node_id.as_str())
        .await?;
    let destination = input.branch_id.as_ref().map(|id| id.as_str().to_owned());
    Ok(PreviewResult {
        impact,
        preview_snapshot,
        source_branch_id: facts.node.as_ref().and_then(|node| node.branch_id.clone()),
        destination_branch_id: destination,
    })
}

/// Retained `reviewChange`: independent review, version, relocation recheck and
/// the safe lifecycle check before the single-transaction apply.
pub async fn review_change<R: OrganizationNodeRepository>(
    repository: &mut R,
    actor: OrganizationCommandActor,
    id: &str,
    input: NodeDecisionInput,
    key: &str,
    clock: &dyn ClockSource,
) -> Result<Value, R::Error> {
    let projection = actor.actor()?;
    let operation = "organization.node-change.review";
    let receipt_input = decision_json(&input);
    if let Some(prior) = repository
        .replay(&actor, key, operation, id, &receipt_input)
        .await?
    {
        return Ok(prior);
    }
    input.require_unbound_proof()?;
    let change = repository
        .change(&actor.command.company_id, id)
        .await?
        .ok_or_else(change_not_found)?;
    let independent = repository
        .independent_reviewer(&projection, &change.requested_by)
        .await?;
    validate_review_state(
        &change.requested_by,
        change.reviewed_by.as_ref(),
        &projection,
        change.state,
        change.version,
        input.expected_version,
        independent,
    )?;
    let state = match input.decision {
        NodeReviewDecision::Approve => NodeChangeState::Applied,
        NodeReviewDecision::Reject => NodeChangeState::Rejected,
    };
    let mut node_id = change.target_node_id.clone();
    if input.decision == NodeReviewDecision::Approve {
        if change.action == NodeChangeAction::Relocate {
            let submitted = repository.submitted_snapshot(id).await?;
            let target = change.target_node_id.clone().unwrap_or_default();
            let current = repository
                .relocation_snapshot(&actor.command.company_id, &target)
                .await?;
            validate_review_before_apply(submitted.as_ref(), &current)?;
        }
        let revalidated = change.as_input();
        revalidated.validate_structure()?;
        let facts = repository
            .save_target_facts(&projection, &revalidated, Some(&change))
            .await?;
        validate_target_facts(
            &revalidated,
            facts
                .node
                .as_ref()
                .map(|node| crate::domain::organization_nodes::NodeFacts {
                    id_kind: node.kind,
                    branch_id: node
                        .branch_id
                        .as_ref()
                        .and_then(|id| RecordId::new(id.clone()).ok()),
                    status: node.status,
                    hierarchy_revision: node.hierarchy_revision,
                }),
            facts.branch_active,
            revalidated.branch_id.as_ref(),
        )?;
        if change.action == NodeChangeAction::Create {
            node_id = Some(format!("org-node-{id}"));
        } else if change.action == NodeChangeAction::Relocate {
            node_id = Some(format!("org-node-{id}"));
            validate_relocation_children(facts.has_children)?;
            validate_relocation_slug_conflict(facts.slug_conflict)?;
            if let Some(target) = &change.target_node_id {
                let (impact, _) = repository
                    .preview(&actor.command.company_id, target)
                    .await?;
                validate_lifecycle_impact(change.action, impact)?;
            }
        }
        if matches!(
            change.action,
            NodeChangeAction::Suspend | NodeChangeAction::Archive
        ) {
            let target = change.target_node_id.clone().unwrap_or_default();
            let (impact, _) = repository
                .preview(&actor.command.company_id, &target)
                .await?;
            validate_lifecycle_impact(change.action, impact)?;
        }
    }
    let command = ReviewChangeCommand {
        actor,
        key: key.to_owned(),
        change,
        input,
        state,
        node_id,
    };
    repository.persist_review(&command, clock.now().get()).await
}

/// Retained `cancelChange`: owner-only, current state and exact version.
pub async fn cancel_change<R: OrganizationNodeRepository>(
    repository: &mut R,
    actor: OrganizationCommandActor,
    id: &str,
    expected_version: i64,
    key: &str,
    clock: &dyn ClockSource,
) -> Result<Value, R::Error> {
    let projection = actor.actor()?;
    let operation = "organization.node-change.cancel";
    let mut receipt = serde_json::Map::new();
    receipt.insert("expectedVersion".into(), Value::from(expected_version));
    let receipt_input = Value::Object(receipt);
    if let Some(prior) = repository
        .replay(&actor, key, operation, id, &receipt_input)
        .await?
    {
        return Ok(prior);
    }
    let change = repository
        .change(&actor.command.company_id, id)
        .await?
        .ok_or_else(change_not_found)?;
    validate_change_owner(&change.requested_by, &projection)?;
    validate_cancellable(change.state, change.version, expected_version)?;
    let command = CancelChangeCommand {
        actor,
        key: key.to_owned(),
        version: change.version + 1,
        change,
    };
    repository.persist_cancel(&command, clock.now().get()).await
}

pub async fn read_workspace<R: OrganizationNodeRepository>(
    repository: &mut R,
    actor: &OrganizationCommandActor,
    filters: &ChangeFilters,
) -> Result<OrganizationWorkspace, R::Error> {
    let projection = actor.projection();
    repository.workspace(&projection, filters).await
}

/// Clock input stays a trait so application tests can supply a fixed instant.
pub trait ClockSource: Sync {
    fn now(&self) -> erpx_server_shared::public::TimestampMillis;
}

impl<C: Clock + Sync> ClockSource for C {
    fn now(&self) -> erpx_server_shared::public::TimestampMillis {
        Clock::now(self)
    }
}

/// Stable scope guard for the visibility read.
pub fn assert_node_scope(scope: &VisibleNodeScope) -> Result<(), BusinessError> {
    match scope {
        VisibleNodeScope::Company => Ok(()),
        VisibleNodeScope::Branch(Some(_)) | VisibleNodeScope::Department(Some(_)) => Ok(()),
        _ => Err(company_authority_required()),
    }
}
