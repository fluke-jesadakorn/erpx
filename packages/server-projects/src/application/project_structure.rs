//! Native Projects work-structure and handoff command use cases.
//!
//! The repository owns one serializable unit and re-checks the same current
//! source facts after it locks the project. A replay returns the retained
//! receipt only when the command key, operation, project and request
//! fingerprint all agree. Current staff, purchase and Finance facts are
//! supplied by composition from their owning current-authority ports; they are
//! never inferred from a client label.
use crate::domain::project_commands::ProjectCommandProject;
use crate::domain::project_graph::{
    GraphActual, GraphTask, ProjectGraphActor, build_project_graph_with,
};
use crate::domain::project_structure::{
    ActualSourceKind, ApprovedCost, ApprovedTime, ProjectHandoffFact, ProjectPackageFact,
    ProjectStructureActor, ProjectStructureCommand, ProjectStructureReceipt, ProjectStructureTask,
    ProjectTaskBinding, StoredStructureReceipt, StructurePlan, StructurePlanAction,
    cost_source_changed, finance_permission_required, fingerprint, handoff_changed,
    handoff_decided, handoff_incomplete, handoff_not_ready, idempotency_conflict,
    incomplete_receipt, independent_handoff_required, invalid_receipt, missing_handoff,
    missing_project, owner_required, package_changed, package_terminal, preparer_required,
    recipient_not_member, recipient_reopen_required, recipient_required, snapshot_invalid,
    source_changed, structure_frozen, task_binding_changed, time_source_changed, authority,
};
use crate::domain::project_wbs::{MilestoneRow, ProjectReadActor, ProjectRow};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, RecordId, TimestampMillis,
};
use std::future::Future;

pub type ProjectStructureFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait ProjectStructureCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ProjectStructureFuture<'a, Option<StoredStructureReceipt>, Self::Error>;
    fn project<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectStructureFuture<'a, Option<ProjectCommandProject>, Self::Error>;
    fn task<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        task_id: &'a str,
    ) -> ProjectStructureFuture<'a, Option<ProjectStructureTask>, Self::Error>;
    fn task_binding<'a>(
        &'a mut self,
        company: &'a CompanyId,
        task_id: &'a str,
    ) -> ProjectStructureFuture<'a, Option<ProjectTaskBinding>, Self::Error>;
    fn package<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        milestone_id: &'a str,
    ) -> ProjectStructureFuture<'a, Option<ProjectPackageFact>, Self::Error>;
    fn approved_time<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        source_id: &'a str,
        source_version: i64,
    ) -> ProjectStructureFuture<'a, Option<ApprovedTime>, Self::Error>;
    fn approved_cost<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        source_id: &'a str,
        source_version: i64,
    ) -> ProjectStructureFuture<'a, Option<ApprovedCost>, Self::Error>;
    fn handoff_readiness<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
    ) -> ProjectStructureFuture<'a, bool, Self::Error>;
    fn handoff<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        handoff_id: &'a str,
    ) -> ProjectStructureFuture<'a, Option<ProjectHandoffFact>, Self::Error>;
    fn recipient_member<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectStructureFuture<'a, bool, Self::Error>;
    #[allow(clippy::type_complexity)]
    fn snapshot_rows<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        actor_user_id: &'a str,
    ) -> ProjectStructureFuture<
        'a,
        (ProjectRow, Vec<MilestoneRow>, Vec<GraphTask>, Vec<GraphActual>),
        Self::Error,
    >;
    fn commit<'a>(
        &'a mut self,
        plan: StructurePlan,
    ) -> ProjectStructureFuture<'a, ProjectStructureReceipt, Self::Error>;
}

fn replay(
    stored: Option<StoredStructureReceipt>,
    operation: &str,
    record_id: &str,
    request_fingerprint: &str,
) -> Result<Option<ProjectStructureReceipt>, BusinessError> {
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
    let receipt: ProjectStructureReceipt =
        serde_json::from_str(&result).map_err(|_| invalid_receipt())?;
    if receipt.id != record_id || receipt.version < 2 || receipt.status.is_empty() {
        return Err(invalid_receipt());
    }
    Ok(Some(receipt))
}

fn validate_current_project(
    actor: &ProjectStructureActor,
    project: &ProjectCommandProject,
) -> Result<(), BusinessError> {
    crate::domain::project_commands::validate_project(project, &actor.command.company_id)?;
    if !project.is_owner && !project.is_member {
        return Err(missing_project());
    }
    Ok(())
}

fn snapshot_digest(snapshot: &str) -> String {
    crate::domain::project_structure::sha256_hex(snapshot)
}

fn read_actor(actor: &ProjectStructureActor) -> ProjectReadActor {
    ProjectReadActor {
        user_id: actor.command.user_id.clone(),
        company_id: actor.command.company_id.clone(),
        branch_id: actor.command.branch_id.clone(),
        department_id: actor.command.department_id.clone(),
        permissions: actor.command.permissions.clone(),
        modules: actor.command.modules.clone(),
        expires_at: actor.command.expires_at,
    }
}

fn is_owner(actor: &ProjectStructureActor, project: &ProjectCommandProject) -> bool {
    project.is_owner && project.owner_user_id == actor.command.user_id
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_structure_command<R: ProjectStructureCommandRepository>(
    actor: &ProjectStructureActor,
    repository: &mut R,
    project_id: &str,
    input: &ProjectStructureCommand,
    key: &str,
    now: TimestampMillis,
) -> Result<ProjectStructureReceipt, R::Error> {
    actor.require_write(now)?;
    if input.purpose_is_new_work() {
        actor.command.require_new_work()?;
    } else {
        actor.command.require_settle_existing()?;
    }
    input.validate()?;
    let key = CommandKey::new(key)?;
    let project_id = RecordId::new(project_id)?;
    let operation = input.operation();
    let request_fingerprint = fingerprint(&operation, project_id.as_str(), input)?;
    if let Some(receipt) = replay(
        repository.stored_receipt(key.as_str()).await?,
        &operation,
        project_id.as_str(),
        &request_fingerprint,
    )? {
        return Ok(receipt);
    }
    let project = repository
        .project(
            &actor.command.company_id,
            project_id.as_str(),
            &actor.command.user_id,
        )
        .await?
        .ok_or_else(missing_project)?;
    validate_current_project(actor, &project)?;
    if project.version != input.expected_version() {
        return Err(source_changed().into());
    }
    let next_version = project.version + 1;
    let mut lifecycle = project.lifecycle_status.clone();
    let mut required_lifecycle = "active";
    let mut result = ProjectStructureReceipt {
        id: project_id.as_str().to_owned(),
        version: next_version,
        status: lifecycle.clone(),
        task_id: None,
        binding_version: None,
        actual_id: None,
        handoff_id: None,
        snapshot_sha256: None,
    };
    let action = match input {
        ProjectStructureCommand::AssignTask {
            task_id,
            task_version,
            milestone_id,
            milestone_version,
            binding_version,
            ..
        } => {
            if !is_owner(actor, &project) {
                return Err(owner_required().into());
            }
            if project.lifecycle_status != "active" {
                return Err(structure_frozen().into());
            }
            validate_task_binding(
                repository,
                actor,
                &project,
                task_id,
                *task_version,
                Some((milestone_id, *milestone_version)),
                *binding_version,
            )
            .await?;
            result.task_id = Some(task_id.clone());
            result.binding_version = Some(binding_version + 1);
            StructurePlanAction::AssignTask {
                task_id: task_id.clone(),
                milestone_id: milestone_id.clone(),
                task_version: *task_version,
                milestone_version: *milestone_version,
                binding_version: *binding_version,
            }
        }
        ProjectStructureCommand::UnassignTask {
            task_id,
            task_version,
            binding_version,
            ..
        } => {
            if !is_owner(actor, &project) {
                return Err(owner_required().into());
            }
            if project.lifecycle_status != "active" {
                return Err(structure_frozen().into());
            }
            validate_task_binding(
                repository,
                actor,
                &project,
                task_id,
                *task_version,
                None,
                *binding_version,
            )
            .await?;
            result.task_id = Some(task_id.clone());
            result.binding_version = Some(0);
            StructurePlanAction::UnassignTask {
                task_id: task_id.clone(),
                task_version: *task_version,
                binding_version: *binding_version,
            }
        }
        ProjectStructureCommand::LinkActual {
            milestone_id,
            milestone_version,
            source_kind,
            source_id,
            source_version,
            ..
        } => {
            if project.lifecycle_status != "active" {
                return Err(structure_frozen().into());
            }
            let kind = ActualSourceKind::parse(source_kind)?;
            let package = repository
                .package(
                    &actor.command.company_id,
                    project_id.as_str(),
                    milestone_id,
                )
                .await?
                .ok_or_else(package_changed)?;
            if package.version != *milestone_version {
                return Err(package_changed().into());
            }
            let (minutes, amount_minor, currency) = match kind {
                ActualSourceKind::WorkLog => {
                    if !is_owner(actor, &project) {
                        return Err(owner_required().into());
                    }
                    let source = repository
                        .approved_time(
                            &actor.command.company_id,
                            project_id.as_str(),
                            source_id,
                            *source_version,
                        )
                        .await?
                        .ok_or_else(time_source_changed)?;
                    (Some(source.minutes), None, None)
                }
                ActualSourceKind::CostAllocation => {
                    if !actor.has_finance_write() {
                        return Err(finance_permission_required().into());
                    }
                    if !actor.cost_access(&project) {
                        return Err(missing_project().into());
                    }
                    let source = repository
                        .approved_cost(
                            &actor.command.company_id,
                            project_id.as_str(),
                            source_id,
                            *source_version,
                        )
                        .await?
                        .ok_or_else(cost_source_changed)?;
                    (None, Some(source.amount_minor), Some(source.currency))
                }
            };
            let actual_id = format!("project-package-actual-{}", key.as_str());
            result.actual_id = Some(actual_id.clone());
            StructurePlanAction::LinkActual {
                actual_id,
                milestone_id: milestone_id.clone(),
                milestone_version: *milestone_version,
                source_kind: kind,
                source_id: source_id.clone(),
                source_version: *source_version,
                minutes,
                amount_minor,
                currency,
            }
        }
        ProjectStructureCommand::PrepareHandoff {
            recipient_id,
            evidence,
            ..
        } => {
            if !is_owner(actor, &project) {
                return Err(owner_required().into());
            }
            if project.lifecycle_status != "active" {
                return Err(handoff_not_ready().into());
            }
            if recipient_id == &actor.command.user_id {
                return Err(independent_handoff_required().into());
            }
            if !repository
                .recipient_member(
                    &actor.command.company_id,
                    project_id.as_str(),
                    recipient_id,
                )
                .await?
            {
                return Err(recipient_not_member().into());
            }
            if !repository
                .handoff_readiness(&actor.command.company_id, project_id.as_str())
                .await?
            {
                return Err(handoff_incomplete().into());
            }
            let (graph_project, milestones, tasks, actuals) = repository
                .snapshot_rows(
                    &actor.command.company_id,
                    project_id.as_str(),
                    &actor.command.user_id,
                )
                .await?;
            let graph_actor = ProjectGraphActor {
                read: read_actor(actor),
                can_read_cost_history: true,
            };
            let graph = build_project_graph_with(
                &graph_actor,
                graph_project,
                milestones,
                tasks,
                actuals,
                now,
                true,
            )?;
            let snapshot_json = serde_json::to_string(&graph).map_err(|_| snapshot_invalid())?;
            let snapshot_sha256 = snapshot_digest(&snapshot_json);
            let handoff_id = format!("project-handoff-{}", key.as_str());
            result.handoff_id = Some(handoff_id.clone());
            result.snapshot_sha256 = Some(snapshot_sha256.clone());
            lifecycle = "handoff_pending".to_owned();
            StructurePlanAction::PrepareHandoff {
                handoff_id,
                recipient_id: recipient_id.clone(),
                snapshot_json,
                snapshot_sha256,
                evidence: evidence.clone(),
            }
        }
        ProjectStructureCommand::AcceptHandoff { .. }
        | ProjectStructureCommand::RejectHandoff { .. }
        | ProjectStructureCommand::CancelHandoff { .. } => {
            let (handoff_id, handoff_version, status, cancel) = match input {
                ProjectStructureCommand::AcceptHandoff {
                    handoff_id,
                    handoff_version,
                    ..
                } => (handoff_id, handoff_version, Some("accepted"), false),
                ProjectStructureCommand::RejectHandoff {
                    handoff_id,
                    handoff_version,
                    ..
                } => (handoff_id, handoff_version, Some("rejected"), false),
                ProjectStructureCommand::CancelHandoff {
                    handoff_id,
                    handoff_version,
                    ..
                } => (handoff_id, handoff_version, Some("cancelled"), true),
                _ => unreachable!(),
            };
            required_lifecycle = "handoff_pending";
            let handoff = repository
                .handoff(
                    &actor.command.company_id,
                    project_id.as_str(),
                    handoff_id,
                )
                .await?
                .ok_or_else(missing_handoff)?;
            if handoff.version != *handoff_version {
                return Err(handoff_changed().into());
            }
            if handoff.status != "pending" || project.lifecycle_status != "handoff_pending" {
                return Err(handoff_decided().into());
            }
            if cancel {
                if handoff.prepared_by != actor.command.user_id {
                    return Err(preparer_required().into());
                }
            } else if handoff.recipient_id != actor.command.user_id
                || handoff.prepared_by == actor.command.user_id
            {
                return Err(recipient_required().into());
            }
            let status = status.unwrap_or("cancelled");
            lifecycle = if status == "accepted" {
                "completed".to_owned()
            } else {
                "active".to_owned()
            };
            result.handoff_id = Some(handoff.id.clone());
            result.snapshot_sha256 = Some(handoff.snapshot_sha256.clone());
            StructurePlanAction::Decision {
                handoff_id: handoff.id,
                handoff_version: *handoff_version,
                status: status.to_owned(),
            }
        }
        ProjectStructureCommand::Reopen {
            handoff_id,
            handoff_version,
            ..
        } => {
            required_lifecycle = "completed";
            let handoff = repository
                .handoff(
                    &actor.command.company_id,
                    project_id.as_str(),
                    handoff_id,
                )
                .await?
                .ok_or_else(missing_handoff)?;
            if handoff.version != *handoff_version {
                return Err(handoff_changed().into());
            }
            if project.lifecycle_status != "completed"
                || handoff.status != "accepted"
                || handoff.recipient_id != actor.command.user_id
                || actor.command.user_id == project.owner_user_id
            {
                return Err(recipient_reopen_required().into());
            }
            lifecycle = "active".to_owned();
            result.handoff_id = Some(handoff.id.clone());
            result.snapshot_sha256 = Some(handoff.snapshot_sha256);
            StructurePlanAction::Reopen {
                handoff_id: handoff.id,
                handoff_version: *handoff_version,
            }
        }
    };
    result.status = lifecycle.clone();
    let plan = StructurePlan {
        key,
        operation,
        company_id: actor.command.company_id.clone(),
        actor_id: actor.command.user_id.clone(),
        assignment_id: actor.command.assignment_id.clone(),
        authority_json: authority(actor)?,
        project_id: project_id.as_str().to_owned(),
        project_version: project.version,
        required_lifecycle: required_lifecycle.to_owned(),
        next_version,
        lifecycle,
        action,
        event_action: input.action().to_owned(),
        reason: input.reason().to_owned(),
        result: result.clone(),
        request_fingerprint,
        now: now.get(),
    };
    let receipt = repository.commit(plan).await?;
    if receipt.id != result.id || receipt.version != result.version {
        return Err(invalid_receipt().into());
    }
    Ok(receipt)
}

async fn validate_task_binding<R: ProjectStructureCommandRepository>(
    repository: &mut R,
    actor: &ProjectStructureActor,
    project: &ProjectCommandProject,
    task_id: &str,
    task_version: i64,
    assign: Option<(&String, i64)>,
    binding_version: i64,
) -> Result<(), R::Error> {
    let task = repository
        .task(
            &actor.command.company_id,
            &project.id,
            task_id,
        )
        .await?
        .ok_or_else(task_binding_changed)?;
    let binding = repository
        .task_binding(&actor.command.company_id, task_id)
        .await?;
    if task.id != task_id
        || task.project_id != project.id
        || task.version != task_version
        || binding.as_ref().map_or(0, |row| row.version) != binding_version
    {
        return Err(task_binding_changed().into());
    }
    if let Some((milestone_id, milestone_version)) = assign {
        let package = repository
            .package(&actor.command.company_id, &project.id, milestone_id)
            .await?
            .ok_or_else(package_changed)?;
        if package.id != *milestone_id
            || package.version != milestone_version
            || !matches!(package.state.as_str(), "planned" | "in_progress")
        {
            return Err(package_changed().into());
        }
    } else {
        let Some(binding) = binding else {
            return Err(task_binding_changed().into());
        };
        let package = repository
            .package(
                &actor.command.company_id,
                &project.id,
                &binding.milestone_id,
            )
            .await?
            .ok_or_else(package_changed)?;
        if !matches!(package.state.as_str(), "planned" | "in_progress") {
            return Err(package_terminal().into());
        }
    }
    Ok(())
}

