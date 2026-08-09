pub(crate) mod contexts {
    pub mod assets;
    pub mod brand;
    pub mod commercial;
    pub mod crm;
    pub mod customer_360;
    pub mod customer_authority;
    pub mod finance;
    pub mod identity_access;
    pub mod inventory;
    pub mod legal;
    pub mod maintenance;
    pub mod marketing;
    pub mod operations;
    pub mod operations_source;
    pub mod organization;
    pub mod people_payroll;
    pub mod platform_administration;
    pub mod pos;
    pub mod procurement;
    pub mod quality;
    pub mod sales;
    pub mod support;
}
mod wave4_composition;
mod wave5_composition;
mod wave6_composition;
mod wave7_composition;
mod wave8_composition;
mod wave9_composition;

use std::{env, sync::Arc};

use chrono::{Duration, Utc};
use erpx_cqrs_sqlx::{ProjectionPool, ProjectionRunnerConfig, WritePool};
use erpx_domain::{access::Scope, contracts::TenantScope};
use erpx_identity::{DirectoryRoleAccess, SqlxIdentityStore};
use erpx_infrastructure::{
    AssetMaintenanceSourceEventCursor, MarketingDeliveryScheduleRequest, ProjectionClaimRecord,
    ProjectionJobKind, ProjectionSchedulerAuthorization, RoleReviewPersistence, RoutedDatabaseSide,
    SalesSourceEventCursor, SqlxControlPlane, SqlxMarketingContentStore,
    SqlxMarketingDeliverySchedulerRepository, SqlxMarketingDeliveryStore,
    SqlxMarketingJourneyExecutionStore, SqlxOperationsStore, SqlxTenantStoreRegistry,
    SupportWorkflowStore, TenantDatabaseProvisioningConfig, TenantDatabaseRouter,
    UnconfiguredBankProviderCredentialResolver,
};
use erpx_worker::{
    AssetMaintenanceProjectionRequest, AssetMaintenanceProjectionRunner,
    BRANCH_PROVISIONING_HANDLER_NAME, BackupRuntime, BankProviderAdapterRegistry,
    BranchProvisioningRuntimeHandler, CLUSTER_PITR_CONTROLLER_HANDLER_NAME,
    ClusterPitrControllerRuntimeHandler, DIRECTORY_SUPERVISOR_HANDLER_NAME,
    DIRECTORY_SYNC_HANDLER_NAME, DirectorySupervisorRuntimeHandler, DirectorySyncRuntimeHandler,
    FINANCE_BANK_PROVIDER_IMPORT_HANDLER_NAME, FINANCE_SUBLEDGER_RECONCILIATION_MAX_ROWS,
    FinanceBankProviderImportAuthorizer, FinanceBankProviderImportError,
    FinanceBankingTenantProjectionExecutor, FinanceChartTenantProjectionExecutor,
    FinanceCloseTenantProjectionExecutor, FinanceCompletionAuthorizationProof,
    FinanceCompletionRequest, FinanceCompletionRuntimeAdapter,
    FinanceSubledgerReconciliationRequest, FinanceSubledgerReconciliationRunner,
    FinanceSubledgerTenantProjectionExecutor, FinanceThaiTaxTenantProjectionExecutor,
    G13_TARGET_HANDLER_NAME, G13TargetRuntimeHandler, HR_COMPLETION_HANDLER_NAME,
    HrCompletionRuntimeHandler, HrProjectionRequest, HrProjectionRunner, HrProjectionWorkerError,
    LEGAL_RETENTION_DEFAULT_BATCH_SIZE, LEGAL_RETENTION_HANDLER_NAME, LegalProjectionRequest,
    LegalProjectionRunner, LegalProjectionWorkerError, LegalRetentionRequest, LegalRetentionRunner,
    LegalRetentionWorkerContext, MARKETING_CONTENT_PREVIEW_EXPIRY_HANDLER_NAME,
    MARKETING_CONTENT_PREVIEW_EXPIRY_MAX_BATCH_SIZE, MARKETING_CONTENT_PRIVACY_DEFAULT_BATCH_SIZE,
    MARKETING_CONTENT_PRIVACY_HANDLER_NAME, MARKETING_DELIVERY_HANDLER_NAME,
    MARKETING_JOURNEY_EXECUTION_HANDLER_NAME, MARKETING_JOURNEY_EXECUTION_MAX_BATCH_SIZE,
    MARKETING_JOURNEY_INTEGRATION_HANDLER_NAME, MARKETING_PUBLIC_ABUSE_CLEANUP_DEFAULT_BATCH_SIZE,
    MARKETING_PUBLIC_ABUSE_CLEANUP_HANDLER_NAME, MARKETING_PUBLIC_ABUSE_CLEANUP_MAX_BATCH_SIZE,
    MarketingContentPreviewExpiryHandler, MarketingContentPreviewExpiryRequest,
    MarketingContentPrivacyRequest, MarketingContentPrivacyRunner,
    MarketingContentPrivacyWorkerContext, MarketingDeliveryHandler,
    MarketingDeliverySchedulerHandler, MarketingDeliverySecretManagerResolver,
    MarketingJourneyExecutionHandler, MarketingJourneyIntegrationMultiTenantRuntimeHandler,
    MarketingPublicAbuseCleanupRunner, MarketingTenantProjectionExecutor, NamedWorkerHandler,
    OPERATIONS_COMPLETION_HANDLER_NAME, OPERATIONS_SLA_NOTIFICATION_HANDLER_NAME,
    OperationsCompletionPass, OperationsCompletionWorker, OperationsOutboxSlaNotificationPublisher,
    OperationsSlaNotificationHandler, OperationsTenantProjectionExecutor, PosProjectionRequest,
    PosProjectionRunner, PosProjectionWorkerError, SUPPORT_WORKFLOW_HANDLER_NAME,
    SalesProjectionRequest, SalesProjectionRunner, ScmProjectionPageRequest,
    ScmProjectionRebuildRequest, ScmProjectionRunner, ScmProjectionWorkerError,
    SqlxMarketingContentPreviewExpiryExecutor, SqlxMarketingJourneyExecutionDecisionSource,
    SqlxMarketingProjectionFactory, TENANT_BACKUP_HANDLER_NAME,
    TENANT_BACKUP_RETENTION_HANDLER_NAME, TENANT_PROJECTION_SCHEDULER_HANDLER_NAME,
    TENANT_PROJECTION_STATUS_PROJECTION_HANDLER_NAME, TenantBackupJobRuntimeHandler,
    TenantBackupRetentionRuntimeHandler, TenantProjectionExecutor, TenantProjectionExecutorError,
    WorkerHandlerReport, WorkerRuntime, WorkerRuntimeConfig, WorkerRuntimeError,
};
use erpx_worker::{
    ROLE_REVIEW_NOTIFICATION_HANDLER_NAME, ROLE_REVIEW_SNAPSHOT_HANDLER_NAME,
    RoleReviewNotificationPublishError, RoleReviewNotificationPublisher,
    RoleReviewSnapshotRuntimeHandler,
};
use uuid::Uuid;

use contexts::commercial::CommercialProjectionKind;
use contexts::operations_source::{
    OPERATIONS_SOURCE_INGESTION_HANDLER_NAME, OPERATIONS_SOURCE_MAX_ACTIVE_TENANTS,
    OPERATIONS_SOURCE_MAX_PAGE_SIZE,
};
use contexts::quality::QualityProjectionFactory;
use wave4_composition::{
    COMMERCIAL_CATALOG_PROJECTION_HANDLER_NAME, CommercialCatalogProjectionHandler,
    IdentityAccessProjectionKind, Wave4ProjectionExecutor,
};
use wave5_composition::{WAVE5_CONTROL_PROJECTION_HANDLER_NAME, Wave5ControlProjectionHandler};
use wave6_composition::{Wave6ProjectionExecutor, Wave6ProjectionKind};
use wave7_composition::{Wave7ProjectionExecutor, Wave7ProjectionKind};
use wave8_composition::{Wave8ProjectionExecutor, Wave8ProjectionKind};
use wave9_composition::{
    CUSTOMER360_CONTROL_PROJECTION_HANDLER_NAME, CUSTOMER360_TENANT_PROJECTION_HANDLER_NAME,
    Customer360ProjectionHandler, Customer360ProjectionKind, Wave9ProjectionExecutor,
    Wave9ProjectionKind, operations_source_ingestion_handler,
};

const FINANCE_COMPLETION_HANDLER_NAME: &str = "finance_completion";

mod g13_migration_runtime;
use g13_migration_runtime::{G13_MIGRATION_HANDLER_NAME, G13MigrationRuntimeHandler};

#[derive(Debug, Clone)]
struct ControlOutboxRoleReviewPublisher {
    persistence: RoleReviewPersistence,
    actor_ref: String,
}

#[derive(Clone)]
struct ControlPlaneFinanceBankProviderImportAuthorizer {
    control: Arc<SqlxControlPlane>,
    identity: Arc<SqlxIdentityStore>,
}

#[async_trait::async_trait]
impl FinanceBankProviderImportAuthorizer for ControlPlaneFinanceBankProviderImportAuthorizer {
    async fn authorize(
        &self,
        context: &erpx_domain::contracts::CommandContext,
        account_scope: &TenantScope,
    ) -> Result<(), FinanceBankProviderImportError> {
        if !context.tenant_scope.contains(account_scope) {
            return Err(FinanceBankProviderImportError::AuthorizationDenied);
        }
        let requested_scope = finance_access_scope(account_scope);
        let directory_scope = requested_scope.canonical_key();
        let holding_scope = match requested_scope {
            Scope::Platform => None,
            Scope::Holding { holding_id }
            | Scope::Company { holding_id, .. }
            | Scope::Branch { holding_id, .. }
            | Scope::Subtree { holding_id, .. } => Some(format!("holding:{holding_id}")),
        };
        match self
            .identity
            .directory_role_access(
                &context.actor.principal_ref,
                "finance.write",
                &directory_scope,
                holding_scope.as_deref(),
                Utc::now(),
            )
            .await
            .map_err(|_| FinanceBankProviderImportError::AuthorizationDenied)?
        {
            DirectoryRoleAccess::Allowed => Ok(()),
            DirectoryRoleAccess::Denied => Err(FinanceBankProviderImportError::AuthorizationDenied),
            DirectoryRoleAccess::Unmanaged => {
                let allowed = self
                    .control
                    .principal_has_role(
                        &context.actor.principal_ref,
                        &requested_scope,
                        "finance.write",
                        Utc::now(),
                    )
                    .await
                    .map_err(|_| FinanceBankProviderImportError::AuthorizationDenied)?;
                allowed
                    .then_some(())
                    .ok_or(FinanceBankProviderImportError::AuthorizationDenied)
            }
        }
    }
}

fn finance_access_scope(scope: &TenantScope) -> Scope {
    match scope {
        TenantScope::Platform => Scope::Platform,
        TenantScope::Holding { holding_id } => Scope::Holding {
            holding_id: *holding_id,
        },
        TenantScope::Company {
            holding_id,
            company_id,
        } => Scope::Company {
            holding_id: *holding_id,
            company_id: *company_id,
        },
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Scope::Branch {
            holding_id: *holding_id,
            company_id: *company_id,
            branch_id: *branch_id,
        },
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => Scope::Subtree {
            holding_id: *holding_id,
            company_ids: company_ids.clone(),
            branch_ids: branch_ids.clone(),
        },
    }
}

#[async_trait::async_trait]
impl RoleReviewNotificationPublisher for ControlOutboxRoleReviewPublisher {
    async fn publish(
        &self,
        notification: &erpx_infrastructure::ClaimedRoleExpiryNotification,
    ) -> Result<(), RoleReviewNotificationPublishError> {
        self.persistence
            .append_expiry_notification_event(notification, &self.actor_ref)
            .await
            .map(|_| ())
            .map_err(|_| RoleReviewNotificationPublishError::Unavailable)
    }
}

fn role_review_scope_from_env() -> Result<Scope, Box<dyn std::error::Error>> {
    let value = required_env("ERPX_ROLE_REVIEW_SCOPE")?;
    let parts = value.split(':').collect::<Vec<_>>();
    let parse_id = |field: &str| {
        Uuid::parse_str(field).map_err(|_| format!("invalid role-review scope UUID: {field}"))
    };
    match parts.as_slice() {
        ["platform"] => Ok(Scope::Platform),
        ["holding", holding] => Ok(Scope::Holding {
            holding_id: parse_id(holding)?,
        }),
        ["company", holding, company] => Ok(Scope::Company {
            holding_id: parse_id(holding)?,
            company_id: parse_id(company)?,
        }),
        ["branch", holding, company, branch] => Ok(Scope::Branch {
            holding_id: parse_id(holding)?,
            company_id: parse_id(company)?,
            branch_id: parse_id(branch)?,
        }),
        _ => Err("ERPX_ROLE_REVIEW_SCOPE must be platform, holding:<uuid>, company:<uuid>:<uuid>, or branch:<uuid>:<uuid>:<uuid>".into()),
    }
}

#[derive(Debug, Clone, Copy)]
enum FinanceCompletionMode {
    Reconcile,
    RollbackClose,
}

#[derive(Clone)]
struct FinanceCompletionRuntimeHandler {
    adapter: FinanceCompletionRuntimeAdapter,
    request: FinanceCompletionRequest,
    proof: FinanceCompletionAuthorizationProof,
    mode: FinanceCompletionMode,
    source_version: Option<u64>,
}

impl FinanceCompletionRuntimeHandler {
    fn from_env(router: TenantDatabaseRouter) -> Result<Self, Box<dyn std::error::Error>> {
        let tenant_id = required_env("ERPX_FINANCE_COMPLETION_TENANT_ID")?;
        let holding_ref = required_env("ERPX_FINANCE_COMPLETION_HOLDING_REF")?;
        let reconciliation_max_rows = env::var("ERPX_FINANCE_COMPLETION_MAX_ROWS")
            .unwrap_or_else(|_| "1000".into())
            .parse::<u32>()
            .map_err(|_| "ERPX_FINANCE_COMPLETION_MAX_ROWS must be an integer")?;
        let page_size = env::var("ERPX_FINANCE_COMPLETION_PAGE_SIZE")
            .unwrap_or_else(|_| "500".into())
            .parse::<u32>()
            .map_err(|_| "ERPX_FINANCE_COMPLETION_PAGE_SIZE must be an integer")?;
        let request = FinanceCompletionRequest::new(
            tenant_id,
            holding_ref,
            reconciliation_max_rows,
            page_size,
        )
        .map_err(|_| "Finance completion request is invalid")?;
        let proof_json = required_env("ERPX_FINANCE_COMPLETION_PROOF_JSON")?;
        let proof = serde_json::from_str(&proof_json)
            .map_err(|_| "ERPX_FINANCE_COMPLETION_PROOF_JSON is invalid")?;
        let mode = match env::var("ERPX_FINANCE_COMPLETION_MODE")
            .unwrap_or_else(|_| "reconcile".into())
            .as_str()
        {
            "reconcile" => FinanceCompletionMode::Reconcile,
            "rollback_close" => FinanceCompletionMode::RollbackClose,
            _ => return Err("ERPX_FINANCE_COMPLETION_MODE is invalid".into()),
        };
        let source_version = env::var("ERPX_FINANCE_COMPLETION_SOURCE_VERSION")
            .ok()
            .map(|value| {
                value
                    .parse::<u64>()
                    .map_err(|_| "ERPX_FINANCE_COMPLETION_SOURCE_VERSION must be an integer")
            })
            .transpose()?;
        Ok(Self {
            adapter: FinanceCompletionRuntimeAdapter::new(router),
            request,
            proof,
            mode,
            source_version,
        })
    }
}

#[async_trait::async_trait]
impl NamedWorkerHandler for FinanceCompletionRuntimeHandler {
    fn name(&self) -> &str {
        FINANCE_COMPLETION_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "Finance completion worker identity is required".into(),
            ));
        }
        let now = Utc::now();
        match self.mode {
            FinanceCompletionMode::Reconcile => self
                .adapter
                .reconcile(self.request.clone(), &self.proof, now)
                .await
                .map(|report| WorkerHandlerReport {
                    claimed: report
                        .source_aggregate_count
                        .saturating_add(report.close_events_seen),
                    processed: report.close_events_applied,
                    has_work: !report.healthy,
                    ..WorkerHandlerReport::default()
                }),
            FinanceCompletionMode::RollbackClose => self
                .adapter
                .rollback_close_projection(
                    self.request.clone(),
                    &self.proof,
                    self.source_version,
                    now,
                )
                .await
                .map(|report| WorkerHandlerReport {
                    claimed: report.close_events_replayed,
                    processed: report.close_events_replayed,
                    has_work: false,
                    ..WorkerHandlerReport::default()
                }),
        }
        .map_err(|_| WorkerRuntimeError::Configuration("Finance completion runtime failed".into()))
    }
}

#[derive(Debug, Clone)]
enum ProductProjectionExecutor {
    AssetMaintenance(AssetMaintenanceProjectionRunner),
    QualityCqrs(QualityCqrsProjectionExecutor),
    Wave4(Wave4ProjectionExecutor),
    Wave6(Wave6ProjectionExecutor),
    Wave7(Wave7ProjectionExecutor),
    Wave8(Wave8ProjectionExecutor),
    Wave9(Wave9ProjectionExecutor),
    Legal(LegalProjectionRunner),
    Scm(ScmProjectionRunner),
    Pos(PosProjectionRunner),
    Hr(HrProjectionRunner),
    Sales(SalesProjectionRunner),
}

#[derive(Debug, Clone)]
struct QualityCqrsProjectionExecutor {
    router: TenantDatabaseRouter,
}

impl QualityCqrsProjectionExecutor {
    const fn new(router: TenantDatabaseRouter) -> Self {
        Self { router }
    }

    async fn runtime(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        page_size: u32,
    ) -> Result<contexts::quality::QualityProjectionRuntime, TenantProjectionExecutorError> {
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
        let runtime = QualityProjectionFactory
            .build(
                WritePool::from_pg_pool(write.database_pool().clone()),
                ProjectionPool::from_pg_pool(read.database_pool().clone()),
                ProjectionRunnerConfig {
                    page_size,
                    ..ProjectionRunnerConfig::default()
                },
            )
            .map_err(map_cqrs_projection_error)?;
        debug_assert_eq!(
            runtime.registration(),
            QualityProjectionFactory.registration()
        );
        // The runtime owns cloned pool handles for this bounded pass.
        drop(write);
        drop(read);
        Ok(runtime)
    }

    async fn project(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        page_size: u32,
    ) -> Result<(), TenantProjectionExecutorError> {
        let run = self
            .runtime(tenant_id, holding_ref, page_size)
            .await?
            .run_once()
            .await
            .map_err(map_cqrs_projection_error)?;
        if run.retry_scheduled || run.dead_lettered != 0 {
            return Err(TenantProjectionExecutorError::Retryable);
        }
        Ok(())
    }

    async fn rebuild(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        page_size: u32,
    ) -> Result<(), TenantProjectionExecutorError> {
        let runtime = self.runtime(tenant_id, holding_ref, page_size).await?;
        let handle = runtime
            .begin_rebuild()
            .await
            .map_err(map_cqrs_projection_error)?;
        let mut cursor = 0;
        loop {
            let run = runtime
                .run_rebuild_page(&handle, cursor)
                .await
                .map_err(map_cqrs_projection_error)?;
            if run.retry_scheduled || run.dead_lettered != 0 {
                return Err(TenantProjectionExecutorError::Retryable);
            }
            cursor = run.cursor;
            if run.fetched < page_size {
                break;
            }
        }
        runtime
            .activate_rebuild(&handle)
            .await
            .map_err(map_cqrs_projection_error)
    }
}

#[async_trait::async_trait]
impl TenantProjectionExecutor for ProductProjectionExecutor {
    fn projection_name_filter(&self) -> Option<&str> {
        match self {
            Self::QualityCqrs(_) => Some(erpx_quality_sqlx::PROJECTION_NAME),
            Self::Wave4(executor) => executor.projection_name_filter(),
            Self::Wave6(executor) => executor.projection_name_filter(),
            Self::Wave7(executor) => executor.projection_name_filter(),
            Self::Wave8(executor) => executor.projection_name_filter(),
            Self::Wave9(executor) => executor.projection_name_filter(),
            _ => None,
        }
    }

    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), TenantProjectionExecutorError> {
        if self
            .projection_name_filter()
            .is_some_and(|expected| expected != claim.job.projection_name)
            || claim.job.payload.get("candidate_database_ref").is_some()
            || claim.job.payload.get("source_version").is_some()
            || claim.job.payload.get("generation").is_some()
        {
            return Err(TenantProjectionExecutorError::Permanent);
        }
        let tenant_id = claim.job.binding.tenant_id.clone();
        let holding_ref = claim.job.binding.holding_ref.clone();
        let page_size = projection_page_size(&claim.job.payload, 500)?;
        match (self, claim.job.kind) {
            (Self::AssetMaintenance(runner), ProjectionJobKind::Incremental) => runner
                .project_page(
                    AssetMaintenanceProjectionRequest::new(
                        tenant_id,
                        holding_ref,
                        projection_asset_maintenance_cursor(&claim.job.payload)?,
                        page_size,
                    )
                    .map_err(|_| TenantProjectionExecutorError::Permanent)?,
                )
                .await
                .map(|_| ())
                .map_err(map_worker_projection_error),
            (Self::AssetMaintenance(runner), ProjectionJobKind::Rebuild) => runner
                .rebuild(tenant_id, holding_ref, page_size)
                .await
                .map(|_| ())
                .map_err(map_worker_projection_error),
            (Self::QualityCqrs(runner), ProjectionJobKind::Incremental) => {
                if claim.job.payload.get("after").is_some()
                    || claim.job.payload.get("cursor").is_some()
                {
                    return Err(TenantProjectionExecutorError::Permanent);
                }
                runner.project(&tenant_id, &holding_ref, page_size).await
            }
            (Self::QualityCqrs(runner), ProjectionJobKind::Rebuild) => {
                runner.rebuild(&tenant_id, &holding_ref, page_size).await
            }
            (Self::Wave4(runner), _) => runner.execute(claim).await,
            (Self::Wave6(runner), _) => runner.execute(claim).await,
            (Self::Wave7(runner), _) => runner.execute(claim).await,
            (Self::Wave8(runner), _) => runner.execute(claim).await,
            (Self::Wave9(runner), _) => runner.execute(claim).await,
            (Self::Legal(runner), ProjectionJobKind::Incremental) => runner
                .project_page(
                    LegalProjectionRequest::new(
                        tenant_id,
                        holding_ref,
                        projection_legal_cursor(&claim.job.payload)?,
                        page_size,
                    )
                    .map_err(|_| TenantProjectionExecutorError::Permanent)?,
                )
                .await
                .map(|_| ())
                .map_err(map_legal_projection_error),
            (Self::Legal(runner), ProjectionJobKind::Rebuild) => runner
                .rebuild(tenant_id, holding_ref, page_size)
                .await
                .map(|_| ())
                .map_err(map_legal_projection_error),
            (Self::Scm(runner), ProjectionJobKind::Incremental) => runner
                .project_to_caught_up_and_reconcile(
                    ScmProjectionPageRequest::new(tenant_id, holding_ref, page_size)
                        .map_err(|_| TenantProjectionExecutorError::Permanent)?,
                )
                .await
                .map(|_| ())
                .map_err(map_scm_projection_error),
            (Self::Scm(runner), ProjectionJobKind::Rebuild) => runner
                .rebuild(
                    ScmProjectionRebuildRequest::new(tenant_id, holding_ref, page_size)
                        .map_err(|_| TenantProjectionExecutorError::Permanent)?,
                )
                .await
                .map(|_| ())
                .map_err(map_scm_projection_error),
            (Self::Pos(runner), ProjectionJobKind::Incremental) => runner
                .project_page(
                    PosProjectionRequest::new(
                        tenant_id,
                        holding_ref,
                        projection_pos_cursor(&claim.job.payload)?,
                        page_size,
                    )
                    .map_err(|_| TenantProjectionExecutorError::Permanent)?,
                )
                .await
                .map(|_| ())
                .map_err(map_pos_projection_error),
            (Self::Pos(runner), ProjectionJobKind::Rebuild) => runner
                .rebuild(tenant_id, holding_ref, page_size)
                .await
                .map(|_| ())
                .map_err(map_pos_projection_error),
            (Self::Hr(runner), ProjectionJobKind::Incremental) => runner
                .project_page(
                    HrProjectionRequest::new(
                        tenant_id,
                        holding_ref,
                        projection_hr_cursor(&claim.job.payload)?,
                        page_size,
                    )
                    .map_err(|_| TenantProjectionExecutorError::Permanent)?,
                )
                .await
                .map(|_| ())
                .map_err(map_hr_projection_error),
            (Self::Hr(runner), ProjectionJobKind::Rebuild) => runner
                .rebuild(tenant_id, holding_ref, page_size)
                .await
                .map(|_| ())
                .map_err(map_hr_projection_error),
            (Self::Sales(runner), ProjectionJobKind::Incremental) => runner
                .project_page(
                    SalesProjectionRequest::new(
                        tenant_id,
                        holding_ref,
                        projection_sales_cursor(&claim.job.payload)?,
                        page_size,
                    )
                    .map_err(|_| TenantProjectionExecutorError::Permanent)?,
                )
                .await
                .map(|_| ())
                .map_err(map_worker_projection_error),
            (Self::Sales(runner), ProjectionJobKind::Rebuild) => runner
                .rebuild(tenant_id, holding_ref, page_size)
                .await
                .map(|_| ())
                .map_err(map_worker_projection_error),
        }
    }
}

pub(crate) fn projection_page_size(
    payload: &serde_json::Value,
    default: u32,
) -> Result<u32, TenantProjectionExecutorError> {
    let Some(value) = payload.get("page_size") else {
        return Ok(default);
    };
    let Some(value) = value.as_u64().and_then(|value| u32::try_from(value).ok()) else {
        return Err(TenantProjectionExecutorError::Permanent);
    };
    if value == 0 || value > 500 {
        return Err(TenantProjectionExecutorError::Permanent);
    }
    Ok(value)
}

fn projection_cursor_object(
    payload: &serde_json::Value,
) -> Result<Option<&serde_json::Map<String, serde_json::Value>>, TenantProjectionExecutorError> {
    let Some(value) = payload.get("cursor") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_object()
        .map(Some)
        .ok_or(TenantProjectionExecutorError::Permanent)
}

fn projection_uuid(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<Uuid, TenantProjectionExecutorError> {
    object
        .get(field)
        .and_then(serde_json::Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .filter(|value| !value.is_nil())
        .ok_or(TenantProjectionExecutorError::Permanent)
}

fn projection_version(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<u64, TenantProjectionExecutorError> {
    object
        .get("aggregate_version")
        .and_then(serde_json::Value::as_u64)
        .filter(|value| *value != 0)
        .ok_or(TenantProjectionExecutorError::Permanent)
}

fn projection_pos_cursor(
    payload: &serde_json::Value,
) -> Result<Option<erpx_infrastructure::PosSourceEventCursor>, TenantProjectionExecutorError> {
    let Some(cursor) = projection_cursor_object(payload)? else {
        return Ok(None);
    };
    Ok(Some(erpx_infrastructure::PosSourceEventCursor {
        aggregate_id: projection_uuid(cursor, "aggregate_id")?,
        aggregate_version: projection_version(cursor)?,
    }))
}

fn projection_hr_cursor(
    payload: &serde_json::Value,
) -> Result<Option<erpx_infrastructure::HrSourceEventCursor>, TenantProjectionExecutorError> {
    let Some(cursor) = projection_cursor_object(payload)? else {
        return Ok(None);
    };
    let aggregate_type = cursor
        .get("aggregate_type")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(TenantProjectionExecutorError::Permanent)?
        .to_owned();
    Ok(Some(erpx_infrastructure::HrSourceEventCursor {
        aggregate_type,
        aggregate_id: projection_uuid(cursor, "aggregate_id")?,
        aggregate_version: projection_version(cursor)?,
    }))
}

fn projection_aggregate_type(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<String, TenantProjectionExecutorError> {
    object
        .get("aggregate_type")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or(TenantProjectionExecutorError::Permanent)
}

fn projection_asset_maintenance_cursor(
    payload: &serde_json::Value,
) -> Result<Option<AssetMaintenanceSourceEventCursor>, TenantProjectionExecutorError> {
    let Some(cursor) = projection_cursor_object(payload)? else {
        return Ok(None);
    };
    Ok(Some(AssetMaintenanceSourceEventCursor {
        aggregate_type: projection_aggregate_type(cursor)?,
        aggregate_id: projection_uuid(cursor, "aggregate_id")?,
        aggregate_version: projection_version(cursor)?,
    }))
}

fn projection_legal_cursor(
    payload: &serde_json::Value,
) -> Result<Option<erpx_infrastructure::LegalSourceEventCursor>, TenantProjectionExecutorError> {
    let Some(cursor) = projection_cursor_object(payload)? else {
        return Ok(None);
    };
    Ok(Some(erpx_infrastructure::LegalSourceEventCursor {
        aggregate_type: projection_aggregate_type(cursor)?,
        aggregate_id: projection_uuid(cursor, "aggregate_id")?,
        aggregate_version: projection_version(cursor)?,
    }))
}

fn projection_sales_cursor(
    payload: &serde_json::Value,
) -> Result<Option<SalesSourceEventCursor>, TenantProjectionExecutorError> {
    let Some(cursor) = projection_cursor_object(payload)? else {
        return Ok(None);
    };
    Ok(Some(SalesSourceEventCursor {
        aggregate_type: projection_aggregate_type(cursor)?,
        aggregate_id: projection_uuid(cursor, "aggregate_id")?,
        aggregate_version: projection_version(cursor)?,
    }))
}

fn map_scm_projection_error(error: ScmProjectionWorkerError) -> TenantProjectionExecutorError {
    match error {
        ScmProjectionWorkerError::Configuration(_) => TenantProjectionExecutorError::Permanent,
        ScmProjectionWorkerError::Projection { .. } => TenantProjectionExecutorError::Retryable,
    }
}

fn map_pos_projection_error(error: PosProjectionWorkerError) -> TenantProjectionExecutorError {
    match error {
        PosProjectionWorkerError::Configuration(_) => TenantProjectionExecutorError::Permanent,
        PosProjectionWorkerError::Projection { .. } => TenantProjectionExecutorError::Retryable,
    }
}

fn map_hr_projection_error(error: HrProjectionWorkerError) -> TenantProjectionExecutorError {
    match error {
        HrProjectionWorkerError::Configuration(_) => TenantProjectionExecutorError::Permanent,
        HrProjectionWorkerError::Projection { .. } => TenantProjectionExecutorError::Retryable,
    }
}

fn map_worker_projection_error(error: WorkerRuntimeError) -> TenantProjectionExecutorError {
    match error {
        WorkerRuntimeError::Configuration(_) => TenantProjectionExecutorError::Permanent,
        WorkerRuntimeError::Persistence(_)
        | WorkerRuntimeError::Database(_)
        | WorkerRuntimeError::Migration(_) => TenantProjectionExecutorError::Retryable,
    }
}

pub(crate) fn map_cqrs_projection_error(
    error: erpx_cqrs_sqlx::CqrsSqlxError,
) -> TenantProjectionExecutorError {
    match error {
        erpx_cqrs_sqlx::CqrsSqlxError::InvalidInput(_)
        | erpx_cqrs_sqlx::CqrsSqlxError::NumericRange
        | erpx_cqrs_sqlx::CqrsSqlxError::ProjectionPositionConflict
        | erpx_cqrs_sqlx::CqrsSqlxError::ProjectionOutOfOrder => {
            TenantProjectionExecutorError::Permanent
        }
        _ => TenantProjectionExecutorError::Retryable,
    }
}

fn map_legal_projection_error(error: LegalProjectionWorkerError) -> TenantProjectionExecutorError {
    match error {
        LegalProjectionWorkerError::Configuration(_) => TenantProjectionExecutorError::Permanent,
        LegalProjectionWorkerError::Projection { .. } => TenantProjectionExecutorError::Retryable,
    }
}

/// The executable composition boundary for the tenant-scoped marketing
/// preview expiry seam. The durable worker loop supplies restart-safe polling;
/// the injected SQLx executor supplies the tenant route and idempotent write.
struct MarketingContentPreviewExpiryRuntimeHandler {
    handler: MarketingContentPreviewExpiryHandler<SqlxMarketingContentPreviewExpiryExecutor>,
    tenant_id: String,
    holding_ref: String,
    batch_size: u32,
}

/// Coordinator-owned composition for the read-only Finance subledger
/// reconciliation seam. A non-green report remains an incident/rebuild signal;
/// this handler never repairs authoritative or projected rows in place.
struct FinanceSubledgerReconciliationRuntimeHandler {
    runner: FinanceSubledgerReconciliationRunner,
    request: FinanceSubledgerReconciliationRequest,
}

impl FinanceSubledgerReconciliationRuntimeHandler {
    fn from_env(router: TenantDatabaseRouter) -> Result<Self, Box<dyn std::error::Error>> {
        let tenant_id = required_env("ERPX_FINANCE_RECONCILIATION_TENANT_ID")?;
        let holding_ref = required_env("ERPX_FINANCE_RECONCILIATION_HOLDING_REF")?;
        let max_rows = match env::var("ERPX_FINANCE_RECONCILIATION_MAX_ROWS") {
            Ok(value) => value
                .parse::<u32>()
                .map_err(|_| "ERPX_FINANCE_RECONCILIATION_MAX_ROWS must be an integer")?,
            Err(_) => FINANCE_SUBLEDGER_RECONCILIATION_MAX_ROWS,
        };
        let request = FinanceSubledgerReconciliationRequest::new(tenant_id, holding_ref, max_rows)?;
        Ok(Self {
            runner: FinanceSubledgerReconciliationRunner::new(router),
            request,
        })
    }
}

const FINANCE_SUBLEDGER_RECONCILIATION_HANDLER_NAME: &str = "finance_subledger_reconciliation";

#[async_trait::async_trait]
impl NamedWorkerHandler for FinanceSubledgerReconciliationRuntimeHandler {
    fn name(&self) -> &str {
        FINANCE_SUBLEDGER_RECONCILIATION_HANDLER_NAME
    }

    async fn run_once(&self, _worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let report = self
            .runner
            .reconcile(self.request.clone())
            .await
            .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
        let reconciled = report.is_reconciled();
        if !reconciled {
            tracing::warn!(
                tenant_id = %self.request.tenant_id,
                holding_ref = %self.request.holding_ref,
                issue_count = report.issues.len(),
                "Finance subledger reconciliation requires projection rebuild or incident review"
            );
        }
        Ok(WorkerHandlerReport {
            claimed: 1,
            processed: 1,
            published: 0,
            retried: 0,
            dead_lettered: 0,
            has_work: !reconciled,
        })
    }
}

impl MarketingContentPreviewExpiryRuntimeHandler {
    async fn new(
        router: &TenantDatabaseRouter,
        tenant_id: String,
        holding_ref: String,
        holding_id: Uuid,
        actor_ref: String,
        batch_size: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let write = router
            .route(&tenant_id, &holding_ref, RoutedDatabaseSide::Write)
            .await?;
        let read = router
            .route(&tenant_id, &holding_ref, RoutedDatabaseSide::Read)
            .await?;
        let store = SqlxMarketingContentStore::new(
            write.database_pool().clone(),
            read.database_pool().clone(),
            holding_ref.clone(),
        )?;
        // Do not close either routed handle here: the store owns cloned pools
        // for the lifetime of the registered handler. Dropping the handles
        // releases only the route wrappers and preserves the pool ownership.
        drop(write);
        drop(read);

        let scope = TenantScope::Holding { holding_id };
        let executor = SqlxMarketingContentPreviewExpiryExecutor::new(
            store,
            tenant_id.clone(),
            scope,
            actor_ref,
        )?;
        Ok(Self {
            handler: MarketingContentPreviewExpiryHandler::new(executor),
            tenant_id,
            holding_ref,
            batch_size,
        })
    }
}

#[async_trait::async_trait]
impl NamedWorkerHandler for MarketingContentPreviewExpiryRuntimeHandler {
    fn name(&self) -> &str {
        MARKETING_CONTENT_PREVIEW_EXPIRY_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let request = MarketingContentPreviewExpiryRequest::new(
            worker_id,
            self.tenant_id.clone(),
            self.holding_ref.clone(),
            self.batch_size,
            Utc::now(),
        )
        .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
        let report = self
            .handler
            .run_once(request)
            .await
            .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;

        let mut retryable = 0_u64;
        let mut permanent = 0_u64;
        for item in &report.items {
            if let erpx_worker::MarketingContentPreviewExpiryItemResult::Failed { kind } =
                &item.result
            {
                match kind {
                    erpx_worker::MarketingContentPreviewExpiryFailureKind::Retryable => {
                        retryable = retryable.saturating_add(1)
                    }
                    erpx_worker::MarketingContentPreviewExpiryFailureKind::Permanent
                    | erpx_worker::MarketingContentPreviewExpiryFailureKind::InvalidCandidate => {
                        permanent = permanent.saturating_add(1)
                    }
                }
            }
        }
        if retryable != 0 || permanent != 0 {
            tracing::warn!(
                tenant_id = %self.tenant_id,
                holding_ref = %self.holding_ref,
                retryable,
                permanent,
                "marketing content preview expiry items were not applied"
            );
        }

        let processed = u64::from(report.expired) + u64::from(report.already_expired);
        Ok(WorkerHandlerReport {
            claimed: u64::from(report.scanned),
            processed,
            // The preview-expiry seam has no durable scheduler/DLQ contract
            // yet. Keep these counters at zero rather than claiming a retry
            // or dead-letter that this executable did not persist.
            retried: 0,
            dead_lettered: 0,
            // A failed item remains eligible for a future bounded poll. Keep
            // the runtime's established idle sleep when this pass applied no
            // state, avoiding a tight retry loop without inventing a second
            // queue or an in-memory production repository.
            has_work: processed != 0,
            ..WorkerHandlerReport::default()
        })
    }
}

const MAX_MARKETING_DELIVERY_ACTIVE_HOLDINGS: usize = 1_000;

struct OperationsCompletionRuntimeHandler {
    worker: OperationsCompletionWorker,
    pass: OperationsCompletionPass,
}

impl OperationsCompletionRuntimeHandler {
    fn from_env(router: TenantDatabaseRouter) -> Result<Self, Box<dyn std::error::Error>> {
        let pass_json = env::var("ERPX_OPERATIONS_COMPLETION_PASS_JSON").map_err(|_| {
            "ERPX_OPERATIONS_COMPLETION_PASS_JSON is required; no local authorization fallback is permitted"
        })?;
        let pass = serde_json::from_str(&pass_json)
            .map_err(|error| format!("ERPX_OPERATIONS_COMPLETION_PASS_JSON is invalid: {error}"))?;
        Ok(Self {
            worker: OperationsCompletionWorker::new(router),
            pass,
        })
    }
}

#[async_trait::async_trait]
impl NamedWorkerHandler for OperationsCompletionRuntimeHandler {
    fn name(&self) -> &str {
        OPERATIONS_COMPLETION_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let mut pass = self.pass.clone();
        pass.worker_id = worker_id;
        let report = self
            .worker
            .run_once(pass)
            .await
            .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
        let processed = report
            .detections_scheduled
            .saturating_add(report.reassignments_scheduled)
            .saturating_add(report.escalations_scheduled)
            .saturating_add(report.detections_applied)
            .saturating_add(report.outbox_rows_repaired)
            .saturating_add(report.reassignments_applied)
            .saturating_add(report.escalations_applied)
            .saturating_add(report.projection_events_seen);
        Ok(WorkerHandlerReport {
            claimed: processed,
            processed,
            has_work: processed != 0,
            ..WorkerHandlerReport::default()
        })
    }
}

/// Durable Marketing delivery composition over every active holding in the
/// control-plane registry. Each holding/account still gets its own tenant
/// write/read pools and account-bound provider; no tenant credentials or
/// provider account set is inferred from environment configuration.
struct MarketingDeliveryMultiTenantRuntimeHandler {
    control: Arc<SqlxControlPlane>,
    registry: SqlxTenantStoreRegistry,
    router: TenantDatabaseRouter,
    credentials: MarketingDeliverySecretManagerResolver,
    batch_size: u32,
    lease: Duration,
}

impl MarketingDeliveryMultiTenantRuntimeHandler {
    fn validate_config(batch_size: u32, lease: Duration) -> Result<(), WorkerRuntimeError> {
        if batch_size == 0 || batch_size > erpx_worker::MARKETING_DELIVERY_MAX_BATCH_SIZE {
            return Err(WorkerRuntimeError::Configuration(
                "Marketing delivery batch size is outside its bound".into(),
            ));
        }
        if lease < Duration::seconds(erpx_worker::MARKETING_DELIVERY_MIN_LEASE_SECONDS as i64)
            || lease > Duration::seconds(erpx_worker::MARKETING_DELIVERY_MAX_LEASE_SECONDS as i64)
        {
            return Err(WorkerRuntimeError::Configuration(
                "Marketing delivery lease is outside its bound".into(),
            ));
        }
        Ok(())
    }

    fn new(
        control: Arc<SqlxControlPlane>,
        registry: SqlxTenantStoreRegistry,
        router: TenantDatabaseRouter,
        credentials: MarketingDeliverySecretManagerResolver,
        batch_size: u32,
        lease: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        Self::validate_config(batch_size, lease)?;
        Ok(Self {
            control,
            registry,
            router,
            credentials,
            batch_size,
            lease,
        })
    }
}

#[async_trait::async_trait]
impl NamedWorkerHandler for MarketingDeliveryMultiTenantRuntimeHandler {
    fn name(&self) -> &str {
        MARKETING_DELIVERY_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let registrations = self
            .registry
            .list_active_bounded(MAX_MARKETING_DELIVERY_ACTIVE_HOLDINGS)
            .await
            .map_err(|_| {
                WorkerRuntimeError::Configuration("Marketing delivery worker failed".into())
            })?;
        let mut total = WorkerHandlerReport::default();
        for registration in registrations {
            let accounts = self
                .control
                .list_marketing_provider_accounts_for_outbound(
                    &registration.tenant_id,
                    &registration.holding_ref,
                )
                .await
                .map_err(|_| {
                    WorkerRuntimeError::Configuration("Marketing delivery worker failed".into())
                })?;
            for binding in accounts {
                let remaining = self
                    .batch_size
                    .saturating_sub(total.claimed.try_into().unwrap_or(u32::MAX));
                if remaining == 0 {
                    break;
                }
                let write = self
                    .router
                    .route(
                        &registration.tenant_id,
                        &registration.holding_ref,
                        RoutedDatabaseSide::Write,
                    )
                    .await
                    .map_err(|_| {
                        WorkerRuntimeError::Configuration("Marketing delivery worker failed".into())
                    })?;
                let read = match self
                    .router
                    .route(
                        &registration.tenant_id,
                        &registration.holding_ref,
                        RoutedDatabaseSide::Read,
                    )
                    .await
                {
                    Ok(read) => read,
                    Err(_) => {
                        write.close().await;
                        return Err(WorkerRuntimeError::Configuration(
                            "Marketing delivery worker failed".into(),
                        ));
                    }
                };
                let source = match SqlxMarketingDeliveryStore::new_for_channels(
                    write.database_pool().clone(),
                    read.database_pool().clone(),
                    registration.holding_ref.clone(),
                    binding.channels.clone(),
                ) {
                    Ok(source) => source,
                    Err(_) => {
                        write.close().await;
                        read.close().await;
                        return Err(WorkerRuntimeError::Configuration(
                            "Marketing delivery worker failed".into(),
                        ));
                    }
                };
                let projected = source
                    .project_pending_read_model(
                        erpx_infrastructure::MAX_MARKETING_DELIVERY_PROJECTION_PAGE_SIZE,
                    )
                    .await
                    .map_err(|_| {
                        WorkerRuntimeError::Configuration(
                            "Marketing delivery projection failed".into(),
                        )
                    })?;
                // A full page may still have more authoritative events. Once
                // the bounded page is drained, reconcile exact source/read
                // payloads and the durable cursor before any provider claim.
                if projected
                    < u64::from(erpx_infrastructure::MAX_MARKETING_DELIVERY_PROJECTION_PAGE_SIZE)
                {
                    let reconciliation = source.reconcile_projection().await.map_err(|_| {
                        WorkerRuntimeError::Configuration(
                            "Marketing delivery projection failed".into(),
                        )
                    })?;
                    if !reconciliation.healthy {
                        return Err(WorkerRuntimeError::Configuration(
                            "Marketing delivery projection failed".into(),
                        ));
                    }
                }
                drop(write);
                drop(read);
                let provider =
                    erpx_worker::HttpMarketingDeliveryProvider::from_account_with_resolver(
                        &binding,
                        self.credentials.clone(),
                    )
                    .map_err(|_| {
                        WorkerRuntimeError::Configuration("Marketing delivery worker failed".into())
                    })?;
                let account_resolver = erpx_worker::SqlxMarketingDeliveryAccountResolver::new(
                    Arc::clone(&self.control),
                    binding.provider_account_ref.clone(),
                    registration.tenant_id.clone(),
                    registration.holding_ref.clone(),
                )
                .map_err(|_| {
                    WorkerRuntimeError::Configuration("Marketing delivery worker failed".into())
                })?;
                let provider = erpx_worker::AccountBoundMarketingDeliveryProvider::new(
                    provider,
                    account_resolver,
                    &binding,
                )
                .map_err(|_| {
                    WorkerRuntimeError::Configuration("Marketing delivery worker failed".into())
                })?;
                let handler =
                    MarketingDeliveryHandler::new(source, provider, remaining, self.lease)?;
                let report = handler.run_once(worker_id).await?;
                total.claimed = total.claimed.saturating_add(report.claimed);
                total.processed = total.processed.saturating_add(report.processed);
                total.published = total.published.saturating_add(report.published);
                total.retried = total.retried.saturating_add(report.retried);
                total.dead_lettered = total.dead_lettered.saturating_add(report.dead_lettered);
                total.has_work |= report.has_work;
            }
            if total.claimed >= u64::from(self.batch_size) {
                break;
            }
        }
        Ok(total)
    }
}

fn required_env(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    env::var(name).map_err(|_| format!("{name} is required").into())
}

fn optional_env(primary: &str, fallback: &str, default: String) -> String {
    env::var(primary)
        .or_else(|_| env::var(fallback))
        .unwrap_or(default)
}

pub(crate) async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let log_format = env::var("ERPX_LOG_FORMAT").unwrap_or_else(|_| "pretty".into());
    if log_format == "json" {
        tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_span_list(true)
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .init();
    } else if log_format == "pretty" {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .init();
    } else {
        return Err("ERPX_LOG_FORMAT must be pretty or json".into());
    }
    let runtime = WorkerRuntime::connect(WorkerRuntimeConfig::from_env()?).await?;
    match env::args().nth(1).as_deref() {
        Some("run") => {
            let handler_name = env::var("ERPX_WORKER_HANDLER").unwrap_or_default();
            let worker_id = env::var("ERPX_WORKER_ID").unwrap_or_default();
            tracing::info!(
                worker_id = %worker_id,
                handler = %handler_name,
                "worker_started"
            );
            let registry = if handler_name == TENANT_BACKUP_RETENTION_HANDLER_NAME {
                let batch_size = runtime
                    .config()
                    .batch_size
                    .min(erpx_infrastructure::MAX_TENANT_BACKUP_RETENTION_PAGE_SIZE);
                let handler = match BackupRuntime::from_env() {
                    Ok(backup_runtime) => TenantBackupRetentionRuntimeHandler::with_backup_adapter(
                        Arc::clone(runtime.store()),
                        Arc::new(backup_runtime.adapter().clone()),
                        batch_size,
                        runtime.config().claim_timeout,
                    )?,
                    Err(error) => {
                        tracing::warn!(
                            blocker = "backup_external_credentials_unavailable",
                            "tenant backup retention certification blocker queued; retention worker will fail closed until the external adapter is configured"
                        );
                        tracing::debug!(?error, "tenant backup retention adapter is disabled");
                        TenantBackupRetentionRuntimeHandler::new(
                            Arc::clone(runtime.store()),
                            batch_size,
                            runtime.config().claim_timeout,
                        )?
                    }
                };
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == COMMERCIAL_CATALOG_PROJECTION_HANDLER_NAME {
                let handler = CommercialCatalogProjectionHandler::new(
                    Arc::clone(runtime.store()),
                    runtime.config().batch_size,
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == WAVE5_CONTROL_PROJECTION_HANDLER_NAME {
                let handler = Wave5ControlProjectionHandler::new(
                    Arc::clone(runtime.store()),
                    runtime.config().batch_size,
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if matches!(
                handler_name.as_str(),
                CUSTOMER360_CONTROL_PROJECTION_HANDLER_NAME
                    | CUSTOMER360_TENANT_PROJECTION_HANDLER_NAME
            ) {
                let kind = match handler_name.as_str() {
                    CUSTOMER360_CONTROL_PROJECTION_HANDLER_NAME => {
                        Customer360ProjectionKind::ControlCustomers
                    }
                    CUSTOMER360_TENANT_PROJECTION_HANDLER_NAME => {
                        Customer360ProjectionKind::TenantActivity
                    }
                    _ => unreachable!("matched Customer 360 projection handler"),
                };
                let handler = Customer360ProjectionHandler::new(
                    kind,
                    Arc::clone(runtime.store()),
                    runtime.config().batch_size,
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == OPERATIONS_SOURCE_INGESTION_HANDLER_NAME {
                let page_size = runtime
                    .config()
                    .batch_size
                    .min(OPERATIONS_SOURCE_MAX_PAGE_SIZE);
                let handler = operations_source_ingestion_handler(
                    Arc::clone(runtime.store()),
                    page_size,
                    OPERATIONS_SOURCE_MAX_ACTIVE_TENANTS,
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(handler)?;
                registry
            } else if handler_name == BRANCH_PROVISIONING_HANDLER_NAME {
                let handler =
                    BranchProvisioningRuntimeHandler::from_env(Arc::clone(runtime.store()))?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == DIRECTORY_SUPERVISOR_HANDLER_NAME {
                let handler = DirectorySupervisorRuntimeHandler::from_env(
                    runtime.store().pools().write.clone(),
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == DIRECTORY_SYNC_HANDLER_NAME {
                let handler =
                    DirectorySyncRuntimeHandler::from_env(runtime.store().pools().write.clone())?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == CLUSTER_PITR_CONTROLLER_HANDLER_NAME {
                let handler = ClusterPitrControllerRuntimeHandler::from_env(
                    runtime.store().pools().write.clone(),
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == SUPPORT_WORKFLOW_HANDLER_NAME {
                // Support's rebuildable read tables live on the control-read
                // route, while grant/session facts and the source outbox stay
                // on the control-write route.  Compose this handler only from
                // those live SQLx pools and initialize the read schema before
                // claiming any Support event; there is no local or in-memory
                // fallback that could make a partial deployment appear ready.
                let workflow_store = SupportWorkflowStore::new(
                    runtime.store().pools().write.clone(),
                    runtime.store().pools().read.clone(),
                );
                workflow_store.ensure_read_model().await?;
                runtime.handler_registry_with_support_workflow(workflow_store)?
            } else if handler_name == ROLE_REVIEW_SNAPSHOT_HANDLER_NAME {
                let handler = RoleReviewSnapshotRuntimeHandler::from_env(
                    runtime.store().pools().write.clone(),
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == ROLE_REVIEW_NOTIFICATION_HANDLER_NAME {
                let scope = role_review_scope_from_env()?;
                let claimant_ref = env::var("ERPX_ROLE_REVIEW_CLAIMANT_REF")
                    .unwrap_or_else(|_| "erpx-role-review".into());
                let actor_ref = env::var("ERPX_ROLE_REVIEW_ACTOR_REF")
                    .unwrap_or_else(|_| format!("erpx-worker:{}", runtime.config().worker_id));
                let horizon_seconds = env::var("ERPX_ROLE_REVIEW_HORIZON_SECONDS")
                    .unwrap_or_else(|_| "604800".into())
                    .parse::<i64>()
                    .map_err(|_| "ERPX_ROLE_REVIEW_HORIZON_SECONDS must be an integer")?;
                let lease_seconds = env::var("ERPX_ROLE_REVIEW_LEASE_SECONDS")
                    .unwrap_or_else(|_| "300".into())
                    .parse::<i64>()
                    .map_err(|_| "ERPX_ROLE_REVIEW_LEASE_SECONDS must be an integer")?;
                let publisher = ControlOutboxRoleReviewPublisher {
                    persistence: RoleReviewPersistence::new(runtime.store().pools().write.clone()),
                    actor_ref,
                };
                runtime.handler_registry_with_role_review_notifications(
                    scope,
                    publisher,
                    claimant_ref,
                    Duration::seconds(horizon_seconds),
                    Duration::seconds(lease_seconds),
                )?
            } else if handler_name == G13_MIGRATION_HANDLER_NAME {
                let handler = G13MigrationRuntimeHandler::from_env(Arc::clone(runtime.store()))?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == G13_TARGET_HANDLER_NAME {
                let handler =
                    G13TargetRuntimeHandler::from_env(Arc::clone(runtime.store())).await?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == OPERATIONS_COMPLETION_HANDLER_NAME {
                let routing = TenantDatabaseProvisioningConfig::from_env()?;
                let router = TenantDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let handler = OperationsCompletionRuntimeHandler::from_env(router)?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == OPERATIONS_SLA_NOTIFICATION_HANDLER_NAME {
                let tenant_id = required_env("ERPX_OPERATIONS_NOTIFICATION_TENANT_ID")?;
                let holding_ref = required_env("ERPX_OPERATIONS_NOTIFICATION_HOLDING_REF")?;
                let routing = TenantDatabaseProvisioningConfig::from_env()?;
                let router = TenantDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let write = router
                    .route(&tenant_id, &holding_ref, RoutedDatabaseSide::Write)
                    .await?;
                let read = router
                    .route(&tenant_id, &holding_ref, RoutedDatabaseSide::Read)
                    .await?;
                let store = SqlxOperationsStore::from_pools(
                    write.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|_| "Operations notification tenant route is invalid")?;
                drop(write);
                drop(read);
                let actor_ref = optional_env(
                    "ERPX_OPERATIONS_NOTIFICATION_ACTOR_REF",
                    "ERPX_WORKER_ACTOR_REF",
                    format!("erpx-worker:{}", runtime.config().worker_id),
                );
                let publisher =
                    OperationsOutboxSlaNotificationPublisher::new(store.clone(), actor_ref)?;
                let handler = OperationsSlaNotificationHandler::new(
                    store,
                    publisher,
                    runtime
                        .config()
                        .batch_size
                        .min(erpx_infrastructure::MAX_OPERATIONS_SLA_NOTIFICATION_PAGE_SIZE),
                    runtime.config().claim_timeout,
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == HR_COMPLETION_HANDLER_NAME {
                let routing = TenantDatabaseProvisioningConfig::from_env()?;
                let router = TenantDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let handler = HrCompletionRuntimeHandler::from_env(router)
                    .map_err(|_| "HR completion runtime configuration is invalid")?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == FINANCE_COMPLETION_HANDLER_NAME {
                let routing = TenantDatabaseProvisioningConfig::from_env()?;
                let router = TenantDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let handler = FinanceCompletionRuntimeHandler::from_env(router)?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == FINANCE_SUBLEDGER_RECONCILIATION_HANDLER_NAME {
                let routing = TenantDatabaseProvisioningConfig::from_env()?;
                let router = TenantDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let handler = FinanceSubledgerReconciliationRuntimeHandler::from_env(router)?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == FINANCE_BANK_PROVIDER_IMPORT_HANDLER_NAME {
                let routing = TenantDatabaseProvisioningConfig::from_env()?;
                let router = TenantDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let authorizer = ControlPlaneFinanceBankProviderImportAuthorizer {
                    control: Arc::clone(runtime.store()),
                    identity: Arc::new(SqlxIdentityStore::new(
                        runtime.store().pools().write.clone(),
                    )),
                };
                // Provider credentials and concrete adapters are deployment
                // inputs. The executable composition remains reachable, but
                // fails closed until a customer-approved secret manager and
                // provider factory are injected; it never uses a mock.
                let credential_resolver = UnconfiguredBankProviderCredentialResolver;
                let adapter_factory = BankProviderAdapterRegistry::empty();
                runtime.handler_registry_with_finance_bank_provider_import(
                    router,
                    authorizer,
                    credential_resolver,
                    adapter_factory,
                    runtime.config().batch_size.min(100),
                    runtime.config().claim_timeout,
                )?
            } else if handler_name == TENANT_BACKUP_HANDLER_NAME {
                let handler = TenantBackupJobRuntimeHandler::from_env(
                    Arc::clone(runtime.store()),
                    runtime
                        .config()
                        .batch_size
                        .min(erpx_infrastructure::MAX_TENANT_BACKUP_JOB_PAGE_SIZE),
                    runtime.config().claim_timeout,
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == TENANT_PROJECTION_STATUS_PROJECTION_HANDLER_NAME {
                runtime.handler_registry_with_control_plane_tenant_projection_status_projection()?
            } else if handler_name == MARKETING_PUBLIC_ABUSE_CLEANUP_HANDLER_NAME {
                let batch_size = match env::var("ERPX_MARKETING_PUBLIC_ABUSE_CLEANUP_BATCH_SIZE") {
                    Ok(value) => value.parse::<u32>().map_err(
                        |_| "ERPX_MARKETING_PUBLIC_ABUSE_CLEANUP_BATCH_SIZE must be an integer",
                    )?,
                    Err(_) => runtime
                        .config()
                        .batch_size
                        .min(MARKETING_PUBLIC_ABUSE_CLEANUP_DEFAULT_BATCH_SIZE),
                };
                if !(1..=MARKETING_PUBLIC_ABUSE_CLEANUP_MAX_BATCH_SIZE).contains(&batch_size) {
                    return Err(format!(
                        "ERPX_MARKETING_PUBLIC_ABUSE_CLEANUP_BATCH_SIZE must be 1..={MARKETING_PUBLIC_ABUSE_CLEANUP_MAX_BATCH_SIZE}"
                    )
                    .into());
                }
                let handler = MarketingPublicAbuseCleanupRunner::new(
                    runtime.store().as_ref().clone(),
                    batch_size,
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == MARKETING_DELIVERY_HANDLER_NAME {
                let routing = TenantDatabaseProvisioningConfig::from_env()?;
                let router = TenantDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let credential_resolver = MarketingDeliverySecretManagerResolver::from_env()?;
                let inner = MarketingDeliveryMultiTenantRuntimeHandler::new(
                    Arc::clone(runtime.store()),
                    SqlxTenantStoreRegistry::new(Arc::clone(runtime.store())),
                    router,
                    credential_resolver,
                    runtime
                        .config()
                        .batch_size
                        .min(erpx_worker::MARKETING_DELIVERY_MAX_BATCH_SIZE),
                    runtime.config().claim_timeout,
                )?;
                let scheduler = SqlxMarketingDeliverySchedulerRepository::new(
                    runtime.store().pools().write.clone(),
                );
                let now = Utc::now();
                scheduler
                    .ensure_scheduled(
                        MarketingDeliveryScheduleRequest::default_global(
                            chrono::DateTime::<Utc>::UNIX_EPOCH,
                        ),
                        now,
                    )
                    .await
                    .map_err(|_| "Marketing delivery scheduler setup failed")?;
                let handler = MarketingDeliverySchedulerHandler::new(
                    scheduler,
                    inner,
                    runtime.config().claim_timeout,
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == MARKETING_JOURNEY_EXECUTION_HANDLER_NAME {
                let routing = TenantDatabaseProvisioningConfig::from_env()?;
                let router = TenantDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let tenant_id = env::var("ERPX_MARKETING_JOURNEY_TENANT_ID")
                    .map_err(|_| "ERPX_MARKETING_JOURNEY_TENANT_ID is required")?;
                let holding_ref = env::var("ERPX_MARKETING_JOURNEY_HOLDING_REF")
                    .map_err(|_| "ERPX_MARKETING_JOURNEY_HOLDING_REF is required")?;
                let routed = router
                    .route(&tenant_id, &holding_ref, RoutedDatabaseSide::Write)
                    .await?;
                let write_pool = routed.database_pool().clone();
                let store = SqlxMarketingJourneyExecutionStore::new_with_control(
                    write_pool,
                    holding_ref,
                    Arc::clone(runtime.store()),
                )?;
                let handler = MarketingJourneyExecutionHandler::new(
                    store.clone(),
                    SqlxMarketingJourneyExecutionDecisionSource::new(store),
                    runtime
                        .config()
                        .batch_size
                        .min(MARKETING_JOURNEY_EXECUTION_MAX_BATCH_SIZE),
                    runtime.config().claim_timeout,
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == MARKETING_JOURNEY_INTEGRATION_HANDLER_NAME {
                let routing = TenantDatabaseProvisioningConfig::from_env()?;
                let router = TenantDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let credentials = MarketingDeliverySecretManagerResolver::from_env()?;
                let handler = MarketingJourneyIntegrationMultiTenantRuntimeHandler::new(
                    Arc::clone(runtime.store()),
                    SqlxTenantStoreRegistry::new(Arc::clone(runtime.store())),
                    router,
                    credentials,
                    runtime
                        .config()
                        .batch_size
                        .min(erpx_worker::MARKETING_JOURNEY_INTEGRATION_MAX_BATCH_SIZE),
                    runtime.config().claim_timeout,
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == MARKETING_CONTENT_PREVIEW_EXPIRY_HANDLER_NAME {
                let routing = TenantDatabaseProvisioningConfig::from_env()?;
                let router = TenantDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let tenant_id = required_env("ERPX_MARKETING_CONTENT_PREVIEW_EXPIRY_TENANT_ID")?;
                let holding_ref =
                    required_env("ERPX_MARKETING_CONTENT_PREVIEW_EXPIRY_HOLDING_REF")?;
                let holding_id_ref = env::var("ERPX_MARKETING_CONTENT_PREVIEW_EXPIRY_HOLDING_ID")
                    .unwrap_or_else(|_| holding_ref.clone());
                let holding_id = Uuid::parse_str(&holding_id_ref).map_err(|_| {
                    "ERPX_MARKETING_CONTENT_PREVIEW_EXPIRY_HOLDING_ID (or HOLDING_REF fallback) must be a UUID"
                })?;
                let actor_ref = optional_env(
                    "ERPX_MARKETING_CONTENT_PREVIEW_EXPIRY_ACTOR_REF",
                    "ERPX_WORKER_ACTOR_REF",
                    format!("erpx-worker:{}", runtime.config().worker_id),
                );
                let batch_size = match env::var("ERPX_MARKETING_CONTENT_PREVIEW_EXPIRY_BATCH_SIZE")
                {
                    Ok(value) => value.parse::<u32>().map_err(
                        |_| "ERPX_MARKETING_CONTENT_PREVIEW_EXPIRY_BATCH_SIZE must be an integer",
                    )?,
                    Err(_) => runtime
                        .config()
                        .batch_size
                        .min(MARKETING_CONTENT_PREVIEW_EXPIRY_MAX_BATCH_SIZE),
                };
                if !(1..=MARKETING_CONTENT_PREVIEW_EXPIRY_MAX_BATCH_SIZE).contains(&batch_size) {
                    return Err(format!(
                        "ERPX_MARKETING_CONTENT_PREVIEW_EXPIRY_BATCH_SIZE must be 1..={MARKETING_CONTENT_PREVIEW_EXPIRY_MAX_BATCH_SIZE}"
                    )
                    .into());
                }
                let handler = MarketingContentPreviewExpiryRuntimeHandler::new(
                    &router,
                    tenant_id,
                    holding_ref,
                    holding_id,
                    actor_ref,
                    batch_size,
                )
                .await?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == MARKETING_CONTENT_PRIVACY_HANDLER_NAME {
                let routing = TenantDatabaseProvisioningConfig::from_env()?;
                let router = TenantDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let tenant_id = required_env("ERPX_MARKETING_CONTENT_PRIVACY_TENANT_ID")?;
                let holding_ref = required_env("ERPX_MARKETING_CONTENT_PRIVACY_HOLDING_REF")?;
                let holding_id_ref = env::var("ERPX_MARKETING_CONTENT_PRIVACY_HOLDING_ID")
                    .unwrap_or_else(|_| holding_ref.clone());
                let holding_id = Uuid::parse_str(&holding_id_ref).map_err(|_| {
                    "ERPX_MARKETING_CONTENT_PRIVACY_HOLDING_ID (or HOLDING_REF fallback) must be a UUID"
                })?;
                let actor_ref = optional_env(
                    "ERPX_MARKETING_CONTENT_PRIVACY_ACTOR_REF",
                    "ERPX_WORKER_ACTOR_REF",
                    format!("erpx-worker:{}", runtime.config().worker_id),
                );
                let purpose = env::var("ERPX_MARKETING_CONTENT_PRIVACY_PURPOSE")
                    .unwrap_or_else(|_| "expire Marketing form submission retention".into());
                let correlation_id = match env::var("ERPX_MARKETING_CONTENT_PRIVACY_CORRELATION_ID")
                {
                    Ok(value) => Uuid::parse_str(&value).map_err(
                        |_| "ERPX_MARKETING_CONTENT_PRIVACY_CORRELATION_ID must be a UUID",
                    )?,
                    Err(_) => Uuid::new_v4(),
                };
                let context =
                    MarketingContentPrivacyWorkerContext::new(actor_ref, purpose, correlation_id)?;
                let batch_size = match env::var("ERPX_MARKETING_CONTENT_PRIVACY_BATCH_SIZE") {
                    Ok(value) => value.parse::<u32>().map_err(
                        |_| "ERPX_MARKETING_CONTENT_PRIVACY_BATCH_SIZE must be an integer",
                    )?,
                    Err(_) => runtime
                        .config()
                        .batch_size
                        .min(MARKETING_CONTENT_PRIVACY_DEFAULT_BATCH_SIZE),
                };
                let request = MarketingContentPrivacyRequest::new(
                    tenant_id,
                    holding_ref,
                    holding_id,
                    batch_size,
                    context,
                )?;
                let runner = MarketingContentPrivacyRunner::new(router, request)?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(runner))?;
                registry
            } else if handler_name == LEGAL_RETENTION_HANDLER_NAME {
                let routing = TenantDatabaseProvisioningConfig::from_env()?;
                let router = TenantDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let tenant_id = env::var("ERPX_LEGAL_RETENTION_TENANT_ID")
                    .map_err(|_| "ERPX_LEGAL_RETENTION_TENANT_ID is required")?;
                let holding_ref = env::var("ERPX_LEGAL_RETENTION_HOLDING_REF")
                    .map_err(|_| "ERPX_LEGAL_RETENTION_HOLDING_REF is required")?;
                let actor_ref = env::var("ERPX_WORKER_ACTOR_REF")
                    .unwrap_or_else(|_| "system-worker:legal-retention".into());
                let purpose = env::var("ERPX_LEGAL_RETENTION_PURPOSE")
                    .unwrap_or_else(|_| "expire legal contract retention".into());
                let correlation_id = match env::var("ERPX_LEGAL_RETENTION_CORRELATION_ID") {
                    Ok(value) => Uuid::parse_str(&value)
                        .map_err(|_| "ERPX_LEGAL_RETENTION_CORRELATION_ID must be a UUID")?,
                    Err(_) => Uuid::new_v4(),
                };
                let context = LegalRetentionWorkerContext::new(actor_ref, purpose, correlation_id)?;
                let batch_size = match env::var("ERPX_LEGAL_RETENTION_BATCH_SIZE") {
                    Ok(value) => value
                        .parse::<u32>()
                        .map_err(|_| "ERPX_LEGAL_RETENTION_BATCH_SIZE must be an integer")?,
                    Err(_) => runtime
                        .config()
                        .batch_size
                        .min(LEGAL_RETENTION_DEFAULT_BATCH_SIZE),
                };
                let request =
                    LegalRetentionRequest::new(tenant_id, holding_ref, batch_size, context)?;
                let runner = LegalRetentionRunner::new(router, request)?;
                runtime.handler_registry_with_legal_retention(runner)?
            } else if handler_name == TENANT_PROJECTION_SCHEDULER_HANDLER_NAME {
                let routing = TenantDatabaseProvisioningConfig::from_env()?;
                let router = TenantDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let actor_ref = env::var("ERPX_WORKER_ACTOR_REF")
                    .unwrap_or_else(|_| format!("erpx-worker:{}", runtime.config().worker_id));
                let purpose = env::var("ERPX_WORKER_PROJECTION_PURPOSE")
                    .unwrap_or_else(|_| "durable tenant projection".into());
                let correlation_id = match env::var("ERPX_WORKER_PROJECTION_CORRELATION_ID") {
                    Ok(value) => Uuid::parse_str(&value)
                        .map_err(|_| "ERPX_WORKER_PROJECTION_CORRELATION_ID must be a UUID")?,
                    Err(_) => Uuid::new_v4(),
                };
                let authorization =
                    ProjectionSchedulerAuthorization::new(actor_ref, purpose, correlation_id)?;
                let claim_key_prefix = env::var("ERPX_TENANT_PROJECTION_CLAIM_KEY_PREFIX")
                    .unwrap_or_else(|_| "tenant-projection".into());
                match env::var("ERPX_TENANT_PROJECTION_KIND")
                    .unwrap_or_else(|_| "operations".into())
                    .as_str()
                {
                    "asset_maintenance" => {
                        let executor = ProductProjectionExecutor::AssetMaintenance(
                            AssetMaintenanceProjectionRunner::new(router),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "quality" => {
                        let registration = QualityProjectionFactory.registration();
                        tracing::info!(
                            projection = %registration.name,
                            event_types = ?registration.event_types,
                            "registered typed Quality CQRS projector"
                        );
                        let executor = ProductProjectionExecutor::QualityCqrs(
                            QualityCqrsProjectionExecutor::new(router),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    kind @ ("marketing_lead_candidates"
                    | "crm_leads"
                    | "crm_opportunities"
                    | "procurement_purchase_orders"
                    | "inventory_stock_items"
                    | "assets_cqrs"
                    | "maintenance_work_orders"
                    | "maintenance_asset_facts") => {
                        let kind = match kind {
                            "marketing_lead_candidates" => {
                                Wave6ProjectionKind::MarketingLeadCandidates
                            }
                            "crm_leads" => Wave6ProjectionKind::CrmLeads,
                            "crm_opportunities" => Wave6ProjectionKind::CrmOpportunities,
                            "procurement_purchase_orders" => {
                                Wave6ProjectionKind::ProcurementPurchaseOrders
                            }
                            "inventory_stock_items" => Wave6ProjectionKind::InventoryStockItems,
                            "assets_cqrs" => Wave6ProjectionKind::Assets,
                            "maintenance_work_orders" => Wave6ProjectionKind::MaintenanceWorkOrders,
                            "maintenance_asset_facts" => Wave6ProjectionKind::MaintenanceAssetFacts,
                            _ => unreachable!("matched Wave 6 projection kind"),
                        };
                        let executor = ProductProjectionExecutor::Wave6(
                            Wave6ProjectionExecutor::new(router, kind),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    kind @ ("sales_quotes"
                    | "sales_orders"
                    | "people_payroll_employees"
                    | "people_payroll_leave_requests"
                    | "people_payroll_expense_claims"
                    | "people_payroll_payroll_runs"
                    | "pos_sales"
                    | "pos_refunds") => {
                        let kind = match kind {
                            "sales_quotes" => Wave7ProjectionKind::SalesQuotes,
                            "sales_orders" => Wave7ProjectionKind::SalesOrders,
                            "people_payroll_employees" => {
                                Wave7ProjectionKind::PeoplePayrollEmployees
                            }
                            "people_payroll_leave_requests" => {
                                Wave7ProjectionKind::PeoplePayrollLeaveRequests
                            }
                            "people_payroll_expense_claims" => {
                                Wave7ProjectionKind::PeoplePayrollExpenseClaims
                            }
                            "people_payroll_payroll_runs" => {
                                Wave7ProjectionKind::PeoplePayrollPayrollRuns
                            }
                            "pos_sales" => Wave7ProjectionKind::PosSales,
                            "pos_refunds" => Wave7ProjectionKind::PosRefunds,
                            _ => unreachable!("matched Wave 7 projection kind"),
                        };
                        let executor = ProductProjectionExecutor::Wave7(
                            Wave7ProjectionExecutor::new(router, kind),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    kind @ ("finance_charts"
                    | "finance_periods"
                    | "finance_journals"
                    | "finance_subledger"
                    | "finance_trial_balance"
                    | "finance_bank_transactions"
                    | "finance_statutory_tax"
                    | "legal_contracts"
                    | "legal_documents"
                    | "legal_access_history"
                    | "support_tickets"
                    | "support_incidents"
                    | "support_sessions"
                    | "support_capabilities") => {
                        let kind = match kind {
                            "finance_charts" => Wave8ProjectionKind::FinanceCharts,
                            "finance_periods" => Wave8ProjectionKind::FinancePeriods,
                            "finance_journals" => Wave8ProjectionKind::FinanceJournals,
                            "finance_subledger" => Wave8ProjectionKind::FinanceSubledger,
                            "finance_trial_balance" => Wave8ProjectionKind::FinanceTrialBalance,
                            "finance_bank_transactions" => {
                                Wave8ProjectionKind::FinanceBankTransactions
                            }
                            "finance_statutory_tax" => Wave8ProjectionKind::FinanceStatutoryTax,
                            "legal_contracts" => Wave8ProjectionKind::LegalContracts,
                            "legal_documents" => Wave8ProjectionKind::LegalDocuments,
                            "legal_access_history" => Wave8ProjectionKind::LegalAccessHistory,
                            "support_tickets" => Wave8ProjectionKind::SupportTickets,
                            "support_incidents" => Wave8ProjectionKind::SupportIncidents,
                            "support_sessions" => Wave8ProjectionKind::SupportSessions,
                            "support_capabilities" => Wave8ProjectionKind::SupportCapabilities,
                            _ => unreachable!("matched Wave 8 projection kind"),
                        };
                        let executor = ProductProjectionExecutor::Wave8(
                            Wave8ProjectionExecutor::new(router, kind),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    kind @ ("operations_exceptions"
                    | "operations_source_signals"
                    | "operations_work_queue_policies") => {
                        let kind = match kind {
                            "operations_exceptions" => Wave9ProjectionKind::Exceptions,
                            "operations_source_signals" => Wave9ProjectionKind::SourceSignals,
                            "operations_work_queue_policies" => {
                                Wave9ProjectionKind::WorkQueuePolicies
                            }
                            _ => unreachable!("matched Wave 9 projection kind"),
                        };
                        let executor = ProductProjectionExecutor::Wave9(
                            Wave9ProjectionExecutor::new(router, kind),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "identity_access_principals" => {
                        let executor = ProductProjectionExecutor::Wave4(
                            Wave4ProjectionExecutor::identity_access(
                                router,
                                IdentityAccessProjectionKind::Principals,
                            ),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "identity_access_roles" => {
                        let executor = ProductProjectionExecutor::Wave4(
                            Wave4ProjectionExecutor::identity_access(
                                router,
                                IdentityAccessProjectionKind::Roles,
                            ),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "organization" => {
                        let executor = ProductProjectionExecutor::Wave4(
                            Wave4ProjectionExecutor::organization(router),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "commercial_subscriptions" => {
                        let executor =
                            ProductProjectionExecutor::Wave4(Wave4ProjectionExecutor::commercial(
                                Arc::clone(runtime.store()),
                                router,
                                CommercialProjectionKind::Subscriptions,
                            ));
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "commercial_entitlements" => {
                        let executor =
                            ProductProjectionExecutor::Wave4(Wave4ProjectionExecutor::commercial(
                                Arc::clone(runtime.store()),
                                router,
                                CommercialProjectionKind::Entitlements,
                            ));
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "legacy_legal_documents" => {
                        let executor =
                            ProductProjectionExecutor::Legal(LegalProjectionRunner::new(router));
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "operations" => {
                        let page_size = match env::var("ERPX_OPERATIONS_PROJECTION_PAGE_SIZE") {
                            Ok(value) => value.parse::<u32>().map_err(
                                |_| "ERPX_OPERATIONS_PROJECTION_PAGE_SIZE must be an integer",
                            )?,
                            Err(_) => erpx_infrastructure::MAX_OPERATIONS_EVENT_PAGE_SIZE,
                        };
                        let executor = OperationsTenantProjectionExecutor::new(page_size)?;
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler(
                                router,
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "finance_banking" => {
                        let page_size = match env::var("ERPX_FINANCE_BANKING_PROJECTION_PAGE_SIZE")
                        {
                            Ok(value) => value.parse::<u32>().map_err(
                                |_| "ERPX_FINANCE_BANKING_PROJECTION_PAGE_SIZE must be an integer",
                            )?,
                            Err(_) => {
                                erpx_infrastructure::MAX_FINANCE_BANKING_SOURCE_EVENT_PAGE_SIZE
                            }
                        };
                        let executor = FinanceBankingTenantProjectionExecutor::new(page_size)?;
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler(
                                router,
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "finance_chart" => {
                        let page_size = match env::var("ERPX_FINANCE_CHART_PROJECTION_PAGE_SIZE") {
                            Ok(value) => value.parse::<u32>().map_err(
                                |_| "ERPX_FINANCE_CHART_PROJECTION_PAGE_SIZE must be an integer",
                            )?,
                            Err(_) => erpx_worker::FINANCE_CHART_PROJECTION_MAX_PAGE_SIZE,
                        };
                        let executor =
                            FinanceChartTenantProjectionExecutor::new(router, page_size)?;
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "legacy_finance_subledger" => {
                        let page_size = match env::var("ERPX_FINANCE_SUBLEDGER_PROJECTION_PAGE_SIZE")
                        {
                            Ok(value) => value.parse::<u32>().map_err(
                                |_| "ERPX_FINANCE_SUBLEDGER_PROJECTION_PAGE_SIZE must be an integer",
                            )?,
                            Err(_) => erpx_worker::FINANCE_SUBLEDGER_PROJECTION_MAX_PAGE_SIZE,
                        };
                        let scheduler_router = router.clone();
                        let executor =
                            FinanceSubledgerTenantProjectionExecutor::new(router, page_size)?;
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_unfenced_rebuilds(
                                scheduler_router,
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "finance_close" => {
                        let page_size = match env::var("ERPX_FINANCE_CLOSE_PROJECTION_PAGE_SIZE") {
                            Ok(value) => value.parse::<u32>().map_err(
                                |_| "ERPX_FINANCE_CLOSE_PROJECTION_PAGE_SIZE must be an integer",
                            )?,
                            Err(_) => erpx_worker::FINANCE_CLOSE_PROJECTION_MAX_PAGE_SIZE,
                        };
                        let scheduler_router = router.clone();
                        let executor =
                            FinanceCloseTenantProjectionExecutor::new(router, page_size)?;
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_unfenced_rebuilds(
                                scheduler_router,
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "finance_thai_tax" => {
                        let page_size = match env::var("ERPX_FINANCE_THAI_TAX_PROJECTION_PAGE_SIZE")
                        {
                            Ok(value) => value.parse::<u32>().map_err(
                                |_| "ERPX_FINANCE_THAI_TAX_PROJECTION_PAGE_SIZE must be an integer",
                            )?,
                            Err(_) => erpx_worker::FINANCE_THAI_TAX_PROJECTION_MAX_PAGE_SIZE,
                        };
                        let scheduler_router = router.clone();
                        let executor =
                            FinanceThaiTaxTenantProjectionExecutor::new(router, page_size)
                                .map_err(
                                    |_| "Finance Thai tax projection configuration is invalid",
                                )?;
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_unfenced_rebuilds(
                                scheduler_router,
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "scm" => {
                        let executor =
                            ProductProjectionExecutor::Scm(ScmProjectionRunner::new(router));
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "pos" => {
                        let executor =
                            ProductProjectionExecutor::Pos(PosProjectionRunner::new(router));
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "hr" => {
                        let executor =
                            ProductProjectionExecutor::Hr(HrProjectionRunner::new(router));
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "sales" => {
                        let executor =
                            ProductProjectionExecutor::Sales(SalesProjectionRunner::new(router));
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "marketing" => {
                        let page_size = match env::var("ERPX_MARKETING_PROJECTION_PAGE_SIZE") {
                            Ok(value) => value.parse::<u32>().map_err(
                                |_| "ERPX_MARKETING_PROJECTION_PAGE_SIZE must be an integer",
                            )?,
                            Err(_) => erpx_worker::MARKETING_PROJECTION_MAX_PAGE_SIZE,
                        };
                        let executor = MarketingTenantProjectionExecutor::new(
                            router,
                            SqlxMarketingProjectionFactory,
                            page_size,
                        )
                        .map_err(|_| "ERPX_MARKETING_PROJECTION_PAGE_SIZE is outside its bound")?;
                        runtime
                            .handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    _ => {
                        return Err("ERPX_TENANT_PROJECTION_KIND must be one of the registered legacy or CQRS projection scheduler names (Waves 3-9)".into());
                    }
                }
            } else {
                runtime.handler_registry()?
            };
            let report = runtime
                .run_registered_until_shutdown(&registry, &handler_name)
                .await?;
            tracing::info!(
                handler = handler_name,
                passes = report.passes,
                claimed = report.claimed,
                processed = report.processed,
                published = report.published,
                retried = report.retried,
                dead_lettered = report.dead_lettered,
                "worker stopped"
            );
        }
        Some(command) => {
            return Err(format!("unsupported worker command {command}; use `run`").into());
        }
        None => {
            return Err("worker command is required; use `run`".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod marketing_delivery_runtime_tests {
    use super::MarketingDeliveryMultiTenantRuntimeHandler;
    use chrono::Duration;

    #[test]
    fn marketing_delivery_runtime_bounds_are_fail_closed() {
        assert!(
            MarketingDeliveryMultiTenantRuntimeHandler::validate_config(1, Duration::seconds(1))
                .is_ok()
        );
        assert!(
            MarketingDeliveryMultiTenantRuntimeHandler::validate_config(0, Duration::seconds(1))
                .is_err()
        );
        assert!(
            MarketingDeliveryMultiTenantRuntimeHandler::validate_config(1, Duration::zero())
                .is_err()
        );
    }
}

#[cfg(test)]
mod product_projection_registration_tests {
    use super::{
        TenantProjectionExecutorError, projection_hr_cursor, projection_page_size,
        projection_pos_cursor,
    };
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn product_projection_page_size_is_bounded() {
        assert_eq!(projection_page_size(&json!({}), 500).unwrap(), 500);
        assert_eq!(
            projection_page_size(&json!({"page_size": 37}), 500).unwrap(),
            37
        );
        assert!(matches!(
            projection_page_size(&json!({"page_size": 0}), 500),
            Err(TenantProjectionExecutorError::Permanent)
        ));
        assert!(matches!(
            projection_page_size(&json!({"page_size": 501}), 500),
            Err(TenantProjectionExecutorError::Permanent)
        ));
    }

    #[test]
    fn product_projection_pos_cursor_requires_immutable_position() {
        let aggregate_id = Uuid::new_v4();
        let cursor = json!({
            "cursor": {
                "aggregate_id": aggregate_id.to_string(),
                "aggregate_version": 12
            }
        });
        let parsed = projection_pos_cursor(&cursor).unwrap().unwrap();
        assert_eq!(parsed.aggregate_id, aggregate_id);
        assert_eq!(parsed.aggregate_version, 12);
        assert!(projection_pos_cursor(&json!({"cursor": {}})).is_err());
        assert!(projection_pos_cursor(&json!({"cursor": "not-an-object"})).is_err());
    }

    #[test]
    fn product_projection_hr_cursor_requires_aggregate_type() {
        let aggregate_id = Uuid::new_v4();
        let cursor = json!({
            "cursor": {
                "aggregate_type": "employee",
                "aggregate_id": aggregate_id.to_string(),
                "aggregate_version": 3
            }
        });
        let parsed = projection_hr_cursor(&cursor).unwrap().unwrap();
        assert_eq!(parsed.aggregate_type, "employee");
        assert_eq!(parsed.aggregate_id, aggregate_id);
        assert_eq!(parsed.aggregate_version, 3);
        assert!(
            projection_hr_cursor(&json!({
                "cursor": {
                    "aggregate_id": aggregate_id.to_string(),
                    "aggregate_version": 3
                }
            }))
            .is_err()
        );
    }

    #[test]
    fn product_projection_cursors_share_bounded_event_identity() {
        let aggregate_id = Uuid::new_v4();
        let payload = json!({
            "cursor": {
                "aggregate_type": "asset",
                "aggregate_id": aggregate_id.to_string(),
                "aggregate_version": 7
            }
        });
        let asset = super::projection_asset_maintenance_cursor(&payload)
            .unwrap()
            .unwrap();
        let legal = super::projection_legal_cursor(&payload).unwrap().unwrap();
        let sales = super::projection_sales_cursor(&payload).unwrap().unwrap();
        assert_eq!(asset.aggregate_id, aggregate_id);
        assert_eq!(legal.aggregate_version, 7);
        assert_eq!(sales.aggregate_type, "asset");
    }

    #[test]
    fn product_projection_wrapper_preserves_wave_specific_claim_filters() {
        let source = include_str!("app.rs");
        let filter = source
            .split("fn projection_name_filter(&self)")
            .nth(1)
            .expect("Product projection filter implementation");
        assert!(
            filter.contains("Self::QualityCqrs(_) => Some(erpx_quality_sqlx::PROJECTION_NAME)")
        );
        assert!(filter.contains("Self::Wave4(executor) => executor.projection_name_filter()"));
        assert!(filter.contains("Self::Wave6(executor) => executor.projection_name_filter()"));
        assert!(filter.contains("Self::Wave7(executor) => executor.projection_name_filter()"));
        assert!(filter.contains("Self::Wave8(executor) => executor.projection_name_filter()"));
        assert!(filter.contains("Self::Wave9(executor) => executor.projection_name_filter()"));
    }
}
