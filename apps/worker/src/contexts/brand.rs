//! Brand projection runtime registration. Runtime composition supplies the
//! capability-separated control-write and control-read projection pools.

use erpx_brand_sqlx::{BrandHistoryProjector, BrandProfileProjector, BrandResolvedProjector};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrandProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrandProjectionKind {
    Profiles,
    Resolved,
    History,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BrandProjectionFactory;

impl BrandProjectionFactory {
    pub fn registrations(self) -> Vec<BrandProjectionRegistration> {
        [
            registration(&BrandProfileProjector::new()),
            registration(&BrandResolvedProjector::new()),
            registration(&BrandHistoryProjector::new()),
        ]
        .into()
    }

    pub fn build(
        self,
        control_write: WritePool,
        control_projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<BrandProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(control_projection);
        let runner =
            ProjectionRunner::new(ProjectionSource::new(control_write), store.clone(), config)?;
        Ok(BrandProjectionRuntime {
            profiles: BrandProfileProjector::new(),
            resolved: BrandResolvedProjector::new(),
            history: BrandHistoryProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct BrandProjectionRuntime {
    profiles: BrandProfileProjector,
    resolved: BrandResolvedProjector,
    history: BrandHistoryProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl BrandProjectionRuntime {
    pub fn registrations(&self) -> Vec<BrandProjectionRegistration> {
        BrandProjectionFactory.registrations()
    }

    /// Duplicate/out-of-order detection, retries, DLQ records, receipts, and
    /// checkpoints are owned by the shared projection runner.
    pub async fn run_once(
        &self,
        kind: BrandProjectionKind,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            BrandProjectionKind::Profiles => self.runner.run_active(&self.profiles).await,
            BrandProjectionKind::Resolved => self.runner.run_active(&self.resolved).await,
            BrandProjectionKind::History => self.runner.run_active(&self.history).await,
        }
    }

    pub async fn begin_rebuild(
        &self,
        kind: BrandProjectionKind,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            BrandProjectionKind::Profiles => self.store.begin_rebuild(&self.profiles).await,
            BrandProjectionKind::Resolved => self.store.begin_rebuild(&self.resolved).await,
            BrandProjectionKind::History => self.store.begin_rebuild(&self.history).await,
        }
    }

    pub async fn run_rebuild_page(
        &self,
        kind: BrandProjectionKind,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            BrandProjectionKind::Profiles => {
                self.runner
                    .run_rebuild(&self.profiles, handle, after_sequence)
                    .await
            }
            BrandProjectionKind::Resolved => {
                self.runner
                    .run_rebuild(&self.resolved, handle, after_sequence)
                    .await
            }
            BrandProjectionKind::History => {
                self.runner
                    .run_rebuild(&self.history, handle, after_sequence)
                    .await
            }
        }
    }

    pub async fn activate_rebuild(
        &self,
        kind: BrandProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            BrandProjectionKind::Profiles => self.profiles.name(),
            BrandProjectionKind::Resolved => self.resolved.name(),
            BrandProjectionKind::History => self.history.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Brand projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

fn registration(projector: &impl Projector) -> BrandProjectionRegistration {
    BrandProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use erpx_brand_sqlx::{
        HISTORY_PROJECTION, PROFILE_PROJECTION, RESOLVED_PROJECTION, SNAPSHOT_EVENT_TYPE,
    };
    use erpx_cqrs_sqlx::RetryPolicy;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn registration_catalog_has_three_snapshot_only_projectors() {
        assert_eq!(
            BrandProjectionFactory.registrations(),
            vec![
                BrandProjectionRegistration {
                    name: PROFILE_PROJECTION.into(),
                    event_types: vec![SNAPSHOT_EVENT_TYPE.into()],
                },
                BrandProjectionRegistration {
                    name: RESOLVED_PROJECTION.into(),
                    event_types: vec![SNAPSHOT_EVENT_TYPE.into()],
                },
                BrandProjectionRegistration {
                    name: HISTORY_PROJECTION.into(),
                    event_types: vec![SNAPSHOT_EVENT_TYPE.into()],
                },
            ]
        );
    }

    fn lazy_pools() -> (WritePool, ProjectionPool) {
        (
            WritePool::from_control_pg_pool(
                sqlx::PgPool::connect_lazy("postgres://localhost/brand_control_write").unwrap(),
            ),
            ProjectionPool::from_control_pg_pool(
                sqlx::PgPool::connect_lazy("postgres://localhost/brand_control_read").unwrap(),
            ),
        )
    }

    #[tokio::test]
    async fn factory_builds_all_runtimes_without_connecting() {
        let (write, projection) = lazy_pools();
        let runtime = BrandProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .unwrap();
        assert_eq!(
            runtime.registrations(),
            BrandProjectionFactory.registrations()
        );
    }

    #[tokio::test]
    async fn invalid_configuration_fails_before_runtime_access() {
        let (write, projection) = lazy_pools();
        let result = BrandProjectionFactory.build(
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
    async fn cross_projection_rebuild_handle_fails_closed() {
        let (write, projection) = lazy_pools();
        let runtime = BrandProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .unwrap();
        let wrong = RebuildHandle {
            projection: "brand.another".into(),
            generation: Uuid::new_v4(),
            replaces: None,
        };
        assert!(
            runtime
                .run_rebuild_page(BrandProjectionKind::Profiles, &wrong, 0)
                .await
                .is_err()
        );
    }
}
