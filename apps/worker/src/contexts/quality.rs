//! Quality projection runtime adapter.
//!
//! Composition supplies capability-separated pools. This module neither
//! creates database connections nor depends on the Server runtime.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_quality_sqlx::QualityInspectionProjector;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct QualityProjectionFactory;

impl QualityProjectionFactory {
    pub fn registration(self) -> QualityProjectionRegistration {
        let projector = QualityInspectionProjector::new();
        QualityProjectionRegistration {
            name: projector.name().into(),
            event_types: projector.event_types().to_vec(),
        }
    }

    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<QualityProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(QualityProjectionRuntime {
            projector: QualityInspectionProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct QualityProjectionRuntime {
    projector: QualityInspectionProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl QualityProjectionRuntime {
    /// Metadata consumed by the worker catalog without exposing the SQLx
    /// projector implementation to the composition root.
    pub fn registration(&self) -> QualityProjectionRegistration {
        QualityProjectionFactory.registration()
    }

    /// Runs one bounded page against the active generation. Duplicate,
    /// out-of-order, retry, DLQ, receipt, and checkpoint semantics are owned by
    /// the shared projection runner.
    pub async fn run_once(&self) -> Result<ProjectionRun, CqrsSqlxError> {
        self.runner.run_active(&self.projector).await
    }

    /// Creates and resets a shadow generation without disturbing active reads.
    pub async fn begin_rebuild(&self) -> Result<RebuildHandle, CqrsSqlxError> {
        self.store.begin_rebuild(&self.projector).await
    }

    /// Replays one bounded page into a shadow generation. The caller must not
    /// activate while this report has a scheduled retry.
    pub async fn run_rebuild_page(
        &self,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        self.runner
            .run_rebuild(&self.projector, handle, after_sequence)
            .await
    }

    /// Atomically switches read traffic after replay has reached its source
    /// fence. A generation race fails closed in `ProjectionStore`.
    pub async fn activate_rebuild(&self, handle: &RebuildHandle) -> Result<(), CqrsSqlxError> {
        self.store.activate_rebuild(handle).await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use erpx_cqrs_sqlx::RetryPolicy;
    use erpx_quality_sqlx::{PROJECTION_NAME, SNAPSHOT_EVENT_TYPE};
    use uuid::Uuid;

    use super::*;

    #[test]
    fn registration_is_typed_and_snapshot_only() {
        let registration = QualityProjectionFactory.registration();
        assert_eq!(registration.name, PROJECTION_NAME);
        assert_eq!(registration.event_types, vec![SNAPSHOT_EVENT_TYPE]);
    }

    fn lazy_pools() -> (WritePool, ProjectionPool) {
        let write = WritePool::from_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/quality_write").unwrap(),
        );
        let projection = ProjectionPool::from_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/quality_read").unwrap(),
        );
        (write, projection)
    }

    #[tokio::test]
    async fn factory_builds_catalogued_runtime_without_connecting() {
        let (write, projection) = lazy_pools();
        let runtime = QualityProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .expect("valid runner configuration");

        assert_eq!(
            runtime.registration(),
            QualityProjectionFactory.registration()
        );
    }

    #[tokio::test]
    async fn invalid_runner_configuration_fails_before_runtime_access() {
        let (write, projection) = lazy_pools();
        let result = QualityProjectionFactory.build(
            write,
            projection,
            ProjectionRunnerConfig {
                page_size: u32::MAX,
                retry: RetryPolicy {
                    max_attempts: 3,
                    base_delay: Duration::from_millis(10),
                    maximum_delay: Duration::from_secs(1),
                },
            },
        );
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn rebuild_handle_for_another_projection_fails_before_database_access() {
        let (write, projection) = lazy_pools();
        let runtime = QualityProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .expect("valid runner configuration");
        let wrong_handle = RebuildHandle {
            projection: "another.projection".into(),
            generation: Uuid::new_v4(),
            replaces: None,
        };

        let error = runtime
            .run_rebuild_page(&wrong_handle, 0)
            .await
            .expect_err("cross-projector rebuild must fail closed");
        assert!(matches!(
            error,
            CqrsSqlxError::InvalidInput("rebuild handle belongs to another projector")
        ));
    }
}
