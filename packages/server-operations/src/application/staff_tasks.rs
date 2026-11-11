//! Native Staff Work task/alert use cases.
//!
//! Every command is validated against the current selected actor and canonical
//! People/Projects/Collaboration/Workflow facts supplied by their public ports.
//! The repository owns one serializable transaction, the receipt/audit/outbox
//! ledger and owner rows. A replay is valid only when the key, operation,
//! record, request fingerprint and selected scope all agree.
use crate::domain::staff_tasks::{
    ActiveDelegation, ApprovalView, AttachmentRow, DecisionFact, DelegateTaskCommand,
    DelegationHistoryRow, MAX_ALERTS, MAX_LIST, RevokeTaskDelegationCommand, SaveTaskCommand,
    StaffTaskActor, StaffTaskAlertResult, StoredTaskReceipt, TaskAccessFacts, TaskAction,
    TaskDetail, TaskReceipt, TaskRow, TaskScope, TransitionPlan, TransitionTaskCommand, can_attach,
    can_configure, delegation_record_id, delegation_required, fingerprint_json,
    idempotency_conflict, incomplete_receipt, invalid_receipt, is_assigned, missing_delegation,
    permitted_actions, plan_transition, require_scope, task_access_denied, task_not_found,
    task_record_id,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::future::Future;
use std::pin::Pin;

pub type StaffTaskFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct StaffFact {
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}
#[derive(Clone, Debug)]
pub struct ProjectFact {
    pub owner_user_id: String,
    pub title: String,
}
#[derive(Clone, Debug)]
pub struct PostFact {
    pub id: String,
    pub author_id: String,
}
#[derive(Clone, Debug)]
pub struct RouteFact {
    pub blocked_reason: Option<String>,
}
#[derive(Clone, Debug)]
pub struct DelegationFact {
    pub id: String,
    pub delegate_user_id: String,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct SaveTaskPlan {
    pub key: CommandKey,
    pub operation: String,
    pub record_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub actor_id: String,
    pub assignment_id: String,
    pub input: SaveTaskCommand,
    pub due_at: Option<i64>,
    pub state: String,
    pub version: i64,
    pub revision: bool,
    pub now: i64,
    pub request_fingerprint: String,
    pub receipt: TaskReceipt,
}
#[derive(Clone, Debug)]
pub struct TransitionTaskPlan {
    pub key: CommandKey,
    pub operation: String,
    pub record_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub actor_id: String,
    pub assignment_id: String,
    pub input: TransitionTaskCommand,
    pub row: TaskRow,
    pub plan: TransitionPlan,
    pub now: i64,
    pub request_fingerprint: String,
    pub receipt: TaskReceipt,
}
#[derive(Clone, Debug)]
pub struct DelegateTaskPlan {
    pub key: CommandKey,
    pub operation: String,
    pub record_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub actor_id: String,
    pub assignment_id: String,
    pub task_id: String,
    pub delegation_id: String,
    pub delegate_user_id: String,
    pub expires_at: i64,
    pub reason: String,
    pub version: i64,
    pub now: i64,
    pub request_fingerprint: String,
    pub receipt: TaskReceipt,
}
#[derive(Clone, Debug)]
pub struct RevokeDelegationPlan {
    pub key: CommandKey,
    pub operation: String,
    pub record_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub actor_id: String,
    pub assignment_id: String,
    pub task_id: String,
    pub delegation_id: String,
    pub delegate_user_id: String,
    pub reason: String,
    pub version: i64,
    pub now: i64,
    pub request_fingerprint: String,
    pub receipt: TaskReceipt,
}
#[derive(Clone, Debug)]
pub struct AlertPlan {
    pub company_id: String,
    pub task_id: String,
    pub user_id: String,
    pub message: String,
    pub phase: &'static str,
    pub version: i64,
    pub now: i64,
}

/// Owner persistence boundary. Implementations must use the caller's retained
/// serializable unit and write the command receipt, audit/outbox and owner rows
/// atomically. A key may never commit a second time.
pub trait StaffTaskRepository: Send {
    type Error: From<BusinessError> + Send;
    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> StaffTaskFuture<'a, Option<StoredTaskReceipt>, Self::Error>;
    fn task<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        id: &'a str,
        now: i64,
    ) -> StaffTaskFuture<'a, Option<TaskRow>, Self::Error>;
    fn list<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        project_id: Option<&'a str>,
    ) -> StaffTaskFuture<'a, Vec<TaskRow>, Self::Error>;
    fn delegation<'a>(
        &'a mut self,
        company_id: &'a str,
        task_id: &'a str,
        delegation_id: &'a str,
        now: i64,
    ) -> StaffTaskFuture<'a, Option<DelegationFact>, Self::Error>;
    fn delegation_history<'a>(
        &'a mut self,
        company_id: &'a str,
        task_id: &'a str,
    ) -> StaffTaskFuture<'a, Vec<DelegationHistoryRow>, Self::Error>;
    fn save<'a>(&'a mut self, plan: SaveTaskPlan) -> StaffTaskFuture<'a, TaskReceipt, Self::Error>;
    fn transition<'a>(
        &'a mut self,
        plan: TransitionTaskPlan,
    ) -> StaffTaskFuture<'a, TaskReceipt, Self::Error>;
    fn delegate<'a>(
        &'a mut self,
        plan: DelegateTaskPlan,
    ) -> StaffTaskFuture<'a, TaskReceipt, Self::Error>;
    fn revoke<'a>(
        &'a mut self,
        plan: RevokeDelegationPlan,
    ) -> StaffTaskFuture<'a, TaskReceipt, Self::Error>;
    fn alert_rows<'a>(
        &'a mut self,
        now: i64,
        warning_window_ms: i64,
    ) -> StaffTaskFuture<'a, Vec<crate::domain::staff_tasks::AlertTask>, Self::Error>;
    fn stage_alert<'a>(&'a mut self, plan: AlertPlan) -> StaffTaskFuture<'a, bool, Self::Error>;
}

/// Current cross-owner facts and effects. None of these implementations live in
/// this crate: composition binds the owning People, Projects, Collaboration,
/// Files, PMS feature and Workflow ports. A missing fact is a refusal, never a
/// silent default.
pub trait StaffTaskSources: Send {
    type Error: From<BusinessError> + Send;
    fn active_staff<'a>(
        &'a mut self,
        company_id: &'a str,
        user_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<StaffFact>, Self::Error>;
    fn project<'a>(
        &'a mut self,
        company_id: &'a str,
        project_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<ProjectFact>, Self::Error>;
    fn same_person<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        user_id: &'a str,
    ) -> StaffTaskFuture<'a, bool, Self::Error>;
    fn same_person_between<'a>(
        &'a mut self,
        company_id: &'a str,
        user_a: &'a str,
        user_b: &'a str,
    ) -> StaffTaskFuture<'a, bool, Self::Error>;
    fn readable_post<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        post_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<PostFact>, Self::Error>;
    fn task_route<'a>(
        &'a mut self,
        scope: &'a TaskScope,
        creator_user_id: &'a str,
        assignee_user_id: &'a str,
        phase: &'a str,
        approvers: &'a [String],
        service_task: bool,
    ) -> StaffTaskFuture<'a, RouteFact, Self::Error>;
    fn approval_view<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        task: &'a TaskRow,
    ) -> StaffTaskFuture<'a, ApprovalView, Self::Error>;
    fn open_approval<'a>(
        &'a mut self,
        key: &'a str,
        task: &'a TaskRow,
        phase: &'a str,
        route: &'a RouteFact,
        version: i64,
    ) -> StaffTaskFuture<'a, (), Self::Error>;
    fn prepare_decision<'a>(
        &'a mut self,
        key: &'a str,
        task: &'a TaskRow,
        approve: bool,
        note: &'a str,
        expected_version: i64,
    ) -> StaffTaskFuture<'a, DecisionFact, Self::Error>;
    fn close_approvals<'a>(
        &'a mut self,
        key: &'a str,
        task_id: &'a str,
        reason: &'a str,
        expected_version: i64,
    ) -> StaffTaskFuture<'a, (), Self::Error>;
    fn attachments<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        task_id: &'a str,
    ) -> StaffTaskFuture<'a, Vec<AttachmentRow>, Self::Error>;
    fn person_name<'a>(
        &'a mut self,
        company_id: &'a str,
        user_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<String>, Self::Error>;
    fn feature_enabled<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> StaffTaskFuture<'a, bool, Self::Error>;
}

fn replay(
    stored: Option<StoredTaskReceipt>,
    operation: &str,
    record_id: &str,
    request_fingerprint: &str,
    actor: &StaffTaskActor,
) -> Result<Option<TaskReceipt>, BusinessError> {
    let Some(stored) = stored else {
        return Ok(None);
    };
    if stored.operation != operation
        || stored.record_id != record_id
        || stored.request_fingerprint != request_fingerprint
        || stored.company_id != actor.company_id
        || stored.branch_id != actor.branch_id
        || stored.department_id != actor.department_id
        || stored.actor_id != actor.user_id
    {
        return Err(idempotency_conflict());
    }
    let Some(result) = stored.result_json else {
        return Err(incomplete_receipt());
    };
    let receipt: TaskReceipt = serde_json::from_str(&result).map_err(|_| invalid_receipt())?;
    if receipt.id.is_empty() || receipt.version < 1 || receipt.state.is_empty() {
        return Err(invalid_receipt());
    }
    Ok(Some(receipt))
}

fn check_receipt(
    receipt: TaskReceipt,
    record_id: &str,
    state: &str,
    version: i64,
) -> Result<TaskReceipt, BusinessError> {
    if receipt.id != record_id || receipt.state != state || receipt.version != version {
        return Err(invalid_receipt());
    }
    Ok(receipt)
}

async fn facts<R, S>(
    actor: &StaffTaskActor,
    row: &TaskRow,
    repository: &mut R,
    sources: &mut S,
) -> Result<TaskAccessFacts, R::Error>
where
    R: StaffTaskRepository,
    S: StaffTaskSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    let same_creator = sources.same_person(actor, &row.creator_user_id).await?;
    let same_assignee = sources.same_person(actor, &row.assignee_user_id).await?;
    let same_delegate = match row.active_delegate_user_id.as_deref() {
        Some(user) => sources.same_person(actor, user).await?,
        None => false,
    };
    let (same_project_owner, project_member) = match row.project_id.as_deref() {
        Some(project_id) => match sources.project(&actor.company_id, project_id).await? {
            Some(project) => (
                sources.same_person(actor, &project.owner_user_id).await?,
                false,
            ),
            None => (false, false),
        },
        None => (false, false),
    };
    let approver = {
        let users = row.approvers()?;
        let mut found = false;
        for user in users {
            if sources.same_person(actor, &user).await? {
                found = true;
                break;
            }
        }
        found
    };
    let _ = repository;
    Ok(TaskAccessFacts {
        same_creator,
        same_assignee,
        same_delegate,
        same_project_owner,
        approver,
        project_member,
    })
}

async fn require_visible<R, S>(
    actor: &StaffTaskActor,
    row: &TaskRow,
    repository: &mut R,
    sources: &mut S,
) -> Result<TaskAccessFacts, R::Error>
where
    R: StaffTaskRepository,
    S: StaffTaskSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    require_scope(actor, row)?;
    let facts = facts(actor, row, repository, sources).await?;
    if !facts.can_read() {
        return Err(task_not_found().into());
    }
    Ok(facts)
}

async fn require_manage<R, S>(
    actor: &StaffTaskActor,
    row: &TaskRow,
    repository: &mut R,
    sources: &mut S,
) -> Result<TaskAccessFacts, R::Error>
where
    R: StaffTaskRepository,
    S: StaffTaskSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    let facts = require_visible(actor, row, repository, sources).await?;
    if !can_configure(&facts) {
        return Err(task_access_denied().into());
    }
    Ok(facts)
}

pub async fn list_tasks<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    project_id: Option<&str>,
    now: i64,
) -> Result<Vec<TaskRow>, R::Error>
where
    R: StaffTaskRepository,
    S: StaffTaskSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    actor.require_read(now)?;
    if let Some(project_id) = project_id {
        crate::domain::field_policy::identifier(project_id)?;
    }
    let rows = repository.list(actor, project_id).await?;
    let mut visible = Vec::new();
    for row in rows {
        if row.company_id != actor.company_id
            || row
                .branch_id
                .as_deref()
                .is_some_and(|branch| actor.branch_id.as_deref() != Some(branch))
            || row
                .department_id
                .as_deref()
                .is_some_and(|department| actor.department_id.as_deref() != Some(department))
        {
            continue;
        }
        let access = facts(actor, &row, repository, sources).await?;
        if access.can_read() {
            visible.push(row);
        }
        if visible.len() == MAX_LIST {
            break;
        }
    }
    Ok(visible)
}

pub async fn task_detail<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    id: &str,
    now: i64,
) -> Result<TaskDetail, R::Error>
where
    R: StaffTaskRepository,
    S: StaffTaskSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    actor.require_read(now)?;
    crate::domain::field_policy::identifier(id)?;
    let row = repository
        .task(actor, id, now)
        .await?
        .ok_or_else(task_not_found)?;
    let access = require_visible(actor, &row, repository, sources).await?;
    let approval = sources.approval_view(actor, &row).await?;
    let blocked = approval
        .blocked_reason
        .clone()
        .or_else(|| row.blocked_reason.clone());
    let permitted_actions = permitted_actions(&row, &access, Some(&approval), blocked.as_deref())?;
    let delegation = match row.active_delegation_id.as_deref() {
        Some(delegation_id) => {
            let delegate_user_id = row.active_delegate_user_id.clone().unwrap_or_default();
            let delegate_name = match row.active_delegate_name.clone() {
                Some(name) if !name.is_empty() => name,
                _ => sources
                    .person_name(&actor.company_id, &delegate_user_id)
                    .await?
                    .unwrap_or_else(|| delegate_user_id.clone()),
            };
            Some(ActiveDelegation {
                id: delegation_id.to_owned(),
                delegate_user_id,
                delegate_name,
                expires_at: row.active_delegate_expires_at.unwrap_or_default(),
                reason: row.active_delegate_reason.clone().unwrap_or_default(),
                version: row.active_delegate_version.unwrap_or(1),
            })
        }
        None => None,
    };
    let mut delegation_history = repository.delegation_history(&actor.company_id, id).await?;
    for history in &mut delegation_history {
        if let Some(name) = sources
            .person_name(&actor.company_id, &history.delegate_name)
            .await?
        {
            history.delegate_name = name;
        }
        if let Some(name) = sources
            .person_name(&actor.company_id, &history.delegator_name)
            .await?
        {
            history.delegator_name = name;
        }
    }
    let attachments = sources.attachments(actor, id).await?;
    Ok(TaskDetail {
        task: row,
        approval,
        permitted_actions,
        blocked_reason: blocked,
        delegation,
        delegation_history,
        attachments,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn save_task<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    id: Option<&str>,
    input: &SaveTaskCommand,
    key: &str,
    revision: bool,
    now: i64,
) -> Result<TaskReceipt, R::Error>
where
    R: StaffTaskRepository,
    S: StaffTaskSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    actor.require_write(now)?;
    input.validate()?;
    if let Some(id) = id {
        crate::domain::field_policy::identifier(id)?;
    }
    let key = CommandKey::new(key)?;
    let record_id = task_record_id(input, key.as_str(), id);
    let operation = if revision {
        "task.revise"
    } else if id.is_some() {
        "task.update"
    } else {
        "task.create"
    };
    let fingerprint = fingerprint_json(operation, &record_id, input)?;
    if let Some(receipt) = replay(
        repository.stored_receipt(key.as_str()).await?,
        operation,
        &record_id,
        &fingerprint,
        actor,
    )? {
        return Ok(receipt);
    }
    let current = match id {
        Some(id) => Some(
            repository
                .task(actor, id, now)
                .await?
                .ok_or_else(task_not_found)?,
        ),
        None => None,
    };
    if let Some(current) = current.as_ref() {
        let facts = require_manage(actor, current, repository, sources).await?;
        let _ = facts;
        let was_reviewed = !current.is_ordinary()?;
        if was_reviewed && input.is_ordinary() {
            return Err(BusinessError::invalid(
                "TASK_REVIEW_REQUIRED",
                "A reviewed task must keep at least one approval step.",
            )
            .into());
        }
        if revision && !input.is_ordinary() && input.reason.trim().is_empty() {
            return Err(BusinessError::invalid(
                "REVISION_REASON_REQUIRED",
                "Explain the material revision; approvals will restart.",
            )
            .into());
        }
    }
    if revision && id.is_none() {
        return Err(BusinessError::invalid("TASK_NOT_FOUND", "Choose the task to revise.").into());
    }
    let creator = current
        .as_ref()
        .map(|row| row.creator_user_id.as_str())
        .unwrap_or(actor.user_id.as_str());
    let staff = sources
        .active_staff(&actor.company_id, &actor.user_id)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::Forbidden,
                "TASK_STAFF_REQUIRED",
                "Choose a current active staff member.",
            )
        })?;
    sources
        .active_staff(&actor.company_id, &input.assignee_user_id)
        .await?
        .ok_or_else(|| {
            BusinessError::invalid(
                "TASK_ASSIGNEE_INVALID",
                "Choose a current active staff member as the assignee.",
            )
        })?;
    if let Some(project_id) = input.project_id.as_deref() {
        sources
            .project(&actor.company_id, project_id)
            .await?
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "TASK_PROJECT_NOT_FOUND",
                    "Project not found.",
                )
            })?;
    }
    let scope = TaskScope {
        company_id: actor.company_id.clone(),
        branch_id: current
            .as_ref()
            .and_then(|row| row.branch_id.clone())
            .or_else(|| staff.branch_id.clone()),
        department_id: current
            .as_ref()
            .and_then(|row| row.department_id.clone())
            .or_else(|| staff.department_id.clone()),
    };
    let service_task = false;
    let phases = [
        ("start", input.start_approvers.as_slice()),
        ("completion", input.completion_approvers.as_slice()),
    ];
    for (phase, approvers) in phases {
        let route = sources
            .task_route(
                &scope,
                creator,
                &input.assignee_user_id,
                phase,
                approvers,
                service_task,
            )
            .await?;
        if let Some(_blocked) = route.blocked_reason {
            return Err(BusinessError::invalid(
                "TASK_ROUTE_INVALID",
                "The approval route is invalid for the selected scope.",
            )
            .into());
        }
    }
    let state = if input.is_ordinary() {
        "ready"
    } else {
        "draft"
    };
    let version = current
        .as_ref()
        .map(|_row| input.expected_version + 1)
        .unwrap_or(1);
    if let Some(current) = current.as_ref()
        && current.version != input.expected_version
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "TASK_VERSION_CHANGED",
            "The task changed. Reload it and review again.",
        )
        .into());
    }
    let branch_id = current
        .as_ref()
        .and_then(|row| row.branch_id.clone())
        .or_else(|| staff.branch_id.clone());
    let department_id = current
        .as_ref()
        .and_then(|row| row.department_id.clone())
        .or_else(|| staff.department_id.clone());
    let receipt = repository
        .save(SaveTaskPlan {
            key,
            operation: operation.into(),
            record_id: record_id.clone(),
            company_id: actor.company_id.clone(),
            branch_id,
            department_id,
            actor_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            input: input.clone(),
            due_at: input.due_millis()?,
            state: state.into(),
            version,
            revision,
            now,
            request_fingerprint: fingerprint,
            receipt: TaskReceipt::new(record_id.clone(), state, version),
        })
        .await?;
    check_receipt(receipt, &record_id, state, version).map_err(Into::into)
}

pub async fn transition_task<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    id: &str,
    input: &TransitionTaskCommand,
    key: &str,
    now: i64,
) -> Result<TaskReceipt, R::Error>
where
    R: StaffTaskRepository,
    S: StaffTaskSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    actor.require_write(now)?;
    input.validate()?;
    crate::domain::field_policy::identifier(id)?;
    let key = CommandKey::new(key)?;
    let operation = format!("task.{}", input.action.as_str());
    let fingerprint = fingerprint_json(&operation, id, input)?;
    if let Some(receipt) = replay(
        repository.stored_receipt(key.as_str()).await?,
        &operation,
        id,
        &fingerprint,
        actor,
    )? {
        return Ok(receipt);
    }
    let row = repository
        .task(actor, id, now)
        .await?
        .ok_or_else(task_not_found)?;
    let facts = require_visible(actor, &row, repository, sources).await?;
    let approval = sources.approval_view(actor, &row).await?;
    if row.is_terminal() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "TASK_TERMINAL",
            "Completed or cancelled tasks cannot be changed.",
        )
        .into());
    }
    let permitted = permitted_actions(
        &row,
        &facts,
        Some(&approval),
        approval
            .blocked_reason
            .as_deref()
            .or(row.blocked_reason.as_deref()),
    )?;
    if !permitted.iter().any(|value| value == input.action.as_str()) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "TASK_ACTION_UNAVAILABLE",
            "This action is unavailable for this participant and task state.",
        )
        .into());
    }
    if row.version != input.expected_version {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "TASK_VERSION_CHANGED",
            "The task changed. Reload it and review again.",
        )
        .into());
    }
    let mut decision = None;
    match input.action {
        TaskAction::Submit | TaskAction::Complete => {
            let approvers = if input.action == TaskAction::Submit {
                row.start_approvers()?
            } else {
                row.completion_approvers()?
            };
            if !approvers.is_empty() {
                let phase = if input.action == TaskAction::Submit {
                    "start"
                } else {
                    "completion"
                };
                let route = sources
                    .task_route(
                        &row.scope(),
                        &row.creator_user_id,
                        &row.assignee_user_id,
                        phase,
                        &approvers,
                        false,
                    )
                    .await?;
                if let Some(_blocked) = route.blocked_reason {
                    return Err(BusinessError::invalid(
                        "TASK_ROUTE_INVALID",
                        "The approval route is invalid for the selected scope.",
                    )
                    .into());
                }
                sources
                    .open_approval(
                        key.as_str(),
                        &row,
                        phase,
                        &route,
                        input.expected_version + 1,
                    )
                    .await?;
            }
        }
        TaskAction::Approve | TaskAction::Reject => {
            decision = Some(
                sources
                    .prepare_decision(
                        key.as_str(),
                        &row,
                        input.action == TaskAction::Approve,
                        &input.note,
                        input.expected_version,
                    )
                    .await?,
            );
        }
        TaskAction::Cancel => {
            sources
                .close_approvals(key.as_str(), id, &input.note, input.expected_version)
                .await?;
        }
        TaskAction::Start | TaskAction::SaveProgress => {}
    }
    let plan = plan_transition(&row, input, &facts, Some(&approval), decision.as_ref())?;
    let state = plan.state;
    let version = plan.version;
    let receipt = repository
        .transition(TransitionTaskPlan {
            key,
            operation,
            record_id: id.to_owned(),
            company_id: actor.company_id.clone(),
            branch_id: row.branch_id.clone(),
            department_id: row.department_id.clone(),
            actor_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            input: input.clone(),
            row,
            plan,
            now,
            request_fingerprint: fingerprint,
            receipt: TaskReceipt::new(id, state, version),
        })
        .await?;
    check_receipt(receipt, id, state, version).map_err(Into::into)
}

pub async fn delegate_task<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    id: &str,
    input: &DelegateTaskCommand,
    key: &str,
    now: i64,
) -> Result<TaskReceipt, R::Error>
where
    R: StaffTaskRepository,
    S: StaffTaskSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    actor.require_write(now)?;
    let expires_at = input.validate(now)?;
    crate::domain::field_policy::identifier(id)?;
    let key = CommandKey::new(key)?;
    let operation = "task.delegate";
    let fingerprint = fingerprint_json(operation, id, input)?;
    if let Some(receipt) = replay(
        repository.stored_receipt(key.as_str()).await?,
        operation,
        id,
        &fingerprint,
        actor,
    )? {
        return Ok(receipt);
    }
    let row = repository
        .task(actor, id, now)
        .await?
        .ok_or_else(task_not_found)?;
    let facts = require_visible(actor, &row, repository, sources).await?;
    if !can_configure(&facts) {
        return Err(delegation_required().into());
    }
    if row.is_terminal() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "TASK_TERMINAL",
            "Completed or cancelled tasks cannot be delegated.",
        )
        .into());
    }
    if sources.same_person(actor, &input.delegate_user_id).await?
        || sources
            .same_person_between(
                &actor.company_id,
                &input.delegate_user_id,
                &row.assignee_user_id,
            )
            .await?
    {
        return Err(BusinessError::invalid(
            "TASK_DELEGATE_INVALID",
            "Choose a different active staff member as the temporary delegate.",
        )
        .into());
    }
    if row
        .approvers()?
        .iter()
        .any(|user| user == &input.delegate_user_id)
    {
        return Err(BusinessError::invalid(
            "TASK_DELEGATE_INDEPENDENCE",
            "A configured approver cannot also receive temporary task ownership.",
        )
        .into());
    }
    sources
        .active_staff(&actor.company_id, &input.delegate_user_id)
        .await?
        .ok_or_else(|| {
            BusinessError::invalid(
                "TASK_DELEGATE_INVALID",
                "Choose a current active staff member as the delegate.",
            )
        })?;
    let delegation_id = delegation_record_id(key.as_str());
    let version = input.expected_version + 1;
    let receipt = TaskReceipt::new(id, "delegated", version);
    let committed = repository
        .delegate(DelegateTaskPlan {
            key,
            operation: operation.into(),
            record_id: id.to_owned(),
            company_id: actor.company_id.clone(),
            branch_id: row.branch_id.clone(),
            department_id: row.department_id.clone(),
            actor_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            task_id: id.to_owned(),
            delegation_id,
            delegate_user_id: input.delegate_user_id.clone(),
            expires_at,
            reason: input.reason.trim().into(),
            version,
            now,
            request_fingerprint: fingerprint,
            receipt,
        })
        .await?;
    check_receipt(committed, id, "delegated", version).map_err(Into::into)
}

pub async fn revoke_task_delegation<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    id: &str,
    input: &RevokeTaskDelegationCommand,
    key: &str,
    now: i64,
) -> Result<TaskReceipt, R::Error>
where
    R: StaffTaskRepository,
    S: StaffTaskSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    actor.require_write(now)?;
    input.validate()?;
    crate::domain::field_policy::identifier(id)?;
    let key = CommandKey::new(key)?;
    let operation = "task.delegate-revoke";
    let record_id = format!("{id}:{}", input.delegation_id);
    let fingerprint = fingerprint_json(operation, &record_id, input)?;
    if let Some(receipt) = replay(
        repository.stored_receipt(key.as_str()).await?,
        operation,
        &record_id,
        &fingerprint,
        actor,
    )? {
        return Ok(receipt);
    }
    let row = repository
        .task(actor, id, now)
        .await?
        .ok_or_else(task_not_found)?;
    require_manage(actor, &row, repository, sources).await?;
    let delegation = repository
        .delegation(&actor.company_id, id, &input.delegation_id, now)
        .await?
        .ok_or_else(missing_delegation)?;
    let version = input.expected_version + 1;
    let receipt = TaskReceipt::new(id, "revoked", version);
    let committed = repository
        .revoke(RevokeDelegationPlan {
            key,
            operation: operation.into(),
            record_id: record_id.clone(),
            company_id: actor.company_id.clone(),
            branch_id: row.branch_id.clone(),
            department_id: row.department_id.clone(),
            actor_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            task_id: id.to_owned(),
            delegation_id: delegation.id,
            delegate_user_id: delegation.delegate_user_id,
            reason: input.reason.trim().into(),
            version,
            now,
            request_fingerprint: fingerprint,
            receipt,
        })
        .await?;
    check_receipt(committed, id, "revoked", version).map_err(Into::into)
}

pub async fn escalate_staff_tasks<R, S>(
    repository: &mut R,
    sources: &mut S,
    now: i64,
    warning_window_ms: i64,
) -> Result<StaffTaskAlertResult, R::Error>
where
    R: StaffTaskRepository,
    S: StaffTaskSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    if !(0..=7 * 86_400_000).contains(&warning_window_ms) {
        return Err(BusinessError::invalid(
            "TASK_ALERT_WINDOW_INVALID",
            "Use a bounded task alert window.",
        )
        .into());
    }
    let rows = repository.alert_rows(now, warning_window_ms).await?;
    if rows.len() > MAX_ALERTS {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "TASK_ALERT_LIMIT",
            "The task alert cohort exceeds its bounded read.",
        )
        .into());
    }
    let mut due_soon = 0;
    let mut overdue = 0;
    for mut task in rows {
        if !sources.feature_enabled(&task.company_id).await? {
            continue;
        }
        if task.project_owner_id.is_none()
            && let Some(project_id) = task.project_id.clone()
        {
            task.project_owner_id = sources
                .project(&task.company_id, &project_id)
                .await?
                .map(|project| project.owner_user_id);
        }
        let is_overdue = task.due_at < now;
        let phase = if is_overdue { "overdue" } else { "due-soon" };
        let message = if is_overdue {
            format!(
                "Task overdue: {}. Follow up or escalate the owner.",
                task.title
            )
        } else {
            format!("Task due within 24 hours: {}.", task.title)
        };
        let mut recipients = vec![task.assignee_user_id.clone()];
        if let Some(delegate) = task.delegate_user_id.clone() {
            recipients.push(delegate);
        }
        if is_overdue {
            recipients.push(task.creator_user_id.clone());
            if let Some(owner) = task.project_owner_id.clone() {
                recipients.push(owner);
            }
        }
        recipients.extend(task.approvers.iter().cloned());
        recipients.sort();
        recipients.dedup();
        recipients.retain(|user| !user.is_empty());
        for user_id in recipients {
            let inserted = repository
                .stage_alert(AlertPlan {
                    company_id: task.company_id.clone(),
                    task_id: task.id.clone(),
                    user_id,
                    message: message.clone(),
                    phase,
                    version: task.version,
                    now,
                })
                .await?;
            if inserted {
                if is_overdue {
                    overdue += 1;
                } else {
                    due_soon += 1;
                }
            }
        }
    }
    Ok(StaffTaskAlertResult { due_soon, overdue })
}

pub fn can_attach_task(task: &TaskRow, facts: &TaskAccessFacts) -> Result<bool, BusinessError> {
    can_attach(task, facts)
}
pub fn task_is_assigned(facts: &TaskAccessFacts) -> bool {
    is_assigned(facts)
}
