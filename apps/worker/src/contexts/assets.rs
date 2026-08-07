//! Assets projection runtime. Composition supplies capability-separated tenant pools.

use erpx_assets_sqlx::AssetProjector;
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetsProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AssetsProjectionFactory;

impl AssetsProjectionFactory {
    pub fn registration(self) -> AssetsProjectionRegistration {
        let projector = AssetProjector::new();
        AssetsProjectionRegistration {
            name: projector.name().into(),
            event_types: projector.event_types().to_vec(),
        }
    }

    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<AssetsProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(AssetsProjectionRuntime {
            projector: AssetProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AssetsProjectionRuntime {
    projector: AssetProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl AssetsProjectionRuntime {
    pub fn registration(&self) -> AssetsProjectionRegistration {
        AssetsProjectionFactory.registration()
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
    use erpx_assets_sqlx::{PROJECTION_NAME, SNAPSHOT_EVENT_TYPE};

    use super::*;

    #[test]
    fn registration_is_snapshot_only() {
        assert_eq!(
            AssetsProjectionFactory.registration(),
            AssetsProjectionRegistration {
                name: PROJECTION_NAME.into(),
                event_types: vec![SNAPSHOT_EVENT_TYPE.into()],
            }
        );
    }

    #[tokio::test]
    async fn factory_builds_without_connecting() {
        let write = WritePool::from_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/assets_write").unwrap(),
        );
        let projection = ProjectionPool::from_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/assets_read").unwrap(),
        );
        let runtime = AssetsProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .unwrap();
        assert_eq!(
            runtime.registration(),
            AssetsProjectionFactory.registration()
        );
    }
}
