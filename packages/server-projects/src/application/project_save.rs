//! Native Projects create/edit command use cases.
//!
//! The repository owns one serializable unit and re-checks the current project
//! owner, lifecycle and version after it locks the project. A replay returns the
//! retained receipt only when the command key, operation, record and request
//! fingerprint all agree, and is admitted by retained purchase history rather
//! than by new work. Active-staff and purchase facts are supplied by
//! composition from their owning current-authority ports.
use crate::domain::project_commands::{ProjectCommandActor, ProjectCommandProject};
use crate::domain::project_save::{
    SaveProjectActor, SaveProjectCommand, SaveProjectPlan, SaveProjectReceipt,
    StoredProjectReceipt, authority_json, fingerprint, idempotency_conflict,
    incomplete_receipt, invalid_receipt, missing_project, owner_required, package_history_required,
    package_required, staff_link_required, structure_frozen, version_conflict,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, RecordId, TimestampMillis,
};
use std::future::Future;

pub type ProjectSaveFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait SaveProjectRepository: Send {
    type Error: From<BusinessError> + Send;

    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ProjectSaveFuture<'a, Option<StoredProjectReceipt>, Self::Error>;
    fn project<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectSaveFuture<'a, Option<ProjectCommandProject>, Self::Error>;
    fn save<'a>(
        &'a mut self,
        plan: SaveProjectPlan,
    ) -> ProjectSaveFuture<'a, SaveProjectReceipt, Self::Error>;
}

fn replay(
    stored: Option<StoredProjectReceipt>,
    operation: &str,
    record_id: &str,
    request_fingerprint: &str,
) -> Result<Option<SaveProjectReceipt>, BusinessError> {
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
    let receipt: SaveProjectReceipt =
        serde_json::from_str(&result).map_err(|_| invalid_receipt())?;
    if receipt.id != record_id || receipt.version < 1 {
        return Err(invalid_receipt());
    }
    Ok(Some(receipt))
}

fn check_receipt(
    receipt: SaveProjectReceipt,
    id: &str,
    version: i64,
) -> Result<SaveProjectReceipt, BusinessError> {
    if receipt.id != id || receipt.version != version {
        return Err(invalid_receipt());
    }
    Ok(receipt)
}

fn require_new_work(actor: &SaveProjectActor) -> Result<(), BusinessError> {
    if !actor.purchase.new_work {
        return Err(package_required());
    }
    Ok(())
}

fn require_history(actor: &SaveProjectActor) -> Result<(), BusinessError> {
    if !actor.purchase.history {
        return Err(package_history_required());
    }
    Ok(())
}

fn validate_current_project(
    actor: &ProjectCommandActor,
    project: &ProjectCommandProject,
) -> Result<(), BusinessError> {
    if project.company_id != actor.company_id.as_str()
        || project.id.is_empty()
        || project.version < 1
        || !matches!(
            project.lifecycle_status.as_str(),
            "active" | "handoff_pending" | "completed" | "cancelled"
        )
    {
        return Err(missing_project());
    }
    if !project.is_owner && !project.is_member {
        return Err(missing_project());
    }
    Ok(())
}

fn require_active_staff(actor: &SaveProjectActor, members: &[String]) -> Result<(), BusinessError> {
    for user in members {
        if !actor.active_staff_users.contains(user) {
            return Err(staff_link_required());
        }
    }
    Ok(())
}

/// Create (no `project_id`) or edit (existing `project_id`) one project brief
/// and its complete membership.
#[allow(clippy::too_many_arguments)]
pub async fn save_project<R: SaveProjectRepository>(
    actor: &SaveProjectActor,
    repository: &mut R,
    project_id: Option<&str>,
    input: &SaveProjectCommand,
    key: &str,
    now: TimestampMillis,
) -> Result<SaveProjectReceipt, R::Error> {
    actor.command.require_write(now)?;
    input.validate()?;
    let key = CommandKey::new(key)?;
    let record_id = match project_id {
        Some(value) => RecordId::new(value)?.as_str().to_owned(),
        None => format!("project-{}", key.as_str()),
    };
    if record_id.encode_utf16().count() > 200 {
        return Err(BusinessError::invalid(
            "PROJECT_INVALID",
            "Use a shorter command key for this project.",
        )
        .into());
    }
    let operation = if project_id.is_some() {
        "project.update"
    } else {
        "project.create"
    };
    let fingerprint = fingerprint(
        operation,
        &record_id,
        &serde_json::to_value(input).map_err(|_| invalid_receipt())?,
    )?;
    if let Some(receipt) = replay(
        repository.stored_receipt(key.as_str()).await?,
        operation,
        &record_id,
        &fingerprint,
    )? {
        // The retained command admits an exact replay through retained history,
        // not through currently purchased new work.
        require_history(actor)?;
        return Ok(receipt);
    }
    require_new_work(actor)?;
    let members = input.unique_member_user_ids();
    require_active_staff(actor, &members)?;
    let current = match project_id {
        Some(value) => {
            let project = repository
                .project(&actor.command.company_id, value, &actor.command.user_id)
                .await?
                .ok_or_else(missing_project)?;
            validate_current_project(&actor.command, &project)?;
            if !project.is_owner {
                return Err(owner_required().into());
            }
            if project.lifecycle_status != "active" {
                return Err(structure_frozen().into());
            }
            if project.version != input.expected_version {
                return Err(version_conflict().into());
            }
            Some(project)
        }
        None => None,
    };
    let version = current
        .as_ref()
        .map_or(1, |project| project.version + 1);
    let plan = SaveProjectPlan {
        key,
        operation: operation.to_owned(),
        company_id: actor.command.company_id.clone(),
        actor_id: actor.command.user_id.clone(),
        assignment_id: actor.command.assignment_id.clone(),
        authority_json: authority_json(&actor.command)?,
        project_id: record_id.clone(),
        project_version: current.as_ref().map_or(0, |project| project.version),
        title: input.title.clone(),
        description: input.description.clone(),
        branch_id: if current.is_some() {
            None
        } else {
            actor.staff_branch_id.clone()
        },
        department_id: if current.is_some() {
            None
        } else {
            actor.staff_department_id.clone()
        },
        member_user_ids: members,
        version,
        request_fingerprint: fingerprint,
        now: now.get(),
    };
    let receipt = repository.save(plan).await?;
    check_receipt(receipt, &record_id, version).map_err(Into::into)
}
