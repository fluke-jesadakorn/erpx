//! Native Projects read use cases.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. The application enforces the current actor policy and validates
//! the bounded owner rows before presentation.
use crate::domain::project_wbs::{
    MilestoneRow, ProjectReadActor, ProjectRow, ProjectWbs, not_found, validate_milestones,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis};
use std::future::Future;

pub type ProjectWbsReadFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait ProjectWbsReadRepository: Send {
    type Error: From<BusinessError> + Send;
    fn project<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectWbsReadFuture<'a, Option<ProjectRow>, Self::Error>;
    fn milestones<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
    ) -> ProjectWbsReadFuture<'a, Vec<MilestoneRow>, Self::Error>;
}

pub async fn project_wbs<R: ProjectWbsReadRepository>(
    actor: &ProjectReadActor,
    repository: &mut R,
    project_id: &str,
    now: TimestampMillis,
) -> Result<ProjectWbs, R::Error> {
    actor.require_wbs(now)?;
    let project_id = RecordId::new(project_id).map_err(|_| not_found())?;
    let project = repository
        .project(&actor.company_id, project_id.as_str(), &actor.user_id)
        .await?
        .ok_or_else(not_found)?;
    project.validate(&actor.company_id, project_id.as_str())?;
    if !project.is_owner && !project.is_member {
        return Err(not_found().into());
    }
    let milestones = repository
        .milestones(&actor.company_id, project_id.as_str())
        .await?;
    let milestones = validate_milestones(&actor.company_id, project_id.as_str(), milestones, now)?;
    Ok(ProjectWbs {
        project,
        milestones,
    })
}
