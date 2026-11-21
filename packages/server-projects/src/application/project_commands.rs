//! Native Projects milestone command use cases.
//!
//! The repository owns one serializable unit and must re-check the same current
//! source facts after it locks the project. A replay returns the retained
//! receipt only when the command key, operation, record and request fingerprint
//! all agree. Purchase and active-staff facts are supplied by composition from
//! their owning current-authority ports; they are never inferred from a client
//! label.
use crate::domain::project_commands::{
    DependencyFact, MilestoneAction, MilestoneCommandReceipt, ParentMilestone, ProjectCommandActor,
    ProjectCommandMilestone, ProjectCommandProject, SaveMilestoneCommand, SaveMilestonePlan,
    StoredMilestoneReceipt, TransitionMilestoneCommand, TransitionMilestonePlan,
    authority_json, dependencies_incomplete, dependent_completed,
    fingerprint, idempotency_conflict, incomplete_receipt, invalid_receipt, member_required,
    milestone_terminal, missing_milestone, missing_project, owner_required,
    prerequisite_still_used, structure_frozen, transition_invalid,
    validate_milestone, validate_project,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use std::future::Future;

pub type ProjectCommandFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait ProjectMilestoneCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ProjectCommandFuture<'a, Option<StoredMilestoneReceipt>, Self::Error>;
    fn project<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectCommandFuture<'a, Option<ProjectCommandProject>, Self::Error>;
    fn milestone<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
    ) -> ProjectCommandFuture<'a, Option<ProjectCommandMilestone>, Self::Error>;
    fn member<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectCommandFuture<'a, bool, Self::Error>;
    fn parent<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        parent_id: &'a str,
    ) -> ProjectCommandFuture<'a, Option<ParentMilestone>, Self::Error>;
    fn parent_cycle<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
        parent_id: &'a str,
    ) -> ProjectCommandFuture<'a, bool, Self::Error>;
    fn dependencies<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
    ) -> ProjectCommandFuture<'a, Vec<String>, Self::Error>;
    fn dependency_facts<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        dependency_ids: &'a [String],
    ) -> ProjectCommandFuture<'a, Vec<DependencyFact>, Self::Error>;
    fn dependency_cycle<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
        dependency_ids: &'a [String],
    ) -> ProjectCommandFuture<'a, bool, Self::Error>;
    fn incomplete_prerequisites<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
    ) -> ProjectCommandFuture<'a, Vec<String>, Self::Error>;
    fn active_successor<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
    ) -> ProjectCommandFuture<'a, bool, Self::Error>;
    fn completed_successor<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
    ) -> ProjectCommandFuture<'a, bool, Self::Error>;
    fn save<'a>(
        &'a mut self,
        plan: SaveMilestonePlan,
    ) -> ProjectCommandFuture<'a, MilestoneCommandReceipt, Self::Error>;
    fn transition<'a>(
        &'a mut self,
        plan: TransitionMilestonePlan,
    ) -> ProjectCommandFuture<'a, MilestoneCommandReceipt, Self::Error>;
}

fn replay(
    stored: Option<StoredMilestoneReceipt>,
    operation: &str,
    record_id: &str,
    request_fingerprint: &str,
    expected_state: Option<&str>,
) -> Result<Option<MilestoneCommandReceipt>, BusinessError> {
    let Some(stored) = stored else {
        return Ok(None);
    };
    if stored.operation != operation
        || stored.record_id != record_id
        || stored.request_fingerprint != request_fingerprint
    {
        return Err(idempotency_conflict());
    }
    let Some(result) = stored.result_json else {
        return Err(incomplete_receipt());
    };
    let receipt: MilestoneCommandReceipt =
        serde_json::from_str(&result).map_err(|_| invalid_receipt())?;
    if receipt.id != record_id
        || receipt.version < 1
        || receipt.state.as_deref() != expected_state
    {
        return Err(invalid_receipt());
    }
    Ok(Some(receipt))
}

fn check_save_receipt(
    receipt: MilestoneCommandReceipt,
    id: &str,
    version: i64,
) -> Result<MilestoneCommandReceipt, BusinessError> {
    if receipt.id != id || receipt.version != version || receipt.state.is_some() {
        return Err(invalid_receipt());
    }
    Ok(receipt)
}

fn check_transition_receipt(
    receipt: MilestoneCommandReceipt,
    id: &str,
    version: i64,
    state: &str,
) -> Result<MilestoneCommandReceipt, BusinessError> {
    if receipt.id != id || receipt.version != version || receipt.state.as_deref() != Some(state) {
        return Err(invalid_receipt());
    }
    Ok(receipt)
}

fn validate_current_project(
    actor: &ProjectCommandActor,
    project: &ProjectCommandProject,
) -> Result<(), BusinessError> {
    validate_project(project, &actor.company_id)?;
    if project.id.is_empty() || project.company_id != actor.company_id.as_str() {
        return Err(missing_project());
    }
    if !project.is_owner && !project.is_member {
        return Err(missing_project());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn save_milestone<R: ProjectMilestoneCommandRepository>(
    actor: &ProjectCommandActor,
    repository: &mut R,
    project_id: &str,
    milestone_id: Option<&str>,
    input: &SaveMilestoneCommand,
    key: &str,
    now: TimestampMillis,
) -> Result<MilestoneCommandReceipt, R::Error> {
    actor.require_write(now)?;
    actor.require_new_work()?;
    input.validate()?;
    let key = CommandKey::new(key)?;
    let project_id = RecordId::new(project_id)?;
    let record_id = match milestone_id {
        Some(value) => RecordId::new(value)?.as_str().to_owned(),
        None => format!("milestone-{}", key.as_str()),
    };
    if record_id.len() > 200 {
        return Err(BusinessError::invalid(
            "PROJECT_MILESTONE_INVALID",
            "Use a shorter command key for this milestone.",
        )
        .into());
    }
    let operation = if milestone_id.is_some() {
        "project.milestone.revise"
    } else {
        "project.milestone.create"
    };
    let fingerprint = fingerprint(
        operation,
        project_id.as_str(),
        &record_id,
        &serde_json::to_value(input).map_err(|_| invalid_receipt())?,
    )?;
    if let Some(receipt) = replay(
        repository.stored_receipt(key.as_str()).await?,
        operation,
        &record_id,
        &fingerprint,
        None,
    )? {
        return Ok(receipt);
    }
    let project = repository
        .project(
            &actor.company_id,
            project_id.as_str(),
            &actor.user_id,
        )
        .await?
        .ok_or_else(missing_project)?;
    validate_current_project(actor, &project)?;
    if !project.is_owner {
        return Err(owner_required().into());
    }
    if project.lifecycle_status != "active" {
        return Err(structure_frozen().into());
    }
    let current = if milestone_id.is_some() {
        let current = repository
            .milestone(
                &actor.company_id,
                project_id.as_str(),
                &record_id,
            )
            .await?
            .ok_or_else(missing_milestone)?;
        validate_milestone(&current, &actor.company_id, project_id.as_str())?;
        if current.state == "completed" || current.state == "cancelled" {
            return Err(milestone_terminal().into());
        }
        if current.version != input.expected_version {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "PROJECT_MILESTONE_CHANGED",
                "The milestone changed. Reload it before saving.",
            )
            .into());
        }
        Some(current)
    } else {
        None
    };
    let parent_id = input.resolved_parent(current.as_ref());
    if parent_id.as_deref() == Some(record_id.as_str()) {
        return Err(BusinessError::invalid(
            "PROJECT_PACKAGE_PARENT_INVALID",
            "Choose a different parent package in this project.",
        )
        .into());
    }
    if let Some(parent_id) = parent_id.as_deref() {
        let parent = repository
            .parent(&actor.company_id, project_id.as_str(), parent_id)
            .await?
            .ok_or_else(|| {
                BusinessError::invalid(
                    "PROJECT_PACKAGE_PARENT_INVALID",
                    "Choose a different parent package in this project.",
                )
            })?;
        if parent.state != "planned" && parent.state != "in_progress" {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "PROJECT_PACKAGE_PARENT_TERMINAL",
                "Reopen the parent package before adding or revising its work.",
            )
            .into());
        }
        if repository
            .parent_cycle(
                &actor.company_id,
                project_id.as_str(),
                &record_id,
                parent_id,
            )
            .await?
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "PROJECT_PACKAGE_CYCLE_OR_DEPTH",
                "Choose a parent that keeps the work structure acyclic and within thirty levels.",
            )
            .into());
        }
    }
    if !repository
        .member(&actor.company_id, project_id.as_str(), &input.owner_user_id)
        .await?
    {
        return Err(member_required().into());
    }
    let dependencies = match &input.depends_on_milestone_ids {
        Some(dependencies) => dependencies.clone(),
        None => {
            if current.is_some() {
                repository
                    .dependencies(&actor.company_id, project_id.as_str(), &record_id)
                    .await?
            } else {
                Vec::new()
            }
        }
    };
    if dependencies.iter().any(|id| id == &record_id) {
        return Err(BusinessError::invalid(
            "PROJECT_DEPENDENCY_CYCLE",
            "A milestone cannot depend on itself.",
        )
        .into());
    }
    if !dependencies.is_empty() {
        let facts = repository
            .dependency_facts(&actor.company_id, project_id.as_str(), &dependencies)
            .await?;
        if facts.len() != dependencies.len() {
            return Err(BusinessError::invalid(
                "PROJECT_DEPENDENCY_SCOPE_INVALID",
                "Prerequisites must be milestones in this project.",
            )
            .into());
        }
        if facts.iter().any(|row| row.state == "cancelled") {
            return Err(BusinessError::invalid(
                "PROJECT_DEPENDENCY_CANCELLED",
                "A cancelled milestone cannot be a prerequisite.",
            )
            .into());
        }
        if repository
            .dependency_cycle(
                &actor.company_id,
                project_id.as_str(),
                &record_id,
                &dependencies,
            )
            .await?
        {
            return Err(BusinessError::invalid(
                "PROJECT_DEPENDENCY_CYCLE",
                "This prerequisite would create a project dependency cycle.",
            )
            .into());
        }
    }
    let ordinal_position = input
        .ordinal_position
        .or_else(|| current.as_ref().map(|row| row.ordinal_position))
        .unwrap_or(0);
    let version = current.as_ref().map_or(1, |row| row.version + 1);
    let plan = SaveMilestonePlan {
        key,
        operation: operation.to_owned(),
        company_id: actor.company_id.clone(),
        actor_id: actor.user_id.clone(),
        assignment_id: actor.assignment_id.clone(),
        authority_json: authority_json(actor)?,
        project_id: project_id.as_str().to_owned(),
        project_version: project.version,
        milestone_id: record_id.clone(),
        current,
        title: input.title.clone(),
        description: input.description.clone(),
        due_at: input.due_at,
        owner_user_id: input.owner_user_id.clone(),
        parent_milestone_id: parent_id,
        ordinal_position,
        depends_on_milestone_ids: dependencies,
        reason: input.reason.clone(),
        version,
        request_fingerprint: fingerprint,
        now: now.get(),
    };
    let receipt = repository.save(plan).await?;
    check_save_receipt(receipt, &record_id, version).map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
pub async fn transition_milestone<R: ProjectMilestoneCommandRepository>(
    actor: &ProjectCommandActor,
    repository: &mut R,
    project_id: &str,
    milestone_id: &str,
    input: &TransitionMilestoneCommand,
    key: &str,
    now: TimestampMillis,
) -> Result<MilestoneCommandReceipt, R::Error> {
    actor.require_write(now)?;
    if input.action == MilestoneAction::Reopen {
        actor.require_new_work()?;
    } else {
        actor.require_settle_existing()?;
    }
    input.validate()?;
    let key = CommandKey::new(key)?;
    let project_id = RecordId::new(project_id)?;
    let milestone_id = RecordId::new(milestone_id)?;
    let operation = format!("project.milestone.{}", input.action.as_str());
    let next_state = input.action.next_state();
    let fingerprint = fingerprint(
        &operation,
        project_id.as_str(),
        milestone_id.as_str(),
        &serde_json::to_value(input).map_err(|_| invalid_receipt())?,
    )?;
    if let Some(receipt) = replay(
        repository.stored_receipt(key.as_str()).await?,
        &operation,
        milestone_id.as_str(),
        &fingerprint,
        Some(next_state),
    )? {
        return Ok(receipt);
    }
    let project = repository
        .project(
            &actor.company_id,
            project_id.as_str(),
            &actor.user_id,
        )
        .await?
        .ok_or_else(missing_project)?;
    validate_current_project(actor, &project)?;
    if !project.is_owner {
        return Err(owner_required().into());
    }
    if project.lifecycle_status != "active" {
        return Err(structure_frozen().into());
    }
    let current = repository
        .milestone(
            &actor.company_id,
            project_id.as_str(),
            milestone_id.as_str(),
        )
        .await?
        .ok_or_else(missing_milestone)?;
    validate_milestone(&current, &actor.company_id, project_id.as_str())?;
    if current.version != input.expected_version {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "PROJECT_MILESTONE_CHANGED",
            "The milestone changed. Reload it before transitioning.",
        )
        .into());
    }
    let allowed = match input.action {
        MilestoneAction::Start => current.state == "planned",
        MilestoneAction::Complete => current.state == "planned" || current.state == "in_progress",
        MilestoneAction::Reopen => current.state == "completed",
        MilestoneAction::Cancel => current.state == "planned" || current.state == "in_progress",
    };
    if !allowed {
        return Err(transition_invalid().into());
    }
    if matches!(input.action, MilestoneAction::Start | MilestoneAction::Complete) {
        let incomplete = repository
            .incomplete_prerequisites(
                &actor.company_id,
                project_id.as_str(),
                milestone_id.as_str(),
            )
            .await?;
        if !incomplete.is_empty() {
            return Err(dependencies_incomplete().into());
        }
    }
    if input.action == MilestoneAction::Cancel
        && repository
            .active_successor(
                &actor.company_id,
                project_id.as_str(),
                milestone_id.as_str(),
            )
            .await?
    {
        return Err(prerequisite_still_used().into());
    }
    if input.action == MilestoneAction::Reopen
        && repository
            .completed_successor(
                &actor.company_id,
                project_id.as_str(),
                milestone_id.as_str(),
            )
            .await?
    {
        return Err(dependent_completed().into());
    }
    let version = current.version + 1;
    let plan = TransitionMilestonePlan {
        key,
        operation,
        company_id: actor.company_id.clone(),
        actor_id: actor.user_id.clone(),
        assignment_id: actor.assignment_id.clone(),
        authority_json: authority_json(actor)?,
        project_id: project_id.as_str().to_owned(),
        project_version: project.version,
        milestone: current,
        action: input.action,
        next_state: next_state.to_owned(),
        reason: input.reason.clone(),
        version,
        request_fingerprint: fingerprint,
        now: now.get(),
    };
    let receipt = repository.transition(plan).await?;
    check_transition_receipt(
        receipt,
        milestone_id.as_str(),
        version,
        next_state,
    )
    .map_err(Into::into)
}

