//! Commercial projection runtime adapter with three independently rebuildable
//! read models. Composition supplies capability-separated database pools.

use erpx_commercial_sqlx::{CatalogPlanProjector, EntitlementProjector, SubscriptionProjector};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommercialProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommercialProjectionKind {
    CatalogPlans,
    Subscriptions,
    Entitlements,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CommercialProjectionFactory;

impl CommercialProjectionFactory {
    pub fn registrations(self) -> Vec<CommercialProjectionRegistration> {
        [
            registration(&CatalogPlanProjector::new()),
            registration(&SubscriptionProjector::new()),
            registration(&EntitlementProjector::new()),
        ]
        .into()
    }

    pub fn build(
        self,
        control_write: WritePool,
        control_projection: ProjectionPool,
        tenant_write: WritePool,
        tenant_projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<CommercialProjectionRuntime, CqrsSqlxError> {
        let control_store = ProjectionStore::new(control_projection);
        let control_runner = ProjectionRunner::new(
            ProjectionSource::new(control_write),
            control_store.clone(),
            config,
        )?;
        let tenant_store = ProjectionStore::new(tenant_projection);
        let tenant_runner = ProjectionRunner::new(
            ProjectionSource::new(tenant_write),
            tenant_store.clone(),
            config,
        )?;
        Ok(CommercialProjectionRuntime {
            catalog: CatalogPlanProjector::new(),
            subscriptions: SubscriptionProjector::new(),
            entitlements: EntitlementProjector::new(),
            control_runner,
            control_store,
            tenant_runner,
            tenant_store,
        })
    }

    pub fn build_single_plane(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<CommercialProjectionRuntime, CqrsSqlxError> {
        self.build(write.clone(), projection.clone(), write, projection, config)
    }
}

#[derive(Debug, Clone)]
pub struct CommercialProjectionRuntime {
    catalog: CatalogPlanProjector,
    subscriptions: SubscriptionProjector,
    entitlements: EntitlementProjector,
    control_runner: ProjectionRunner,
    control_store: ProjectionStore,
    tenant_runner: ProjectionRunner,
    tenant_store: ProjectionStore,
}

impl CommercialProjectionRuntime {
    pub fn registrations(&self) -> Vec<CommercialProjectionRegistration> {
        CommercialProjectionFactory.registrations()
    }

    pub async fn run_once(
        &self,
        kind: CommercialProjectionKind,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            CommercialProjectionKind::CatalogPlans => {
                self.control_runner.run_active(&self.catalog).await
            }
            CommercialProjectionKind::Subscriptions => {
                self.tenant_runner.run_active(&self.subscriptions).await
            }
            CommercialProjectionKind::Entitlements => {
                self.tenant_runner.run_active(&self.entitlements).await
            }
        }
    }

    pub async fn begin_rebuild(
        &self,
        kind: CommercialProjectionKind,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            CommercialProjectionKind::CatalogPlans => {
                self.control_store.begin_rebuild(&self.catalog).await
            }
            CommercialProjectionKind::Subscriptions => {
                self.tenant_store.begin_rebuild(&self.subscriptions).await
            }
            CommercialProjectionKind::Entitlements => {
                self.tenant_store.begin_rebuild(&self.entitlements).await
            }
        }
    }

    pub async fn run_rebuild_page(
        &self,
        kind: CommercialProjectionKind,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            CommercialProjectionKind::CatalogPlans => {
                self.control_runner
                    .run_rebuild(&self.catalog, handle, after_sequence)
                    .await
            }
            CommercialProjectionKind::Subscriptions => {
                self.tenant_runner
                    .run_rebuild(&self.subscriptions, handle, after_sequence)
                    .await
            }
            CommercialProjectionKind::Entitlements => {
                self.tenant_runner
                    .run_rebuild(&self.entitlements, handle, after_sequence)
                    .await
            }
        }
    }

    pub async fn activate_rebuild(
        &self,
        kind: CommercialProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            CommercialProjectionKind::CatalogPlans => self.catalog.name(),
            CommercialProjectionKind::Subscriptions => self.subscriptions.name(),
            CommercialProjectionKind::Entitlements => self.entitlements.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Commercial projector",
            ));
        }
        match kind {
            CommercialProjectionKind::CatalogPlans => {
                self.control_store.activate_rebuild(handle).await
            }
            CommercialProjectionKind::Subscriptions | CommercialProjectionKind::Entitlements => {
                self.tenant_store.activate_rebuild(handle).await
            }
        }
    }
}

fn registration(projector: &impl Projector) -> CommercialProjectionRegistration {
    CommercialProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_commercial_sqlx::{
        CATALOG_PLAN_PROJECTION, CATALOG_PLAN_SNAPSHOT_EVENT, ENTITLEMENT_PROJECTION,
        ENTITLEMENT_SNAPSHOT_EVENT, SUBSCRIPTION_PROJECTION, SUBSCRIPTION_SNAPSHOT_EVENT,
    };
    use erpx_cqrs_sqlx::RetryPolicy;
    use std::time::Duration;
    use uuid::Uuid;

    #[test]
    fn registration_catalog_is_complete_and_snapshot_only() {
        assert_eq!(
            CommercialProjectionFactory.registrations(),
            vec![
                CommercialProjectionRegistration {
                    name: CATALOG_PLAN_PROJECTION.into(),
                    event_types: vec![CATALOG_PLAN_SNAPSHOT_EVENT.into()]
                },
                CommercialProjectionRegistration {
                    name: SUBSCRIPTION_PROJECTION.into(),
                    event_types: vec![SUBSCRIPTION_SNAPSHOT_EVENT.into()]
                },
                CommercialProjectionRegistration {
                    name: ENTITLEMENT_PROJECTION.into(),
                    event_types: vec![ENTITLEMENT_SNAPSHOT_EVENT.into()]
                }
            ]
        );
    }

    fn lazy_pools() -> (WritePool, ProjectionPool, WritePool, ProjectionPool) {
        (
            WritePool::from_pg_pool(
                sqlx::PgPool::connect_lazy("postgres://localhost/commercial_control_write")
                    .unwrap(),
            ),
            ProjectionPool::from_pg_pool(
                sqlx::PgPool::connect_lazy("postgres://localhost/commercial_control_read").unwrap(),
            ),
            WritePool::from_pg_pool(
                sqlx::PgPool::connect_lazy("postgres://localhost/commercial_tenant_write").unwrap(),
            ),
            ProjectionPool::from_pg_pool(
                sqlx::PgPool::connect_lazy("postgres://localhost/commercial_tenant_read").unwrap(),
            ),
        )
    }

    #[tokio::test]
    async fn factory_builds_without_connecting() {
        let (control_write, control_projection, tenant_write, tenant_projection) = lazy_pools();
        let runtime = CommercialProjectionFactory
            .build(
                control_write,
                control_projection,
                tenant_write,
                tenant_projection,
                ProjectionRunnerConfig::default(),
            )
            .unwrap();
        assert_eq!(
            runtime.registrations(),
            CommercialProjectionFactory.registrations()
        );
    }

    #[tokio::test]
    async fn invalid_configuration_fails_before_runtime_access() {
        let (control_write, control_projection, tenant_write, tenant_projection) = lazy_pools();
        let result = CommercialProjectionFactory.build(
            control_write,
            control_projection,
            tenant_write,
            tenant_projection,
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
    async fn cross_projector_rebuild_handle_fails_closed() {
        let (control_write, control_projection, tenant_write, tenant_projection) = lazy_pools();
        let runtime = CommercialProjectionFactory
            .build(
                control_write,
                control_projection,
                tenant_write,
                tenant_projection,
                ProjectionRunnerConfig::default(),
            )
            .unwrap();
        let handle = RebuildHandle {
            projection: "another.projection".into(),
            generation: Uuid::new_v4(),
            replaces: None,
        };
        let error = runtime
            .run_rebuild_page(CommercialProjectionKind::CatalogPlans, &handle, 0)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            CqrsSqlxError::InvalidInput("rebuild handle belongs to another projector")
        ));
    }
}
