//! Procurement projection runtime; composition supplies capability-separated pools.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_procurement_sqlx::ProcurementProjector;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ProcurementProjectionFactory;

impl ProcurementProjectionFactory {
    pub fn registration(self) -> ProcurementProjectionRegistration {
        let projector = ProcurementProjector::new();
        ProcurementProjectionRegistration {
            name: projector.name().into(),
            event_types: projector.event_types().to_vec(),
        }
    }

    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<ProcurementProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(ProcurementProjectionRuntime {
            projector: ProcurementProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ProcurementProjectionRuntime {
    projector: ProcurementProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl ProcurementProjectionRuntime {
    pub fn registration(&self) -> ProcurementProjectionRegistration {
        ProcurementProjectionFactory.registration()
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
    use erpx_procurement_sqlx::{PROJECTION_NAME, SNAPSHOT_EVENT_TYPE};

    use super::*;

    #[test]
    fn registration_is_snapshot_only() {
        let value = ProcurementProjectionFactory.registration();
        assert_eq!(value.name, PROJECTION_NAME);
        assert_eq!(value.event_types, vec![SNAPSHOT_EVENT_TYPE]);
    }
}
