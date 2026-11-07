//! Native Planning workspace use case.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. The application checks the current Planning read policy before
//! any owner read and validates the bounded source before presentation.
use crate::domain::planning::{PlanningActor, PlanningWorkspace, validate_workspace};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use std::future::Future;

pub type PlanningReadFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait PlanningWorkspaceRepository: Send {
    type Error: From<BusinessError> + Send;

    fn workspace<'a>(
        &'a mut self,
        actor: &'a PlanningActor,
    ) -> PlanningReadFuture<'a, PlanningWorkspace, Self::Error>;
}

pub async fn planning_workspace<R: PlanningWorkspaceRepository>(
    actor: &PlanningActor,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<PlanningWorkspace, R::Error> {
    actor.require_read(now)?;
    let workspace = repository.workspace(actor).await?;
    validate_workspace(actor, workspace).map_err(Into::into)
}
