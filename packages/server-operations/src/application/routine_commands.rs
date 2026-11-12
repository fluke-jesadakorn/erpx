//! Native routine template/schedule use cases.
//!
//! Every command is validated against the current selected actor and canonical
//! People/Projects/Workflow facts supplied by their public ports. The repository
//! owns one serializable transaction, the command receipt/audit/outbox ledger,
//! the owner rows and the routine execution authorization lineage. A replay is
//! valid only when the key, operation, record, request fingerprint and selected
//! scope all agree.
use crate::domain::routine_commands::{
    RoutineAuthority, RoutineReceipt, RoutineScheduleRow, RoutineTemplateRow, RunRoutineTemplateCommand,
    ScheduleStateCommand, SaveRoutineScheduleCommand, SaveRoutineTemplateCommand,
    StoredRoutineReceipt, fingerprint_json, idempotency_conflict, incomplete_receipt,
    invalid_receipt, next_occurrence, run_fingerprint, schedule_ended, schedule_not_found,
    source_changed, template_not_found, template_owner_required, template_unpublished,
};
use crate::application::staff_tasks::{
    ProjectFact, RouteFact, SaveTaskMode, RoutineTaskWrite, StaffFact,
    StaffTaskRepository, StaffTaskSources, replay as replay_task, save_task_mode,
};
use crate::domain::staff_tasks::{
    SaveTaskCommand, StaffTaskActor, TaskReceipt, TaskScope,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::future::Future;
use std::pin::Pin;

pub type RoutineFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Owner-owned lineage capture for one schedule mutation. Composition supplies
/// the opaque current IAM/assignment image; the owner never infers authority.
#[derive(Clone, Debug)]
pub struct RoutineCapture {
    pub kind: &'static str,
    pub authority: RoutineAuthority,
}

#[derive(Clone, Debug)]
pub struct SaveTemplatePlan {
    pub key: CommandKey,
    pub operation: String,
    pub record_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub actor_id: String,
    pub assignment_id: String,
    pub title: String,
    pub description: String,
    pub assignee_user_id: String,
    pub project_id: Option<String>,
    pub start_approvers_json: String,
    pub completion_approvers_json: String,
    pub published: i64,
    pub version: i64,
    pub now: i64,
    pub request_fingerprint: String,
    pub receipt: RoutineReceipt,
}

#[derive(Clone, Debug)]
pub struct SaveSchedulePlan {
    pub key: CommandKey,
    pub operation: String,
    pub record_id: String,
    pub template_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub actor_id: String,
    pub assignment_id: String,
    pub frequency: String,
    pub timezone: String,
    pub occurrence_time: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub weekday: Option<i64>,
    pub month_day: Option<i64>,
    pub due_offset_hours: i64,
    pub next_occurrence_at: Option<i64>,
    pub version: i64,
    pub now: i64,
    pub request_fingerprint: String,
    pub receipt: RoutineReceipt,
    pub capture: Option<RoutineCapture>,
}

#[derive(Clone, Debug)]
pub struct ScheduleStatePlan {
    pub key: CommandKey,
    pub operation: String,
    pub record_id: String,
    pub template_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub actor_id: String,
    pub assignment_id: String,
    pub state: String,
    pub expected_version: i64,
    pub version: i64,
    pub next_occurrence_at: Option<i64>,
    pub now: i64,
    pub request_fingerprint: String,
    pub receipt: RoutineReceipt,
    pub capture: Option<RoutineCapture>,
}

/// Owner persistence boundary. Implementations must use the caller's retained
/// serializable unit and write the command receipt, audit/outbox, owner rows and
/// execution authorization atomically. A key may never commit a second time.
pub trait RoutineCommandRepository: Send {
    type Error: From<BusinessError> + Send;
    fn connection(&mut self) -> Option<&mut sqlx::PgConnection>;
    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> RoutineFuture<'a, Option<StoredRoutineReceipt>, Self::Error>;
    fn template<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
    ) -> RoutineFuture<'a, Option<RoutineTemplateRow>, Self::Error>;
    fn schedule<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
    ) -> RoutineFuture<'a, Option<RoutineScheduleRow>, Self::Error>;
    fn save_template<'a>(
        &'a mut self,
        plan: SaveTemplatePlan,
    ) -> RoutineFuture<'a, RoutineReceipt, Self::Error>;
    fn save_schedule<'a>(
        &'a mut self,
        plan: SaveSchedulePlan,
    ) -> RoutineFuture<'a, RoutineReceipt, Self::Error>;
    fn change_schedule_state<'a>(
        &'a mut self,
        plan: ScheduleStatePlan,
    ) -> RoutineFuture<'a, RoutineReceipt, Self::Error>;
    fn end_schedule<'a>(
        &'a mut self,
        plan: ScheduleStatePlan,
    ) -> RoutineFuture<'a, RoutineReceipt, Self::Error>;
}

/// Current cross-owner facts. None of these implementations live in this crate:
/// composition binds the owning People, Projects and Workflow ports. A missing
/// fact is a refusal, never a silent default.
pub trait RoutineCommandSources: Send {
    type Error: From<BusinessError> + Send;
    /// Identity: the actor holds the current Company holding-owner role.
    fn company_owner<'a>(
        &'a mut self,
        unit: Option<&'a mut sqlx::PgConnection>,
        company_id: &'a str,
        user_id: &'a str,
    ) -> RoutineFuture<'a, bool, Self::Error>;
    /// People: the user is current active staff with branch/department placement.
    fn active_staff<'a>(
        &'a mut self,
        unit: Option<&'a mut sqlx::PgConnection>,
        company_id: &'a str,
        user_id: &'a str,
    ) -> RoutineFuture<'a, Option<StaffFact>, Self::Error>;
    /// Projects: the project identity for the Company.
    fn project<'a>(
        &'a mut self,
        unit: Option<&'a mut sqlx::PgConnection>,
        company_id: &'a str,
        project_id: &'a str,
    ) -> RoutineFuture<'a, Option<ProjectFact>, Self::Error>;
    /// Workflow: validate the ordered approval route for the routine scope.
    #[allow(clippy::too_many_arguments)]
    fn task_route<'a>(
        &'a mut self,
        unit: Option<&'a mut sqlx::PgConnection>,
        scope: &'a TaskScope,
        creator_user_id: &'a str,
        assignee_user_id: &'a str,
        phase: &'a str,
        approvers: &'a [String],
        service_task: bool,
    ) -> RoutineFuture<'a, RouteFact, Self::Error>;
}

fn replay(
    stored: Option<StoredRoutineReceipt>,
    operation: &str,
    record_id: &str,
    request_fingerprint: &str,
    actor: &StaffTaskActor,
) -> Result<Option<RoutineReceipt>, BusinessError> {
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
    let receipt: RoutineReceipt = serde_json::from_str(&result).map_err(|_| invalid_receipt())?;
    if receipt.id.is_empty() || receipt.version < 1 {
        return Err(invalid_receipt());
    }
    Ok(Some(receipt))
}

fn check_receipt(
    receipt: RoutineReceipt,
    id: &str,
    state: Option<&str>,
    version: i64,
) -> Result<RoutineReceipt, BusinessError> {
    if receipt.id != id || receipt.state.as_deref() != state || receipt.version != version {
        return Err(invalid_receipt());
    }
    Ok(receipt)
}

async fn owner<R, S>(actor: &StaffTaskActor, repository: &mut R, sources: &mut S) -> Result<bool, R::Error>
where
    R: RoutineCommandRepository,
    S: RoutineCommandSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    Ok(sources
        .company_owner(
            repository.connection(),
            &actor.company_id,
            &actor.user_id,
        )
        .await?)
}

async fn visible<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    id: &str,
) -> Result<RoutineTemplateRow, R::Error>
where
    R: RoutineCommandRepository,
    S: RoutineCommandSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    let row = repository
        .template(&actor.company_id, id)
        .await?
        .ok_or_else(template_not_found)?;
    if !row.is_published() && row.creator_user_id != actor.user_id && !owner(actor, repository, sources).await? {
        return Err(template_not_found().into());
    }
    Ok(row)
}

async fn editable<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    row: &RoutineTemplateRow,
) -> Result<(), R::Error>
where
    R: RoutineCommandRepository,
    S: RoutineCommandSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    if row.creator_user_id == actor.user_id || owner(actor, repository, sources).await? {
        return Ok(());
    }
    Err(template_owner_required().into())
}

/// Retained configuration validation: a current active assignee, an accessible
/// project and an unblocked start/completion approval route.
async fn validate_configuration<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    creator: &str,
    input: &SaveRoutineTemplateCommand,
) -> Result<StaffFact, R::Error>
where
    R: RoutineCommandRepository,
    S: RoutineCommandSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    sources
        .active_staff(
            repository.connection(),
            &actor.company_id,
            &actor.user_id,
        )
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::Forbidden,
                "ROUTINE_STAFF_REQUIRED",
                "Choose a current active staff member.",
            )
        })?;
    let staff = sources
        .active_staff(
            repository.connection(),
            &actor.company_id,
            &input.assignee_user_id,
        )
        .await?
        .ok_or_else(|| {
            BusinessError::invalid(
                "ROUTINE_ASSIGNEE_INVALID",
                "Choose a current active staff member as the assignee.",
            )
        })?;
    if let Some(project_id) = input.project_id.as_deref() {
        sources
            .project(repository.connection(), &actor.company_id, project_id)
            .await?
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "ROUTINE_PROJECT_NOT_FOUND",
                    "Project not found.",
                )
            })?;
    }
    let scope = TaskScope {
        company_id: actor.company_id.clone(),
        branch_id: staff.branch_id.clone(),
        department_id: staff.department_id.clone(),
    };
    for (phase, approvers) in [
        ("start", input.start_approvers.as_slice()),
        ("completion", input.completion_approvers.as_slice()),
    ] {
        let route = sources
            .task_route(
                repository.connection(),
                &scope,
                creator,
                &input.assignee_user_id,
                phase,
                approvers,
                false,
            )
            .await?;
        if route.blocked_reason.is_some() {
            return Err(BusinessError::invalid(
                "ROUTINE_ROUTE_INVALID",
                "The approval route is invalid for the selected scope.",
            )
            .into());
        }
    }
    Ok(staff)
}

/// Template create, update or clone. `clone_id` narrows the command to the
/// retained clone shape, which is a create whose source must stay visible.
pub async fn save_template<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    id: Option<&str>,
    input: &SaveRoutineTemplateCommand,
    clone_id: Option<&str>,
    key: &str,
    now: i64,
) -> Result<RoutineReceipt, R::Error>
where
    R: RoutineCommandRepository,
    S: RoutineCommandSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    actor.require_write(now)?;
    input.validate()?;
    if let Some(id) = id {
        crate::domain::field_policy::identifier(id)?;
    }
    if let Some(clone_id) = clone_id {
        crate::domain::field_policy::identifier(clone_id)?;
    }
    let key = CommandKey::new(key)?;
    let record_id = id
        .map(str::to_owned)
        .unwrap_or_else(|| format!("routine-{key}", key = key.as_str()));
    let operation = if clone_id.is_some() {
        "routine.clone"
    } else if id.is_some() {
        "routine.update"
    } else {
        "routine.create"
    };
    let fingerprint = fingerprint_json(
        operation,
        &record_id,
        &serde_json::json!({
            "title": input.title,
            "description": input.description,
            "assigneeUserId": input.assignee_user_id,
            "projectId": input.project_id,
            "startApprovers": input.start_approvers,
            "completionApprovers": input.completion_approvers,
            "published": input.published,
            "expectedVersion": input.expected_version,
            "cloneId": clone_id,
        }),
    )?;
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
        Some(id) => Some(visible(actor, repository, sources, id).await?),
        None => None,
    };
    if let Some(clone_id) = clone_id {
        visible(actor, repository, sources, clone_id).await?;
    }
    if let Some(current) = current.as_ref() {
        editable(actor, repository, sources, current).await?;
    }
    if let Some(current) = current.as_ref()
        && current.version != input.expected_version
    {
        return Err(source_changed().into());
    }
    let creator = current
        .as_ref()
        .map(|row| row.creator_user_id.as_str())
        .unwrap_or(actor.user_id.as_str());
    let staff = validate_configuration(actor, repository, sources, creator, input).await?;
    let version = current
        .as_ref()
        .map(|_row| input.expected_version + 1)
        .unwrap_or(1);
    let branch_id = current
        .as_ref()
        .and_then(|row| row.branch_id.clone())
        .or_else(|| staff.branch_id.clone());
    let department_id = current
        .as_ref()
        .and_then(|row| row.department_id.clone())
        .or_else(|| staff.department_id.clone());
    let start_json = serde_json::to_string(&input.start_approvers).map_err(|_| {
        BusinessError::invalid("ROUTINE_FIELDS_INVALID", "Review the routine fields.")
    })?;
    let completion_json = serde_json::to_string(&input.completion_approvers).map_err(|_| {
        BusinessError::invalid("ROUTINE_FIELDS_INVALID", "Review the routine fields.")
    })?;
    let committed = repository
        .save_template(SaveTemplatePlan {
            key,
            operation: operation.into(),
            record_id: record_id.clone(),
            company_id: actor.company_id.clone(),
            branch_id,
            department_id,
            actor_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            title: input.title(),
            description: input.description(),
            assignee_user_id: input.assignee_user_id.clone(),
            project_id: input.project_id.clone(),
            start_approvers_json: start_json,
            completion_approvers_json: completion_json,
            published: i64::from(input.published),
            version,
            now,
            request_fingerprint: fingerprint,
            receipt: RoutineReceipt::template(record_id.clone(), version),
        })
        .await?;
    check_receipt(committed, &record_id, None, version).map_err(Into::into)
}

/// Load the published template a manual run snapshots. Visibility and ownership
/// stay the retained `getTemplate` rule: an unpublished template is visible only
/// to its creator or a Company owner, and running any unpublished source is a
/// retained 409.
pub async fn run_template_source<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    id: &str,
) -> Result<RoutineTemplateRow, R::Error>
where
    R: RoutineCommandRepository,
    S: RoutineCommandSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    crate::domain::field_policy::identifier(id)?;
    let row = visible(actor, repository, sources, id).await?;
    if !row.is_published() {
        return Err(template_unpublished().into());
    }
    Ok(row)
}

/// Manual `routine.run`: one occurrence task from the published template.
///
/// Replay is checked before the template is needed so the same key returns the
/// original receipt even after the source was unpublished or removed. A fresh
/// run computes the retained start/completion routes, project availability,
/// draft/start_approval/ready state, assignee/creator notices and the start
/// approval before committing the owner task and ledger.
#[allow(clippy::too_many_arguments)]
pub async fn run_template<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    template: Option<&RoutineTemplateRow>,
    template_id: &str,
    input: &RunRoutineTemplateCommand,
    key: &str,
    now: i64,
) -> Result<TaskReceipt, R::Error>
where
    R: StaffTaskRepository,
    S: StaffTaskSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    actor.require_write(now)?;
    let _due_at = input.due_at_millis()?;
    let key = CommandKey::new(key)?;
    let record_id = format!("task-{}", key.as_str());
    let fingerprint = run_fingerprint(&record_id, template_id, &input.due_at)?;
    if let Some(receipt) = replay_task(
        repository.stored_receipt(key.as_str()).await?,
        "routine.run",
        &record_id,
        &fingerprint,
        actor,
    )? {
        return Ok(receipt);
    }
    let template = template.ok_or_else(template_not_found)?;
    if template.company_id != actor.company_id {
        return Err(template_not_found().into());
    }
    if !template.is_published() {
        return Err(template_unpublished().into());
    }
    let start = template.start_approvers()?;
    let completion = template.completion_approvers()?;
    let scope = TaskScope {
        company_id: template.company_id.clone(),
        branch_id: template.branch_id.clone(),
        department_id: template.department_id.clone(),
    };
    let start_route = run_route(
        actor,
        repository,
        sources,
        &scope,
        &template.assignee_user_id,
        "start",
        &start,
    )
    .await?;
    let completion_route = run_route(
        actor,
        repository,
        sources,
        &scope,
        &template.assignee_user_id,
        "completion",
        &completion,
    )
    .await?;
    let mut blocked = start_route
        .blocked_reason
        .or(completion_route.blocked_reason);
    if let Some(project_id) = template.project_id.as_deref()
        && sources
            .project(repository.connection(), &template.company_id, project_id)
            .await?
            .is_none()
    {
        blocked.get_or_insert_with(|| "The project is unavailable.".into());
    }
    let state = if blocked.is_some() {
        "draft"
    } else if !start.is_empty() {
        "start_approval"
    } else {
        "ready"
    };
    let notification = if blocked.is_some() {
        "Routine task blocked: correction required"
    } else {
        "Routine task assigned to you"
    };
    let creator_notification = blocked
        .as_ref()
        .map(|reason| format!("Routine blocked: {reason}"));
    let task_input = SaveTaskCommand {
        title: template.title.clone(),
        description: template.description.clone(),
        assignee_user_id: template.assignee_user_id.clone(),
        project_id: template.project_id.clone(),
        start_approvers: start,
        completion_approvers: completion,
        due_at: Some(input.due_at.clone()),
        expected_version: 1,
        reason: String::new(),
    };
    save_task_mode(
        actor,
        repository,
        sources,
        None,
        &task_input,
        key.as_str(),
        false,
        now,
        SaveTaskMode::Routine(RoutineTaskWrite {
            template_id: template.id.clone(),
            template_version: template.version,
            branch_id: template.branch_id.clone(),
            department_id: template.department_id.clone(),
            state: state.into(),
            blocked_reason: blocked,
            notification: notification.into(),
            creator_notification,
            open_start_approval: state == "start_approval",
            request_fingerprint: fingerprint,
        }),
    )
    .await
}

/// Retained `taskRoute` for a run occurrence. An empty phase still proves the
/// assignee is current active staff; a non-empty phase goes to the Workflow
/// port, which owns the ordered route and department flow.
async fn run_route<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    scope: &TaskScope,
    assignee_user_id: &str,
    phase: &str,
    approvers: &[String],
) -> Result<RouteFact, R::Error>
where
    R: StaffTaskRepository,
    S: StaffTaskSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    if approvers.is_empty() {
        let current = sources
            .active_staff(
                repository.connection(),
                &scope.company_id,
                assignee_user_id,
            )
            .await?
            .is_some();
        return Ok(RouteFact {
            blocked_reason: (!current)
                .then(|| "Choose a current active staff member.".to_owned()),
        });
    }
    Ok(sources
        .task_route(
            repository.connection(),
            scope,
            &actor.user_id,
            assignee_user_id,
            phase,
            approvers,
            false,
        )
        .await?)
}

/// Schedule create/update. A saved schedule is always paused, matching the
/// retained owner rule; resume is a separate explicit command.
#[allow(clippy::too_many_arguments)]
pub async fn save_schedule<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    template_id: &str,
    id: Option<&str>,
    input: &SaveRoutineScheduleCommand,
    key: &str,
    capture: Option<RoutineCapture>,
    now: i64,
) -> Result<RoutineReceipt, R::Error>
where
    R: RoutineCommandRepository,
    S: RoutineCommandSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    actor.require_write(now)?;
    input.validate()?;
    crate::domain::field_policy::identifier(template_id)?;
    if let Some(id) = id {
        crate::domain::field_policy::identifier(id)?;
    }
    let template = visible(actor, repository, sources, template_id).await?;
    editable(actor, repository, sources, &template).await?;
    let key = CommandKey::new(key)?;
    let record_id = id
        .map(str::to_owned)
        .unwrap_or_else(|| format!("schedule-{key}", key = key.as_str()));
    let operation = if id.is_some() {
        "routine.schedule.update"
    } else {
        "routine.schedule.create"
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
    if let Some(id) = id {
        let current = repository
            .schedule(&actor.company_id, id)
            .await?
            .ok_or_else(schedule_not_found)?;
        if current.template_id != template_id || current.version != input.expected_version {
            return Err(source_changed().into());
        }
        if current.state != "paused" {
            return Err(source_changed().into());
        }
    } else if input.expected_version != 1 {
        return Err(source_changed().into());
    }
    let rule = input.rule();
    let next = next_occurrence(&rule, now - 1)?;
    let version = if id.is_some() {
        input.expected_version + 1
    } else {
        1
    };
    let committed = repository
        .save_schedule(SaveSchedulePlan {
            key,
            operation: operation.into(),
            record_id: record_id.clone(),
            template_id: template.id.clone(),
            company_id: actor.company_id.clone(),
            branch_id: template.branch_id.clone(),
            department_id: template.department_id.clone(),
            actor_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            frequency: rule.frequency.as_str().into(),
            timezone: rule.timezone,
            occurrence_time: rule.occurrence_time,
            start_date: rule.start_date,
            end_date: rule.end_date,
            weekday: rule.weekday,
            month_day: rule.month_day,
            due_offset_hours: input.due_offset_hours,
            next_occurrence_at: next,
            version,
            now,
            request_fingerprint: fingerprint,
            receipt: RoutineReceipt::schedule(record_id.clone(), "paused", version),
            capture: capture.map(|capture| RoutineCapture {
                kind: if id.is_some() { "update" } else { "create" },
                authority: capture.authority,
            }),
        })
        .await?;
    check_receipt(committed, &record_id, Some("paused"), version).map_err(Into::into)
}

/// Pause or resume one schedule. Resume requires a published source.
pub async fn change_schedule_state<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    id: &str,
    input: &ScheduleStateCommand,
    resume: bool,
    key: &str,
    capture: Option<RoutineCapture>,
    now: i64,
) -> Result<RoutineReceipt, R::Error>
where
    R: RoutineCommandRepository,
    S: RoutineCommandSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    actor.require_write(now)?;
    input.validate()?;
    crate::domain::field_policy::identifier(id)?;
    let key = CommandKey::new(key)?;
    let operation = if resume {
        "routine.schedule.resume"
    } else {
        "routine.schedule.pause"
    };
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
        .schedule(&actor.company_id, id)
        .await?
        .ok_or_else(schedule_not_found)?;
    let template = visible(actor, repository, sources, &row.template_id).await?;
    editable(actor, repository, sources, &template).await?;
    if resume && !template.is_published() {
        return Err(template_unpublished().into());
    }
    let expected_state = if resume { "paused" } else { "active" };
    if row.version != input.expected_version || row.state != expected_state {
        return Err(source_changed().into());
    }
    let state = if resume { "active" } else { "paused" };
    let version = input.expected_version + 1;
    let committed = repository
        .change_schedule_state(ScheduleStatePlan {
            key,
            operation: operation.into(),
            record_id: id.to_owned(),
            template_id: template.id.clone(),
            company_id: actor.company_id.clone(),
            branch_id: template.branch_id.clone(),
            department_id: template.department_id.clone(),
            actor_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            state: state.into(),
            expected_version: input.expected_version,
            version,
            next_occurrence_at: row.next_occurrence_at,
            now,
            request_fingerprint: fingerprint,
            receipt: RoutineReceipt::schedule(id, state, version),
            capture: if resume {
                capture.map(|capture| RoutineCapture {
                    kind: "resume",
                    authority: capture.authority,
                })
            } else {
                None
            },
        })
        .await?;
    check_receipt(committed, id, Some(state), version).map_err(Into::into)
}

/// End one schedule. Ending an already ended schedule is a retained conflict.
pub async fn end_schedule<R, S>(
    actor: &StaffTaskActor,
    repository: &mut R,
    sources: &mut S,
    id: &str,
    input: &ScheduleStateCommand,
    key: &str,
    now: i64,
) -> Result<RoutineReceipt, R::Error>
where
    R: RoutineCommandRepository,
    S: RoutineCommandSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    actor.require_write(now)?;
    input.validate()?;
    crate::domain::field_policy::identifier(id)?;
    let key = CommandKey::new(key)?;
    let operation = "routine.schedule.end";
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
        .schedule(&actor.company_id, id)
        .await?
        .ok_or_else(schedule_not_found)?;
    let template = visible(actor, repository, sources, &row.template_id).await?;
    editable(actor, repository, sources, &template).await?;
    if row.state == "ended" {
        return Err(schedule_ended().into());
    }
    if row.version != input.expected_version {
        return Err(source_changed().into());
    }
    let version = input.expected_version + 1;
    let committed = repository
        .end_schedule(ScheduleStatePlan {
            key,
            operation: operation.into(),
            record_id: id.to_owned(),
            template_id: template.id.clone(),
            company_id: actor.company_id.clone(),
            branch_id: template.branch_id.clone(),
            department_id: template.department_id.clone(),
            actor_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            state: "ended".into(),
            expected_version: input.expected_version,
            version,
            next_occurrence_at: None,
            now,
            request_fingerprint: fingerprint,
            receipt: RoutineReceipt::schedule(id, "ended", version),
            capture: None,
        })
        .await?;
    check_receipt(committed, id, Some("ended"), version).map_err(Into::into)
}
