//! Native Projects graph read use case.
//!
//! The caller owns the retained serializable Holding transaction. This use
//! case checks the current actor before the first source read, rejects hidden
//! projects as not found, and validates every joined owner fact before the
//! graph is presented.
use crate::domain::project_graph::{
    GraphActual, GraphTask, ProjectGraph, ProjectGraphActor, build_project_graph,
};
use crate::application::project_wbs::ProjectWbsReadRepository;
use crate::domain::project_wbs::not_found;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis};
use std::future::Future;

pub type ProjectGraphFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait ProjectGraphReadRepository: ProjectWbsReadRepository {
    fn tasks<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
    ) -> ProjectGraphFuture<'a, Vec<GraphTask>, Self::Error>;
    fn actuals<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        actor_user_id: &'a str,
    ) -> ProjectGraphFuture<'a, Vec<GraphActual>, Self::Error>;
}

pub async fn project_graph<R: ProjectGraphReadRepository>(
    actor: &ProjectGraphActor,
    repository: &mut R,
    project_id: &str,
    now: TimestampMillis,
) -> Result<ProjectGraph, R::Error> {
    actor.require_graph(now)?;
    let project_id = RecordId::new(project_id).map_err(|_| not_found())?;
    let project = repository
        .project(
            &actor.read.company_id,
            project_id.as_str(),
            &actor.read.user_id,
        )
        .await?
        .ok_or_else(not_found)?;
    project
        .validate(&actor.read.company_id, project_id.as_str())
        .map_err(|error| {
            if error.code == "PROJECT_WBS_SOURCE_INVALID" {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "PROJECT_GRAPH_SOURCE_INVALID",
                    "The current project graph is invalid.",
                )
                .into()
            } else {
                error.into()
            }
        })?;
    if !project.is_owner && !project.is_member {
        return Err(not_found().into());
    }
    let milestones = repository
        .milestones(&actor.read.company_id, project_id.as_str())
        .await?;
    let tasks = repository
        .tasks(&actor.read.company_id, project_id.as_str())
        .await?;
    let actuals = repository
        .actuals(
            &actor.read.company_id,
            project_id.as_str(),
            &actor.read.user_id,
        )
        .await?;
    build_project_graph(actor, project, milestones, tasks, actuals, now).map_err(Into::into)
}
