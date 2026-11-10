//! Native Projects attachment binding use case.
//!
//! Projects authorizes the target and advances its own project version; Files
//! owns the attachment set/link bind. The Files port must execute inside the
//! caller's retained transaction so both effects commit or roll back together.
//! This use case never creates, scans, stores or reads attachment bytes itself.
use crate::domain::project_attachment::{
    ProjectAttachmentActor, ProjectAttachmentBindingCommand, ProjectAttachmentBindingReceipt,
    ProjectAttachmentGrant, ProjectAttachmentTargetFacts, attachment_changed, attachment_invalid,
    attachment_not_found, authorize_binding, authorize_read, witness,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis};
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
    let grant = authorize_binding(
        actor,
        &facts,
        command,
        witness(&facts),
        now,
    )?;
    let mut receipt = files
        .bind(grant.clone(), command, now.get())
        .await?;
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

pub async fn authorize_project_attachment_read<R: ProjectAttachmentFactsRepository>(
    actor: &ProjectAttachmentActor,
    repository: &mut R,
    target_type: &str,
    target_id: &str,
    now: TimestampMillis,
) -> Result<(), R::Error> {
    actor.require_read(now)?;
    let target_type = crate::domain::project_attachment::ProjectAttachmentTargetType::parse(
        target_type,
    )?;
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
