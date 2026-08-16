//! Organization projection runtime adapter.
//!
//! Composition supplies capability-separated pools. This module neither
//! creates database connections nor depends on the Server runtime.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_organization_sqlx::{OrganizationNodeProjector, OrganizationSharingProjector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganizationProjectionKind {
    Nodes,
    SharingAgreements,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OrganizationProjectionFactory;

impl OrganizationProjectionFactory {
    pub fn registrations(self) -> Vec<OrganizationProjectionRegistration> {
        let nodes = OrganizationNodeProjector::new();
        let sharing = OrganizationSharingProjector::new();
        vec![registration(&nodes), registration(&sharing)]
    }

    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<OrganizationProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(OrganizationProjectionRuntime {
            nodes: OrganizationNodeProjector::new(),
            sharing: OrganizationSharingProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct OrganizationProjectionRuntime {
    nodes: OrganizationNodeProjector,
    sharing: OrganizationSharingProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl OrganizationProjectionRuntime {
    pub fn registrations(&self) -> Vec<OrganizationProjectionRegistration> {
        OrganizationProjectionFactory.registrations()
    }

    /// Duplicate, out-of-order, retry, DLQ, receipt, and checkpoint semantics
    /// are provided by the shared projection runner.
    pub async fn run_once(
        &self,
        kind: OrganizationProjectionKind,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            OrganizationProjectionKind::Nodes => self.runner.run_active(&self.nodes).await,
            OrganizationProjectionKind::SharingAgreements => {
                self.runner.run_active(&self.sharing).await
            }
        }
    }

    pub async fn begin_rebuild(
        &self,
        kind: OrganizationProjectionKind,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            OrganizationProjectionKind::Nodes => self.store.begin_rebuild(&self.nodes).await,
            OrganizationProjectionKind::SharingAgreements => {
                self.store.begin_rebuild(&self.sharing).await
            }
        }
    }

    pub async fn run_rebuild_page(
        &self,
        kind: OrganizationProjectionKind,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            OrganizationProjectionKind::Nodes => {
                self.runner
                    .run_rebuild(&self.nodes, handle, after_sequence)
                    .await
            }
            OrganizationProjectionKind::SharingAgreements => {
                self.runner
                    .run_rebuild(&self.sharing, handle, after_sequence)
                    .await
            }
        }
    }

    pub async fn activate_rebuild(
        &self,
        kind: OrganizationProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            OrganizationProjectionKind::Nodes => self.nodes.name(),
            OrganizationProjectionKind::SharingAgreements => self.sharing.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

fn registration(projector: &impl Projector) -> OrganizationProjectionRegistration {
    OrganizationProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use erpx_cqrs_sqlx::RetryPolicy;
    use erpx_organization_sqlx::{
        PROJECTION_NAME, SHARING_PROJECTION_NAME, SHARING_SNAPSHOT_EVENT_TYPE, SNAPSHOT_EVENT_TYPE,
    };
    use uuid::Uuid;

    use super::*;

    #[test]
    fn registrations_are_typed_and_snapshot_only() {
        let registrations = OrganizationProjectionFactory.registrations();
        assert_eq!(registrations.len(), 2);
        assert_eq!(registrations[0].name, PROJECTION_NAME);
        assert_eq!(registrations[0].event_types, vec![SNAPSHOT_EVENT_TYPE]);
        assert_eq!(registrations[1].name, SHARING_PROJECTION_NAME);
        assert_eq!(
            registrations[1].event_types,
            vec![SHARING_SNAPSHOT_EVENT_TYPE]
        );
    }

    fn lazy_pools() -> (WritePool, ProjectionPool) {
        let write = WritePool::from_control_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/organization_write").unwrap(),
        );
        let projection = ProjectionPool::from_control_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/organization_read").unwrap(),
        );
        (write, projection)
    }

    #[tokio::test]
    async fn factory_builds_catalogued_runtime_without_connecting() {
        let (write, projection) = lazy_pools();
        let runtime = OrganizationProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .expect("valid runner configuration");
        assert_eq!(
            runtime.registrations(),
            OrganizationProjectionFactory.registrations()
        );
    }

    #[tokio::test]
    async fn invalid_runner_configuration_fails_before_runtime_access() {
        let (write, projection) = lazy_pools();
        let result = OrganizationProjectionFactory.build(
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
        let runtime = OrganizationProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .unwrap();
        let wrong = RebuildHandle {
            projection: "another.projection".into(),
            generation: Uuid::new_v4(),
            replaces: None,
        };
        let error = runtime
            .run_rebuild_page(OrganizationProjectionKind::Nodes, &wrong, 0)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            CqrsSqlxError::InvalidInput("rebuild handle belongs to another projector")
        ));
    }
}
