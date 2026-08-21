pub(crate) mod contexts {
    pub mod assets;
    pub mod brand;
    pub mod commercial;
    pub mod crm;
    pub mod customer_360;
    pub mod customer_authority;
    pub mod finance;
    pub mod global_collaboration;
    pub mod identity_access;
    pub mod inventory;
    pub mod legal;
    pub mod maintenance;
    pub mod marketing;
    pub mod marketing_approval;
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
mod current_completion_projection;
mod wave10_composition;
mod wave4_composition;
mod wave5_composition;
mod wave6_composition;
mod wave7_composition;
mod wave8_composition;
mod wave9_composition;

use std::{env, sync::Arc};

use chrono::{Duration, Utc};
use erpx_cqrs_sqlx::{ProjectionPool, ProjectionRunnerConfig, WritePool};
use erpx_domain::contracts::AuthorityScope;
use erpx_identity::{DirectoryRoleAccess, SqlxIdentityStore};
use erpx_infrastructure::{
    AssetMaintenanceSourceEventCursor, HoldingDatabaseProvisioningConfig, HoldingDatabaseRouter,
    MarketingDeliveryScheduleRequest, ProjectionClaimRecord, ProjectionJobKind,
    ProjectionSchedulerAuthorization, RoleReviewPersistence, RoutedDatabaseSide,
    SalesSourceEventCursor, SqlxControlPlane, SqlxHoldingStoreRegistry, SqlxMarketingContentStore,
    SqlxMarketingDeliverySchedulerRepository, SqlxMarketingDeliveryStore,
    SqlxMarketingJourneyExecutionStore, SupportWorkflowStore,
    UnconfiguredBankProviderCredentialResolver,
};
use erpx_worker::FutureEnterpriseProjectionKind;
use erpx_worker::{
    AGENT_ASSIGNMENT_HANDLER_NAME, AssetMaintenanceProjectionRequest,
    AssetMaintenanceProjectionRunner, BRANCH_PROVISIONING_HANDLER_NAME, BackupRuntime,
    BankProviderAdapterRegistry, BranchProvisioningRuntimeHandler,
    CLUSTER_PITR_CONTROLLER_HANDLER_NAME, ClusterPitrControllerRuntimeHandler,
    DIRECTORY_SUPERVISOR_HANDLER_NAME, DIRECTORY_SYNC_HANDLER_NAME,
    DirectorySupervisorRuntimeHandler, DirectorySyncRuntimeHandler,
    FINANCE_BANK_PROVIDER_IMPORT_HANDLER_NAME, FINANCE_SUBLEDGER_RECONCILIATION_MAX_ROWS,
    FinanceBankProviderImportAuthorizer, FinanceBankProviderImportError,
    FinanceBankingHoldingProjectionExecutor, FinanceChartHoldingProjectionExecutor,
    FinanceCloseHoldingProjectionExecutor, FinanceCompletionAuthorizationProof,
    FinanceCompletionRequest, FinanceCompletionRuntimeAdapter,
    FinanceSubledgerHoldingProjectionExecutor, FinanceSubledgerReconciliationRequest,
    FinanceSubledgerReconciliationRunner, FinanceThaiTaxHoldingProjectionExecutor,
    G13_TARGET_HANDLER_NAME, G13TargetRuntimeHandler, HOLDING_BACKUP_HANDLER_NAME,
    HOLDING_BACKUP_RETENTION_HANDLER_NAME, HOLDING_PROJECTION_SCHEDULER_HANDLER_NAME,
    HOLDING_PROJECTION_STATUS_PROJECTION_HANDLER_NAME, HR_COMPLETION_HANDLER_NAME,
    HoldingBackupJobRuntimeHandler, HoldingBackupRetentionRuntimeHandler,
    HoldingProjectionExecutor, HoldingProjectionExecutorError, HrCompletionRuntimeHandler,
    HrProjectionRequest, HrProjectionRunner, HrProjectionWorkerError,
    ITEM_DIRECTORY_PROJECTION_MAX_PAGE_SIZE, ItemDirectoryHoldingProjectionExecutor,
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
    MarketingHoldingProjectionExecutor, MarketingJourneyExecutionHandler,
    MarketingJourneyIntegrationMultiHoldingRuntimeHandler, MarketingPublicAbuseCleanupRunner,
    NamedWorkerHandler, PosProjectionRequest, PosProjectionRunner, PosProjectionWorkerError,
    SUPPORT_WORKFLOW_HANDLER_NAME, SalesProjectionRequest, SalesProjectionRunner,
    ScmProjectionPageRequest, ScmProjectionRebuildRequest, ScmProjectionRunner,
    ScmProjectionWorkerError, SqlxMarketingContentPreviewExpiryExecutor,
    SqlxMarketingJourneyExecutionDecisionSource, SqlxMarketingProjectionFactory,
    TrustedAgentHoldingOperationsConfig, WorkerHandlerReport, WorkerRuntime, WorkerRuntimeConfig,
    WorkerRuntimeError, compose_trusted_holding_agent_operations,
};
use erpx_worker::{
    ROLE_REVIEW_NOTIFICATION_HANDLER_NAME, ROLE_REVIEW_SNAPSHOT_HANDLER_NAME,
    RoleReviewNotificationPublishError, RoleReviewNotificationPublisher,
    RoleReviewSnapshotRuntimeHandler,
};
use uuid::Uuid;

use contexts::commercial::CommercialProjectionKind;
use contexts::marketing_approval::{
    MARKETING_APPROVAL_DECISIONS_HANDLER_NAME, MARKETING_APPROVAL_DECISIONS_MAX_ACTIVE_HOLDINGS,
    MARKETING_APPROVAL_DECISIONS_MAX_PAGE_SIZE, MarketingApprovalDecisionHandler,
};
use contexts::operations_source::{
    OPERATIONS_SOURCE_INGESTION_HANDLER_NAME, OPERATIONS_SOURCE_MAX_ACTIVE_HOLDINGS,
    OPERATIONS_SOURCE_MAX_PAGE_SIZE,
};
use contexts::organization::OrganizationProjectionKind;
use contexts::quality::{QualityProjectionFactory, QualityProjectionKind};
use current_completion_projection::{
    CurrentCompletionProjectionExecutor, CurrentCompletionProjectionKind,
};
use wave4_composition::{
    COMMERCIAL_CATALOG_PROJECTION_HANDLER_NAME, CommercialCatalogProjectionHandler,
    IdentityAccessProjectionKind, Wave4ProjectionExecutor,
};
use wave5_composition::{WAVE5_CONTROL_PROJECTION_HANDLER_NAME, Wave5ControlProjectionHandler};
use wave6_composition::{Wave6ProjectionExecutor, Wave6ProjectionKind};
use wave7_composition::{Wave7ProjectionExecutor, Wave7ProjectionKind};
use wave8_composition::{Wave8ProjectionExecutor, Wave8ProjectionKind};
use wave9_composition::{
    CUSTOMER360_CONTROL_PROJECTION_HANDLER_NAME, CUSTOMER360_HOLDING_PROJECTION_HANDLER_NAME,
    Customer360ProjectionHandler, Customer360ProjectionKind, Wave9ProjectionExecutor,
    Wave9ProjectionKind, operations_source_ingestion_handler,
};
use wave10_composition::Wave10ProjectionExecutor;

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
        account_scope: &AuthorityScope,
    ) -> Result<(), FinanceBankProviderImportError> {
        if !context.active_scope.contains(account_scope) {
            return Err(FinanceBankProviderImportError::AuthorizationDenied);
        }
        let requested_scope = finance_access_scope(account_scope);
        let directory_scope = requested_scope.canonical_key();
        let holding_scope = match requested_scope {
            AuthorityScope::Platform => None,
            AuthorityScope::Holding { holding_id }
            | AuthorityScope::Company { holding_id, .. }
            | AuthorityScope::Branch { holding_id, .. }
            | AuthorityScope::Subtree { holding_id, .. } => Some(format!("holding:{holding_id}")),
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

fn finance_access_scope(scope: &AuthorityScope) -> AuthorityScope {
    match scope {
        AuthorityScope::Platform => AuthorityScope::Platform,
        AuthorityScope::Holding { holding_id } => AuthorityScope::Holding {
            holding_id: *holding_id,
        },
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => AuthorityScope::Company {
            holding_id: *holding_id,
            company_id: *company_id,
        },
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => AuthorityScope::Branch {
            holding_id: *holding_id,
            company_id: *company_id,
            branch_id: *branch_id,
        },
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => AuthorityScope::Subtree {
            holding_id: *holding_id,
            organization_node_id: *organization_node_id,
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

fn role_review_scope_from_env() -> Result<AuthorityScope, Box<dyn std::error::Error>> {
    let value = required_env("ERPX_ROLE_REVIEW_SCOPE")?;
    let parts = value.split(':').collect::<Vec<_>>();
    let parse_id = |field: &str| {
        Uuid::parse_str(field).map_err(|_| format!("invalid role-review scope UUID: {field}"))
    };
    match parts.as_slice() {
        ["platform"] => Ok(AuthorityScope::Platform),
        ["holding", holding] => Ok(AuthorityScope::Holding {
            holding_id: parse_id(holding)?,
        }),
        ["company", holding, company] => Ok(AuthorityScope::Company {
            holding_id: parse_id(holding)?,
            company_id: parse_id(company)?,
        }),
        ["branch", holding, company, branch] => Ok(AuthorityScope::Branch {
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
    fn from_env(router: HoldingDatabaseRouter) -> Result<Self, Box<dyn std::error::Error>> {
        let holding_id = required_env("ERPX_FINANCE_COMPLETION_HOLDING_ID")?;
        let data_profile_key = required_env("ERPX_FINANCE_COMPLETION_DATA_PROFILE_KEY")?;
        let reconciliation_max_rows = env::var("ERPX_FINANCE_COMPLETION_MAX_ROWS")
            .unwrap_or_else(|_| "1000".into())
            .parse::<u32>()
            .map_err(|_| "ERPX_FINANCE_COMPLETION_MAX_ROWS must be an integer")?;
        let page_size = env::var("ERPX_FINANCE_COMPLETION_PAGE_SIZE")
            .unwrap_or_else(|_| "500".into())
            .parse::<u32>()
            .map_err(|_| "ERPX_FINANCE_COMPLETION_PAGE_SIZE must be an integer")?;
        let request = FinanceCompletionRequest::new(
            holding_id,
            data_profile_key,
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
    Wave10(Wave10ProjectionExecutor),
    Legal(LegalProjectionRunner),
    Scm(ScmProjectionRunner),
    Pos(PosProjectionRunner),
    Hr(HrProjectionRunner),
    Sales(SalesProjectionRunner),
}

#[derive(Debug, Clone)]
struct QualityCqrsProjectionExecutor {
    router: HoldingDatabaseRouter,
    kind: QualityProjectionKind,
}

impl QualityCqrsProjectionExecutor {
    const fn new(router: HoldingDatabaseRouter, kind: QualityProjectionKind) -> Self {
        Self { router, kind }
    }

    async fn runtime(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        page_size: u32,
    ) -> Result<contexts::quality::QualityProjectionRuntime, HoldingProjectionExecutorError> {
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
        let holding_id = cqrs_holding_uuid(holding_id)?;
        let runtime = QualityProjectionFactory
            .build(
                WritePool::for_holding(write.database_pool().clone(), holding_id)
                    .map_err(map_cqrs_projection_error)?,
                ProjectionPool::for_holding(read.database_pool().clone(), holding_id)
                    .map_err(map_cqrs_projection_error)?,
                ProjectionRunnerConfig {
                    page_size,
                    ..ProjectionRunnerConfig::default()
                },
            )
            .map_err(map_cqrs_projection_error)?;
        debug_assert_eq!(
            runtime.registrations(),
            QualityProjectionFactory.registrations()
        );
        // The runtime owns cloned pool handles for this bounded pass.
        drop(write);
        drop(read);
        Ok(runtime)
    }

    async fn project(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        page_size: u32,
    ) -> Result<(), HoldingProjectionExecutorError> {
        let run = self
            .runtime(holding_id, data_profile_key, page_size)
            .await?
            .run_once(self.kind)
            .await
            .map_err(map_cqrs_projection_error)?;
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
        let runtime = self
            .runtime(holding_id, data_profile_key, page_size)
            .await?;
        let handle = runtime
            .begin_rebuild(self.kind)
            .await
            .map_err(map_cqrs_projection_error)?;
        let mut cursor = 0;
        loop {
            let run = runtime
                .run_rebuild_page(self.kind, &handle, cursor)
                .await
                .map_err(map_cqrs_projection_error)?;
            if run.retry_scheduled || run.dead_lettered != 0 {
                return Err(HoldingProjectionExecutorError::Retryable);
            }
            cursor = run.cursor;
            if run.fetched < page_size {
                break;
            }
        }
        runtime
            .activate_rebuild(self.kind, &handle)
            .await
            .map_err(map_cqrs_projection_error)
    }
}

#[async_trait::async_trait]
impl HoldingProjectionExecutor for ProductProjectionExecutor {
    fn projection_name_filter(&self) -> Option<&str> {
        match self {
            Self::QualityCqrs(executor) => Some(match executor.kind {
                QualityProjectionKind::Inspections => erpx_quality_sqlx::PROJECTION_NAME,
                QualityProjectionKind::LifecycleRecords => {
                    erpx_quality_sqlx::LIFECYCLE_RECORD_PROJECTION_NAME
                }
            }),
            Self::Wave4(executor) => executor.projection_name_filter(),
            Self::Wave6(executor) => executor.projection_name_filter(),
            Self::Wave7(executor) => executor.projection_name_filter(),
            Self::Wave8(executor) => executor.projection_name_filter(),
            Self::Wave9(executor) => executor.projection_name_filter(),
            Self::Wave10(executor) => executor.projection_name_filter(),
            Self::AssetMaintenance(_) => Some("asset_maintenance"),
            Self::Legal(_) => Some(erpx_infrastructure::LEGAL_PROJECTION_NAME),
            Self::Scm(_) => Some("scm"),
            Self::Pos(_) => Some(erpx_infrastructure::POS_PROJECTION_NAME),
            Self::Hr(_) => Some(erpx_infrastructure::HR_PROJECTION_NAME),
            Self::Sales(_) => Some("sales"),
        }
    }

    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), HoldingProjectionExecutorError> {
        if self
            .projection_name_filter()
            .is_some_and(|expected| expected != claim.job.projection_name)
            || claim.job.payload.get("candidate_database_ref").is_some()
            || claim.job.payload.get("source_version").is_some()
            || claim.job.payload.get("generation").is_some()
        {
            return Err(HoldingProjectionExecutorError::Permanent);
        }
        let holding_id = claim.job.binding.holding_id.clone();
        let data_profile_key = claim.job.binding.data_profile_key.clone();
        let page_size = projection_page_size(&claim.job.payload, 500)?;
        match (self, claim.job.kind) {
            (Self::AssetMaintenance(runner), ProjectionJobKind::Incremental) => runner
                .project_page(
                    AssetMaintenanceProjectionRequest::new(
                        holding_id,
                        data_profile_key,
                        projection_asset_maintenance_cursor(&claim.job.payload)?,
                        page_size,
                    )
                    .map_err(|_| HoldingProjectionExecutorError::Permanent)?,
                )
                .await
                .map(|_| ())
                .map_err(map_worker_projection_error),
            (Self::AssetMaintenance(runner), ProjectionJobKind::Rebuild) => runner
                .rebuild(holding_id, data_profile_key, page_size)
                .await
                .map(|_| ())
                .map_err(map_worker_projection_error),
            (Self::QualityCqrs(runner), ProjectionJobKind::Incremental) => {
                if claim.job.payload.get("after").is_some()
                    || claim.job.payload.get("cursor").is_some()
                {
                    return Err(HoldingProjectionExecutorError::Permanent);
                }
                runner
                    .project(&holding_id, &data_profile_key, page_size)
                    .await
            }
            (Self::QualityCqrs(runner), ProjectionJobKind::Rebuild) => {
                runner
                    .rebuild(&holding_id, &data_profile_key, page_size)
                    .await
            }
            (Self::Wave4(runner), _) => runner.execute(claim).await,
            (Self::Wave6(runner), _) => runner.execute(claim).await,
            (Self::Wave7(runner), _) => runner.execute(claim).await,
            (Self::Wave8(runner), _) => runner.execute(claim).await,
            (Self::Wave9(runner), _) => runner.execute(claim).await,
            (Self::Wave10(runner), _) => runner.execute(claim).await,
            (Self::Legal(runner), ProjectionJobKind::Incremental) => runner
                .project_page(
                    LegalProjectionRequest::new(
                        holding_id,
                        data_profile_key,
                        projection_legal_cursor(&claim.job.payload)?,
                        page_size,
                    )
                    .map_err(|_| HoldingProjectionExecutorError::Permanent)?,
                )
                .await
                .map(|_| ())
                .map_err(map_legal_projection_error),
            (Self::Legal(runner), ProjectionJobKind::Rebuild) => runner
                .rebuild(holding_id, data_profile_key, page_size)
                .await
                .map(|_| ())
                .map_err(map_legal_projection_error),
            (Self::Scm(runner), ProjectionJobKind::Incremental) => runner
                .project_to_caught_up_and_reconcile(
                    ScmProjectionPageRequest::new(holding_id, data_profile_key, page_size)
                        .map_err(|_| HoldingProjectionExecutorError::Permanent)?,
                )
                .await
                .map(|_| ())
                .map_err(map_scm_projection_error),
            (Self::Scm(runner), ProjectionJobKind::Rebuild) => runner
                .rebuild(
                    ScmProjectionRebuildRequest::new(holding_id, data_profile_key, page_size)
                        .map_err(|_| HoldingProjectionExecutorError::Permanent)?,
                )
                .await
                .map(|_| ())
                .map_err(map_scm_projection_error),
            (Self::Pos(runner), ProjectionJobKind::Incremental) => runner
                .project_page(
                    PosProjectionRequest::new(
                        holding_id,
                        data_profile_key,
                        projection_pos_cursor(&claim.job.payload)?,
                        page_size,
                    )
                    .map_err(|_| HoldingProjectionExecutorError::Permanent)?,
                )
                .await
                .map(|_| ())
                .map_err(map_pos_projection_error),
            (Self::Pos(runner), ProjectionJobKind::Rebuild) => runner
                .rebuild(holding_id, data_profile_key, page_size)
                .await
                .map(|_| ())
                .map_err(map_pos_projection_error),
            (Self::Hr(runner), ProjectionJobKind::Incremental) => runner
                .project_page(
                    HrProjectionRequest::new(
                        holding_id,
                        data_profile_key,
                        projection_hr_cursor(&claim.job.payload)?,
                        page_size,
                    )
                    .map_err(|_| HoldingProjectionExecutorError::Permanent)?,
                )
                .await
                .map(|_| ())
                .map_err(map_hr_projection_error),
            (Self::Hr(runner), ProjectionJobKind::Rebuild) => runner
                .rebuild(holding_id, data_profile_key, page_size)
                .await
                .map(|_| ())
                .map_err(map_hr_projection_error),
            (Self::Sales(runner), ProjectionJobKind::Incremental) => runner
                .project_page(
                    SalesProjectionRequest::new(
                        holding_id,
                        data_profile_key,
                        projection_sales_cursor(&claim.job.payload)?,
                        page_size,
                    )
                    .map_err(|_| HoldingProjectionExecutorError::Permanent)?,
                )
                .await
                .map(|_| ())
                .map_err(map_worker_projection_error),
            (Self::Sales(runner), ProjectionJobKind::Rebuild) => runner
                .rebuild(holding_id, data_profile_key, page_size)
                .await
                .map(|_| ())
                .map_err(map_worker_projection_error),
        }
    }
}

pub(crate) fn projection_page_size(
    payload: &serde_json::Value,
    default: u32,
) -> Result<u32, HoldingProjectionExecutorError> {
    let Some(value) = payload.get("page_size") else {
        return Ok(default);
    };
    let Some(value) = value.as_u64().and_then(|value| u32::try_from(value).ok()) else {
        return Err(HoldingProjectionExecutorError::Permanent);
    };
    if value == 0 || value > 500 {
        return Err(HoldingProjectionExecutorError::Permanent);
    }
    Ok(value)
}

fn projection_cursor_object(
    payload: &serde_json::Value,
) -> Result<Option<&serde_json::Map<String, serde_json::Value>>, HoldingProjectionExecutorError> {
    let Some(value) = payload.get("cursor") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_object()
        .map(Some)
        .ok_or(HoldingProjectionExecutorError::Permanent)
}

fn projection_uuid(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<Uuid, HoldingProjectionExecutorError> {
    object
        .get(field)
        .and_then(serde_json::Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .filter(|value| !value.is_nil())
        .ok_or(HoldingProjectionExecutorError::Permanent)
}

fn projection_version(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<u64, HoldingProjectionExecutorError> {
    object
        .get("aggregate_version")
        .and_then(serde_json::Value::as_u64)
        .filter(|value| *value != 0)
        .ok_or(HoldingProjectionExecutorError::Permanent)
}

fn projection_pos_cursor(
    payload: &serde_json::Value,
) -> Result<Option<erpx_infrastructure::PosSourceEventCursor>, HoldingProjectionExecutorError> {
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
) -> Result<Option<erpx_infrastructure::HrSourceEventCursor>, HoldingProjectionExecutorError> {
    let Some(cursor) = projection_cursor_object(payload)? else {
        return Ok(None);
    };
    let aggregate_type = cursor
        .get("aggregate_type")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(HoldingProjectionExecutorError::Permanent)?
        .to_owned();
    Ok(Some(erpx_infrastructure::HrSourceEventCursor {
        aggregate_type,
        aggregate_id: projection_uuid(cursor, "aggregate_id")?,
        aggregate_version: projection_version(cursor)?,
    }))
}

fn projection_aggregate_type(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<String, HoldingProjectionExecutorError> {
    object
        .get("aggregate_type")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or(HoldingProjectionExecutorError::Permanent)
}

fn projection_asset_maintenance_cursor(
    payload: &serde_json::Value,
) -> Result<Option<AssetMaintenanceSourceEventCursor>, HoldingProjectionExecutorError> {
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
) -> Result<Option<erpx_infrastructure::LegalSourceEventCursor>, HoldingProjectionExecutorError> {
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
) -> Result<Option<SalesSourceEventCursor>, HoldingProjectionExecutorError> {
    let Some(cursor) = projection_cursor_object(payload)? else {
        return Ok(None);
    };
    Ok(Some(SalesSourceEventCursor {
        aggregate_type: projection_aggregate_type(cursor)?,
        aggregate_id: projection_uuid(cursor, "aggregate_id")?,
        aggregate_version: projection_version(cursor)?,
    }))
}

fn map_scm_projection_error(error: ScmProjectionWorkerError) -> HoldingProjectionExecutorError {
    match error {
        ScmProjectionWorkerError::Configuration(_) => HoldingProjectionExecutorError::Permanent,
        ScmProjectionWorkerError::Projection { .. } => HoldingProjectionExecutorError::Retryable,
    }
}

fn map_pos_projection_error(error: PosProjectionWorkerError) -> HoldingProjectionExecutorError {
    match error {
        PosProjectionWorkerError::Configuration(_) => HoldingProjectionExecutorError::Permanent,
        PosProjectionWorkerError::Projection { .. } => HoldingProjectionExecutorError::Retryable,
    }
}

fn map_hr_projection_error(error: HrProjectionWorkerError) -> HoldingProjectionExecutorError {
    match error {
        HrProjectionWorkerError::Configuration(_) => HoldingProjectionExecutorError::Permanent,
        HrProjectionWorkerError::Projection { .. } => HoldingProjectionExecutorError::Retryable,
    }
}

fn map_worker_projection_error(error: WorkerRuntimeError) -> HoldingProjectionExecutorError {
    match error {
        WorkerRuntimeError::Configuration(_) => HoldingProjectionExecutorError::Permanent,
        WorkerRuntimeError::Persistence(_)
        | WorkerRuntimeError::Database(_)
        | WorkerRuntimeError::Migration(_) => HoldingProjectionExecutorError::Retryable,
    }
}

pub(crate) fn map_cqrs_projection_error(
    error: erpx_cqrs_sqlx::CqrsSqlxError,
) -> HoldingProjectionExecutorError {
    match error {
        erpx_cqrs_sqlx::CqrsSqlxError::InvalidInput(_)
        | erpx_cqrs_sqlx::CqrsSqlxError::NumericRange
        | erpx_cqrs_sqlx::CqrsSqlxError::ProjectionPositionConflict
        | erpx_cqrs_sqlx::CqrsSqlxError::ProjectionOutOfOrder => {
            HoldingProjectionExecutorError::Permanent
        }
        _ => HoldingProjectionExecutorError::Retryable,
    }
}

pub(crate) fn cqrs_holding_uuid(holding_id: &str) -> Result<Uuid, HoldingProjectionExecutorError> {
    let holding_id =
        Uuid::parse_str(holding_id).map_err(|_| HoldingProjectionExecutorError::Permanent)?;
    if holding_id.is_nil() {
        return Err(HoldingProjectionExecutorError::Permanent);
    }
    Ok(holding_id)
}

fn map_legal_projection_error(error: LegalProjectionWorkerError) -> HoldingProjectionExecutorError {
    match error {
        LegalProjectionWorkerError::Configuration(_) => HoldingProjectionExecutorError::Permanent,
        LegalProjectionWorkerError::Projection { .. } => HoldingProjectionExecutorError::Retryable,
    }
}

/// The executable composition boundary for the holding-scoped marketing
/// preview expiry seam. The durable worker loop supplies restart-safe polling;
/// the injected SQLx executor supplies the holding route and idempotent write.
struct MarketingContentPreviewExpiryRuntimeHandler {
    handler: MarketingContentPreviewExpiryHandler<SqlxMarketingContentPreviewExpiryExecutor>,
    holding_id: String,
    data_profile_key: String,
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
    fn from_env(router: HoldingDatabaseRouter) -> Result<Self, Box<dyn std::error::Error>> {
        let holding_id = required_env("ERPX_FINANCE_RECONCILIATION_HOLDING_ID")?;
        let data_profile_key = required_env("ERPX_FINANCE_RECONCILIATION_DATA_PROFILE_KEY")?;
        let max_rows = match env::var("ERPX_FINANCE_RECONCILIATION_MAX_ROWS") {
            Ok(value) => value
                .parse::<u32>()
                .map_err(|_| "ERPX_FINANCE_RECONCILIATION_MAX_ROWS must be an integer")?,
            Err(_) => FINANCE_SUBLEDGER_RECONCILIATION_MAX_ROWS,
        };
        let request =
            FinanceSubledgerReconciliationRequest::new(holding_id, data_profile_key, max_rows)?;
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
                holding_id = %self.request.holding_id,
                data_profile_key = %self.request.data_profile_key,
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
        router: &HoldingDatabaseRouter,
        holding_key: String,
        data_profile_key: String,
        holding_id: Uuid,
        actor_ref: String,
        batch_size: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let write = router
            .route(&holding_key, &data_profile_key, RoutedDatabaseSide::Write)
            .await?;
        let read = router
            .route(&holding_key, &data_profile_key, RoutedDatabaseSide::Read)
            .await?;
        let store = SqlxMarketingContentStore::new(
            write.database_pool().clone(),
            read.database_pool().clone(),
            data_profile_key.clone(),
        )?;
        // Do not close either routed handle here: the store owns cloned pools
        // for the lifetime of the registered handler. Dropping the handles
        // releases only the route wrappers and preserves the pool ownership.
        drop(write);
        drop(read);

        let scope = AuthorityScope::Holding { holding_id };
        let executor = SqlxMarketingContentPreviewExpiryExecutor::new(
            store,
            holding_key.clone(),
            scope,
            actor_ref,
        )?;
        Ok(Self {
            handler: MarketingContentPreviewExpiryHandler::new(executor),
            holding_id: holding_key,
            data_profile_key,
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
            self.holding_id.clone(),
            self.data_profile_key.clone(),
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
                holding_id = %self.holding_id,
                data_profile_key = %self.data_profile_key,
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

/// Durable Marketing delivery composition over every active holding in the
/// control-plane registry. Each holding/account still gets its own holding
/// write/read pools and account-bound provider; no holding credentials or
/// provider account set is inferred from environment configuration.
struct MarketingDeliveryMultiHoldingRuntimeHandler {
    control: Arc<SqlxControlPlane>,
    registry: SqlxHoldingStoreRegistry,
    router: HoldingDatabaseRouter,
    credentials: MarketingDeliverySecretManagerResolver,
    batch_size: u32,
    lease: Duration,
}

impl MarketingDeliveryMultiHoldingRuntimeHandler {
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
        registry: SqlxHoldingStoreRegistry,
        router: HoldingDatabaseRouter,
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
impl NamedWorkerHandler for MarketingDeliveryMultiHoldingRuntimeHandler {
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
                    &registration.holding_id,
                    &registration.data_profile_key,
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
                        &registration.holding_id,
                        &registration.data_profile_key,
                        RoutedDatabaseSide::Write,
                    )
                    .await
                    .map_err(|_| {
                        WorkerRuntimeError::Configuration("Marketing delivery worker failed".into())
                    })?;
                let read = match self
                    .router
                    .route(
                        &registration.holding_id,
                        &registration.data_profile_key,
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
                    registration.data_profile_key.clone(),
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
                    registration.holding_id.clone(),
                    registration.data_profile_key.clone(),
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
            let registry = if handler_name == AGENT_ASSIGNMENT_HANDLER_NAME {
                let routing = HoldingDatabaseProvisioningConfig::from_env()?;
                let router = HoldingDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let runner = compose_trusted_holding_agent_operations(
                    SqlxHoldingStoreRegistry::new(Arc::clone(runtime.store())),
                    router,
                    TrustedAgentHoldingOperationsConfig::try_new(
                        std::time::Duration::from_secs(60),
                        100,
                    )?,
                )?;
                runtime.handler_registry_with_agent_assignment(runner)?
            } else if handler_name == HOLDING_BACKUP_RETENTION_HANDLER_NAME {
                let batch_size = runtime
                    .config()
                    .batch_size
                    .min(erpx_infrastructure::MAX_HOLDING_BACKUP_RETENTION_PAGE_SIZE);
                let handler = match BackupRuntime::from_env() {
                    Ok(backup_runtime) => {
                        HoldingBackupRetentionRuntimeHandler::with_backup_adapter(
                            Arc::clone(runtime.store()),
                            Arc::new(backup_runtime.adapter().clone()),
                            batch_size,
                            runtime.config().claim_timeout,
                        )?
                    }
                    Err(error) => {
                        tracing::warn!(
                            blocker = "backup_external_credentials_unavailable",
                            "holding backup retention certification blocker queued; retention worker will fail closed until the external adapter is configured"
                        );
                        tracing::debug!(?error, "holding backup retention adapter is disabled");
                        HoldingBackupRetentionRuntimeHandler::new(
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
                    | CUSTOMER360_HOLDING_PROJECTION_HANDLER_NAME
            ) {
                let kind = match handler_name.as_str() {
                    CUSTOMER360_CONTROL_PROJECTION_HANDLER_NAME => {
                        Customer360ProjectionKind::ControlCustomers
                    }
                    CUSTOMER360_HOLDING_PROJECTION_HANDLER_NAME => {
                        Customer360ProjectionKind::HoldingActivity
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
                    OPERATIONS_SOURCE_MAX_ACTIVE_HOLDINGS,
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(handler)?;
                registry
            } else if handler_name == MARKETING_APPROVAL_DECISIONS_HANDLER_NAME {
                let routing = HoldingDatabaseProvisioningConfig::from_env()?;
                let router = HoldingDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let handler = MarketingApprovalDecisionHandler::new(
                    SqlxHoldingStoreRegistry::new(Arc::clone(runtime.store())),
                    router,
                    runtime
                        .config()
                        .batch_size
                        .min(MARKETING_APPROVAL_DECISIONS_MAX_PAGE_SIZE),
                    MARKETING_APPROVAL_DECISIONS_MAX_ACTIVE_HOLDINGS,
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
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
            } else if handler_name == HR_COMPLETION_HANDLER_NAME {
                let routing = HoldingDatabaseProvisioningConfig::from_env()?;
                let router = HoldingDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let handler = HrCompletionRuntimeHandler::from_env(router)
                    .map_err(|_| "HR completion runtime configuration is invalid")?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == FINANCE_COMPLETION_HANDLER_NAME {
                let routing = HoldingDatabaseProvisioningConfig::from_env()?;
                let router = HoldingDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let handler = FinanceCompletionRuntimeHandler::from_env(router)?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == FINANCE_SUBLEDGER_RECONCILIATION_HANDLER_NAME {
                let routing = HoldingDatabaseProvisioningConfig::from_env()?;
                let router = HoldingDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let handler = FinanceSubledgerReconciliationRuntimeHandler::from_env(router)?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == FINANCE_BANK_PROVIDER_IMPORT_HANDLER_NAME {
                let routing = HoldingDatabaseProvisioningConfig::from_env()?;
                let router = HoldingDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
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
            } else if handler_name == HOLDING_BACKUP_HANDLER_NAME {
                let handler = HoldingBackupJobRuntimeHandler::from_env(
                    Arc::clone(runtime.store()),
                    runtime
                        .config()
                        .batch_size
                        .min(erpx_infrastructure::MAX_HOLDING_BACKUP_JOB_PAGE_SIZE),
                    runtime.config().claim_timeout,
                )?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == HOLDING_PROJECTION_STATUS_PROJECTION_HANDLER_NAME {
                runtime
                    .handler_registry_with_control_plane_holding_projection_status_projection()?
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
                let routing = HoldingDatabaseProvisioningConfig::from_env()?;
                let router = HoldingDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let credential_resolver = MarketingDeliverySecretManagerResolver::from_env()?;
                let inner = MarketingDeliveryMultiHoldingRuntimeHandler::new(
                    Arc::clone(runtime.store()),
                    SqlxHoldingStoreRegistry::new(Arc::clone(runtime.store())),
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
                let routing = HoldingDatabaseProvisioningConfig::from_env()?;
                let router = HoldingDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let holding_id = env::var("ERPX_MARKETING_JOURNEY_HOLDING_ID")
                    .map_err(|_| "ERPX_MARKETING_JOURNEY_HOLDING_ID is required")?;
                let data_profile_key = env::var("ERPX_MARKETING_JOURNEY_DATA_PROFILE_KEY")
                    .map_err(|_| "ERPX_MARKETING_JOURNEY_DATA_PROFILE_KEY is required")?;
                let routed = router
                    .route(&holding_id, &data_profile_key, RoutedDatabaseSide::Write)
                    .await?;
                let write_pool = routed.database_pool().clone();
                let store = SqlxMarketingJourneyExecutionStore::new_with_control(
                    write_pool,
                    data_profile_key,
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
                let routing = HoldingDatabaseProvisioningConfig::from_env()?;
                let router = HoldingDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let credentials = MarketingDeliverySecretManagerResolver::from_env()?;
                let handler = MarketingJourneyIntegrationMultiHoldingRuntimeHandler::new(
                    Arc::clone(runtime.store()),
                    SqlxHoldingStoreRegistry::new(Arc::clone(runtime.store())),
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
                let routing = HoldingDatabaseProvisioningConfig::from_env()?;
                let router = HoldingDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let holding_key = required_env("ERPX_MARKETING_CONTENT_PREVIEW_EXPIRY_HOLDING_ID")?;
                let data_profile_key =
                    required_env("ERPX_MARKETING_CONTENT_PREVIEW_EXPIRY_DATA_PROFILE_KEY")?;
                let holding_id_ref = env::var("ERPX_MARKETING_CONTENT_PREVIEW_EXPIRY_HOLDING_ID")
                    .unwrap_or_else(|_| data_profile_key.clone());
                let holding_id = Uuid::parse_str(&holding_id_ref).map_err(|_| {
                    "ERPX_MARKETING_CONTENT_PREVIEW_EXPIRY_HOLDING_ID (or DATA_PROFILE_KEY fallback) must be a UUID"
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
                    holding_key,
                    data_profile_key,
                    holding_id,
                    actor_ref,
                    batch_size,
                )
                .await?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(handler))?;
                registry
            } else if handler_name == MARKETING_CONTENT_PRIVACY_HANDLER_NAME {
                let routing = HoldingDatabaseProvisioningConfig::from_env()?;
                let router = HoldingDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let holding_key = required_env("ERPX_MARKETING_CONTENT_PRIVACY_HOLDING_ID")?;
                let data_profile_key =
                    required_env("ERPX_MARKETING_CONTENT_PRIVACY_DATA_PROFILE_KEY")?;
                let holding_id_ref = env::var("ERPX_MARKETING_CONTENT_PRIVACY_HOLDING_ID")
                    .unwrap_or_else(|_| data_profile_key.clone());
                let holding_id = Uuid::parse_str(&holding_id_ref).map_err(|_| {
                    "ERPX_MARKETING_CONTENT_PRIVACY_HOLDING_ID (or DATA_PROFILE_KEY fallback) must be a UUID"
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
                    holding_key,
                    data_profile_key,
                    holding_id,
                    batch_size,
                    context,
                )?;
                let runner = MarketingContentPrivacyRunner::new(router, request)?;
                let mut registry = runtime.handler_registry()?;
                registry.register(Arc::new(runner))?;
                registry
            } else if handler_name == LEGAL_RETENTION_HANDLER_NAME {
                let routing = HoldingDatabaseProvisioningConfig::from_env()?;
                let router = HoldingDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let holding_id = env::var("ERPX_LEGAL_RETENTION_HOLDING_ID")
                    .map_err(|_| "ERPX_LEGAL_RETENTION_HOLDING_ID is required")?;
                let data_profile_key = env::var("ERPX_LEGAL_RETENTION_DATA_PROFILE_KEY")
                    .map_err(|_| "ERPX_LEGAL_RETENTION_DATA_PROFILE_KEY is required")?;
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
                    LegalRetentionRequest::new(holding_id, data_profile_key, batch_size, context)?;
                let runner = LegalRetentionRunner::new(router, request)?;
                runtime.handler_registry_with_legal_retention(runner)?
            } else if handler_name == HOLDING_PROJECTION_SCHEDULER_HANDLER_NAME {
                let routing = HoldingDatabaseProvisioningConfig::from_env()?;
                let router = HoldingDatabaseRouter::new(Arc::clone(runtime.store()), routing)?;
                let actor_ref = env::var("ERPX_WORKER_ACTOR_REF")
                    .unwrap_or_else(|_| format!("erpx-worker:{}", runtime.config().worker_id));
                let purpose = env::var("ERPX_WORKER_PROJECTION_PURPOSE")
                    .unwrap_or_else(|_| "durable holding projection".into());
                let correlation_id = match env::var("ERPX_WORKER_PROJECTION_CORRELATION_ID") {
                    Ok(value) => Uuid::parse_str(&value)
                        .map_err(|_| "ERPX_WORKER_PROJECTION_CORRELATION_ID must be a UUID")?,
                    Err(_) => Uuid::new_v4(),
                };
                let authorization =
                    ProjectionSchedulerAuthorization::new(actor_ref, purpose, correlation_id)?;
                let claim_key_prefix = env::var("ERPX_HOLDING_PROJECTION_CLAIM_KEY_PREFIX")
                    .unwrap_or_else(|_| "holding-projection".into());
                match env::var("ERPX_HOLDING_PROJECTION_KIND")
                    .unwrap_or_else(|_| "operations".into())
                    .as_str()
                {
                    "asset_maintenance" => {
                        let executor = ProductProjectionExecutor::AssetMaintenance(
                            AssetMaintenanceProjectionRunner::new(router),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    kind @ ("quality" | "quality_lifecycle_records") => {
                        let kind = match kind {
                            "quality" => QualityProjectionKind::Inspections,
                            "quality_lifecycle_records" => QualityProjectionKind::LifecycleRecords,
                            _ => unreachable!("matched Quality projection kind"),
                        };
                        let registrations = QualityProjectionFactory.registrations();
                        tracing::info!(
                            projections = ?registrations,
                            "registered typed Quality CQRS projector"
                        );
                        let executor = ProductProjectionExecutor::QualityCqrs(
                            QualityCqrsProjectionExecutor::new(router, kind),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    kind @ ("marketing_lead_candidates"
                    | "marketing_campaign_budgets"
                    | "marketing_business_records"
                    | "crm_leads"
                    | "crm_opportunities"
                    | "crm_business_records"
                    | "procurement_purchase_orders"
                    | "procurement_workflow_records"
                    | "procurement_business_records"
                    | "inventory_stock_items"
                    | "inventory_lifecycle_records"
                    | "assets_cqrs"
                    | "asset_lifecycle_records"
                    | "maintenance_work_orders"
                    | "maintenance_lifecycle_records"
                    | "maintenance_asset_facts") => {
                        let kind = match kind {
                            "marketing_lead_candidates" => {
                                Wave6ProjectionKind::MarketingLeadCandidates
                            }
                            "marketing_campaign_budgets" => {
                                Wave6ProjectionKind::MarketingCampaignBudgets
                            }
                            "marketing_business_records" => {
                                Wave6ProjectionKind::MarketingBusinessRecords
                            }
                            "crm_leads" => Wave6ProjectionKind::CrmLeads,
                            "crm_opportunities" => Wave6ProjectionKind::CrmOpportunities,
                            "crm_business_records" => Wave6ProjectionKind::CrmBusinessRecords,
                            "procurement_purchase_orders" => {
                                Wave6ProjectionKind::ProcurementPurchaseOrders
                            }
                            "procurement_workflow_records" => {
                                Wave6ProjectionKind::ProcurementWorkflowRecords
                            }
                            "procurement_business_records" => {
                                Wave6ProjectionKind::ProcurementBusinessRecords
                            }
                            "inventory_stock_items" => Wave6ProjectionKind::InventoryStockItems,
                            "inventory_lifecycle_records" => {
                                Wave6ProjectionKind::InventoryLifecycleRecords
                            }
                            "assets_cqrs" => Wave6ProjectionKind::Assets,
                            "asset_lifecycle_records" => Wave6ProjectionKind::AssetLifecycleRecords,
                            "maintenance_work_orders" => Wave6ProjectionKind::MaintenanceWorkOrders,
                            "maintenance_lifecycle_records" => {
                                Wave6ProjectionKind::MaintenanceLifecycleRecords
                            }
                            "maintenance_asset_facts" => Wave6ProjectionKind::MaintenanceAssetFacts,
                            _ => unreachable!("matched Wave 6 projection kind"),
                        };
                        let executor = ProductProjectionExecutor::Wave6(
                            Wave6ProjectionExecutor::new(router, kind),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    kind @ ("sales_quotes"
                    | "sales_orders"
                    | "sales_revenue_arrangements"
                    | "sales_lifecycle_records"
                    | "people_payroll_employees"
                    | "people_payroll_leave_requests"
                    | "people_payroll_expense_claims"
                    | "people_payroll_payroll_runs"
                    | "pos_sales"
                    | "pos_refunds"
                    | "pos_lifecycle_records") => {
                        let kind = match kind {
                            "sales_quotes" => Wave7ProjectionKind::SalesQuotes,
                            "sales_orders" => Wave7ProjectionKind::SalesOrders,
                            "sales_revenue_arrangements" => {
                                Wave7ProjectionKind::SalesRevenueArrangements
                            }
                            "sales_lifecycle_records" => Wave7ProjectionKind::SalesLifecycleRecords,
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
                            "pos_lifecycle_records" => Wave7ProjectionKind::PosLifecycleRecords,
                            _ => unreachable!("matched Wave 7 projection kind"),
                        };
                        let executor = ProductProjectionExecutor::Wave7(
                            Wave7ProjectionExecutor::new(router, kind),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    kind @ ("customer_authority_business_records"
                    | "customer360_owned_records"
                    | "support_platform_cases"
                    | "people_payroll_time_records"
                    | "legal_cases"
                    | "shared_signatures") => {
                        let kind = match kind {
                            "customer_authority_business_records" => {
                                CurrentCompletionProjectionKind::CustomerAuthorityRecords
                            }
                            "customer360_owned_records" => {
                                CurrentCompletionProjectionKind::Customer360OwnedRecords
                            }
                            "support_platform_cases" => {
                                CurrentCompletionProjectionKind::SupportPlatformCases
                            }
                            "people_payroll_time_records" => {
                                CurrentCompletionProjectionKind::PeoplePayrollTimeRecords
                            }
                            "legal_cases" => CurrentCompletionProjectionKind::LegalCases,
                            "shared_signatures" => {
                                CurrentCompletionProjectionKind::SharedSignatures
                            }
                            _ => unreachable!("matched current completion projection kind"),
                        };
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                CurrentCompletionProjectionExecutor::new(router, kind),
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    kind @ ("finance_charts"
                    | "finance_periods"
                    | "finance_journals"
                    | "finance_subledger"
                    | "finance_customer_invoices"
                    | "finance_revenue_schedules"
                    | "finance_trial_balance"
                    | "finance_bank_transactions"
                    | "finance_statutory_tax"
                    | "finance_vendor_invoices"
                    | "finance_payments"
                    | "finance_business_records"
                    | "legal_contracts"
                    | "legal_documents"
                    | "legal_access_history"
                    | "legal_matters"
                    | "support_tickets"
                    | "support_incidents"
                    | "support_sessions"
                    | "support_capabilities") => {
                        let kind = match kind {
                            "finance_charts" => Wave8ProjectionKind::FinanceCharts,
                            "finance_periods" => Wave8ProjectionKind::FinancePeriods,
                            "finance_journals" => Wave8ProjectionKind::FinanceJournals,
                            "finance_subledger" => Wave8ProjectionKind::FinanceSubledger,
                            "finance_customer_invoices" => {
                                Wave8ProjectionKind::FinanceCustomerInvoices
                            }
                            "finance_revenue_schedules" => {
                                Wave8ProjectionKind::FinanceRevenueSchedules
                            }
                            "finance_trial_balance" => Wave8ProjectionKind::FinanceTrialBalance,
                            "finance_bank_transactions" => {
                                Wave8ProjectionKind::FinanceBankTransactions
                            }
                            "finance_statutory_tax" => Wave8ProjectionKind::FinanceStatutoryTax,
                            "finance_vendor_invoices" => Wave8ProjectionKind::FinanceVendorInvoices,
                            "finance_payments" => Wave8ProjectionKind::FinancePayments,
                            "finance_business_records" => {
                                Wave8ProjectionKind::FinanceBusinessRecords
                            }
                            "legal_contracts" => Wave8ProjectionKind::LegalContracts,
                            "legal_documents" => Wave8ProjectionKind::LegalDocuments,
                            "legal_access_history" => Wave8ProjectionKind::LegalAccessHistory,
                            "legal_matters" => Wave8ProjectionKind::LegalMatters,
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
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    kind @ ("operations_exceptions"
                    | "operations_source_signals"
                    | "operations_work_queue_policies"
                    | "operations_lifecycle_records") => {
                        let kind = match kind {
                            "operations_exceptions" => Wave9ProjectionKind::Exceptions,
                            "operations_source_signals" => Wave9ProjectionKind::SourceSignals,
                            "operations_work_queue_policies" => {
                                Wave9ProjectionKind::WorkQueuePolicies
                            }
                            "operations_lifecycle_records" => Wave9ProjectionKind::LifecycleRecords,
                            _ => unreachable!("matched Wave 9 projection kind"),
                        };
                        let executor = ProductProjectionExecutor::Wave9(
                            Wave9ProjectionExecutor::new(router, kind),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    kind @ ("manufacturing_orders"
                    | "planning_runs"
                    | "ecommerce_channels"
                    | "ecommerce_listings"
                    | "ecommerce_journeys"
                    | "talent_candidate"
                    | "talent_workforce"
                    | "learning_enrollments"
                    | "benefits_enrollments"
                    | "field_service_orders"
                    | "projects_projects"
                    | "projects_hierarchy"
                    | "projects_records"
                    | "collaboration_threads"
                    | "collaboration_stream"
                    | "collaboration_inbox") => {
                        let kind = match kind {
                            "manufacturing_orders" => FutureEnterpriseProjectionKind::Manufacturing,
                            "planning_runs" => FutureEnterpriseProjectionKind::Planning,
                            "ecommerce_channels" => {
                                FutureEnterpriseProjectionKind::EcommerceChannels
                            }
                            "ecommerce_listings" => {
                                FutureEnterpriseProjectionKind::EcommerceListings
                            }
                            "ecommerce_journeys" => {
                                FutureEnterpriseProjectionKind::EcommerceJourneys
                            }
                            "talent_candidate" => FutureEnterpriseProjectionKind::TalentCandidate,
                            "talent_workforce" => FutureEnterpriseProjectionKind::TalentWorkforce,
                            "learning_enrollments" => FutureEnterpriseProjectionKind::Learning,
                            "benefits_enrollments" => FutureEnterpriseProjectionKind::Benefits,
                            "field_service_orders" => FutureEnterpriseProjectionKind::FieldService,
                            "projects_projects" => FutureEnterpriseProjectionKind::Projects,
                            "projects_hierarchy" => {
                                FutureEnterpriseProjectionKind::ProjectsHierarchy
                            }
                            "projects_records" => FutureEnterpriseProjectionKind::ProjectsRecords,
                            "collaboration_threads" => {
                                FutureEnterpriseProjectionKind::CollaborationThreads
                            }
                            "collaboration_stream" => {
                                FutureEnterpriseProjectionKind::CollaborationStream
                            }
                            "collaboration_inbox" => {
                                FutureEnterpriseProjectionKind::CollaborationInbox
                            }
                            _ => unreachable!("matched future enterprise projection kind"),
                        };
                        let executor = ProductProjectionExecutor::Wave10(
                            Wave10ProjectionExecutor::new(router, kind),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
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
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
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
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "identity_access_governance" => {
                        let executor = ProductProjectionExecutor::Wave4(
                            Wave4ProjectionExecutor::identity_access(
                                router,
                                IdentityAccessProjectionKind::Governance,
                            ),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "organization" => {
                        let executor = ProductProjectionExecutor::Wave4(
                            Wave4ProjectionExecutor::organization(
                                router,
                                OrganizationProjectionKind::Nodes,
                            ),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "organization_sharing" => {
                        let executor = ProductProjectionExecutor::Wave4(
                            Wave4ProjectionExecutor::organization(
                                router,
                                OrganizationProjectionKind::SharingAgreements,
                            ),
                        );
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
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
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
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
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "commercial_lifecycle_records" => {
                        let executor =
                            ProductProjectionExecutor::Wave4(Wave4ProjectionExecutor::commercial(
                                Arc::clone(runtime.store()),
                                router,
                                CommercialProjectionKind::LifecycleRecords,
                            ));
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "legacy_legal_documents" => {
                        let executor =
                            ProductProjectionExecutor::Legal(LegalProjectionRunner::new(router));
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "item_directory" => {
                        let page_size = match env::var("ERPX_ITEM_DIRECTORY_PROJECTION_PAGE_SIZE") {
                            Ok(value) => value.parse::<u32>().map_err(
                                |_| "ERPX_ITEM_DIRECTORY_PROJECTION_PAGE_SIZE must be an integer",
                            )?,
                            Err(_) => ITEM_DIRECTORY_PROJECTION_MAX_PAGE_SIZE,
                        };
                        let executor = ItemDirectoryHoldingProjectionExecutor::new(page_size)
                            .map_err(
                                |_| "ERPX_ITEM_DIRECTORY_PROJECTION_PAGE_SIZE is outside its bound",
                            )?;
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler(
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
                        let executor = FinanceBankingHoldingProjectionExecutor::new(page_size)?;
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler(
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
                            FinanceChartHoldingProjectionExecutor::new(router, page_size)?;
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
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
                            FinanceSubledgerHoldingProjectionExecutor::new(router, page_size)?;
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_unfenced_rebuilds(
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
                            FinanceCloseHoldingProjectionExecutor::new(router, page_size)?;
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_unfenced_rebuilds(
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
                            FinanceThaiTaxHoldingProjectionExecutor::new(router, page_size)
                                .map_err(
                                    |_| "Finance Thai tax projection configuration is invalid",
                                )?;
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_unfenced_rebuilds(
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
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "pos" => {
                        let executor =
                            ProductProjectionExecutor::Pos(PosProjectionRunner::new(router));
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "hr" => {
                        let executor =
                            ProductProjectionExecutor::Hr(HrProjectionRunner::new(router));
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    "sales" => {
                        let executor =
                            ProductProjectionExecutor::Sales(SalesProjectionRunner::new(router));
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
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
                        let executor = MarketingHoldingProjectionExecutor::new(
                            router,
                            SqlxMarketingProjectionFactory,
                            page_size,
                        )
                        .map_err(|_| "ERPX_MARKETING_PROJECTION_PAGE_SIZE is outside its bound")?;
                        runtime
                            .handler_registry_with_control_plane_multi_holding_projection_scheduler_executor(
                                executor,
                                authorization,
                                claim_key_prefix,
                            )?
                    }
                    _ => {
                        return Err("ERPX_HOLDING_PROJECTION_KIND must be one of the registered legacy or CQRS projection scheduler names, including item_directory (Waves 3-10)".into());
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
    use super::MarketingDeliveryMultiHoldingRuntimeHandler;
    use chrono::Duration;

    #[test]
    fn marketing_delivery_runtime_bounds_are_fail_closed() {
        assert!(
            MarketingDeliveryMultiHoldingRuntimeHandler::validate_config(1, Duration::seconds(1))
                .is_ok()
        );
        assert!(
            MarketingDeliveryMultiHoldingRuntimeHandler::validate_config(0, Duration::seconds(1))
                .is_err()
        );
        assert!(
            MarketingDeliveryMultiHoldingRuntimeHandler::validate_config(1, Duration::zero())
                .is_err()
        );
    }
}

#[cfg(test)]
mod product_projection_registration_tests {
    use super::{
        HoldingProjectionExecutorError, projection_hr_cursor, projection_page_size,
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
            Err(HoldingProjectionExecutorError::Permanent)
        ));
        assert!(matches!(
            projection_page_size(&json!({"page_size": 501}), 500),
            Err(HoldingProjectionExecutorError::Permanent)
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
        assert!(filter.contains("QualityProjectionKind::Inspections =>"));
        assert!(filter.contains("erpx_quality_sqlx::PROJECTION_NAME"));
        assert!(filter.contains(
            "QualityProjectionKind::LifecycleRecords => {\n                    erpx_quality_sqlx::LIFECYCLE_RECORD_PROJECTION_NAME"
        ));
        assert!(filter.contains("Self::Wave4(executor) => executor.projection_name_filter()"));
        assert!(filter.contains("Self::Wave6(executor) => executor.projection_name_filter()"));
        assert!(filter.contains("Self::Wave7(executor) => executor.projection_name_filter()"));
        assert!(filter.contains("Self::Wave8(executor) => executor.projection_name_filter()"));
        assert!(filter.contains("Self::Wave9(executor) => executor.projection_name_filter()"));
        assert!(filter.contains("Self::Wave10(executor) => executor.projection_name_filter()"));
        for owned in [
            "Self::AssetMaintenance(_) => Some(\"asset_maintenance\")",
            "Self::Legal(_) => Some(erpx_infrastructure::LEGAL_PROJECTION_NAME)",
            "Self::Scm(_) => Some(\"scm\")",
            "Self::Pos(_) => Some(erpx_infrastructure::POS_PROJECTION_NAME)",
            "Self::Hr(_) => Some(erpx_infrastructure::HR_PROJECTION_NAME)",
            "Self::Sales(_) => Some(\"sales\")",
        ] {
            assert!(filter.contains(owned));
        }
        assert!(!filter.contains("_ => None"));
    }

    #[test]
    fn future_projection_scheduler_accepts_every_registered_generation() {
        let source = include_str!("app.rs");
        for name in [
            "manufacturing_orders",
            "planning_runs",
            "ecommerce_channels",
            "ecommerce_listings",
            "ecommerce_journeys",
            "projects_projects",
            "projects_hierarchy",
            "projects_records",
        ] {
            assert!(source.matches(&format!("\"{name}\"")).count() >= 2);
        }
    }
}
