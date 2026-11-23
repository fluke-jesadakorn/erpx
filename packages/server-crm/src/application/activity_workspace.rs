//! Native CRM activity workspace read use cases.
//!
//! The repository owns the exact CRM owner SELECTs; the directory supplies the
//! current recipient list from the Identity/People projections. Neither grants
//! authority. Purchase enforcement, selected-actor resolution and the
//! Workflow ledger remain composition responsibilities and are deliberately
//! outside this owner-side port.
use crate::domain::activities_native::ActivityActor;
use crate::domain::activity_workspace::{
    ActivityRecipient, ActivityWorkspaceData, CrmActivityRow, CrmLeadRow, CrmWorkItem,
    workspace_view, work_items,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type ActivityWorkspaceFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// CRM-owned rows. Every method observes the caller's already-open unit, so
/// uncommitted effects inside that transaction are visible to the reader.
pub trait ActivityWorkspaceRepository: Send {
    type Error: From<BusinessError> + Send;

    fn lead<'a>(
        &'a mut self,
        actor: &'a ActivityActor,
        lead_id: &'a str,
    ) -> ActivityWorkspaceFuture<'a, Option<CrmLeadRow>, Self::Error>;

    fn activities<'a>(
        &'a mut self,
        actor: &'a ActivityActor,
        lead_id: &'a str,
    ) -> ActivityWorkspaceFuture<'a, Vec<CrmActivityRow>, Self::Error>;

    fn history<'a>(
        &'a mut self,
        actor: &'a ActivityActor,
        lead_id: &'a str,
    ) -> ActivityWorkspaceFuture<'a, Vec<Value>, Self::Error>;

    fn work_rows<'a>(
        &'a mut self,
        actor: &'a ActivityActor,
    ) -> ActivityWorkspaceFuture<'a, Vec<CrmActivityRow>, Self::Error>;
}

/// Cross-owner recipient reference. The directory is a current read of the
/// Identity assignment and People workforce projections; it grants no
/// assignment or command authority.
pub trait ActivityWorkspaceDirectory: Send {
    type Error: Send;

    fn recipients<'a>(
        &'a mut self,
        company_id: &'a str,
        lead_id: &'a str,
    ) -> ActivityWorkspaceFuture<'a, Vec<ActivityRecipient>, Self::Error>;
}

fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CRM_LEAD_NOT_FOUND",
        "Lead not found in this scope.",
    )
}

fn read_denied() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CRM_ACTIVITY_READ_REQUIRED",
        "commerce.read is required to read the CRM activity workspace.",
    )
}

/// `workspace(leadId)` from the retained application: authorize the read,
/// require the scoped lead, enrich activities, append immutable history and
/// the current recipient directory.
pub async fn activity_workspace<R, D>(
    actor: &ActivityActor,
    lead_id: &str,
    repository: &mut R,
    directory: &mut D,
) -> Result<ActivityWorkspaceData, R::Error>
where
    R: ActivityWorkspaceRepository,
    D: ActivityWorkspaceDirectory,
    R::Error: From<D::Error>,
{
    if !actor.has_read {
        return Err(read_denied().into());
    }
    let lead = repository
        .lead(actor, lead_id)
        .await?
        .ok_or_else(not_found)?;
    let items = repository.activities(actor, lead_id).await?;
    let history = repository.history(actor, lead_id).await?;
    let recipients = directory
        .recipients(&actor.company_id, lead_id)
        .await
        .map_err(R::Error::from)?;
    Ok(ActivityWorkspaceData {
        view: workspace_view(&lead, &items, actor),
        history,
        recipients,
    })
}

/// `work()` from the retained application. A caller without `commerce.read`
/// gets an empty queue, exactly as the TypeScript reader does.
pub async fn activity_work<R>(
    actor: &ActivityActor,
    repository: &mut R,
) -> Result<Vec<CrmWorkItem>, R::Error>
where
    R: ActivityWorkspaceRepository,
{
    if !actor.has_read {
        return Ok(Vec::new());
    }
    let rows = repository.work_rows(actor).await?;
    Ok(work_items(&rows, actor))
}
