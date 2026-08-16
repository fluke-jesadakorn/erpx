//! Quality projection runtime adapter.
//!
//! Composition supplies capability-separated pools. This module neither
//! creates database connections nor depends on the Server runtime.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_quality_sqlx::{QualityInspectionProjector, QualityLifecycleRecordProjector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct QualityProjectionFactory;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityProjectionKind {
    Inspections,
    LifecycleRecords,
}

impl QualityProjectionFactory {
    pub fn registrations(self) -> Vec<QualityProjectionRegistration> {
        vec![
            registration(&QualityInspectionProjector::new()),
            registration(&QualityLifecycleRecordProjector::new()),
        ]
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
            inspections: QualityInspectionProjector::new(),
            lifecycle_records: QualityLifecycleRecordProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct QualityProjectionRuntime {
    inspections: QualityInspectionProjector,
    lifecycle_records: QualityLifecycleRecordProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl QualityProjectionRuntime {
    /// Metadata consumed by the worker catalog without exposing the SQLx
    /// projector implementation to the composition root.
    pub fn registrations(&self) -> Vec<QualityProjectionRegistration> {
        QualityProjectionFactory.registrations()
    }

    /// Runs one bounded page against the active generation. Duplicate,
    /// out-of-order, retry, DLQ, receipt, and checkpoint semantics are owned by
    /// the shared projection runner.
    pub async fn run_once(
        &self,
        kind: QualityProjectionKind,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            QualityProjectionKind::Inspections => self.runner.run_active(&self.inspections).await,
            QualityProjectionKind::LifecycleRecords => {
                self.runner.run_active(&self.lifecycle_records).await
            }
        }
    }

    /// Creates and resets a shadow generation without disturbing active reads.
    pub async fn begin_rebuild(
        &self,
        kind: QualityProjectionKind,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            QualityProjectionKind::Inspections => self.store.begin_rebuild(&self.inspections).await,
            QualityProjectionKind::LifecycleRecords => {
                self.store.begin_rebuild(&self.lifecycle_records).await
            }
        }
    }

    /// Replays one bounded page into a shadow generation. The caller must not
    /// activate while this report has a scheduled retry.
    pub async fn run_rebuild_page(
        &self,
        kind: QualityProjectionKind,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            QualityProjectionKind::Inspections => {
                self.runner
                    .run_rebuild(&self.inspections, handle, after_sequence)
                    .await
            }
            QualityProjectionKind::LifecycleRecords => {
                self.runner
                    .run_rebuild(&self.lifecycle_records, handle, after_sequence)
                    .await
            }
        }
    }

    /// Atomically switches read traffic after replay has reached its source
    /// fence. A generation race fails closed in `ProjectionStore`.
    pub async fn activate_rebuild(
        &self,
        kind: QualityProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            QualityProjectionKind::Inspections => self.inspections.name(),
            QualityProjectionKind::LifecycleRecords => self.lifecycle_records.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Quality projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

fn registration(projector: &impl Projector) -> QualityProjectionRegistration {
    QualityProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use erpx_cqrs_sqlx::RetryPolicy;
    use erpx_quality_sqlx::{
        LIFECYCLE_RECORD_PROJECTION_NAME, LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE, PROJECTION_NAME,
        SNAPSHOT_EVENT_TYPE,
    };
    use uuid::Uuid;

    use super::*;

    #[test]
    fn registration_is_typed_and_snapshot_only() {
        let registrations = QualityProjectionFactory.registrations();
        assert_eq!(registrations.len(), 2);
        assert_eq!(registrations[0].name, PROJECTION_NAME);
        assert_eq!(registrations[0].event_types, vec![SNAPSHOT_EVENT_TYPE]);
        assert_eq!(registrations[1].name, LIFECYCLE_RECORD_PROJECTION_NAME);
        assert_eq!(
            registrations[1].event_types,
            vec![LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE]
        );
    }

    fn lazy_pools() -> (WritePool, ProjectionPool) {
        let write = WritePool::from_control_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/quality_write").unwrap(),
        );
        let projection = ProjectionPool::from_control_pg_pool(
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
            runtime.registrations(),
            QualityProjectionFactory.registrations()
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
            .run_rebuild_page(QualityProjectionKind::Inspections, &wrong_handle, 0)
            .await
            .expect_err("cross-projector rebuild must fail closed");
        assert!(matches!(
            error,
            CqrsSqlxError::InvalidInput("rebuild handle belongs to another projector")
        ));
    }

    #[tokio::test]
    async fn cross_projection_activation_fails_before_database_access() {
        let (write, projection) = lazy_pools();
        let runtime = QualityProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .expect("valid runner configuration");
        let wrong = RebuildHandle {
            projection: PROJECTION_NAME.into(),
            generation: Uuid::new_v4(),
            replaces: None,
        };
        assert!(matches!(
            runtime
                .activate_rebuild(QualityProjectionKind::LifecycleRecords, &wrong)
                .await,
            Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Quality projector"
            ))
        ));
    }
}
