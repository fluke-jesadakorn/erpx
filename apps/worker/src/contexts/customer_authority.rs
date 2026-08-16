//! Customer Authority control-plane projection runtime adapter.
//!
//! Composition supplies capability-separated control pools. This module
//! creates no database connections and owns no scheduler registration.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_customer_authority_sqlx::CustomerAuthorityProjector;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerAuthorityProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CustomerAuthorityProjectionFactory;

impl CustomerAuthorityProjectionFactory {
    pub fn registration(self) -> CustomerAuthorityProjectionRegistration {
        let projector = CustomerAuthorityProjector::new();
        CustomerAuthorityProjectionRegistration {
            name: projector.name().into(),
            event_types: projector.event_types().to_vec(),
        }
    }

    pub fn build(
        self,
        control_write: WritePool,
        control_projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<CustomerAuthorityProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(control_projection);
        let runner =
            ProjectionRunner::new(ProjectionSource::new(control_write), store.clone(), config)?;
        Ok(CustomerAuthorityProjectionRuntime {
            projector: CustomerAuthorityProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CustomerAuthorityProjectionRuntime {
    projector: CustomerAuthorityProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl CustomerAuthorityProjectionRuntime {
    pub fn registration(&self) -> CustomerAuthorityProjectionRegistration {
        CustomerAuthorityProjectionFactory.registration()
    }

    /// Duplicate, out-of-order, checkpoint, retry, DLQ, and projection receipt
    /// semantics are provided by the shared serialized projection runner.
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
    use std::time::Duration;

    use erpx_cqrs_sqlx::RetryPolicy;
    use erpx_customer_authority_sqlx::{PROJECTION_NAME, SNAPSHOT_EVENT_TYPE};
    use uuid::Uuid;

    use super::*;

    #[test]
    fn registration_is_typed_and_snapshot_only() {
        let registration = CustomerAuthorityProjectionFactory.registration();
        assert_eq!(registration.name, PROJECTION_NAME);
        assert_eq!(registration.event_types, vec![SNAPSHOT_EVENT_TYPE]);
    }

    #[test]
    fn runtime_delegates_delivery_guarantees_and_full_rebuild_to_shared_runner() {
        let source = include_str!("customer_authority.rs");
        for required in [
            ".run_active(&self.projector)",
            ".begin_rebuild(&self.projector)",
            ".run_rebuild(&self.projector",
            ".activate_rebuild(handle)",
        ] {
            assert!(
                source.contains(required),
                "missing runtime delegation {required}"
            );
        }
        let shared = include_str!("../../../../crates/adapters/cqrs-sqlx/src/projection.rs");
        for guarantee in [
            "projection_applied_events",
            "projection_checkpoints",
            "projection_failures",
            "dead_lettered_at",
            "projection_receipts",
        ] {
            assert!(
                shared.contains(guarantee),
                "shared runner lacks {guarantee}"
            );
        }
    }

    fn lazy_pools() -> (WritePool, ProjectionPool) {
        let write = WritePool::from_control_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/customer_authority_write").unwrap(),
        );
        let projection = ProjectionPool::from_control_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/customer_authority_read").unwrap(),
        );
        (write, projection)
    }

    #[tokio::test]
    async fn factory_builds_control_runtime_without_connecting() {
        let (write, projection) = lazy_pools();
        let runtime = CustomerAuthorityProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .expect("valid runner configuration");
        assert_eq!(
            runtime.registration(),
            CustomerAuthorityProjectionFactory.registration()
        );
    }

    #[tokio::test]
    async fn invalid_runner_configuration_fails_before_runtime_access() {
        let (write, projection) = lazy_pools();
        let result = CustomerAuthorityProjectionFactory.build(
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
        let runtime = CustomerAuthorityProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .unwrap();
        let wrong = RebuildHandle {
            projection: "another.projection".into(),
            generation: Uuid::new_v4(),
            replaces: None,
        };
        let error = runtime.run_rebuild_page(&wrong, 0).await.unwrap_err();
        assert!(matches!(
            error,
            CqrsSqlxError::InvalidInput("rebuild handle belongs to another projector")
        ));
    }
}
