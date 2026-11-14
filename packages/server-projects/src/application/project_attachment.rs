//! Native Projects attachment binding use case.
//!
//! Projects authorizes the target and advances its own project version; Files
//! owns the attachment set/link bind. The Files port must execute inside the
//! caller's retained transaction so both effects commit or roll back together.
//! This use case never creates, scans, stores or reads attachment bytes itself.
use crate::domain::project_attachment::{
    ProjectAttachmentActor, ProjectAttachmentBindingCommand, ProjectAttachmentBindingReceipt,
    ProjectAttachmentGrant, ProjectAttachmentTargetFacts, attachment_changed, attachment_invalid,
    attachment_not_found, authorize_binding, authorize_read, idempotency_conflict, invalid_receipt,
    witness,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis};
use serde::{Deserialize, Serialize};
use std::future::Future;

pub type ProjectAttachmentFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait ProjectAttachmentFactsRepository: Send {
    type Error: From<BusinessError> + Send;

    fn target<'a>(
        &'a mut self,
        company: &'a CompanyId,
        target_type: &'a str,
        target_id: &'a str,
        user_id: &'a str,
    ) -> ProjectAttachmentFuture<'a, Option<ProjectAttachmentTargetFacts>, Self::Error>;
}

/// Files-owned binding port. `bind` stages the set/link bind; `advance_target`
/// applies the Projects version-step guard in the same transaction. Neither
/// method may commit on its own.
pub trait ProjectAttachmentFilesPort: Send {
    type Error: From<BusinessError> + Send;

    fn bind<'a>(
        &'a mut self,
        grant: ProjectAttachmentGrant,
        command: &'a ProjectAttachmentBindingCommand,
        now: i64,
    ) -> ProjectAttachmentFuture<'a, ProjectAttachmentBindingReceipt, Self::Error>;
    fn advance_target<'a>(
        &'a mut self,
        grant: &'a ProjectAttachmentGrant,
        now: i64,
    ) -> ProjectAttachmentFuture<'a, i64, Self::Error>;
}

pub async fn bind_project_attachment<R, F, E>(
    actor: &ProjectAttachmentActor,
    repository: &mut R,
    files: &mut F,
    command: &ProjectAttachmentBindingCommand,
    now: TimestampMillis,
) -> Result<ProjectAttachmentBindingReceipt, E>
where
    R: ProjectAttachmentFactsRepository<Error = E>,
    F: ProjectAttachmentFilesPort<Error = E>,
    E: From<BusinessError> + Send,
{
    actor.require_write(now)?;
    command.validate()?;
    let target_id = RecordId::new(command.target_id.as_str())?;
    let facts = repository
        .target(
            &actor.company_id,
            command.target_type.as_str(),
            target_id.as_str(),
            &actor.user_id,
        )
        .await?
        .ok_or_else(attachment_not_found)?;
    let grant = authorize_binding(actor, &facts, command, witness(&facts), now)?;
    let mut receipt = files.bind(grant.clone(), command, now.get()).await?;
    if receipt.set_id.is_empty() || receipt.link_ids != command.link_ids {
        return Err(attachment_invalid().into());
    }
    let target_version = files.advance_target(&grant, now.get()).await?;
    if target_version != grant.expected_version + 1 {
        return Err(attachment_changed().into());
    }
    receipt.target_version = target_version;
    Ok(receipt)
}

/// The single Projects-owned operation name for a project attachment bind.
pub const PROJECT_ATTACHMENT_BIND_OPERATION: &str = "project.attachment.bind";

/// Prior binding receipt retained by the Projects owner. Replay returns this
/// exact image only for the same command key, operation, set and expected
/// version; the Files set/link bind and every project version step happen once.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAttachmentCommandReceipt {
    pub id: String,
    pub version: i64,
    pub status: String,
    pub owner_version: i64,
    pub attachment_set_id: String,
    pub attachment_link_id: String,
}

#[derive(Clone, Debug)]
pub struct StoredProjectAttachmentReceipt {
    pub operation: String,
    pub record_id: String,
    pub request_fingerprint: String,
    pub result_json: Option<String>,
}

/// The exact retained row a successful binding writes. Composition supplies the
/// already-bound receipt so the ledger and the Files effects share one unit.
#[derive(Clone, Debug)]
pub struct ProjectAttachmentReceiptPlan {
    pub key: String,
    pub operation: String,
    pub company_id: String,
    pub actor_id: String,
    pub request_fingerprint: String,
    pub set_id: String,
    pub receipt: ProjectAttachmentCommandReceipt,
    pub authority_json: String,
    pub now: i64,
}

pub trait ProjectAttachmentCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ProjectAttachmentFuture<'a, Option<StoredProjectAttachmentReceipt>, Self::Error>;
    fn record_receipt<'a>(
        &'a mut self,
        plan: &'a ProjectAttachmentReceiptPlan,
    ) -> ProjectAttachmentFuture<'a, (), Self::Error>;
}

pub fn project_attachment_fingerprint(
    actor: &ProjectAttachmentActor,
    set_id: &str,
    expected_version: i64,
) -> Result<String, BusinessError> {
    serde_json::to_string(&serde_json::json!({
        "operation": PROJECT_ATTACHMENT_BIND_OPERATION,
        "companyId": actor.company_id.as_str(),
        "actorId": actor.user_id,
        "setId": set_id,
        "expectedTargetVersion": expected_version,
    }))
    .map_err(|_| invalid_receipt())
}

pub fn project_attachment_authority_json(
    actor: &ProjectAttachmentActor,
) -> Result<String, BusinessError> {
    serde_json::to_string(&serde_json::json!({
        "userId": actor.user_id,
        "companyId": actor.company_id.as_str(),
        "branchId": actor.branch_id.as_ref().map(RecordId::as_str),
        "departmentId": actor.department_id.as_ref().map(RecordId::as_str),
        "permissions": actor.permissions,
        "modules": actor.modules,
        "expiresAt": actor.expires_at,
    }))
    .map_err(|_| invalid_receipt())
}

/// Replay policy for the Projects-owned receipt. A key reused for another
/// operation or another actor/Company is `IDEMPOTENCY_CONFLICT`; the same key
/// against a different set or target version is `ATTACHMENT_SET_CHANGED`.
pub fn replay_project_attachment_binding(
    stored: Option<StoredProjectAttachmentReceipt>,
    actor: &ProjectAttachmentActor,
    set_id: &str,
    expected_version: i64,
) -> Result<Option<ProjectAttachmentCommandReceipt>, BusinessError> {
    let Some(stored) = stored else {
        return Ok(None);
    };
    if stored.operation != PROJECT_ATTACHMENT_BIND_OPERATION {
        return Err(idempotency_conflict());
    }
    let Some(result_json) = stored.result_json else {
        return Err(invalid_receipt());
    };
    let receipt: ProjectAttachmentCommandReceipt =
        serde_json::from_str(&result_json).map_err(|_| invalid_receipt())?;
    if stored.record_id != set_id
        || receipt.attachment_set_id != set_id
        || expected_version < 1
        || receipt.owner_version != expected_version + 1
    {
        return Err(crate::domain::project_attachment::attachment_set_changed());
    }
    if receipt.status != "bound"
        || receipt.id.is_empty()
        || receipt.attachment_link_id.is_empty()
        || receipt.version != receipt.owner_version
    {
        return Err(invalid_receipt());
    }
    if stored.request_fingerprint
        != project_attachment_fingerprint(actor, set_id, expected_version)?
    {
        return Err(idempotency_conflict());
    }
    Ok(Some(receipt))
}

/// Project the Files binding receipt plus its bound manifest into the retained
/// Projects command receipt. Both arrivals must describe exactly the same links.
pub fn project_attachment_command_receipt(
    binding: &ProjectAttachmentBindingReceipt,
    manifest: &[crate::domain::project_attachment::ProjectAttachmentBoundLink],
) -> Result<ProjectAttachmentCommandReceipt, BusinessError> {
    if binding.set_id.is_empty()
        || binding.target_version < 1
        || manifest.is_empty()
        || manifest.len() != binding.link_ids.len()
        || manifest
            .iter()
            .zip(&binding.link_ids)
            .any(|(item, link_id)| &item.link_id != link_id)
    {
        return Err(attachment_changed());
    }
    let first = &manifest[0];
    if first.source_id.is_empty() || first.link_id.is_empty() {
        return Err(attachment_changed());
    }
    Ok(ProjectAttachmentCommandReceipt {
        id: first.source_id.clone(),
        version: binding.target_version,
        status: "bound".into(),
        owner_version: binding.target_version,
        attachment_set_id: binding.set_id.clone(),
        attachment_link_id: first.link_id.clone(),
    })
}

pub async fn authorize_project_attachment_read<R: ProjectAttachmentFactsRepository>(
    actor: &ProjectAttachmentActor,
    repository: &mut R,
    target_type: &str,
    target_id: &str,
    now: TimestampMillis,
) -> Result<(), R::Error> {
    actor.require_read(now)?;
    let target_type =
        crate::domain::project_attachment::ProjectAttachmentTargetType::parse(target_type)?;
    let target_id = RecordId::new(target_id)?;
    let facts = repository
        .target(
            &actor.company_id,
            target_type.as_str(),
            target_id.as_str(),
            &actor.user_id,
        )
        .await?
        .ok_or_else(attachment_not_found)?;
    authorize_read(actor, &facts, now)?;
    Ok(())
}
