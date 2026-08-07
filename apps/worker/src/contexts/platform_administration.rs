//! Platform Administration projection runtime adapter.
//! Composition supplies control write/projection pools; this module creates no connections.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionEvent, ProjectionFailure, ProjectionPool, ProjectionRun,
    ProjectionRunner, ProjectionRunnerConfig, ProjectionSource, ProjectionStore, Projector,
    ProjectorContext, RebuildHandle, WritePool,
};
use erpx_platform_administration_sqlx::{
    AdminContextProjector, ConnectorProjector, TenantProjector,
};
use sqlx::{Postgres, Transaction};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformAdministrationProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformAdministrationProjectionKind {
    Tenants,
    Connectors,
    AdminContexts,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PlatformAdministrationProjectionFactory;

impl PlatformAdministrationProjectionFactory {
    pub fn registrations(self) -> Vec<PlatformAdministrationProjectionRegistration> {
        vec![
            registration(&TenantProjector::new()),
            registration(&ConnectorProjector::new()),
            registration(&AdminContextProjector::new()),
        ]
    }
    pub fn build(
        self,
        kind: PlatformAdministrationProjectionKind,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<PlatformAdministrationProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        let projector = match kind {
            PlatformAdministrationProjectionKind::Tenants => {
                PlatformProjector::Tenant(TenantProjector::new())
            }
            PlatformAdministrationProjectionKind::Connectors => {
                PlatformProjector::Connector(ConnectorProjector::new())
            }
            PlatformAdministrationProjectionKind::AdminContexts => {
                PlatformProjector::AdminContext(AdminContextProjector::new())
            }
        };
        Ok(PlatformAdministrationProjectionRuntime {
            projector,
            runner,
            store,
        })
    }
}

fn registration(projector: &dyn Projector) -> PlatformAdministrationProjectionRegistration {
    PlatformAdministrationProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

#[derive(Debug, Clone)]
enum PlatformProjector {
    Tenant(TenantProjector),
    Connector(ConnectorProjector),
    AdminContext(AdminContextProjector),
}

#[async_trait::async_trait]
impl Projector for PlatformProjector {
    fn name(&self) -> &str {
        match self {
            Self::Tenant(value) => value.name(),
            Self::Connector(value) => value.name(),
            Self::AdminContext(value) => value.name(),
        }
    }
    fn event_types(&self) -> &[String] {
        match self {
            Self::Tenant(value) => value.event_types(),
            Self::Connector(value) => value.event_types(),
            Self::AdminContext(value) => value.event_types(),
        }
    }
    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        match self {
            Self::Tenant(value) => value.apply(transaction, context, event).await,
            Self::Connector(value) => value.apply(transaction, context, event).await,
            Self::AdminContext(value) => value.apply(transaction, context, event).await,
        }
    }
    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        match self {
            Self::Tenant(value) => value.reset(transaction, context).await,
            Self::Connector(value) => value.reset(transaction, context).await,
            Self::AdminContext(value) => value.reset(transaction, context).await,
        }
    }
}

pub struct PlatformAdministrationProjectionRuntime {
    projector: PlatformProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl std::fmt::Debug for PlatformAdministrationProjectionRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PlatformAdministrationProjectionRuntime")
            .field("projection", &self.projector.name())
            .finish_non_exhaustive()
    }
}

impl PlatformAdministrationProjectionRuntime {
    pub fn registration(&self) -> PlatformAdministrationProjectionRegistration {
        registration(&self.projector)
    }
    /// Duplicate, out-of-order, retry, DLQ, receipt, and checkpoint behavior is shared-runner owned.
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
    use erpx_platform_administration_sqlx::{
        ADMIN_CONTEXT_PROJECTION, CONNECTOR_PROJECTION, TENANT_PROJECTION,
    };
    #[test]
    fn registration_catalog_is_complete() {
        let names = PlatformAdministrationProjectionFactory
            .registrations()
            .into_iter()
            .map(|value| value.name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                TENANT_PROJECTION,
                CONNECTOR_PROJECTION,
                ADMIN_CONTEXT_PROJECTION
            ]
        );
    }
    #[tokio::test]
    async fn factory_builds_without_connecting() {
        let write = WritePool::from_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/platform_admin_write").unwrap(),
        );
        let projection = ProjectionPool::from_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/platform_admin_read").unwrap(),
        );
        let runtime = PlatformAdministrationProjectionFactory
            .build(
                PlatformAdministrationProjectionKind::Tenants,
                write,
                projection,
                ProjectionRunnerConfig::default(),
            )
            .unwrap();
        assert_eq!(runtime.registration().name, TENANT_PROJECTION);
    }
}
