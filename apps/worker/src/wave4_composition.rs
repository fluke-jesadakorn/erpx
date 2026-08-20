//! Worker catalog composition for Wave 4 projections.

use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionPool, ProjectionRunnerConfig, WritePool};
use erpx_infrastructure::{
    HoldingDatabaseRouter, ProjectionClaimRecord, ProjectionJobKind, RoutedDatabaseSide,
    SqlxControlPlane, holding_holding_scope_id,
};
use erpx_worker::{
    HoldingProjectionExecutor, HoldingProjectionExecutorError, NamedWorkerHandler,
    WorkerHandlerReport, WorkerRuntimeError,
};
use std::sync::Arc;

use crate::contexts::{
    commercial::{
        CommercialProjectionFactory, CommercialProjectionKind, CommercialProjectionRuntime,
    },
    identity_access::{IdentityAccessProjectionFactory, IdentityAccessProjectionRuntime},
    organization::{
        OrganizationProjectionFactory, OrganizationProjectionKind, OrganizationProjectionRuntime,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityAccessProjectionKind {
    Principals,
    Roles,
    Governance,
}

#[derive(Debug, Clone)]
pub enum Wave4ProjectionExecutor {
    Commercial {
        control: Arc<SqlxControlPlane>,
        router: HoldingDatabaseRouter,
        kind: CommercialProjectionKind,
    },
    IdentityAccess {
        router: HoldingDatabaseRouter,
        kind: IdentityAccessProjectionKind,
    },
    Organization {
        router: HoldingDatabaseRouter,
        kind: OrganizationProjectionKind,
    },
}

impl Wave4ProjectionExecutor {
    pub const fn projection_name(&self) -> &'static str {
        match self {
            Self::Commercial { kind, .. } => match kind {
                CommercialProjectionKind::CatalogPlans => {
                    erpx_commercial_sqlx::CATALOG_PLAN_PROJECTION
                }
                CommercialProjectionKind::Subscriptions => {
                    erpx_commercial_sqlx::SUBSCRIPTION_PROJECTION
                }
                CommercialProjectionKind::Entitlements => {
                    erpx_commercial_sqlx::ENTITLEMENT_PROJECTION
                }
                CommercialProjectionKind::LifecycleRecords => {
                    erpx_commercial_sqlx::LIFECYCLE_RECORD_PROJECTION_NAME
                }
            },
            Self::IdentityAccess { kind, .. } => match kind {
                IdentityAccessProjectionKind::Principals => {
                    erpx_identity_access_sqlx::PRINCIPAL_PROJECTION_NAME
                }
                IdentityAccessProjectionKind::Roles => {
                    erpx_identity_access_sqlx::ROLE_PROJECTION_NAME
                }
                IdentityAccessProjectionKind::Governance => {
                    erpx_identity_access_sqlx::GOVERNANCE_PROJECTION_NAME
                }
            },
            Self::Organization { kind, .. } => match kind {
                OrganizationProjectionKind::Nodes => erpx_organization_sqlx::PROJECTION_NAME,
                OrganizationProjectionKind::SharingAgreements => {
                    erpx_organization_sqlx::SHARING_PROJECTION_NAME
                }
            },
        }
    }

    pub fn commercial(
        control: Arc<SqlxControlPlane>,
        router: HoldingDatabaseRouter,
        kind: CommercialProjectionKind,
    ) -> Self {
        debug_assert!(kind != CommercialProjectionKind::CatalogPlans);
        for registration in CommercialProjectionFactory.registrations() {
            tracing::info!(
                projection = %registration.name,
                event_types = ?registration.event_types,
                "registered typed Commercial CQRS projector"
            );
        }
        Self::Commercial {
            control,
            router,
            kind,
        }
    }

    pub fn identity_access(
        router: HoldingDatabaseRouter,
        kind: IdentityAccessProjectionKind,
    ) -> Self {
        for registration in IdentityAccessProjectionFactory.registrations() {
            tracing::info!(
                projection = %registration.name,
                event_types = ?registration.event_types,
                "registered typed Identity & Access CQRS projector"
            );
        }
        Self::IdentityAccess { router, kind }
    }

    pub fn organization(router: HoldingDatabaseRouter, kind: OrganizationProjectionKind) -> Self {
        for registration in OrganizationProjectionFactory.registrations() {
            tracing::info!(
                projection = %registration.name,
                event_types = ?registration.event_types,
                "registered typed Organization CQRS projector"
            );
        }
        Self::Organization { router, kind }
    }

    async fn identity_runtime(
        router: &HoldingDatabaseRouter,
        holding_id: &str,
        data_profile_key: &str,
        page_size: u32,
    ) -> Result<IdentityAccessProjectionRuntime, HoldingProjectionExecutorError> {
        let (write, read) = route_pools(router, holding_id, data_profile_key).await?;
        let runtime = IdentityAccessProjectionFactory
            .build(
                write,
                read,
                ProjectionRunnerConfig {
                    page_size,
                    ..ProjectionRunnerConfig::default()
                },
            )
            .map_err(crate::map_cqrs_projection_error)?;
        debug_assert_eq!(
            runtime.registrations(),
            IdentityAccessProjectionFactory.registrations()
        );
        Ok(runtime)
    }

    async fn organization_runtime(
        router: &HoldingDatabaseRouter,
        holding_id: &str,
        data_profile_key: &str,
        page_size: u32,
    ) -> Result<OrganizationProjectionRuntime, HoldingProjectionExecutorError> {
        let (write, read) = route_pools(router, holding_id, data_profile_key).await?;
        let runtime = OrganizationProjectionFactory
            .build(
                write,
                read,
                ProjectionRunnerConfig {
                    page_size,
                    ..ProjectionRunnerConfig::default()
                },
            )
            .map_err(crate::map_cqrs_projection_error)?;
        debug_assert_eq!(
            runtime.registrations(),
            OrganizationProjectionFactory.registrations()
        );
        Ok(runtime)
    }

    async fn commercial_runtime(
        control: &Arc<SqlxControlPlane>,
        router: &HoldingDatabaseRouter,
        holding_id: &str,
        data_profile_key: &str,
        page_size: u32,
    ) -> Result<CommercialProjectionRuntime, HoldingProjectionExecutorError> {
        let (holding_write, holding_read) =
            route_pools(router, holding_id, data_profile_key).await?;
        let runtime = CommercialProjectionFactory
            .build(
                WritePool::from_control_pg_pool(control.pools().write.clone()),
                ProjectionPool::from_control_pg_pool(control.pools().read.clone()),
                holding_write,
                holding_read,
                ProjectionRunnerConfig {
                    page_size,
                    ..ProjectionRunnerConfig::default()
                },
            )
            .map_err(crate::map_cqrs_projection_error)?;
        debug_assert_eq!(
            runtime.registrations(),
            CommercialProjectionFactory.registrations()
        );
        Ok(runtime)
    }

    async fn incremental(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        page_size: u32,
    ) -> Result<(), HoldingProjectionExecutorError> {
        let run = match self {
            Self::Commercial {
                control,
                router,
                kind,
            } => {
                Self::commercial_runtime(control, router, holding_id, data_profile_key, page_size)
                    .await?
                    .run_once(*kind)
                    .await
            }
            Self::IdentityAccess { router, kind } => {
                let runtime =
                    Self::identity_runtime(router, holding_id, data_profile_key, page_size).await?;
                match kind {
                    IdentityAccessProjectionKind::Principals => runtime.run_principals_once().await,
                    IdentityAccessProjectionKind::Roles => runtime.run_roles_once().await,
                    IdentityAccessProjectionKind::Governance => runtime.run_governance_once().await,
                }
            }
            Self::Organization { router, kind } => {
                Self::organization_runtime(router, holding_id, data_profile_key, page_size)
                    .await?
                    .run_once(*kind)
                    .await
            }
        }
        .map_err(crate::map_cqrs_projection_error)?;
        if run.retry_scheduled || run.dead_lettered != 0 {
            return Err(HoldingProjectionExecutorError::Retryable);
        }
        Ok(())
    }

    async fn rebuild(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        page_size: u32,
    ) -> Result<(), HoldingProjectionExecutorError> {
        match self {
            Self::Commercial {
                control,
                router,
                kind,
            } => {
                let runtime = Self::commercial_runtime(
                    control,
                    router,
                    holding_id,
                    data_profile_key,
                    page_size,
                )
                .await?;
                let handle = runtime
                    .begin_rebuild(*kind)
                    .await
                    .map_err(crate::map_cqrs_projection_error)?;
                let mut cursor = 0;
                loop {
                    let run = runtime
                        .run_rebuild_page(*kind, &handle, cursor)
                        .await
                        .map_err(crate::map_cqrs_projection_error)?;
                    if run.retry_scheduled || run.dead_lettered != 0 {
                        return Err(HoldingProjectionExecutorError::Retryable);
                    }
                    cursor = run.cursor;
                    if run.fetched < page_size {
                        break;
                    }
                }
                runtime
                    .activate_rebuild(*kind, &handle)
                    .await
                    .map_err(crate::map_cqrs_projection_error)
            }
            Self::IdentityAccess { router, kind } => {
                let runtime =
                    Self::identity_runtime(router, holding_id, data_profile_key, page_size).await?;
                let handle = match kind {
                    IdentityAccessProjectionKind::Principals => {
                        runtime.begin_principal_rebuild().await
                    }
                    IdentityAccessProjectionKind::Roles => runtime.begin_role_rebuild().await,
                    IdentityAccessProjectionKind::Governance => {
                        runtime.begin_governance_rebuild().await
                    }
                }
                .map_err(crate::map_cqrs_projection_error)?;
                run_identity_rebuild(&runtime, &handle, page_size).await?;
                runtime
                    .activate_rebuild(&handle)
                    .await
                    .map_err(crate::map_cqrs_projection_error)
            }
            Self::Organization { router, kind } => {
                let runtime =
                    Self::organization_runtime(router, holding_id, data_profile_key, page_size)
                        .await?;
                let handle = runtime
                    .begin_rebuild(*kind)
                    .await
                    .map_err(crate::map_cqrs_projection_error)?;
                let mut cursor = 0;
                loop {
                    let run = runtime
                        .run_rebuild_page(*kind, &handle, cursor)
                        .await
                        .map_err(crate::map_cqrs_projection_error)?;
                    if run.retry_scheduled || run.dead_lettered != 0 {
                        return Err(HoldingProjectionExecutorError::Retryable);
                    }
                    cursor = run.cursor;
                    if run.fetched < page_size {
                        break;
                    }
                }
                runtime
                    .activate_rebuild(*kind, &handle)
                    .await
                    .map_err(crate::map_cqrs_projection_error)
            }
        }
    }
}

pub const COMMERCIAL_CATALOG_PROJECTION_HANDLER_NAME: &str = "commercial_catalog_projection";

#[derive(Debug, Clone)]
pub struct CommercialCatalogProjectionHandler {
    runtime: CommercialProjectionRuntime,
}

impl CommercialCatalogProjectionHandler {
    pub fn new(control: Arc<SqlxControlPlane>, page_size: u32) -> Result<Self, WorkerRuntimeError> {
        let runtime = CommercialProjectionFactory
            .build_single_plane(
                WritePool::from_control_pg_pool(control.pools().write.clone()),
                ProjectionPool::from_control_pg_pool(control.pools().read.clone()),
                ProjectionRunnerConfig {
                    page_size,
                    ..ProjectionRunnerConfig::default()
                },
            )
            .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
        debug_assert_eq!(
            runtime.registrations(),
            CommercialProjectionFactory.registrations()
        );
        Ok(Self { runtime })
    }
}

#[async_trait]
impl NamedWorkerHandler for CommercialCatalogProjectionHandler {
    fn name(&self) -> &str {
        COMMERCIAL_CATALOG_PROJECTION_HANDLER_NAME
    }

    async fn run_once(
        &self,
        _worker_id: uuid::Uuid,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let run = self
            .runtime
            .run_once(CommercialProjectionKind::CatalogPlans)
            .await
            .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
        Ok(WorkerHandlerReport {
            claimed: u64::from(run.fetched),
            processed: u64::from(run.applied + run.duplicates),
            published: 0,
            retried: u64::from(run.retry_scheduled),
            dead_lettered: u64::from(run.dead_lettered),
            has_work: run.fetched != 0 || run.retry_scheduled,
        })
    }
}

#[async_trait]
impl HoldingProjectionExecutor for Wave4ProjectionExecutor {
    fn projection_name_filter(&self) -> Option<&str> {
        Some(self.projection_name())
    }

    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), HoldingProjectionExecutorError> {
        if claim.job.payload.get("candidate_database_ref").is_some()
            || claim.job.payload.get("source_version").is_some()
            || claim.job.payload.get("generation").is_some()
            || claim.job.payload.get("after").is_some()
            || claim.job.payload.get("cursor").is_some()
        {
            return Err(HoldingProjectionExecutorError::Permanent);
        }
        let page_size = crate::projection_page_size(&claim.job.payload, 500)?;
        match claim.job.kind {
            ProjectionJobKind::Incremental => {
                self.incremental(
                    &claim.job.binding.holding_id,
                    &claim.job.binding.data_profile_key,
                    page_size,
                )
                .await
            }
            ProjectionJobKind::Rebuild => {
                self.rebuild(
                    &claim.job.binding.holding_id,
                    &claim.job.binding.data_profile_key,
                    page_size,
                )
                .await
            }
        }
    }
}

async fn route_pools(
    router: &HoldingDatabaseRouter,
    holding_id: &str,
    data_profile_key: &str,
) -> Result<(WritePool, ProjectionPool), HoldingProjectionExecutorError> {
    let write = router
        .route(holding_id, data_profile_key, RoutedDatabaseSide::Write)
        .await
        .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
    let read = router
        .route(holding_id, data_profile_key, RoutedDatabaseSide::Read)
        .await
        .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
    // Identity & Access stores authority and projections under the canonical
    // holding scope derived from the routed data profile. The scheduler's
    // holding ID is an opaque registry key and must never become an RLS key.
    let holding_id = holding_holding_scope_id(data_profile_key);
    Ok((
        WritePool::for_holding(write.database_pool().clone(), holding_id)
            .map_err(crate::map_cqrs_projection_error)?,
        ProjectionPool::for_holding(read.database_pool().clone(), holding_id)
            .map_err(crate::map_cqrs_projection_error)?,
    ))
}

async fn run_identity_rebuild(
    runtime: &IdentityAccessProjectionRuntime,
    handle: &erpx_cqrs_sqlx::RebuildHandle,
    page_size: u32,
) -> Result<(), HoldingProjectionExecutorError> {
    let mut cursor = 0;
    loop {
        let run = runtime
            .run_rebuild_page(handle, cursor)
            .await
            .map_err(crate::map_cqrs_projection_error)?;
        if run.retry_scheduled || run.dead_lettered != 0 {
            return Err(HoldingProjectionExecutorError::Retryable);
        }
        cursor = run.cursor;
        if run.fetched < page_size {
            return Ok(());
        }
    }
}
