//! Assets projection runtime. Composition supplies capability-separated holding pools.

use erpx_assets_sqlx::{AssetLifecycleRecordProjector, AssetProjector};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetsProjectionKind {
    Assets,
    LifecycleRecords,
}

impl AssetsProjectionFactory {
    pub fn registrations(self) -> Vec<AssetsProjectionRegistration> {
        vec![
            registration(&AssetProjector::new()),
            registration(&AssetLifecycleRecordProjector::new()),
        ]
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
            assets: AssetProjector::new(),
            lifecycle_records: AssetLifecycleRecordProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AssetsProjectionRuntime {
    assets: AssetProjector,
    lifecycle_records: AssetLifecycleRecordProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl AssetsProjectionRuntime {
    pub fn registrations(&self) -> Vec<AssetsProjectionRegistration> {
        AssetsProjectionFactory.registrations()
    }

    pub async fn run_once(
        &self,
        kind: AssetsProjectionKind,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            AssetsProjectionKind::Assets => self.runner.run_active(&self.assets).await,
            AssetsProjectionKind::LifecycleRecords => {
                self.runner.run_active(&self.lifecycle_records).await
            }
        }
    }

    pub async fn begin_rebuild(
        &self,
        kind: AssetsProjectionKind,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            AssetsProjectionKind::Assets => self.store.begin_rebuild(&self.assets).await,
            AssetsProjectionKind::LifecycleRecords => {
                self.store.begin_rebuild(&self.lifecycle_records).await
            }
        }
    }

    pub async fn run_rebuild_page(
        &self,
        kind: AssetsProjectionKind,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            AssetsProjectionKind::Assets => {
                self.runner
                    .run_rebuild(&self.assets, handle, after_sequence)
                    .await
            }
            AssetsProjectionKind::LifecycleRecords => {
                self.runner
                    .run_rebuild(&self.lifecycle_records, handle, after_sequence)
                    .await
            }
        }
    }

    pub async fn activate_rebuild(
        &self,
        kind: AssetsProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            AssetsProjectionKind::Assets => self.assets.name(),
            AssetsProjectionKind::LifecycleRecords => self.lifecycle_records.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Assets projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

fn registration(projector: &impl Projector) -> AssetsProjectionRegistration {
    AssetsProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use erpx_assets_sqlx::{
        LIFECYCLE_RECORD_PROJECTION_NAME, LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE, PROJECTION_NAME,
        SNAPSHOT_EVENT_TYPE,
    };

    use super::*;

    #[test]
    fn registrations_are_snapshot_only() {
        let registrations = AssetsProjectionFactory.registrations();
        assert_eq!(registrations.len(), 2);
        assert_eq!(registrations[0].name, PROJECTION_NAME);
        assert_eq!(registrations[0].event_types, vec![SNAPSHOT_EVENT_TYPE]);
        assert_eq!(registrations[1].name, LIFECYCLE_RECORD_PROJECTION_NAME);
        assert_eq!(
            registrations[1].event_types,
            vec![LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE]
        );
    }

    #[tokio::test]
    async fn factory_builds_without_connecting() {
        let write = WritePool::from_control_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/assets_write").unwrap(),
        );
        let projection = ProjectionPool::from_control_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/assets_read").unwrap(),
        );
        let runtime = AssetsProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .unwrap();
        assert_eq!(
            runtime.registrations(),
            AssetsProjectionFactory.registrations()
        );
    }

    #[tokio::test]
    async fn cross_projection_rebuild_handle_fails_before_database_access() {
        let write = WritePool::from_control_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/assets_write").unwrap(),
        );
        let projection = ProjectionPool::from_control_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/assets_read").unwrap(),
        );
        let runtime = AssetsProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .unwrap();
        let wrong = RebuildHandle {
            projection: PROJECTION_NAME.into(),
            generation: uuid::Uuid::new_v4(),
            replaces: None,
        };
        assert!(matches!(
            runtime
                .activate_rebuild(AssetsProjectionKind::LifecycleRecords, &wrong)
                .await,
            Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Assets projector"
            ))
        ));
    }
}
