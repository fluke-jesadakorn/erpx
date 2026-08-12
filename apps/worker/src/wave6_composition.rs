//! Tenant worker composition for the Wave 6 paired bounded contexts.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionPool, ProjectionRun, ProjectionRunnerConfig, WritePool};
use erpx_infrastructure::{
    ProjectionClaimRecord, ProjectionJobKind, RoutedDatabaseSide, TenantDatabaseRouter,
};
use erpx_maintenance::PortError as MaintenancePortError;
use erpx_worker::{TenantProjectionExecutor, TenantProjectionExecutorError};

use crate::contexts::{
    assets::{AssetsProjectionFactory, AssetsProjectionRuntime},
    crm::{CrmProjectionFactory, CrmProjectionKind, CrmProjectionRuntime},
    inventory::{InventoryProjectionFactory, InventoryProjectionRuntime},
    maintenance::{
        MaintenanceAssetFactRuntime, MaintenanceAssetFactRuntimeError,
        MaintenanceProjectionFactory, MaintenanceProjectionRuntime,
    },
    marketing::{MarketingProjectionFactory, MarketingProjectionKind, MarketingProjectionRuntime},
    procurement::{ProcurementProjectionFactory, ProcurementProjectionRuntime},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wave6ProjectionKind {
    MarketingLeadCandidates,
    MarketingCampaignBudgets,
    CrmLeads,
    CrmOpportunities,
    ProcurementPurchaseOrders,
    InventoryStockItems,
    Assets,
    MaintenanceWorkOrders,
    MaintenanceAssetFacts,
}

#[derive(Debug, Clone)]
pub struct Wave6ProjectionExecutor {
    router: TenantDatabaseRouter,
    kind: Wave6ProjectionKind,
    fact_runtimes: Arc<Mutex<BTreeMap<(String, String), MaintenanceAssetFactRuntime>>>,
}

impl Wave6ProjectionExecutor {
    pub fn new(router: TenantDatabaseRouter, kind: Wave6ProjectionKind) -> Self {
        Self {
            router,
            kind,
            fact_runtimes: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub const fn projection_name(&self) -> &'static str {
        match self.kind {
            Wave6ProjectionKind::MarketingLeadCandidates => {
                erpx_marketing_sqlx::LEAD_PROJECTION_NAME
            }
            Wave6ProjectionKind::MarketingCampaignBudgets => {
                erpx_marketing_sqlx::BUDGET_PROJECTION_NAME
            }
            Wave6ProjectionKind::CrmLeads => erpx_crm_sqlx::LEAD_PROJECTION,
            Wave6ProjectionKind::CrmOpportunities => erpx_crm_sqlx::OPPORTUNITY_PROJECTION,
            Wave6ProjectionKind::ProcurementPurchaseOrders => {
                erpx_procurement_sqlx::PROJECTION_NAME
            }
            Wave6ProjectionKind::InventoryStockItems => erpx_inventory_sqlx::PROJECTION_NAME,
            Wave6ProjectionKind::Assets => erpx_assets_sqlx::PROJECTION_NAME,
            Wave6ProjectionKind::MaintenanceWorkOrders => erpx_maintenance_sqlx::PROJECTION_NAME,
            Wave6ProjectionKind::MaintenanceAssetFacts => {
                erpx_maintenance_sqlx::ASSET_FACT_PROJECTION_NAME
            }
        }
    }

    async fn route_pools(
        &self,
        tenant_id: &str,
        holding_ref: &str,
    ) -> Result<(sqlx::PgPool, sqlx::PgPool), TenantProjectionExecutorError> {
        let write = self
            .router
            .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| TenantProjectionExecutorError::Retryable)?;
        let read = self
            .router
            .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
            .await
            .map_err(|_| TenantProjectionExecutorError::Retryable)?;
        Ok((write.database_pool().clone(), read.database_pool().clone()))
    }

    async fn snapshot_runtime(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        page_size: u32,
    ) -> Result<SnapshotRuntime, TenantProjectionExecutorError> {
        let (write, read) = self.route_pools(tenant_id, holding_ref).await?;
        let write = WritePool::from_pg_pool(write);
        let read = ProjectionPool::from_pg_pool(read);
        let config = ProjectionRunnerConfig {
            page_size,
            ..ProjectionRunnerConfig::default()
        };
        let runtime = match self.kind {
            Wave6ProjectionKind::MarketingLeadCandidates
            | Wave6ProjectionKind::MarketingCampaignBudgets => {
                let runtime = MarketingProjectionFactory
                    .build(write, read, config)
                    .map_err(crate::map_cqrs_projection_error)?;
                debug_assert_eq!(
                    runtime.registrations(),
                    MarketingProjectionFactory.registrations()
                );
                let kind = match self.kind {
                    Wave6ProjectionKind::MarketingLeadCandidates => {
                        MarketingProjectionKind::LeadCandidates
                    }
                    Wave6ProjectionKind::MarketingCampaignBudgets => {
                        MarketingProjectionKind::CampaignBudgets
                    }
                    _ => unreachable!("matched Marketing projection kind"),
                };
                SnapshotRuntime::Marketing(runtime, kind)
            }
            Wave6ProjectionKind::CrmLeads => {
                let runtime = CrmProjectionFactory
                    .build(write, read, config)
                    .map_err(crate::map_cqrs_projection_error)?;
                debug_assert_eq!(
                    runtime.registrations(),
                    CrmProjectionFactory.registrations()
                );
                SnapshotRuntime::Crm(runtime, CrmProjectionKind::Leads)
            }
            Wave6ProjectionKind::CrmOpportunities => {
                let runtime = CrmProjectionFactory
                    .build(write, read, config)
                    .map_err(crate::map_cqrs_projection_error)?;
                debug_assert_eq!(
                    runtime.registrations(),
                    CrmProjectionFactory.registrations()
                );
                SnapshotRuntime::Crm(runtime, CrmProjectionKind::Opportunities)
            }
            Wave6ProjectionKind::ProcurementPurchaseOrders => {
                let runtime = ProcurementProjectionFactory
                    .build(write, read, config)
                    .map_err(crate::map_cqrs_projection_error)?;
                debug_assert_eq!(
                    runtime.registration(),
                    ProcurementProjectionFactory.registration()
                );
                SnapshotRuntime::Procurement(runtime)
            }
            Wave6ProjectionKind::InventoryStockItems => {
                let runtime = InventoryProjectionFactory
                    .build(write, read, config)
                    .map_err(crate::map_cqrs_projection_error)?;
                debug_assert_eq!(
                    runtime.registration(),
                    InventoryProjectionFactory.registration()
                );
                SnapshotRuntime::Inventory(runtime)
            }
            Wave6ProjectionKind::Assets => {
                let runtime = AssetsProjectionFactory
                    .build(write, read, config)
                    .map_err(crate::map_cqrs_projection_error)?;
                debug_assert_eq!(
                    runtime.registration(),
                    AssetsProjectionFactory.registration()
                );
                SnapshotRuntime::Assets(runtime)
            }
            Wave6ProjectionKind::MaintenanceWorkOrders => {
                let runtime = MaintenanceProjectionFactory
                    .build(write, read, config)
                    .map_err(crate::map_cqrs_projection_error)?;
                debug_assert_eq!(
                    runtime.registration(),
                    MaintenanceProjectionFactory.registration()
                );
                SnapshotRuntime::Maintenance(runtime)
            }
            Wave6ProjectionKind::MaintenanceAssetFacts => {
                return Err(TenantProjectionExecutorError::Permanent);
            }
        };
        Ok(runtime)
    }

    async fn facts_runtime(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        page_size: u32,
        reset: bool,
    ) -> Result<MaintenanceAssetFactRuntime, TenantProjectionExecutorError> {
        let key = (tenant_id.to_owned(), holding_ref.to_owned());
        if !reset
            && let Some(runtime) = self
                .fact_runtimes
                .lock()
                .map_err(|_| TenantProjectionExecutorError::Retryable)?
                .get(&key)
                .cloned()
        {
            return Ok(runtime);
        }
        let write = self
            .router
            .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| TenantProjectionExecutorError::Retryable)?;
        let raw = write.database_pool().clone();
        let runtime = MaintenanceAssetFactRuntime::with_source(
            WritePool::from_pg_pool(raw.clone()),
            raw,
            page_size,
        )
        .map_err(map_fact_error)?;
        debug_assert_eq!(
            runtime.registration().event_types,
            ["assets.condition_changed", "assets.maintenance_requested"]
        );
        self.fact_runtimes
            .lock()
            .map_err(|_| TenantProjectionExecutorError::Retryable)?
            .insert(key, runtime.clone());
        Ok(runtime)
    }

    async fn run_facts(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        page_size: u32,
        rebuild: bool,
    ) -> Result<(), TenantProjectionExecutorError> {
        let runtime = self
            .facts_runtime(tenant_id, holding_ref, page_size, rebuild)
            .await?;
        loop {
            let run = runtime.run_once().await.map_err(map_fact_error)?;
            if !rebuild || run.fetched < page_size {
                break;
            }
        }
        // A maintenance request writes the Work Order snapshot while consuming
        // the authoritative Assets event. Project that snapshot before this
        // dependency job completes so a separate projector cannot race ahead
        // of Work Order creation and permanently miss the new write.
        let (write, read) = self.route_pools(tenant_id, holding_ref).await?;
        let work_orders = MaintenanceProjectionFactory
            .build(
                WritePool::from_pg_pool(write),
                ProjectionPool::from_pg_pool(read),
                ProjectionRunnerConfig {
                    page_size,
                    ..ProjectionRunnerConfig::default()
                },
            )
            .map_err(crate::map_cqrs_projection_error)?;
        loop {
            let run = work_orders
                .run_once()
                .await
                .map_err(crate::map_cqrs_projection_error)?;
            let complete = run.fetched < page_size;
            validate_run(run)?;
            if complete {
                break;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl TenantProjectionExecutor for Wave6ProjectionExecutor {
    fn projection_name_filter(&self) -> Option<&str> {
        Some(self.projection_name())
    }

    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), TenantProjectionExecutorError> {
        if claim.job.payload.get("after").is_some()
            || claim.job.payload.get("cursor").is_some()
            || claim.job.payload.get("candidate_database_ref").is_some()
            || claim.job.payload.get("source_version").is_some()
            || claim.job.payload.get("generation").is_some()
        {
            return Err(TenantProjectionExecutorError::Permanent);
        }
        let tenant_id = &claim.job.binding.tenant_id;
        let holding_ref = &claim.job.binding.holding_ref;
        let page_size = crate::projection_page_size(&claim.job.payload, 500)?;
        if self.kind == Wave6ProjectionKind::MaintenanceAssetFacts {
            return self
                .run_facts(
                    tenant_id,
                    holding_ref,
                    page_size,
                    claim.job.kind == ProjectionJobKind::Rebuild,
                )
                .await;
        }
        let runtime = self
            .snapshot_runtime(tenant_id, holding_ref, page_size)
            .await?;
        match claim.job.kind {
            ProjectionJobKind::Incremental => validate_run(
                runtime
                    .run_once()
                    .await
                    .map_err(crate::map_cqrs_projection_error)?,
            ),
            ProjectionJobKind::Rebuild => runtime.rebuild(page_size).await,
        }
    }
}

#[derive(Debug, Clone)]
enum SnapshotRuntime {
    Marketing(MarketingProjectionRuntime, MarketingProjectionKind),
    Crm(CrmProjectionRuntime, CrmProjectionKind),
    Procurement(ProcurementProjectionRuntime),
    Inventory(InventoryProjectionRuntime),
    Assets(AssetsProjectionRuntime),
    Maintenance(MaintenanceProjectionRuntime),
}

impl SnapshotRuntime {
    async fn run_once(&self) -> Result<ProjectionRun, erpx_cqrs_sqlx::CqrsSqlxError> {
        match self {
            Self::Marketing(runtime, kind) => runtime.run_once(*kind).await,
            Self::Crm(runtime, kind) => runtime.run_once(*kind).await,
            Self::Procurement(runtime) => runtime.run_once().await,
            Self::Inventory(runtime) => runtime.run_once().await,
            Self::Assets(runtime) => runtime.run_once().await,
            Self::Maintenance(runtime) => runtime.run_once().await,
        }
    }

    async fn rebuild(&self, page_size: u32) -> Result<(), TenantProjectionExecutorError> {
        macro_rules! rebuild_single {
            ($runtime:expr) => {{
                let runtime = $runtime;
                let handle = runtime
                    .begin_rebuild()
                    .await
                    .map_err(crate::map_cqrs_projection_error)?;
                let mut cursor = 0;
                loop {
                    let run = runtime
                        .run_rebuild_page(&handle, cursor)
                        .await
                        .map_err(crate::map_cqrs_projection_error)?;
                    cursor = run.cursor;
                    let complete = run.fetched < page_size;
                    validate_run(run)?;
                    if complete {
                        break;
                    }
                }
                runtime
                    .activate_rebuild(&handle)
                    .await
                    .map_err(crate::map_cqrs_projection_error)
            }};
        }
        match self {
            Self::Crm(runtime, kind) => {
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
                    cursor = run.cursor;
                    let complete = run.fetched < page_size;
                    validate_run(run)?;
                    if complete {
                        break;
                    }
                }
                runtime
                    .activate_rebuild(*kind, &handle)
                    .await
                    .map_err(crate::map_cqrs_projection_error)
            }
            Self::Marketing(runtime, kind) => {
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
                    cursor = run.cursor;
                    let complete = run.fetched < page_size;
                    validate_run(run)?;
                    if complete {
                        break;
                    }
                }
                runtime
                    .activate_rebuild(*kind, &handle)
                    .await
                    .map_err(crate::map_cqrs_projection_error)
            }
            Self::Procurement(runtime) => rebuild_single!(runtime),
            Self::Inventory(runtime) => rebuild_single!(runtime),
            Self::Assets(runtime) => rebuild_single!(runtime),
            Self::Maintenance(runtime) => rebuild_single!(runtime),
        }
    }
}

fn validate_run(run: ProjectionRun) -> Result<(), TenantProjectionExecutorError> {
    if run.retry_scheduled || run.dead_lettered != 0 {
        return Err(TenantProjectionExecutorError::Retryable);
    }
    Ok(())
}

fn map_fact_error(error: MaintenanceAssetFactRuntimeError) -> TenantProjectionExecutorError {
    match error {
        MaintenanceAssetFactRuntimeError::Source(error) => {
            tracing::warn!(error = %error, "Assets fact source poll will be retried");
            TenantProjectionExecutorError::Retryable
        }
        MaintenanceAssetFactRuntimeError::Inbox(MaintenancePortError::Unavailable)
        | MaintenanceAssetFactRuntimeError::Inbox(MaintenancePortError::AssetFactNotReady)
        | MaintenanceAssetFactRuntimeError::Inbox(MaintenancePortError::NotReady(_)) => {
            TenantProjectionExecutorError::Retryable
        }
        MaintenanceAssetFactRuntimeError::Command(error)
            if matches!(
                error.category(),
                erpx_kernel::ErrorCategory::NotReady | erpx_kernel::ErrorCategory::Unavailable
            ) =>
        {
            TenantProjectionExecutorError::Retryable
        }
        MaintenanceAssetFactRuntimeError::Command(_) => TenantProjectionExecutorError::Permanent,
        MaintenanceAssetFactRuntimeError::Inbox(_)
        | MaintenanceAssetFactRuntimeError::InvalidContract(_) => {
            TenantProjectionExecutorError::Permanent
        }
    }
}
