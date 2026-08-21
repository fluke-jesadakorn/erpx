//! Holding worker composition for the Wave 6 paired bounded contexts.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionPool, ProjectionRun, ProjectionRunnerConfig, WritePool};
use erpx_infrastructure::{
    HoldingDatabaseRouter, ProjectionClaimRecord, ProjectionJobKind, RoutedDatabaseSide,
};
use erpx_maintenance::PortError as MaintenancePortError;
use erpx_worker::{HoldingProjectionExecutor, HoldingProjectionExecutorError};

use crate::contexts::{
    assets::{AssetsProjectionFactory, AssetsProjectionKind, AssetsProjectionRuntime},
    crm::{CrmProjectionFactory, CrmProjectionKind, CrmProjectionRuntime},
    inventory::{InventoryProjectionFactory, InventoryProjectionKind, InventoryProjectionRuntime},
    maintenance::{
        MaintenanceAssetFactRuntime, MaintenanceAssetFactRuntimeError,
        MaintenanceProjectionFactory, MaintenanceProjectionKind, MaintenanceProjectionRuntime,
    },
    marketing::{MarketingProjectionFactory, MarketingProjectionKind, MarketingProjectionRuntime},
    procurement::{
        ProcurementProjectionFactory, ProcurementProjectionKind, ProcurementProjectionRuntime,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wave6ProjectionKind {
    MarketingLeadCandidates,
    MarketingCampaignBudgets,
    MarketingBusinessRecords,
    CrmLeads,
    CrmOpportunities,
    CrmBusinessRecords,
    ProcurementPurchaseOrders,
    ProcurementWorkflowRecords,
    ProcurementBusinessRecords,
    InventoryStockItems,
    InventoryLifecycleRecords,
    Assets,
    AssetLifecycleRecords,
    MaintenanceWorkOrders,
    MaintenanceLifecycleRecords,
    MaintenanceAssetFacts,
}

#[derive(Debug, Clone)]
pub struct Wave6ProjectionExecutor {
    router: HoldingDatabaseRouter,
    kind: Wave6ProjectionKind,
    fact_runtimes: Arc<Mutex<BTreeMap<(String, String), MaintenanceAssetFactRuntime>>>,
}

impl Wave6ProjectionExecutor {
    pub fn new(router: HoldingDatabaseRouter, kind: Wave6ProjectionKind) -> Self {
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
            Wave6ProjectionKind::MarketingBusinessRecords => {
                erpx_marketing_sqlx::RECORD_PROJECTION_NAME
            }
            Wave6ProjectionKind::CrmLeads => erpx_crm_sqlx::LEAD_PROJECTION,
            Wave6ProjectionKind::CrmOpportunities => erpx_crm_sqlx::OPPORTUNITY_PROJECTION,
            Wave6ProjectionKind::CrmBusinessRecords => erpx_crm_sqlx::RECORD_PROJECTION_NAME,
            Wave6ProjectionKind::ProcurementPurchaseOrders => {
                erpx_procurement_sqlx::PROJECTION_NAME
            }
            Wave6ProjectionKind::ProcurementWorkflowRecords => {
                erpx_procurement_sqlx::WORKFLOW_PROJECTION_NAME
            }
            Wave6ProjectionKind::ProcurementBusinessRecords => {
                erpx_procurement_sqlx::RECORD_PROJECTION_NAME
            }
            Wave6ProjectionKind::InventoryStockItems => erpx_inventory_sqlx::PROJECTION_NAME,
            Wave6ProjectionKind::InventoryLifecycleRecords => {
                erpx_inventory_sqlx::LIFECYCLE_RECORD_PROJECTION_NAME
            }
            Wave6ProjectionKind::Assets => erpx_assets_sqlx::PROJECTION_NAME,
            Wave6ProjectionKind::AssetLifecycleRecords => {
                erpx_assets_sqlx::LIFECYCLE_RECORD_PROJECTION_NAME
            }
            Wave6ProjectionKind::MaintenanceWorkOrders => erpx_maintenance_sqlx::PROJECTION_NAME,
            Wave6ProjectionKind::MaintenanceLifecycleRecords => {
                erpx_maintenance_sqlx::LIFECYCLE_RECORD_PROJECTION_NAME
            }
            Wave6ProjectionKind::MaintenanceAssetFacts => {
                erpx_maintenance_sqlx::ASSET_FACT_PROJECTION_NAME
            }
        }
    }

    async fn route_pools(
        &self,
        holding_id: &str,
        data_profile_key: &str,
    ) -> Result<(sqlx::PgPool, sqlx::PgPool), HoldingProjectionExecutorError> {
        let write = self
            .router
            .route(holding_id, data_profile_key, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
        let read = self
            .router
            .route(holding_id, data_profile_key, RoutedDatabaseSide::Read)
            .await
            .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
        Ok((write.database_pool().clone(), read.database_pool().clone()))
    }

    async fn snapshot_runtime(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        page_size: u32,
    ) -> Result<SnapshotRuntime, HoldingProjectionExecutorError> {
        let (write, read) = self.route_pools(holding_id, data_profile_key).await?;
        let holding_id = crate::cqrs_holding_uuid(holding_id)?;
        let write =
            WritePool::for_holding(write, holding_id).map_err(crate::map_cqrs_projection_error)?;
        let read = ProjectionPool::for_holding(read, holding_id)
            .map_err(crate::map_cqrs_projection_error)?;
        let config = ProjectionRunnerConfig {
            page_size,
            ..ProjectionRunnerConfig::default()
        };
        let runtime = match self.kind {
            Wave6ProjectionKind::MarketingLeadCandidates
            | Wave6ProjectionKind::MarketingCampaignBudgets
            | Wave6ProjectionKind::MarketingBusinessRecords => {
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
                    Wave6ProjectionKind::MarketingBusinessRecords => {
                        MarketingProjectionKind::BusinessRecords
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
            Wave6ProjectionKind::CrmBusinessRecords => {
                let runtime = CrmProjectionFactory
                    .build(write, read, config)
                    .map_err(crate::map_cqrs_projection_error)?;
                debug_assert_eq!(
                    runtime.registrations(),
                    CrmProjectionFactory.registrations()
                );
                SnapshotRuntime::Crm(runtime, CrmProjectionKind::BusinessRecords)
            }
            Wave6ProjectionKind::ProcurementPurchaseOrders => {
                let runtime = ProcurementProjectionFactory
                    .build(write, read, config)
                    .map_err(crate::map_cqrs_projection_error)?;
                debug_assert_eq!(
                    runtime.registrations(),
                    ProcurementProjectionFactory.registrations()
                );
                SnapshotRuntime::Procurement(runtime, ProcurementProjectionKind::PurchaseOrders)
            }
            Wave6ProjectionKind::ProcurementWorkflowRecords => {
                let runtime = ProcurementProjectionFactory
                    .build(write, read, config)
                    .map_err(crate::map_cqrs_projection_error)?;
                debug_assert_eq!(
                    runtime.registrations(),
                    ProcurementProjectionFactory.registrations()
                );
                SnapshotRuntime::Procurement(runtime, ProcurementProjectionKind::WorkflowRecords)
            }
            Wave6ProjectionKind::ProcurementBusinessRecords => {
                let runtime = ProcurementProjectionFactory
                    .build(write, read, config)
                    .map_err(crate::map_cqrs_projection_error)?;
                debug_assert_eq!(
                    runtime.registrations(),
                    ProcurementProjectionFactory.registrations()
                );
                SnapshotRuntime::Procurement(runtime, ProcurementProjectionKind::BusinessRecords)
            }
            Wave6ProjectionKind::InventoryStockItems
            | Wave6ProjectionKind::InventoryLifecycleRecords => {
                let runtime = InventoryProjectionFactory
                    .build(write, read, config)
                    .map_err(crate::map_cqrs_projection_error)?;
                debug_assert_eq!(
                    runtime.registrations(),
                    InventoryProjectionFactory.registrations()
                );
                let kind = match self.kind {
                    Wave6ProjectionKind::InventoryStockItems => InventoryProjectionKind::StockItems,
                    Wave6ProjectionKind::InventoryLifecycleRecords => {
                        InventoryProjectionKind::LifecycleRecords
                    }
                    _ => unreachable!("matched Inventory projection kind"),
                };
                SnapshotRuntime::Inventory(runtime, kind)
            }
            Wave6ProjectionKind::Assets | Wave6ProjectionKind::AssetLifecycleRecords => {
                let runtime = AssetsProjectionFactory
                    .build(write, read, config)
                    .map_err(crate::map_cqrs_projection_error)?;
                debug_assert_eq!(
                    runtime.registrations(),
                    AssetsProjectionFactory.registrations()
                );
                let kind = match self.kind {
                    Wave6ProjectionKind::Assets => AssetsProjectionKind::Assets,
                    Wave6ProjectionKind::AssetLifecycleRecords => {
                        AssetsProjectionKind::LifecycleRecords
                    }
                    _ => unreachable!("matched Assets projection kind"),
                };
                SnapshotRuntime::Assets(runtime, kind)
            }
            Wave6ProjectionKind::MaintenanceWorkOrders
            | Wave6ProjectionKind::MaintenanceLifecycleRecords => {
                let runtime = MaintenanceProjectionFactory
                    .build(write, read, config)
                    .map_err(crate::map_cqrs_projection_error)?;
                debug_assert_eq!(
                    runtime.registrations(),
                    MaintenanceProjectionFactory.registrations()
                );
                let kind = match self.kind {
                    Wave6ProjectionKind::MaintenanceWorkOrders => {
                        MaintenanceProjectionKind::WorkOrders
                    }
                    Wave6ProjectionKind::MaintenanceLifecycleRecords => {
                        MaintenanceProjectionKind::LifecycleRecords
                    }
                    _ => unreachable!("matched Maintenance projection kind"),
                };
                SnapshotRuntime::Maintenance(runtime, kind)
            }
            Wave6ProjectionKind::MaintenanceAssetFacts => {
                return Err(HoldingProjectionExecutorError::Permanent);
            }
        };
        Ok(runtime)
    }

    async fn facts_runtime(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        page_size: u32,
        reset: bool,
    ) -> Result<MaintenanceAssetFactRuntime, HoldingProjectionExecutorError> {
        let key = (holding_id.to_owned(), data_profile_key.to_owned());
        if !reset
            && let Some(runtime) = self
                .fact_runtimes
                .lock()
                .map_err(|_| HoldingProjectionExecutorError::Retryable)?
                .get(&key)
                .cloned()
        {
            return Ok(runtime);
        }
        let write = self
            .router
            .route(holding_id, data_profile_key, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
        let raw = write.database_pool().clone();
        let holding_uuid = crate::cqrs_holding_uuid(holding_id)?;
        let runtime = MaintenanceAssetFactRuntime::with_source(
            WritePool::for_holding(raw.clone(), holding_uuid)
                .map_err(crate::map_cqrs_projection_error)?,
            raw,
            holding_uuid,
            page_size,
        )
        .map_err(map_fact_error)?;
        debug_assert_eq!(
            runtime.registration().event_types,
            ["assets.condition_changed", "assets.maintenance_requested"]
        );
        self.fact_runtimes
            .lock()
            .map_err(|_| HoldingProjectionExecutorError::Retryable)?
            .insert(key, runtime.clone());
        Ok(runtime)
    }

    async fn run_facts(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        page_size: u32,
        rebuild: bool,
    ) -> Result<(), HoldingProjectionExecutorError> {
        let runtime = self
            .facts_runtime(holding_id, data_profile_key, page_size, rebuild)
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
        let (write, read) = self.route_pools(holding_id, data_profile_key).await?;
        let holding_id = crate::cqrs_holding_uuid(holding_id)?;
        let work_orders = MaintenanceProjectionFactory
            .build(
                WritePool::for_holding(write, holding_id)
                    .map_err(crate::map_cqrs_projection_error)?,
                ProjectionPool::for_holding(read, holding_id)
                    .map_err(crate::map_cqrs_projection_error)?,
                ProjectionRunnerConfig {
                    page_size,
                    ..ProjectionRunnerConfig::default()
                },
            )
            .map_err(crate::map_cqrs_projection_error)?;
        loop {
            let run = work_orders
                .run_once(MaintenanceProjectionKind::WorkOrders)
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
impl HoldingProjectionExecutor for Wave6ProjectionExecutor {
    fn projection_name_filter(&self) -> Option<&str> {
        Some(self.projection_name())
    }

    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), HoldingProjectionExecutorError> {
        if claim.job.payload.get("after").is_some()
            || claim.job.payload.get("cursor").is_some()
            || claim.job.payload.get("candidate_database_ref").is_some()
            || claim.job.payload.get("source_version").is_some()
            || claim.job.payload.get("generation").is_some()
        {
            return Err(HoldingProjectionExecutorError::Permanent);
        }
        let holding_id = &claim.job.binding.holding_id;
        let data_profile_key = &claim.job.binding.data_profile_key;
        let page_size = crate::projection_page_size(&claim.job.payload, 500)?;
        if self.kind == Wave6ProjectionKind::MaintenanceAssetFacts {
            return self
                .run_facts(
                    holding_id,
                    data_profile_key,
                    page_size,
                    claim.job.kind == ProjectionJobKind::Rebuild,
                )
                .await;
        }
        let runtime = self
            .snapshot_runtime(holding_id, data_profile_key, page_size)
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
    Procurement(ProcurementProjectionRuntime, ProcurementProjectionKind),
    Inventory(InventoryProjectionRuntime, InventoryProjectionKind),
    Assets(AssetsProjectionRuntime, AssetsProjectionKind),
    Maintenance(MaintenanceProjectionRuntime, MaintenanceProjectionKind),
}

impl SnapshotRuntime {
    async fn run_once(&self) -> Result<ProjectionRun, erpx_cqrs_sqlx::CqrsSqlxError> {
        match self {
            Self::Marketing(runtime, kind) => runtime.run_once(*kind).await,
            Self::Crm(runtime, kind) => runtime.run_once(*kind).await,
            Self::Procurement(runtime, kind) => runtime.run_once(*kind).await,
            Self::Inventory(runtime, kind) => runtime.run_once(*kind).await,
            Self::Assets(runtime, kind) => runtime.run_once(*kind).await,
            Self::Maintenance(runtime, kind) => runtime.run_once(*kind).await,
        }
    }

    async fn rebuild(&self, page_size: u32) -> Result<(), HoldingProjectionExecutorError> {
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
            Self::Procurement(runtime, kind) => {
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
            Self::Inventory(runtime, kind) => {
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
            Self::Assets(runtime, kind) => {
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
            Self::Maintenance(runtime, kind) => {
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
        }
    }
}

fn validate_run(run: ProjectionRun) -> Result<(), HoldingProjectionExecutorError> {
    if run.retry_scheduled || run.dead_lettered != 0 {
        return Err(HoldingProjectionExecutorError::Retryable);
    }
    Ok(())
}

fn map_fact_error(error: MaintenanceAssetFactRuntimeError) -> HoldingProjectionExecutorError {
    match error {
        MaintenanceAssetFactRuntimeError::Source(error) => {
            tracing::warn!(error = %error, "Assets fact source poll will be retried");
            HoldingProjectionExecutorError::Retryable
        }
        MaintenanceAssetFactRuntimeError::Inbox(MaintenancePortError::Unavailable)
        | MaintenanceAssetFactRuntimeError::Inbox(MaintenancePortError::AssetFactNotReady)
        | MaintenanceAssetFactRuntimeError::Inbox(MaintenancePortError::NotReady(_)) => {
            HoldingProjectionExecutorError::Retryable
        }
        MaintenanceAssetFactRuntimeError::Command(error)
            if matches!(
                error.category(),
                erpx_kernel::ErrorCategory::NotReady | erpx_kernel::ErrorCategory::Unavailable
            ) =>
        {
            HoldingProjectionExecutorError::Retryable
        }
        MaintenanceAssetFactRuntimeError::Command(_) => HoldingProjectionExecutorError::Permanent,
        MaintenanceAssetFactRuntimeError::Inbox(_)
        | MaintenanceAssetFactRuntimeError::InvalidContract(_) => {
            HoldingProjectionExecutorError::Permanent
        }
    }
}
