//! Control-plane runtime for the rebuildable Platform Collaboration projection.

use erpx_collaboration_sqlx::GlobalCollaborationProjector;
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalCollaborationProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GlobalCollaborationProjectionFactory;

impl GlobalCollaborationProjectionFactory {
    pub fn registration(self) -> GlobalCollaborationProjectionRegistration {
        let projector = GlobalCollaborationProjector::new();
        GlobalCollaborationProjectionRegistration {
            name: projector.name().into(),
            event_types: projector.event_types().to_vec(),
        }
    }

    pub fn build(
        self,
        control_write: WritePool,
        control_read: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<GlobalCollaborationProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(control_read);
        let runner =
            ProjectionRunner::new(ProjectionSource::new(control_write), store.clone(), config)?;
        Ok(GlobalCollaborationProjectionRuntime {
            projector: GlobalCollaborationProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct GlobalCollaborationProjectionRuntime {
    projector: GlobalCollaborationProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl GlobalCollaborationProjectionRuntime {
    pub fn registration(&self) -> GlobalCollaborationProjectionRegistration {
        GlobalCollaborationProjectionFactory.registration()
    }

    pub async fn run_once(&self) -> Result<ProjectionRun, CqrsSqlxError> {
        self.runner.run_active(&self.projector).await
    }

    pub async fn begin_rebuild(&self) -> Result<RebuildHandle, CqrsSqlxError> {
        self.store.begin_rebuild(&self.projector).await
    }

    pub async fn run_rebuild_page(
        &self,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        self.runner
            .run_rebuild(&self.projector, handle, after_sequence)
            .await
    }

    pub async fn activate_rebuild(&self, handle: &RebuildHandle) -> Result<(), CqrsSqlxError> {
        self.store.activate_rebuild(handle).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_collaboration_sqlx::GLOBAL_COLLABORATION_PROJECTION;

    #[test]
    fn registration_is_stable_and_typed() {
        let registration = GlobalCollaborationProjectionFactory.registration();
        assert_eq!(registration.name, GLOBAL_COLLABORATION_PROJECTION);
        assert_eq!(registration.event_types.len(), 1);
    }
}
