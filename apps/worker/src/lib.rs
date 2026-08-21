//! Durable outbox worker composition.
//!
//! The worker owns scheduling and persistence semantics. Business contexts
//! provide handlers through [`OutboxHandler`]; the default executable refuses
//! to claim events until such a handler is explicitly installed.

use std::{collections::BTreeMap, env, fmt, sync::Arc, time::Duration as StdDuration};

use chrono::{DateTime, Duration, Utc};
use erpx_domain::asset_maintenance::AssetMaintenanceProjectionReconciliation;
use erpx_domain::sales::SalesProjectionReconciliation;
use erpx_domain::support::Incident;
use erpx_infrastructure::{
    AssetMaintenanceProjectionRebuildPreparation, AssetMaintenanceSourceEventCursor,
    AssetMaintenanceSourceEventPage, BankProviderCredentialResolver, CapabilityMetadata,
    ControlPlaneDatabaseConfig, ControlPlanePoolSettings, ControlPlanePools,
    HoldingDatabaseProvisioner, HoldingDatabaseRouter, HoldingProvisioner,
    MAX_QUALITY_PROJECTION_EVENTS, MAX_QUALITY_SOURCE_PAGE_SIZE,
    MAX_RESTORE_TEST_SCHEDULE_BATCH_SIZE, MarketingSourceEvent as InfraMarketingSourceEvent,
    MarketingSourceEventCursor as InfraMarketingSourceEventCursor, OutboxEnvelope,
    PersistenceError, RestoreTestClaim, RestoreTestScheduleError, RestoreTestScheduler,
    RoleReviewPersistence, RoutedDatabaseSide, RoutedHoldingDatabase,
    SalesProjectionRebuildPreparation, SalesSourceEventCursor, SqlxAssetMaintenanceStore,
    SqlxCommercialStore, SqlxControlPlane, SqlxHoldingProjectionStatusProjectionStore,
    SqlxMarketingStore, SqlxSalesStore, StripeInboxRetryPolicy, StripeInboxTransition,
    StripeSemanticApplyError, StripeSemanticDispatchError, StripeSemanticDispatcher,
    SupportPersistence, SupportPersistenceError, SupportWorkflowStore,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use thiserror::Error;
use uuid::Uuid;

#[cfg(test)]
use erpx_infrastructure::{
    DatabaseSide, HoldingBackupRun, HoldingDatabaseHealth, HoldingStorageHealth,
    ReadinessObservationState,
};
#[cfg(test)]
use std::sync::Mutex;

pub use erpx_infrastructure::{
    ReadinessState, WorkerHeartbeat, WorkerHeartbeatObservation, WorkerHeartbeatReadiness,
    WorkerLiveReadinessObservation, WorkerLiveReadinessPolicy, WorkerReadinessComposition,
    WorkerReadinessMetadata, WorkerReadinessObserver, backup_pitr_readiness,
    holding_routing_readiness, projection_freshness_readiness,
};

mod agent_holding_runtime;
mod agent_material;
mod agent_orchestration_runtime;
mod agent_provider;
mod approval_session_integration;
mod backup_retention;
mod backup_runtime;
mod branch_provisioning_runtime;
mod cluster_pitr_runtime;
pub mod directory_adapters;
mod directory_live;
mod directory_secrets;
mod directory_sync;
mod finance_bank_provider_registry;
mod finance_banking_import;
mod finance_banking_import_source;
mod finance_banking_projection;
mod finance_banking_provider;
mod finance_chart_projection;
mod finance_close_projection;
mod finance_completion_runtime;
mod finance_final_posting;
mod finance_projection;
mod finance_subledger_projection;
mod finance_tax_projection;
mod fixture_suggestion_provider;
mod future_enterprise_projection;
mod g13_target_runtime;
mod holding_projection_scheduler;
mod holding_projection_status_projection;
mod hr_completion_runtime;
mod hr_projection;
mod item_directory_authority;
mod item_directory_policy;
mod item_directory_projection;
mod legal_projection;
mod legal_retention;
mod marketing_content_privacy;
mod marketing_content_workflow;
mod marketing_content_workflow_sqlx;
mod marketing_delivery;
mod marketing_delivery_scheduler;
mod marketing_journey_execution;
mod marketing_journey_integration;
mod marketing_projection;
mod marketing_provider;
mod marketing_provider_secret_manager;
mod marketing_public_abuse;
mod platform_completion_runtime;
mod pos_projection;
mod quality_projection;
mod role_review_notifications;
mod role_review_runtime;
mod sales_finance_inbox_sqlx;
mod sales_finance_integration;
mod scm_projection;
mod support_attachments;
mod support_workflow;

#[cfg(test)]
mod vertical_slice_tests;

pub use agent_holding_runtime::{
    AgentGovernanceMaintenancePass, AgentHoldingRuntimeBundle, AgentHoldingRuntimeFactoryPort,
    AgentProjectionPass, SqlxTrustedAgentHoldingRuntimeFactory, TrustedAgentHoldingDependencies,
    TrustedAgentHoldingDependencyPort, TrustedAgentHoldingOperationsConfig,
    TrustedAgentHoldingRuntimeConfig, TrustedHoldingAgentAssignmentPass,
    compose_trusted_holding_agent_operations, compose_trusted_holding_agent_runner,
};
pub use agent_material::{
    AgentEncryptedObjectPort, AgentEnvelopeCiphertext, AgentEnvelopeKeyManagerPort,
    AgentProtectedMaterialConfig, GovernedAgentProtectedMaterial,
};
pub use agent_orchestration_runtime::{
    AGENT_ASSIGNMENT_HANDLER_NAME, AgentAssignmentNotificationOutboxHandler, AgentAssignmentPass,
    AgentAssignmentRuntimeHandler, AgentAssignmentWorker, AgentExecutionGuardPort,
    AgentNotificationOutboxPass, AgentNotificationOutboxWorker, AgentWorkerError,
    AgentWorkerOutcome, BudgetedAgentProviderPort, ProtectedAgentMaterialPort,
    compose_sqlx_agent_assignment_runner,
};
pub use agent_provider::{
    AgentNetworkProviderConfig, AgentProviderApproval, AgentProviderConfigurationError,
    AgentProviderLimits, GovernedAgentProvider, HoldingBoundAgentProviderPort, OllamaAgentProvider,
    OllamaProviderConfig, OpenAiCompatibleAgentProvider, OpenAiCompatibleCredential,
    OpenAiCompatiblePricing, OpenAiCompatibleProviderConfig,
};
pub use approval_session_integration::{
    ApprovalIngressOutcome, ApprovalIntegrationError, ApprovalOwnerCommand, ApprovalOwnerHandoff,
    ApprovalSessionIngressState, owner_handoff,
};
pub use backup_retention::{
    HOLDING_BACKUP_RETENTION_HANDLER_NAME, HoldingBackupRetentionRuntimeHandler,
};
pub use backup_runtime::{
    BackupRuntime, HOLDING_BACKUP_HANDLER_NAME, HoldingBackupJobRuntimeHandler,
};
pub use branch_provisioning_runtime::{
    BRANCH_PROVISIONING_HANDLER_NAME, BranchProvisioningRuntimeHandler,
};
pub use cluster_pitr_runtime::{
    CLUSTER_PITR_CONTROLLER_HANDLER_NAME, ClusterPitrControllerRuntimeHandler,
};
pub use directory_live::{
    DirectoryPreflightEvidence, LiveDirectoryError, authenticate_directory_user,
    authenticate_directory_user_with_resolver, authenticate_directory_user_with_runtime,
    confirm_ucs_with_resolver, fetch_with_runtime, preflight_directory_connector,
};
pub use directory_secrets::{
    DirectorySecretBundle, DirectorySecretError, DirectorySecretResolver,
    HttpsDirectorySecretResolver, LocalDirectorySecretResolver,
    UnconfiguredDirectorySecretResolver,
};
pub use directory_sync::{
    DIRECTORY_SUPERVISOR_HANDLER_NAME, DIRECTORY_SYNC_HANDLER_NAME,
    DirectorySupervisorRuntimeHandler, DirectorySyncRunReport, DirectorySyncRuntimeHandler,
    DirectorySyncWorker,
};
pub use erpx_infrastructure::{
    OutboxHandler, OutboxWorker, ProjectionHandler, ProjectionRebuildReport, RetryDisposition,
    RetryPolicy, WorkerError, WorkerLoopReport, WorkerRunReport,
};
pub use finance_bank_provider_registry::{
    BankProviderAdapterRegistry, BankProviderAdapterRegistryError,
    MAX_FINANCE_BANK_PROVIDER_ADAPTERS,
};
pub use finance_banking_import::{
    FINANCE_BANK_PROVIDER_IMPORT_HANDLER_NAME, FinanceBankProviderAdapterFactory,
    FinanceBankProviderImportAuthorization, FinanceBankProviderImportAuthorizer,
    FinanceBankProviderImportClaim, FinanceBankProviderImportError,
    FinanceBankProviderImportEvidence, FinanceBankProviderImportFailureClass,
    FinanceBankProviderImportJobSource, FinanceBankProviderImportOutcome,
    FinanceBankProviderImportReconciliationEvidence, FinanceBankProviderImportRequest,
    FinanceBankProviderImportRollbackEvidence, FinanceBankProviderImportWorkflow,
};
pub use finance_banking_import_source::{
    FinanceBankProviderImportJobStatus, SqlxFinanceBankProviderImportJobSource,
};
pub use finance_banking_projection::{
    FINANCE_BANKING_PROJECTION_HANDLER_NAME, FINANCE_BANKING_PROJECTION_MAX_PAGE_SIZE,
    FinanceBankingHoldingProjectionExecutor, FinanceBankingProjectionRebuildReport,
    FinanceBankingProjectionRequest, FinanceBankingProjectionRunReport,
    FinanceBankingProjectionRunner, FinanceBankingProjectionWorkerError,
};
pub use finance_banking_provider::{
    BankProviderImportCoordinator, MAX_BANK_PROVIDER_IMPORT_PAGES,
    MAX_BANK_PROVIDER_IMPORT_TRANSACTIONS,
};
pub use finance_chart_projection::{
    FINANCE_CHART_PROJECTION_HANDLER_NAME, FINANCE_CHART_PROJECTION_MAX_PAGE_SIZE,
    FINANCE_CHART_PROJECTION_NAME, FinanceChartHoldingProjectionExecutor,
    FinanceChartProjectionRebuildReport, FinanceChartProjectionRequest,
    FinanceChartProjectionRunReport, FinanceChartProjectionRunner,
    FinanceChartProjectionWorkerError,
};
pub use finance_close_projection::{
    FINANCE_CLOSE_PROJECTION_HANDLER_NAME, FINANCE_CLOSE_PROJECTION_MAX_PAGE_SIZE,
    FINANCE_CLOSE_PROJECTION_NAME, FinanceCloseHoldingProjectionExecutor,
    FinanceCloseProjectionRebuildReport, FinanceCloseProjectionRequest,
    FinanceCloseProjectionRunReport, FinanceCloseProjectionRunner,
    FinanceCloseProjectionWorkerError,
};
pub use finance_completion_runtime::{
    FINANCE_COMPLETION_AUTHORIZATION_MAX_SECONDS, FINANCE_COMPLETION_MAX_PAGE_SIZE,
    FINANCE_COMPLETION_MAX_PROJECTION_PAGES, FINANCE_COMPLETION_MAX_RECONCILIATION_ROWS,
    FINANCE_COMPLETION_MAX_ROLLBACK_METRIC, FINANCE_COMPLETION_RECONCILE_PERMISSION,
    FINANCE_COMPLETION_ROLLBACK_PERMISSION, FinanceCompletionAuthorizationProof,
    FinanceCompletionOperation, FinanceCompletionReconciliationMetrics, FinanceCompletionRequest,
    FinanceCompletionRollbackMetrics, FinanceCompletionRuntimeAdapter,
    FinanceCompletionRuntimeError,
};
pub use finance_final_posting::{
    FinalPostingCoordinator, FinalPostingError, FinancePostingOwnerPort,
};
pub use finance_projection::{
    FINANCE_PROJECTION_HANDLER_NAME, FINANCE_PROJECTION_MAX_PAGE_SIZE,
    FinanceProjectionRebuildReport, FinanceProjectionRequest, FinanceProjectionRunReport,
    FinanceProjectionRunner, FinanceProjectionWorkerError,
};
pub use finance_subledger_projection::{
    FINANCE_SUBLEDGER_PROJECTION_HANDLER_NAME, FINANCE_SUBLEDGER_PROJECTION_MAX_PAGE_SIZE,
    FINANCE_SUBLEDGER_PROJECTION_NAME, FINANCE_SUBLEDGER_RECONCILIATION_MAX_ROWS,
    FinanceSubledgerHoldingProjectionExecutor, FinanceSubledgerProjectionWorkerError,
    FinanceSubledgerReconciliationRequest, FinanceSubledgerReconciliationRunner,
    FinanceSubledgerReconciliationWorkerError,
};
pub use finance_tax_projection::{
    FINANCE_THAI_TAX_PROJECTION_HANDLER_NAME, FINANCE_THAI_TAX_PROJECTION_MAX_PAGE_SIZE,
    FinanceThaiTaxHoldingProjectionExecutor, FinanceThaiTaxProjectionWorkerError,
};
pub use fixture_suggestion_provider::{
    FixtureAgentProvider, FixtureEnvironment, FixtureSuggestionProviderError,
};
pub use future_enterprise_projection::{
    FutureEnterpriseProjectionKind, FutureEnterpriseProjectionRegistration,
    FutureEnterpriseProjectionRuntime,
};
pub use g13_target_runtime::{G13_TARGET_HANDLER_NAME, G13TargetRuntimeHandler};
pub use hr_completion_runtime::{
    HR_COMPLETION_HANDLER_NAME, HrCompletionConfig, HrCompletionMode, HrCompletionRunReport,
    HrCompletionRuntimeError, HrCompletionRuntimeHandler,
};
pub use hr_projection::{
    HR_PROJECTION_HANDLER_NAME, HR_PROJECTION_MAX_PAGE_SIZE, HrProjectionRebuildReport,
    HrProjectionRequest, HrProjectionRunReport, HrProjectionRunner, HrProjectionWorkerError,
};
pub use item_directory_authority::{
    AuthorityGrantReconciliationReport, AuthorityProjectionError,
    HoldingAuthorityEvidenceProjection, MAX_AUTHORITY_RECONCILIATION_ROWS,
    decode_authority_evidence, decode_organization_lineage,
};
pub use item_directory_policy::{
    IamAuthorityEvidence, MAX_EFFECTIVE_GRANT_TTL, MAX_RECORD_POLICY_FANOUT, MembershipEvidence,
    OrganizationScopeEvidence, OrganizationScopeEvidenceKind, RecordGrantDecisionProducer,
    RecordGrantEvaluation, RecordPolicyDecisionClaim, RecordPolicyDecisionError,
    RecordPolicyDecisionHandler, RecordPolicyDecisionRunReport, RecordPolicyDecisionSource,
    RecordPolicyFactSink, RecordPolicyReleaseKind, RecordPolicyRuntimeError, RecordPolicyTrigger,
};
pub use item_directory_projection::{
    FUTURE_DESCRIPTOR_GAPS, FUTURE_LIVE_RECORD_FAMILIES, HoldingItemDirectoryProjector,
    ITEM_DIRECTORY_PROJECTION_MAX_PAGE_SIZE, ItemDirectoryHoldingProjectionExecutor,
    ItemDirectoryProjectionError, UNROUTED_FUTURE_AUTHORITY, decode_item_directory_event,
    decode_item_directory_owner_event,
};
pub use legal_projection::{
    LEGAL_PROJECTION_HANDLER_NAME, LEGAL_PROJECTION_MAX_PAGE_SIZE, LegalProjectionRebuildReport,
    LegalProjectionRequest, LegalProjectionRunReport, LegalProjectionRunner,
    LegalProjectionWorkerError,
};
pub use legal_retention::{
    LEGAL_RETENTION_DEFAULT_BATCH_SIZE, LEGAL_RETENTION_HANDLER_NAME,
    LEGAL_RETENTION_MAX_BATCH_SIZE, LegalRetentionRequest, LegalRetentionRunReport,
    LegalRetentionRunner, LegalRetentionWorkerContext, LegalRetentionWorkerError,
};
pub use marketing_content_privacy::{
    MARKETING_CONTENT_PRIVACY_DEFAULT_BATCH_SIZE, MARKETING_CONTENT_PRIVACY_HANDLER_NAME,
    MarketingContentPrivacyRequest, MarketingContentPrivacyRunReport,
    MarketingContentPrivacyRunner, MarketingContentPrivacyWorkerContext,
    MarketingContentPrivacyWorkerError,
};
pub use marketing_content_workflow::{
    MARKETING_CONTENT_PREVIEW_EXPIRY_HANDLER_NAME, MARKETING_CONTENT_PREVIEW_EXPIRY_MAX_BATCH_SIZE,
    MarketingContentPreviewExpiryApplyResult, MarketingContentPreviewExpiryCandidate,
    MarketingContentPreviewExpiryExecutor, MarketingContentPreviewExpiryFailureKind,
    MarketingContentPreviewExpiryHandler, MarketingContentPreviewExpiryItemError,
    MarketingContentPreviewExpiryItemReport, MarketingContentPreviewExpiryItemRequest,
    MarketingContentPreviewExpiryItemResult, MarketingContentPreviewExpiryRequest,
    MarketingContentPreviewExpiryRunReport, MarketingContentPreviewExpiryWorkerError,
};
pub use marketing_content_workflow_sqlx::SqlxMarketingContentPreviewExpiryExecutor;
pub use marketing_delivery::{
    MARKETING_DELIVERY_HANDLER_NAME, MARKETING_DELIVERY_MAX_BATCH_SIZE,
    MARKETING_DELIVERY_MAX_LEASE_SECONDS, MARKETING_DELIVERY_MIN_LEASE_SECONDS,
    MarketingDeliveryHandler, MarketingDeliveryProvider, MarketingDeliveryProviderError,
    MarketingDeliverySource,
};
pub use marketing_delivery_scheduler::{
    MARKETING_DELIVERY_SCHEDULER_HANDLER_NAME, MarketingDeliverySchedulerHandler,
    MarketingDeliverySchedulerSource,
};
pub use marketing_journey_execution::{
    DomainOnlyMarketingJourneyExecutionDecisionSource, MARKETING_JOURNEY_EXECUTION_HANDLER_NAME,
    MARKETING_JOURNEY_EXECUTION_MAX_BATCH_SIZE, MARKETING_JOURNEY_EXECUTION_MAX_LEASE_SECONDS,
    MARKETING_JOURNEY_EXECUTION_MIN_LEASE_SECONDS, MARKETING_JOURNEY_EXECUTION_RETRY_DELAY_SECONDS,
    MarketingJourneyExecutionDecisionError, MarketingJourneyExecutionDecisionSource,
    MarketingJourneyExecutionHandler, MarketingJourneyExecutionSource,
    SqlxMarketingJourneyExecutionDecisionSource,
};
pub use marketing_journey_integration::{
    HttpMarketingJourneyIntegrationProvider, MARKETING_JOURNEY_INTEGRATION_HANDLER_NAME,
    MARKETING_JOURNEY_INTEGRATION_MAX_BATCH_SIZE, MARKETING_JOURNEY_INTEGRATION_MAX_LEASE_SECONDS,
    MARKETING_JOURNEY_INTEGRATION_MIN_LEASE_SECONDS, MarketingJourneyIntegrationHandler,
    MarketingJourneyIntegrationMultiHoldingRuntimeHandler, MarketingJourneyIntegrationProvider,
    MarketingJourneyIntegrationProviderError,
};
pub use marketing_projection::{
    MARKETING_PROJECTION_HANDLER_NAME, MARKETING_PROJECTION_MAX_PAGE_SIZE,
    MARKETING_PROJECTION_NAME, MarketingHoldingProjectionExecutor, MarketingProjectionApplication,
    MarketingProjectionFencedRebuild, MarketingProjectionFencedStore,
    MarketingProjectionRebuildPreparation, MarketingProjectionRebuildReport,
    MarketingProjectionRebuildRequest, MarketingProjectionReconciliationReport,
    MarketingProjectionReconciliationStore, MarketingProjectionRequest,
    MarketingProjectionRunReport, MarketingProjectionRunner, MarketingProjectionStore,
    MarketingProjectionStoreFactory, MarketingProjectionWorkerError, MarketingSourceEvent,
    MarketingSourceEventCursor, MarketingSourceEventPage,
};
pub use marketing_provider::{
    AccountBoundMarketingDeliveryProvider, EnvMarketingDeliveryCredentialResolver,
    HttpMarketingDeliveryProvider, MAX_MARKETING_PROVIDER_RESPONSE_BYTES, MarketingChannelRoute,
    MarketingDeliveryAccountResolver, MarketingDeliveryCredential,
    MarketingDeliveryCredentialResolver, MarketingProviderConfigurationError,
    SqlxMarketingDeliveryAccountResolver,
};
pub use marketing_provider_secret_manager::MarketingDeliverySecretManagerResolver;
pub use marketing_public_abuse::{
    MARKETING_PUBLIC_ABUSE_CLEANUP_DEFAULT_BATCH_SIZE, MARKETING_PUBLIC_ABUSE_CLEANUP_HANDLER_NAME,
    MARKETING_PUBLIC_ABUSE_CLEANUP_MAX_BATCH_SIZE,
    MARKETING_PUBLIC_ABUSE_CLEANUP_RETENTION_SECONDS, MarketingPublicAbuseCleanupError,
    MarketingPublicAbuseCleanupReport, MarketingPublicAbuseCleanupRunner,
    MarketingPublicAbusePurgePort,
};
pub use sales_finance_inbox_sqlx::{
    DurableSalesFinanceOutcome, SALES_FINANCE_CONSUMER_NAME, SalesFinanceInboxError,
    SqlxSalesFinanceInbox,
};
pub use sales_finance_integration::{
    SalesFinanceConsumeOutcome, SalesFinanceConsumerError, SalesFinanceConsumerState,
    SalesFinanceFailure, SalesFinanceHandoff, SalesFinanceHandoffKind,
};
pub const PLATFORM_COMPLETION_HANDLER_NAME: &str = "platform_completion";
pub use platform_completion_runtime::{
    PLATFORM_COMPLETION_MAX_BATCH_SIZE, PLATFORM_COMPLETION_READ_PERMISSION,
    PLATFORM_COMPLETION_RUNTIME_HANDLER_NAME, PlatformCompletionRuntimeAdapter,
    PlatformCompletionRuntimeError, PlatformCompletionRuntimeHandler,
};
pub use pos_projection::{
    POS_PROJECTION_HANDLER_NAME, POS_PROJECTION_MAX_PAGE_SIZE, PosProjectionRebuildReport,
    PosProjectionRequest, PosProjectionRunReport, PosProjectionRunner, PosProjectionWorkerError,
};
pub const QUALITY_PROJECTION_HANDLER_NAME: &str = "quality_projection";
pub const QUALITY_PROJECTION_MAX_PAGE_SIZE: u32 = MAX_QUALITY_SOURCE_PAGE_SIZE;
pub const QUALITY_PROJECTION_MAX_EVENTS: u64 = MAX_QUALITY_PROJECTION_EVENTS;
pub use holding_projection_scheduler::{
    HOLDING_PROJECTION_SCHEDULER_HANDLER_NAME, HOLDING_PROJECTION_SCHEDULER_MAX_BATCH_SIZE,
    HoldingProjectionBindingSource, HoldingProjectionExecutor, HoldingProjectionExecutorError,
    HoldingProjectionRoutedExecutor, HoldingProjectionSchedulerHandler,
    HoldingProjectionSchedulerSource, MultiHoldingProjectionSchedulerHandler,
    RoutedHoldingProjectionExecutor,
};
pub use holding_projection_status_projection::{
    HOLDING_PROJECTION_STATUS_PROJECTION_HANDLER_NAME, HoldingProjectionStatusProjectionHandler,
    HoldingProjectionStatusPublisher,
};
pub use quality_projection::{
    QualityProjectionRebuildReport, QualityProjectionReconciliationReport,
    QualityProjectionRequest, QualityProjectionRunReport, QualityProjectionRunner,
};
pub use role_review_notifications::{
    ROLE_REVIEW_NOTIFICATION_HANDLER_NAME, RoleReviewNotificationHandler,
    RoleReviewNotificationPublishError, RoleReviewNotificationPublisher,
    RoleReviewNotificationSource,
};
pub use role_review_runtime::{
    ROLE_REVIEW_SNAPSHOT_HANDLER_NAME, RoleReviewSnapshotRuntimeHandler,
};
pub use scm_projection::{
    SCM_PROJECTION_HANDLER_NAME, SCM_PROJECTION_MAX_PAGE_SIZE, ScmProjectionPageReport,
    ScmProjectionPageRequest, ScmProjectionRebuildReport, ScmProjectionRebuildRequest,
    ScmProjectionRunner, ScmProjectionWorkerError,
};
pub use support_attachments::{
    SUPPORT_ATTACHMENT_EXPIRY_HANDLER_NAME, SUPPORT_SLA_NOTIFICATION_HANDLER_NAME,
    SupportAttachmentExpiryHandler, SupportAttachmentExpiryStore, SupportSlaNotificationHandler,
    SupportSlaNotificationSource, bounded_notification_lease, notification_is_within_window,
};
pub use support_workflow::{
    SUPPORT_WORKFLOW_HANDLER_NAME, SupportWorkflowHandler, SupportWorkflowOutboxHandler,
    SupportWorkflowProjection,
};

/// Infrastructure adapter for the generic Marketing projection worker seam.
///
/// The adapter performs only lossless DTO conversion. Holding routing, scoped
/// database settings, source-event ordering, and read-model writes remain in
/// the infrastructure store and the projection runner respectively.
#[derive(Debug, Clone)]
pub struct SqlxMarketingProjectionStore {
    store: SqlxMarketingStore,
}

#[derive(Debug, Clone, Default)]
pub struct SqlxMarketingProjectionFactory;

#[async_trait::async_trait]
impl marketing_projection::MarketingProjectionStoreFactory for SqlxMarketingProjectionFactory {
    type Store = SqlxMarketingProjectionStore;

    async fn open(
        &self,
        write_pool: sqlx::PgPool,
        read_pool: sqlx::PgPool,
        data_profile_key: String,
    ) -> Result<Self::Store, PersistenceError> {
        Ok(SqlxMarketingProjectionStore {
            store: SqlxMarketingStore::from_pools(&write_pool, &read_pool, data_profile_key)?,
        })
    }
}

#[async_trait::async_trait]
impl marketing_projection::MarketingProjectionStore for SqlxMarketingProjectionStore {
    async fn read_source_event_page(
        &self,
        cursor: Option<&marketing_projection::MarketingSourceEventCursor>,
        page_size: u32,
    ) -> Result<marketing_projection::MarketingSourceEventPage, PersistenceError> {
        let infra_cursor = cursor.map(|cursor| InfraMarketingSourceEventCursor {
            aggregate_type: cursor.aggregate_type.clone(),
            aggregate_id: cursor.aggregate_id,
            aggregate_version: cursor.aggregate_version,
        });
        let page = self
            .store
            .read_source_event_page(infra_cursor.as_ref(), page_size)
            .await?;
        Ok(marketing_projection::MarketingSourceEventPage {
            events: page
                .events
                .into_iter()
                .map(|event: InfraMarketingSourceEvent| {
                    marketing_projection::MarketingSourceEvent {
                        event_id: event.event_id,
                        event_type: event.event_type,
                        aggregate_type: event.aggregate_type,
                        aggregate_id: event.aggregate_id,
                        aggregate_version: event.aggregate_version,
                        payload: event.payload,
                        occurred_at: event.occurred_at,
                    }
                })
                .collect(),
            next_cursor: page.next_cursor.map(|cursor| {
                marketing_projection::MarketingSourceEventCursor {
                    aggregate_type: cursor.aggregate_type,
                    aggregate_id: cursor.aggregate_id,
                    aggregate_version: cursor.aggregate_version,
                }
            }),
            has_more: page.has_more,
        })
    }

    async fn project_event(
        &self,
        event_id: Uuid,
        event_type: &str,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &serde_json::Value,
    ) -> Result<marketing_projection::MarketingProjectionApplication, PersistenceError> {
        let application = self
            .store
            .project_event(
                event_id,
                event_type,
                aggregate_type,
                aggregate_id,
                aggregate_version,
                payload,
            )
            .await?;
        Ok(marketing_projection::MarketingProjectionApplication {
            event_id: application.event_id,
            aggregate_type: application.aggregate_type,
            aggregate_id: application.aggregate_id,
            aggregate_version: application.aggregate_version,
            applied: application.applied,
        })
    }

    async fn clear_projection(
        &self,
    ) -> Result<marketing_projection::MarketingProjectionRebuildPreparation, PersistenceError> {
        let preparation = self.store.clear_projection().await?;
        Ok(
            marketing_projection::MarketingProjectionRebuildPreparation {
                data_profile_key: preparation.data_profile_key,
                rows_cleared: preparation.rows_cleared,
                checkpoints_cleared: preparation.checkpoints_cleared,
            },
        )
    }
}

pub const INCIDENT_HANDLER_NAME: &str = "incident_projection";
pub const STRIPE_SEMANTIC_HANDLER_NAME: &str = "stripe_semantic";
pub const STRIPE_BILLING_POLICY_HANDLER_NAME: &str = "stripe_billing_policy";
pub const STRIPE_SEMANTIC_MAX_BATCH_SIZE: u32 = 500;
pub const STRIPE_SEMANTIC_MAX_CLAIM_TIMEOUT_SECONDS: i64 = 15 * 60;
pub const CUSTOMER_ACCESS_AUDIT_HANDLER_NAME: &str = "customer_access_audit_summary";
pub const SUPPORT_CAPABILITY_EXPIRY_HANDLER_NAME: &str = "support_capability_expiry";
pub const SUPPORT_CAPABILITY_EXPIRY_MAX_BATCH_SIZE: u32 = 1_000;
pub const RESTORE_TEST_HANDLER_NAME: &str = "restore_test";
pub const SALES_PROJECTION_HANDLER_NAME: &str = "sales_projection";
pub const SALES_PROJECTION_MAX_PAGE_SIZE: u32 =
    erpx_infrastructure::MAX_SALES_SOURCE_EVENT_PAGE_SIZE;
pub const SALES_PROJECTION_MAX_EVENTS: u64 = erpx_infrastructure::MAX_SALES_PROJECTION_EVENTS;
/// Bound one worker invocation even when a holding has a large source stream
/// or is receiving a continuous stream of new Sales events.  The scheduler
/// does not persist a product-specific source cursor after completion, so a
/// successful invocation must drain the bounded source stream before it can
/// acknowledge the claim.
/// A page is capped at 500 source events, so this permits one million source
/// events in a single replacement rebuild while retaining a finite worker
/// budget and cursor-advance fence.
pub const SALES_PROJECTION_MAX_PAGES: u32 = 2_000;
pub const ASSET_MAINTENANCE_PROJECTION_HANDLER_NAME: &str = "asset_maintenance_projection";
pub const ASSET_MAINTENANCE_PROJECTION_MAX_PAGE_SIZE: u32 =
    erpx_infrastructure::MAX_ASSET_MAINTENANCE_READ_PAGE_SIZE;
pub const ASSET_MAINTENANCE_PROJECTION_MAX_EVENTS: u64 =
    erpx_infrastructure::MAX_ASSET_MAINTENANCE_PROJECTION_EVENTS;

#[derive(Debug, Error)]
pub enum WorkerRuntimeError {
    #[error("worker configuration error: {0}")]
    Configuration(String),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
    #[error("database connection error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

fn validate_worker_reference(value: &str, field: &str) -> Result<(), WorkerRuntimeError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(WorkerRuntimeError::Configuration(format!(
            "worker {field} is invalid"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesProjectionRequest {
    pub holding_id: String,
    pub data_profile_key: String,
    pub cursor: Option<SalesSourceEventCursor>,
    pub page_size: u32,
}

impl SalesProjectionRequest {
    pub fn new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        cursor: Option<SalesSourceEventCursor>,
        page_size: u32,
    ) -> Result<Self, WorkerRuntimeError> {
        let request = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            cursor,
            page_size,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), WorkerRuntimeError> {
        validate_sales_reference(&self.holding_id, "holding_id")?;
        validate_sales_reference(&self.data_profile_key, "data_profile_key")?;
        if self.page_size == 0 || self.page_size > SALES_PROJECTION_MAX_PAGE_SIZE {
            return Err(WorkerRuntimeError::Configuration(format!(
                "Sales projection page size must be 1..={SALES_PROJECTION_MAX_PAGE_SIZE}"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesProjectionRunReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub events_seen: u64,
    pub events_applied: u64,
    pub next_cursor: Option<SalesSourceEventCursor>,
    pub has_more: bool,
    pub reconciliation: SalesProjectionReconciliation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesProjectionRebuildReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub events_replayed: u64,
    pub preparation: SalesProjectionRebuildPreparation,
    pub reconciliation: SalesProjectionReconciliation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesProjectionReconciliationReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub reconciliation: SalesProjectionReconciliation,
}

/// Holding-routed Sales source-stream projector. Source events are read from
/// the write database and applied to the replaceable read database. The
/// infrastructure projection method enforces payload identity, contiguous
/// versions, and replay idempotency; this runner owns routing and rebuild
/// orchestration only.
#[derive(Debug, Clone)]
pub struct SalesProjectionRunner {
    router: HoldingDatabaseRouter,
}

impl SalesProjectionRunner {
    pub fn new(router: HoldingDatabaseRouter) -> Self {
        Self { router }
    }

    pub async fn project_page(
        &self,
        request: SalesProjectionRequest,
    ) -> Result<SalesProjectionRunReport, WorkerRuntimeError> {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxSalesStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => {
                let result: Result<SalesProjectionRunReport, WorkerRuntimeError> = async {
                    let mut cursor = request.cursor.clone();
                    let mut events_seen = 0_u64;
                    let mut events_applied = 0_u64;
                    loop {
                        let page = store
                            .read_source_event_page(cursor.as_ref(), request.page_size)
                            .await?;
                        events_seen = next_sales_event_count(events_seen, page.events.len())?;
                        for event in &page.events {
                            let application = store
                                .project_event(
                                    event.event_id,
                                    &event.aggregate_type,
                                    event.aggregate_id,
                                    event.aggregate_version,
                                    &event.payload,
                                )
                                .await?;
                            if application.applied {
                                events_applied = events_applied.saturating_add(1);
                            }
                        }
                        if !page.has_more {
                            break;
                        }
                        cursor = next_sales_cursor(true, page.next_cursor)?;
                    }
                    let reconciliation = store.reconcile_projection().await?;
                    require_sales_reconciliation(reconciliation)?;
                    Ok::<SalesProjectionRunReport, PersistenceError>(SalesProjectionRunReport {
                        holding_id: request.holding_id.clone(),
                        data_profile_key: request.data_profile_key.clone(),
                        events_seen,
                        events_applied,
                        next_cursor: None,
                        has_more: false,
                        reconciliation,
                    })
                }
                .await
                .map_err(|_| redacted_sales_error("projection page"));
                result
            }
            Err(_) => Err(redacted_sales_error("store construction")),
        };
        write.close().await;
        read.close().await;
        result
    }

    pub async fn rebuild(
        &self,
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        page_size: u32,
    ) -> Result<SalesProjectionRebuildReport, WorkerRuntimeError> {
        let request = SalesProjectionRequest::new(holding_id, data_profile_key, None, page_size)?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxSalesStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => {
                async {
                    let expected_source_event_count = store
                        .source_event_high_watermark()
                        .await
                        .map_err(|_| redacted_sales_error("source watermark"))?;
                    if expected_source_event_count > SALES_PROJECTION_MAX_EVENTS {
                        return Err(redacted_sales_error("source event bound"));
                    }
                    let (preparation, events_replayed) = store
                        .rebuild_projection_with_source_fence(
                            Some(expected_source_event_count),
                            request.page_size,
                        )
                        .await
                        .map_err(|_| redacted_sales_error("atomic projection rebuild"))?;
                    let reconciliation = store
                        .reconcile_projection()
                        .await
                        .map_err(|_| redacted_sales_error("rebuild reconciliation"))?;
                    require_sales_reconciliation(reconciliation)
                        .map_err(|_| redacted_sales_error("rebuild reconciliation"))?;
                    Ok(SalesProjectionRebuildReport {
                        holding_id: request.holding_id.clone(),
                        data_profile_key: request.data_profile_key.clone(),
                        events_replayed,
                        preparation,
                        reconciliation,
                    })
                }
                .await
            }
            Err(_) => Err(redacted_sales_error("store construction")),
        };
        write.close().await;
        read.close().await;
        result
    }

    /// Reconcile the routed Sales source and read model without mutating
    /// either side. A non-consistent report is returned as evidence to the
    /// caller; projection success paths use `require_sales_reconciliation`
    /// and therefore fail closed instead of acknowledging a damaged read
    /// model.
    pub async fn reconcile(
        &self,
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
    ) -> Result<SalesProjectionReconciliationReport, WorkerRuntimeError> {
        let request = SalesProjectionRequest::new(holding_id, data_profile_key, None, 1)?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxSalesStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => store
                .reconcile_projection()
                .await
                .map(|reconciliation| SalesProjectionReconciliationReport {
                    holding_id: request.holding_id.clone(),
                    data_profile_key: request.data_profile_key.clone(),
                    reconciliation,
                })
                .map_err(|_| redacted_sales_error("reconciliation")),
            Err(_) => Err(redacted_sales_error("store construction")),
        };
        write.close().await;
        read.close().await;
        result
    }

    async fn route_store(
        &self,
        holding_id: &str,
        data_profile_key: &str,
    ) -> Result<
        (
            erpx_infrastructure::RoutedHoldingDatabase,
            erpx_infrastructure::RoutedHoldingDatabase,
        ),
        WorkerRuntimeError,
    > {
        let write = self
            .router
            .route(holding_id, data_profile_key, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| redacted_sales_error("write database routing"))?;
        let read = match self
            .router
            .route(holding_id, data_profile_key, RoutedDatabaseSide::Read)
            .await
        {
            Ok(read) => read,
            Err(_) => {
                write.close().await;
                return Err(redacted_sales_error("read database routing"));
            }
        };
        Ok((write, read))
    }
}

fn validate_sales_reference(value: &str, field: &str) -> Result<(), WorkerRuntimeError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(WorkerRuntimeError::Configuration(format!(
            "Sales {field} is invalid"
        )));
    }
    Ok(())
}

fn redacted_sales_error(operation: &str) -> WorkerRuntimeError {
    tracing::error!(operation, "Sales projection worker seam failed");
    WorkerRuntimeError::Configuration(format!("Sales {operation} failed"))
}

fn next_sales_cursor(
    has_more: bool,
    next_cursor: Option<SalesSourceEventCursor>,
) -> Result<Option<SalesSourceEventCursor>, PersistenceError> {
    if has_more && next_cursor.is_none() {
        return Err(PersistenceError::InvalidData(
            "Sales source page omitted its continuation cursor".into(),
        ));
    }
    Ok(has_more.then_some(next_cursor).flatten())
}

fn next_sales_event_count(events_seen: u64, page_len: usize) -> Result<u64, PersistenceError> {
    let page_len = u64::try_from(page_len)
        .map_err(|_| PersistenceError::InvalidData("Sales page event count is invalid".into()))?;
    let next = events_seen
        .checked_add(page_len)
        .ok_or_else(|| PersistenceError::InvalidData("Sales event count overflow".into()))?;
    if next > SALES_PROJECTION_MAX_EVENTS {
        return Err(PersistenceError::InvalidData(
            "Sales projection source event bound exceeded".into(),
        ));
    }
    Ok(next)
}

fn require_sales_reconciliation(
    reconciliation: SalesProjectionReconciliation,
) -> Result<SalesProjectionReconciliation, PersistenceError> {
    if reconciliation.is_consistent() {
        Ok(reconciliation)
    } else {
        Err(PersistenceError::InvalidData(
            "Sales source/read reconciliation found a mismatch".into(),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetMaintenanceProjectionRequest {
    pub holding_id: String,
    pub data_profile_key: String,
    pub cursor: Option<AssetMaintenanceSourceEventCursor>,
    pub page_size: u32,
}

impl AssetMaintenanceProjectionRequest {
    pub fn new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        cursor: Option<AssetMaintenanceSourceEventCursor>,
        page_size: u32,
    ) -> Result<Self, WorkerRuntimeError> {
        let request = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            cursor,
            page_size,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), WorkerRuntimeError> {
        validate_asset_reference(&self.holding_id, "holding_id")?;
        validate_asset_reference(&self.data_profile_key, "data_profile_key")?;
        if self.page_size == 0 || self.page_size > ASSET_MAINTENANCE_PROJECTION_MAX_PAGE_SIZE {
            return Err(WorkerRuntimeError::Configuration(format!(
                "Asset/Maintenance projection page size must be 1..={ASSET_MAINTENANCE_PROJECTION_MAX_PAGE_SIZE}"
            )));
        }
        if let Some(cursor) = &self.cursor
            && (cursor.aggregate_type.trim().is_empty()
                || cursor.aggregate_id.is_nil()
                || cursor.aggregate_version == 0)
        {
            return Err(WorkerRuntimeError::Configuration(
                "Asset/Maintenance projection cursor is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetMaintenanceProjectionRunReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub events_seen: u64,
    pub events_applied: u64,
    pub next_cursor: Option<AssetMaintenanceSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetMaintenanceProjectionRebuildReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub events_replayed: u64,
    pub preparation: AssetMaintenanceProjectionRebuildPreparation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetMaintenanceProjectionReconciliationReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub reconciliation: AssetMaintenanceProjectionReconciliation,
}

#[derive(Debug, Clone)]
pub struct AssetMaintenanceProjectionRunner {
    router: HoldingDatabaseRouter,
}

impl AssetMaintenanceProjectionRunner {
    pub fn new(router: HoldingDatabaseRouter) -> Self {
        Self { router }
    }

    pub async fn project_page(
        &self,
        request: AssetMaintenanceProjectionRequest,
    ) -> Result<AssetMaintenanceProjectionRunReport, WorkerRuntimeError> {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxAssetMaintenanceStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => {
                let result: Result<AssetMaintenanceProjectionRunReport, WorkerRuntimeError> =
                    async {
                        let mut cursor = request.cursor.clone();
                        let mut events_seen = 0_u64;
                        let mut events_applied = 0_u64;
                        loop {
                            let page = store
                                .read_source_event_page(cursor.as_ref(), request.page_size)
                                .await?;
                            validate_asset_source_event_page(
                                cursor.as_ref(),
                                &page,
                                request.page_size,
                            )?;
                            events_seen = next_asset_event_count(events_seen, page.events.len())?;
                            for event in &page.events {
                                let application = store
                                    .project_event(
                                        event.event_id,
                                        &event.aggregate_type,
                                        event.aggregate_id,
                                        event.aggregate_version,
                                        &event.payload,
                                    )
                                    .await?;
                                if application.applied {
                                    events_applied = events_applied.saturating_add(1);
                                }
                            }
                            if !page.has_more {
                                break;
                            }
                            cursor = page.next_cursor;
                        }
                        Ok::<AssetMaintenanceProjectionRunReport, PersistenceError>(
                            AssetMaintenanceProjectionRunReport {
                                holding_id: request.holding_id.clone(),
                                data_profile_key: request.data_profile_key.clone(),
                                events_seen,
                                events_applied,
                                next_cursor: None,
                                has_more: false,
                            },
                        )
                    }
                    .await
                    .map_err(|_| redacted_asset_error("projection page"));
                result
            }
            Err(_) => Err(redacted_asset_error("store construction")),
        };
        write.close().await;
        read.close().await;
        result
    }

    pub async fn rebuild(
        &self,
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        page_size: u32,
    ) -> Result<AssetMaintenanceProjectionRebuildReport, WorkerRuntimeError> {
        let request =
            AssetMaintenanceProjectionRequest::new(holding_id, data_profile_key, None, page_size)?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxAssetMaintenanceStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => {
                let source_event_count = store
                    .source_event_high_watermark()
                    .await
                    .map_err(|_| redacted_asset_error("source event bound"))?;
                if source_event_count > ASSET_MAINTENANCE_PROJECTION_MAX_EVENTS {
                    return Err(redacted_asset_error("source event bound"));
                }
                let preparation = store
                    .clear_projection()
                    .await
                    .map_err(|_| redacted_asset_error("projection clear"))?;
                async {
                    let mut cursor = None;
                    let mut events_replayed = 0_u64;
                    loop {
                        let page = store
                            .read_source_event_page(cursor.as_ref(), request.page_size)
                            .await
                            .map_err(|_| redacted_asset_error("source event read"))?;
                        events_replayed =
                            next_asset_event_count(events_replayed, page.events.len())?;
                        for event in &page.events {
                            let application = store
                                .project_event(
                                    event.event_id,
                                    &event.aggregate_type,
                                    event.aggregate_id,
                                    event.aggregate_version,
                                    &event.payload,
                                )
                                .await
                                .map_err(|_| redacted_asset_error("projection apply"))?;
                            if application.applied {
                                events_replayed = events_replayed.saturating_add(1);
                            }
                        }
                        if !page.has_more {
                            break;
                        }
                        cursor = Some(
                            page.next_cursor
                                .ok_or_else(|| redacted_asset_error("source page continuation"))?,
                        );
                    }
                    Ok(AssetMaintenanceProjectionRebuildReport {
                        holding_id: request.holding_id.clone(),
                        data_profile_key: request.data_profile_key.clone(),
                        events_replayed,
                        preparation,
                    })
                }
                .await
            }
            Err(_) => Err(redacted_asset_error("store construction")),
        };
        write.close().await;
        read.close().await;
        result
    }

    /// Reconcile authoritative Asset/Maintenance source events with the
    /// disposable read projection without mutating either database.
    pub async fn reconcile(
        &self,
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
    ) -> Result<AssetMaintenanceProjectionReconciliationReport, WorkerRuntimeError> {
        let request =
            AssetMaintenanceProjectionRequest::new(holding_id, data_profile_key, None, 1)?;
        let (write, read) = self
            .route_store(&request.holding_id, &request.data_profile_key)
            .await?;
        let result = match SqlxAssetMaintenanceStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.data_profile_key.clone(),
        ) {
            Ok(store) => store
                .reconcile_projection()
                .await
                .map(
                    |reconciliation| AssetMaintenanceProjectionReconciliationReport {
                        holding_id: request.holding_id.clone(),
                        data_profile_key: request.data_profile_key.clone(),
                        reconciliation,
                    },
                )
                .map_err(|_| redacted_asset_error("reconciliation")),
            Err(_) => Err(redacted_asset_error("store construction")),
        };
        write.close().await;
        read.close().await;
        result
    }

    async fn route_store(
        &self,
        holding_id: &str,
        data_profile_key: &str,
    ) -> Result<
        (
            erpx_infrastructure::RoutedHoldingDatabase,
            erpx_infrastructure::RoutedHoldingDatabase,
        ),
        WorkerRuntimeError,
    > {
        let write = self
            .router
            .route(holding_id, data_profile_key, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| redacted_asset_error("write database routing"))?;
        let read = match self
            .router
            .route(holding_id, data_profile_key, RoutedDatabaseSide::Read)
            .await
        {
            Ok(read) => read,
            Err(_) => {
                write.close().await;
                return Err(redacted_asset_error("read database routing"));
            }
        };
        Ok((write, read))
    }
}

fn validate_asset_reference(value: &str, field: &str) -> Result<(), WorkerRuntimeError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(WorkerRuntimeError::Configuration(format!(
            "Asset/Maintenance {field} is invalid"
        )));
    }
    Ok(())
}

fn redacted_asset_error(operation: &str) -> WorkerRuntimeError {
    tracing::error!(operation, "Asset/Maintenance projection worker seam failed");
    WorkerRuntimeError::Configuration(format!("Asset/Maintenance {operation} failed"))
}

fn next_asset_event_count(events_seen: u64, page_len: usize) -> Result<u64, PersistenceError> {
    let page_len = u64::try_from(page_len).map_err(|_| {
        PersistenceError::InvalidData("Asset/Maintenance page event count is invalid".into())
    })?;
    let next = events_seen.checked_add(page_len).ok_or_else(|| {
        PersistenceError::InvalidData("Asset/Maintenance event count overflow".into())
    })?;
    if next > ASSET_MAINTENANCE_PROJECTION_MAX_EVENTS {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance projection source event bound exceeded".into(),
        ));
    }
    Ok(next)
}

fn validate_asset_source_event_page(
    previous: Option<&AssetMaintenanceSourceEventCursor>,
    page: &AssetMaintenanceSourceEventPage,
    page_size: u32,
) -> Result<(), PersistenceError> {
    if page.events.len() > usize::try_from(page_size).unwrap_or(usize::MAX) {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance source page exceeds its requested size".into(),
        ));
    }
    if page.has_more && page.events.is_empty() {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance source page cannot advance without events".into(),
        ));
    }
    let mut prior = previous.map(|cursor| {
        (
            cursor.aggregate_type.as_str(),
            cursor.aggregate_id,
            cursor.aggregate_version,
        )
    });
    for event in &page.events {
        let current = (
            event.aggregate_type.as_str(),
            event.aggregate_id,
            event.aggregate_version,
        );
        if prior.is_some_and(|prior| current <= prior) {
            return Err(PersistenceError::InvalidData(
                "Asset/Maintenance source page cursor did not advance".into(),
            ));
        }
        prior = Some(current);
    }
    match (page.has_more, page.next_cursor.as_ref(), page.events.last()) {
        (true, Some(next), Some(last))
            if next.aggregate_type == last.aggregate_type
                && next.aggregate_id == last.aggregate_id
                && next.aggregate_version == last.aggregate_version =>
        {
            Ok(())
        }
        (false, None, _) => Ok(()),
        _ => Err(PersistenceError::InvalidData(
            "Asset/Maintenance source page continuation is invalid".into(),
        )),
    }
}

#[cfg(test)]
fn readiness_state_from_observation(state: ReadinessObservationState) -> ReadinessState {
    state.into()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkerHandlerReport {
    pub claimed: u64,
    pub processed: u64,
    pub published: u64,
    pub retried: u64,
    pub dead_lettered: u64,
    pub has_work: bool,
}

impl From<WorkerRunReport> for WorkerHandlerReport {
    fn from(report: WorkerRunReport) -> Self {
        Self {
            claimed: u64::from(report.claimed),
            processed: u64::from(report.processed),
            published: u64::from(report.published),
            retried: u64::from(report.retried),
            dead_lettered: u64::from(report.dead_lettered),
            has_work: report.claimed != 0,
        }
    }
}

#[async_trait::async_trait]
pub trait NamedWorkerHandler: Send + Sync {
    fn name(&self) -> &str;

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError>;
}

#[derive(Clone, Default)]
pub struct WorkerHandlerRegistry {
    handlers: BTreeMap<String, Arc<dyn NamedWorkerHandler>>,
}

impl fmt::Debug for WorkerHandlerRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorkerHandlerRegistry")
            .field("names", &self.handlers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl WorkerHandlerRegistry {
    pub fn register(
        &mut self,
        handler: Arc<dyn NamedWorkerHandler>,
    ) -> Result<(), WorkerRuntimeError> {
        let name = handler.name().trim();
        if name.is_empty()
            || name.len() > 64
            || name.chars().any(|character| character.is_control())
        {
            return Err(WorkerRuntimeError::Configuration(
                "worker handler name is invalid".into(),
            ));
        }
        if self.handlers.contains_key(name) {
            return Err(WorkerRuntimeError::Configuration(
                "worker handler name is already registered".into(),
            ));
        }
        self.handlers.insert(name.into(), handler);
        Ok(())
    }

    pub fn names(&self) -> Vec<String> {
        self.handlers.keys().cloned().collect()
    }

    fn replace_registered(
        &mut self,
        handler: Arc<dyn NamedWorkerHandler>,
    ) -> Result<(), WorkerRuntimeError> {
        let name = handler.name().trim();
        if !self.handlers.contains_key(name) {
            return Err(WorkerRuntimeError::Configuration(
                "worker handler replacement target is not registered".into(),
            ));
        }
        self.handlers.insert(name.to_owned(), handler);
        Ok(())
    }

    fn get(&self, name: &str) -> Option<Arc<dyn NamedWorkerHandler>> {
        self.handlers.get(name).cloned()
    }
}

/// A context supplies the physical read-database cutover implementation. The
/// worker only activates a replacement after a complete rebuild succeeds;
/// failed rebuilds leave the currently active read database untouched.
#[async_trait::async_trait]
pub trait ProjectionReadDatabaseSwitcher: Send + Sync {
    fn replacement_holding_id(&self) -> &str;
    fn replacement_candidate_database_key(&self) -> &str;
    fn replacement_source_version(&self) -> u64;
    async fn prepare_replacement(&self, projection_name: &str) -> Result<(), WorkerError>;
    /// Leave the current read database active after a failed rebuild. The
    /// reason is selected by the worker and must never contain provider or
    /// database details.
    async fn abort_replacement(
        &self,
        projection_name: &str,
        reason: &'static str,
    ) -> Result<(), WorkerError>;
    async fn activate_replacement(&self, projection_name: &str) -> Result<(), WorkerError>;
}

/// A context-specific projector that can rebuild against an explicitly
/// selected candidate read pool. The source write route remains owned by the
/// context; the candidate pool is borrowed only for the rebuild lifetime.
#[async_trait::async_trait]
pub trait CandidateProjectionRebuilder: Send + Sync {
    async fn rebuild_candidate(
        &self,
        projection_name: &str,
        candidate: &RoutedHoldingDatabase,
        source_version: u64,
    ) -> Result<erpx_infrastructure::ProjectionRebuildReport, WorkerError>;
}

/// Injected preparation for the physical candidate. Production composition
/// must migrate and verify the candidate before a projector receives it;
/// tests may provide a deterministic fake without a database.
#[async_trait::async_trait]
pub trait CandidateReadDatabasePreparer: Send + Sync {
    async fn prepare_candidate(
        &self,
        request: &ProjectionCandidateRebuildRequest,
    ) -> Result<(), WorkerError>;
}

/// Fully bound input for one physical replacement-read rebuild. Keeping the
/// holding, holding, candidate, projection, and source checkpoint together
/// prevents callers from accidentally mixing independently selected values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionCandidateRebuildRequest {
    pub projection_name: String,
    pub holding_id: String,
    pub data_profile_key: String,
    pub candidate_read_database_key: String,
    pub source_version: u64,
}

impl ProjectionCandidateRebuildRequest {
    pub fn new(
        projection_name: impl Into<String>,
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        candidate_read_database_key: impl Into<String>,
        source_version: u64,
    ) -> Result<Self, WorkerRuntimeError> {
        let request = Self {
            projection_name: projection_name.into(),
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            candidate_read_database_key: candidate_read_database_key.into(),
            source_version,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), WorkerRuntimeError> {
        validate_switch_reference(&self.projection_name, "projection_name")?;
        validate_switch_reference(&self.holding_id, "holding_id")?;
        validate_switch_reference(&self.data_profile_key, "data_profile_key")?;
        validate_switch_reference(
            &self.candidate_read_database_key,
            "candidate_read_database_key",
        )?;
        if self.source_version == 0
            || self.candidate_read_database_key.len() > 63
            || self.candidate_read_database_key.chars().any(|character| {
                !(character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
            })
            || self.projection_name.len() > erpx_infrastructure::MAX_PROJECTION_NAME_LENGTH
            || self.projection_name.chars().any(|character| {
                !(character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.'))
            })
        {
            return Err(WorkerRuntimeError::Configuration(
                "projection candidate rebuild request is invalid".into(),
            ));
        }
        Ok(())
    }
}

/// Control-plane metadata switcher for a physically rebuilt holding read
/// database. The context-specific projector must complete its rebuild between
/// `prepare_replacement` and `activate_replacement`; this adapter only owns
/// the authoritative, transactional control-plane state transition.
#[derive(Debug, Clone)]
pub struct SqlxHoldingProjectionReadDatabaseSwitcher {
    provisioner: HoldingProvisioner,
    holding_id: String,
    candidate_read_database_key: String,
    source_version: u64,
    projection_name: String,
    generation: Arc<tokio::sync::Mutex<Option<u64>>>,
}

#[async_trait::async_trait]
impl CandidateReadDatabasePreparer for HoldingDatabaseProvisioner {
    async fn prepare_candidate(
        &self,
        request: &ProjectionCandidateRebuildRequest,
    ) -> Result<(), WorkerError> {
        self.provision_replacement_read_database(
            &request.holding_id,
            &request.data_profile_key,
            &request.candidate_read_database_key,
        )
        .await
        .map_err(|_| WorkerError::Permanent("candidate read preparation failed".into()))
    }
}

impl SqlxHoldingProjectionReadDatabaseSwitcher {
    pub fn new(
        control: Arc<SqlxControlPlane>,
        holding_id: impl Into<String>,
        candidate_read_database_key: impl Into<String>,
        source_version: u64,
        projection_name: impl Into<String>,
    ) -> Result<Self, WorkerRuntimeError> {
        let holding_id = holding_id.into();
        let candidate_read_database_key = candidate_read_database_key.into();
        let projection_name = projection_name.into();
        validate_switch_reference(&holding_id, "holding_id")?;
        validate_switch_reference(&candidate_read_database_key, "candidate_read_database_key")?;
        validate_switch_reference(&projection_name, "projection_name")?;
        if source_version == 0
            || candidate_read_database_key.len() > 63
            || candidate_read_database_key.chars().any(|character| {
                !(character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
            })
            || projection_name.len() > erpx_infrastructure::MAX_PROJECTION_NAME_LENGTH
            || projection_name.chars().any(|character| {
                !(character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.'))
            })
        {
            return Err(WorkerRuntimeError::Configuration(
                "projection replacement configuration is invalid".into(),
            ));
        }
        Ok(Self {
            provisioner: HoldingProvisioner::new(control),
            holding_id,
            candidate_read_database_key,
            source_version,
            projection_name,
            generation: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    pub fn holding_id(&self) -> &str {
        &self.holding_id
    }

    pub fn candidate_read_database_key(&self) -> &str {
        &self.candidate_read_database_key
    }

    pub fn source_version(&self) -> u64 {
        self.source_version
    }

    fn require_projection(&self, projection_name: &str) -> Result<(), WorkerError> {
        if projection_name == self.projection_name {
            Ok(())
        } else {
            Err(WorkerError::Permanent(
                "projection replacement name does not match".into(),
            ))
        }
    }
}

#[async_trait::async_trait]
impl ProjectionReadDatabaseSwitcher for SqlxHoldingProjectionReadDatabaseSwitcher {
    fn replacement_holding_id(&self) -> &str {
        &self.holding_id
    }

    fn replacement_candidate_database_key(&self) -> &str {
        &self.candidate_read_database_key
    }

    fn replacement_source_version(&self) -> u64 {
        self.source_version
    }

    async fn prepare_replacement(&self, projection_name: &str) -> Result<(), WorkerError> {
        self.require_projection(projection_name)?;
        let generation = self
            .provisioner
            .begin_read_rebuild_with_generation(
                &self.holding_id,
                &self.candidate_read_database_key,
                self.source_version,
            )
            .await
            .map_err(|_| {
                WorkerError::Permanent("projection replacement preparation failed".into())
            })?;
        *self.generation.lock().await = Some(generation);
        Ok(())
    }

    async fn abort_replacement(
        &self,
        projection_name: &str,
        reason: &'static str,
    ) -> Result<(), WorkerError> {
        self.require_projection(projection_name)?;
        let generation = *self.generation.lock().await;
        match generation {
            Some(generation) => {
                self.provisioner
                    .fail_read_rebuild_if_generation(
                        &self.holding_id,
                        &self.candidate_read_database_key,
                        generation,
                        reason,
                    )
                    .await
            }
            None => {
                self.provisioner
                    .fail_read_rebuild(&self.holding_id, &self.candidate_read_database_key, reason)
                    .await
            }
        }
        .map_err(|_| WorkerError::Permanent("projection replacement abort failed".into()))
    }

    async fn activate_replacement(&self, projection_name: &str) -> Result<(), WorkerError> {
        self.require_projection(projection_name)?;
        // This legacy metadata-only adapter cannot hold the holding source
        // fence through activation. Production callers must use the routed
        // scheduler path, which owns the source lock and final watermark
        // check. Failing closed prevents this compatibility seam from
        // promoting a stale ready candidate.
        return Err(WorkerError::Permanent(
            "direct replacement activation requires the fenced routed scheduler".into(),
        ));
    }
}

fn validate_switch_reference(value: &str, field: &str) -> Result<(), WorkerRuntimeError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(WorkerRuntimeError::Configuration(format!(
            "projection replacement {field} is invalid"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionSwitchReport {
    pub rebuild: ProjectionRebuildReport,
    pub activated: bool,
}

#[async_trait::async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, event: &OutboxEnvelope) -> Result<(), WorkerError>;
}

#[derive(Debug, Clone)]
pub struct PublisherHandler<P> {
    publisher: Arc<P>,
}

impl<P> PublisherHandler<P>
where
    P: EventPublisher + 'static,
{
    pub fn new(publisher: Arc<P>) -> Self {
        Self { publisher }
    }
}

#[async_trait::async_trait]
impl<P> OutboxHandler for PublisherHandler<P>
where
    P: EventPublisher + 'static,
{
    async fn handle(&self, event: &OutboxEnvelope) -> Result<(), WorkerError> {
        self.publisher.publish(event).await
    }
}

#[derive(Debug, Clone)]
pub struct WorkerRuntimeConfig {
    pub worker_id: Uuid,
    pub worker_concurrency: u32,
    pub batch_size: u32,
    pub claim_timeout: chrono::Duration,
    pub poll_interval: StdDuration,
    pub retry_policy: RetryPolicy,
}

impl Default for WorkerRuntimeConfig {
    fn default() -> Self {
        Self {
            worker_id: Uuid::new_v4(),
            worker_concurrency: 1,
            batch_size: 100,
            claim_timeout: chrono::Duration::minutes(5),
            poll_interval: StdDuration::from_secs(1),
            retry_policy: RetryPolicy::default(),
        }
    }
}

impl WorkerRuntimeConfig {
    /// Read only worker scheduling settings. Database URLs are intentionally
    /// parsed by the infrastructure configuration so server and worker use
    /// the same required, physically separated variables.
    pub fn from_env() -> Result<Self, WorkerRuntimeError> {
        let defaults = Self::default();
        Ok(Self {
            worker_id: optional_uuid("ERPX_WORKER_ID")?.unwrap_or(defaults.worker_id),
            worker_concurrency: optional_u32("ERPX_WORKER_CONCURRENCY")?
                .unwrap_or(defaults.worker_concurrency)
                .clamp(1, 16),
            batch_size: optional_u32("ERPX_WORKER_BATCH_SIZE")?
                .unwrap_or(defaults.batch_size)
                .max(1),
            claim_timeout: chrono::Duration::seconds(
                optional_u64("ERPX_WORKER_CLAIM_TIMEOUT_SECONDS")?
                    .unwrap_or(defaults.claim_timeout.num_seconds() as u64)
                    .max(1)
                    .try_into()
                    .map_err(|_| {
                        WorkerRuntimeError::Configuration(
                            "ERPX_WORKER_CLAIM_TIMEOUT_SECONDS is too large".into(),
                        )
                    })?,
            ),
            poll_interval: StdDuration::from_millis(
                optional_u64("ERPX_WORKER_POLL_INTERVAL_MS")?
                    .unwrap_or(defaults.poll_interval.as_millis() as u64)
                    .max(1),
            ),
            retry_policy: RetryPolicy {
                base_delay: chrono::Duration::seconds(
                    optional_u64("ERPX_WORKER_RETRY_BASE_SECONDS")?
                        .unwrap_or(defaults.retry_policy.base_delay.num_seconds() as u64)
                        .max(1)
                        .try_into()
                        .map_err(|_| {
                            WorkerRuntimeError::Configuration(
                                "ERPX_WORKER_RETRY_BASE_SECONDS is too large".into(),
                            )
                        })?,
                ),
                max_delay: chrono::Duration::seconds(
                    optional_u64("ERPX_WORKER_RETRY_MAX_SECONDS")?
                        .unwrap_or(defaults.retry_policy.max_delay.num_seconds() as u64)
                        .max(1)
                        .try_into()
                        .map_err(|_| {
                            WorkerRuntimeError::Configuration(
                                "ERPX_WORKER_RETRY_MAX_SECONDS is too large".into(),
                            )
                        })?,
                ),
            },
        })
    }
}

#[derive(Debug, Clone)]
pub struct WorkerRuntime {
    store: Arc<SqlxControlPlane>,
    worker: OutboxWorker,
    config: WorkerRuntimeConfig,
    heartbeat: WorkerHeartbeat,
}

impl WorkerRuntime {
    pub async fn connect(config: WorkerRuntimeConfig) -> Result<Self, WorkerRuntimeError> {
        let database_config = ControlPlaneDatabaseConfig::from_env()
            .map_err(|error| WorkerRuntimeError::Configuration(error.into()))?;
        let pools = ControlPlanePools::connect_worker(
            &database_config,
            ControlPlanePoolSettings {
                // Every worker is a separately supervised process and the
                // local composition starts dozens of them at once. One
                // eagerly-used control connection per worker is sufficient
                // for the bounded outbox loop and keeps the shared local
                // PostgreSQL cluster below its connection budget during the
                // startup health handoff. The read side remains lazy.
                write_max_connections: 1,
                read_max_connections: 1,
                acquire_timeout: StdDuration::from_secs(10),
            },
        )
        .await?;
        // The fresh-state manifest is published only after the bootstrap has
        // migrated all four databases and committed its complete receipt.
        // Avoid making every worker contend on the same migration advisory
        // locks during the workers-before-Server startup barrier.
        if env::var_os("ERPX_FRESH_STATE_MANIFEST").is_none() {
            migrate_control_plane(&pools.write).await?;
            migrate_control_read(&pools.read).await?;
            // Worker-side authorization rechecks directory-managed roles at
            // the five-minute revocation boundary. Keep the identity schema
            // present for standalone/legacy worker composition.
            erpx_identity::apply_identity_migration(&pools.write)
                .await
                .map_err(|error| {
                    WorkerRuntimeError::Configuration(format!(
                        "identity migration is unavailable: {error}"
                    ))
                })?;
        }
        let store = Arc::new(SqlxControlPlane::new(pools));
        store.require_erpx_command_owner().await?;
        let worker = OutboxWorker::new(Arc::clone(&store), config.worker_id)
            .with_batch_size(config.batch_size)
            .with_claim_timeout(config.claim_timeout)
            .with_retry_policy(config.retry_policy);
        let worker_id = config.worker_id;
        Ok(Self {
            store,
            worker,
            config,
            heartbeat: WorkerHeartbeat::new(worker_id),
        })
    }

    pub fn store(&self) -> &Arc<SqlxControlPlane> {
        &self.store
    }

    pub fn worker(&self) -> &OutboxWorker {
        &self.worker
    }

    pub fn config(&self) -> &WorkerRuntimeConfig {
        &self.config
    }

    pub fn heartbeat(&self) -> &WorkerHeartbeat {
        &self.heartbeat
    }

    pub fn readiness_observer(&self) -> WorkerReadinessObserver {
        WorkerReadinessObserver::from_heartbeat(self.heartbeat.clone())
    }

    /// Record a scheduler-owned successful pass. The method does not expose
    /// any connection data and rejects no timestamps itself; observation
    /// classifies future and stale values as unavailable.
    pub fn record_heartbeat_at(&self, observed_at: DateTime<Utc>) -> bool {
        self.heartbeat.record_at(observed_at)
    }

    /// Compose the worker heartbeat with live holding routing and the durable
    /// projection/backup observations. A persistence failure is returned to
    /// the caller rather than being converted into a green snapshot.
    pub async fn observe_live_readiness(
        &self,
        router: &HoldingDatabaseRouter,
        holding_id: &str,
        now: DateTime<Utc>,
        policy: WorkerLiveReadinessPolicy,
    ) -> Result<WorkerLiveReadinessObservation, WorkerRuntimeError> {
        Ok(self
            .readiness_observer()
            .observe_live_readiness(router, holding_id, now, policy)
            .await?)
    }

    pub fn readiness_snapshot(
        &self,
        heartbeat: WorkerHeartbeatReadiness,
        metadata: WorkerReadinessMetadata<'_>,
    ) -> WorkerReadinessComposition {
        WorkerReadinessComposition::from_metadata(heartbeat, metadata)
    }

    pub fn handler_registry(&self) -> Result<WorkerHandlerRegistry, WorkerRuntimeError> {
        let mut registry = WorkerHandlerRegistry::default();
        registry.register(Arc::new(AgentAssignmentRuntimeHandler::unavailable()))?;
        registry.register(Arc::new(IncidentRegisteredHandler {
            store: Arc::clone(&self.store),
            batch_size: self.config.batch_size,
            claim_timeout: self.config.claim_timeout,
            retry_policy: self.config.retry_policy,
        }))?;
        registry.register(Arc::new(PlatformCompletionRuntimeRegisteredHandler {
            store: Arc::clone(&self.store),
            batch_size: self.config.batch_size,
            claim_timeout: self.config.claim_timeout,
            retry_policy: self.config.retry_policy,
        }))?;
        registry.register(Arc::new(StripeRegisteredHandler {
            store: Arc::clone(&self.store),
            config: self.config.clone(),
        }))?;
        registry.register(Arc::new(StripeBillingPolicyRegisteredHandler {
            store: Arc::clone(&self.store),
            batch_size: self
                .config
                .batch_size
                .min(erpx_infrastructure::MAX_BILLING_POLICY_BATCH_SIZE),
        }))?;
        registry.register(Arc::new(AuditRegisteredHandler {
            store: Arc::clone(&self.store),
        }))?;
        registry.register(Arc::new(SupportCapabilityExpiryHandler::new(
            SupportPersistence::new(self.store.pools().write.clone()),
            self.config.batch_size,
        )))?;
        registry.register(Arc::new(SupportAttachmentExpiryHandler::new(
            SupportPersistence::new(self.store.pools().write.clone()),
            self.config.batch_size,
        )))?;
        Ok(registry)
    }

    /// Replaces the explicit fail-closed Agent Orchestration registration only
    /// after the composition root has supplied every durable queue, current
    /// policy, cancellation/budget guard, protected-material, provider, and
    /// notification dependency.
    pub fn handler_registry_with_agent_assignment(
        &self,
        runner: Arc<dyn AgentAssignmentPass>,
    ) -> Result<WorkerHandlerRegistry, WorkerRuntimeError> {
        let mut registry = self.handler_registry()?;
        registry.replace_registered(Arc::new(AgentAssignmentRuntimeHandler::available(runner)))?;
        Ok(registry)
    }

    /// Compose and register the production Agent handler from trusted active
    /// holding routes. The caller must supply real provider/KMS authority and
    /// object storage; no default or environment holding identity is accepted.
    pub fn handler_registry_with_trusted_agent_assignment<D>(
        &self,
        holding_registry: erpx_infrastructure::SqlxHoldingStoreRegistry,
        router: HoldingDatabaseRouter,
        objects: erpx_infrastructure::MinioObjectStore,
        dependencies: D,
        config: TrustedAgentHoldingRuntimeConfig,
    ) -> Result<WorkerHandlerRegistry, WorkerRuntimeError>
    where
        D: TrustedAgentHoldingDependencyPort + 'static,
    {
        let runner = compose_trusted_holding_agent_runner(
            holding_registry,
            router,
            objects,
            dependencies,
            config,
        )
        .map_err(|_| {
            WorkerRuntimeError::Configuration(
                "trusted Agent holding dependencies are unavailable".into(),
            )
        })?;
        self.handler_registry_with_agent_assignment(runner)
    }

    /// Compose the G4 support session/capability/incident projector only when
    /// its physically separated workflow read store is supplied. The default
    /// registry remains unchanged and therefore cannot claim support events
    /// accidentally during a partial deployment.
    pub fn handler_registry_with_support_workflow(
        &self,
        workflow_store: SupportWorkflowStore,
    ) -> Result<WorkerHandlerRegistry, WorkerRuntimeError> {
        let mut registry = self.handler_registry()?;
        registry.register(Arc::new(SupportWorkflowHandler::new(
            Arc::clone(&self.store),
            workflow_store,
            self.config.worker_id,
            self.config.batch_size,
        )))?;
        Ok(registry)
    }

    /// Opt-in registration for the durable Finance bank-provider import
    /// workflow. Provider credential resolution, authorization, and adapter
    /// construction are explicit arguments; the default registry never claims
    /// a bank import without all three boundaries.
    #[allow(clippy::too_many_arguments)]
    pub fn handler_registry_with_finance_bank_provider_import<A, R, F>(
        &self,
        router: HoldingDatabaseRouter,
        authorizer: A,
        credential_resolver: R,
        adapter_factory: F,
        batch_size: u32,
        lease: Duration,
    ) -> Result<WorkerHandlerRegistry, WorkerRuntimeError>
    where
        A: FinanceBankProviderImportAuthorizer + 'static,
        R: BankProviderCredentialResolver + 'static,
        F: FinanceBankProviderAdapterFactory + 'static,
    {
        let source = SqlxFinanceBankProviderImportJobSource::new(&self.store);
        let handler = finance_banking_import::FinanceBankProviderImportHandler::new(
            source,
            Arc::clone(&self.store),
            router,
            authorizer,
            credential_resolver,
            adapter_factory,
            batch_size,
            lease,
        )?;
        let mut registry = self.handler_registry()?;
        registry.register(Arc::new(handler))?;
        Ok(registry)
    }

    /// Register role-review expiry delivery only with an explicit publisher.
    /// The default registry never claims role-review notifications without a
    /// configured delivery boundary.
    pub fn handler_registry_with_role_review_notifications<P>(
        &self,
        scope: erpx_domain::contracts::AuthorityScope,
        publisher: P,
        claimant_ref: impl Into<String>,
        horizon: Duration,
        lease: Duration,
    ) -> Result<WorkerHandlerRegistry, WorkerRuntimeError>
    where
        P: RoleReviewNotificationPublisher,
    {
        let source = RoleReviewPersistence::new(self.store.pools().write.clone());
        let handler = RoleReviewNotificationHandler::new(
            source,
            publisher,
            scope,
            claimant_ref,
            horizon,
            lease,
        )?;
        let mut registry = self.handler_registry()?;
        registry.register(Arc::new(handler))?;
        Ok(registry)
    }

    /// Compose a Legal retention sweep only from an explicitly routed holding
    /// write runner. Holding enumeration and scheduling remain an external
    /// integration until a control-plane source can provide a safe durable
    /// per-holding claim boundary.
    pub fn handler_registry_with_legal_retention(
        &self,
        runner: LegalRetentionRunner,
    ) -> Result<WorkerHandlerRegistry, WorkerRuntimeError> {
        let mut registry = self.handler_registry()?;
        registry.register(Arc::new(runner))?;
        Ok(registry)
    }

    pub fn handler_registry_with_support_sla_source<S>(
        &self,
        source: S,
    ) -> Result<WorkerHandlerRegistry, WorkerRuntimeError>
    where
        S: SupportSlaNotificationSource + 'static,
    {
        let mut registry = self.handler_registry()?;
        registry.register(Arc::new(SupportSlaNotificationHandler::new(
            source,
            self.config.batch_size,
        )))?;
        Ok(registry)
    }

    /// Register Marketing delivery only when a holding-scoped source and an
    /// explicit provider adapter are supplied. The default registry remains
    /// fail-closed and never claims delivery rows without both boundaries.
    pub fn handler_registry_with_marketing_delivery_source<S, P>(
        &self,
        source: S,
        provider: P,
    ) -> Result<WorkerHandlerRegistry, WorkerRuntimeError>
    where
        S: MarketingDeliverySource,
        P: MarketingDeliveryProvider,
    {
        let mut registry = self.handler_registry()?;
        registry.register(Arc::new(MarketingDeliveryHandler::new(
            source,
            provider,
            self.config
                .batch_size
                .min(MARKETING_DELIVERY_MAX_BATCH_SIZE),
            self.config.claim_timeout,
        )?))?;
        Ok(registry)
    }

    /// Compose the durable control-read status publisher from the runtime's
    /// physically separated control pools. This handler is opt-in and never
    /// claims scheduler jobs; it only publishes metadata from the
    /// control-write scheduler into the rebuildable control-read table.
    pub fn handler_registry_with_control_plane_holding_projection_status_projection(
        &self,
    ) -> Result<WorkerHandlerRegistry, WorkerRuntimeError> {
        let publisher = SqlxHoldingProjectionStatusProjectionStore::new(Arc::clone(&self.store));
        let mut registry = self.handler_registry()?;
        registry.register(Arc::new(HoldingProjectionStatusProjectionHandler::new(
            publisher,
        )))?;
        Ok(registry)
    }

    /// Register holding projection execution only when the caller supplies an
    /// exact holding/holding binding, durable scheduler source, and projector.
    /// The default worker registry never claims scheduler rows implicitly.
    pub fn handler_registry_with_holding_projection_scheduler<S, E>(
        &self,
        source: S,
        executor: E,
        binding: erpx_infrastructure::HoldingHoldingBinding,
        claim_key_prefix: impl Into<String>,
    ) -> Result<WorkerHandlerRegistry, WorkerRuntimeError>
    where
        S: HoldingProjectionSchedulerSource,
        E: HoldingProjectionExecutor,
    {
        let mut registry = self.handler_registry()?;
        registry.register(Arc::new(HoldingProjectionSchedulerHandler::new(
            source,
            executor,
            binding,
            claim_key_prefix,
            self.config
                .batch_size
                .min(HOLDING_PROJECTION_SCHEDULER_MAX_BATCH_SIZE),
            self.config.claim_timeout,
        )?))?;
        Ok(registry)
    }

    /// Compose the durable control-plane scheduler source from the runtime's
    /// own pools and holding registry. The projector remains an explicit
    /// dependency so a worker cannot claim jobs without a real projector, but
    /// production callers no longer need to construct an unbound scheduler
    /// repository by hand.
    pub fn handler_registry_with_control_plane_holding_projection_scheduler<E>(
        &self,
        executor: E,
        binding: erpx_infrastructure::HoldingHoldingBinding,
        authorization: erpx_infrastructure::ProjectionSchedulerAuthorization,
        claim_key_prefix: impl Into<String>,
    ) -> Result<WorkerHandlerRegistry, WorkerRuntimeError>
    where
        E: HoldingProjectionExecutor,
    {
        let source = erpx_infrastructure::HoldingProjectionSchedulerRepository::new(
            self.store.pools().write.clone(),
            Arc::new(erpx_infrastructure::SqlxHoldingStoreRegistry::new(
                Arc::clone(&self.store),
            )),
            authorization,
        );
        self.handler_registry_with_holding_projection_scheduler(
            source,
            executor,
            binding,
            claim_key_prefix,
        )
    }

    /// Compose the multi-holding scheduler from active control-plane registry
    /// enumeration, the physical holding router, and an executor that accepts
    /// both routed database sides. Every pass re-enumerates active holdings so
    /// suspension takes effect without a process restart.
    pub fn handler_registry_with_control_plane_multi_holding_projection_scheduler<E>(
        &self,
        router: HoldingDatabaseRouter,
        executor: E,
        authorization: erpx_infrastructure::ProjectionSchedulerAuthorization,
        claim_key_prefix: impl Into<String>,
    ) -> Result<WorkerHandlerRegistry, WorkerRuntimeError>
    where
        E: HoldingProjectionRoutedExecutor,
    {
        let registry = erpx_infrastructure::SqlxHoldingStoreRegistry::new(Arc::clone(&self.store));
        let source = erpx_infrastructure::HoldingProjectionSchedulerRepository::new(
            self.store.pools().write.clone(),
            Arc::new(registry.clone()),
            authorization,
        );
        let executor = RoutedHoldingProjectionExecutor::new(router, executor);
        let handler = MultiHoldingProjectionSchedulerHandler::new(
            source,
            executor,
            registry,
            claim_key_prefix,
            self.config
                .batch_size
                .min(HOLDING_PROJECTION_SCHEDULER_MAX_BATCH_SIZE),
            self.config.claim_timeout,
        )?;
        let mut worker_registry = self.handler_registry()?;
        worker_registry.register(Arc::new(handler))?;
        Ok(worker_registry)
    }

    /// Compose the routed scheduler while preserving an explicit unfenced
    /// rebuild mode for projectors that support both ordinary rebuilds and
    /// replacement-read candidate rebuilds. Candidate payloads still use the
    /// generic source-lock and activation path.
    pub fn handler_registry_with_control_plane_multi_holding_projection_scheduler_unfenced_rebuilds<
        E,
    >(
        &self,
        router: HoldingDatabaseRouter,
        executor: E,
        authorization: erpx_infrastructure::ProjectionSchedulerAuthorization,
        claim_key_prefix: impl Into<String>,
    ) -> Result<WorkerHandlerRegistry, WorkerRuntimeError>
    where
        E: HoldingProjectionRoutedExecutor,
    {
        let registry = erpx_infrastructure::SqlxHoldingStoreRegistry::new(Arc::clone(&self.store));
        let source = erpx_infrastructure::HoldingProjectionSchedulerRepository::new(
            self.store.pools().write.clone(),
            Arc::new(registry.clone()),
            authorization,
        );
        let executor =
            RoutedHoldingProjectionExecutor::new_allow_unfenced_rebuilds(router, executor);
        let handler = MultiHoldingProjectionSchedulerHandler::new(
            source,
            executor,
            registry,
            claim_key_prefix,
            self.config
                .batch_size
                .min(HOLDING_PROJECTION_SCHEDULER_MAX_BATCH_SIZE),
            self.config.claim_timeout,
        )?;
        let mut worker_registry = self.handler_registry()?;
        worker_registry.register(Arc::new(handler))?;
        Ok(worker_registry)
    }

    /// Compose the same durable multi-holding scheduler for an executor that
    /// owns an existing routed runner. This is intentionally not a second
    /// scheduler: claims, leases, retries, dead letters, authorization, and
    /// active-holding enumeration remain the established scheduler path.
    pub fn handler_registry_with_control_plane_multi_holding_projection_scheduler_executor<E>(
        &self,
        executor: E,
        authorization: erpx_infrastructure::ProjectionSchedulerAuthorization,
        claim_key_prefix: impl Into<String>,
    ) -> Result<WorkerHandlerRegistry, WorkerRuntimeError>
    where
        E: HoldingProjectionExecutor,
    {
        let registry = erpx_infrastructure::SqlxHoldingStoreRegistry::new(Arc::clone(&self.store));
        let source = erpx_infrastructure::HoldingProjectionSchedulerRepository::new(
            self.store.pools().write.clone(),
            Arc::new(registry.clone()),
            authorization,
        );
        let handler = MultiHoldingProjectionSchedulerHandler::new(
            source,
            executor,
            registry,
            claim_key_prefix,
            self.config
                .batch_size
                .min(HOLDING_PROJECTION_SCHEDULER_MAX_BATCH_SIZE),
            self.config.claim_timeout,
        )?;
        let mut worker_registry = self.handler_registry()?;
        worker_registry.register(Arc::new(handler))?;
        Ok(worker_registry)
    }

    /// Register restore-test work only when a holding-scoped source is
    /// explicitly composed. The default registry remains fail-closed and
    /// cannot claim recovery work without an adapter.
    pub fn handler_registry_with_restore_test_source<S>(
        &self,
        source: S,
        holding_id: impl Into<String>,
    ) -> Result<WorkerHandlerRegistry, WorkerRuntimeError>
    where
        S: RestoreTestScheduleSource + 'static,
    {
        let mut registry = self.handler_registry()?;
        registry.register(Arc::new(RestoreTestScheduleHandler::new(
            source,
            holding_id,
            self.config.batch_size,
        )?))?;
        Ok(registry)
    }

    pub async fn run_registered_until_stopped(
        &self,
        registry: &WorkerHandlerRegistry,
        handler_name: &str,
        stop: tokio::sync::watch::Receiver<bool>,
    ) -> Result<WorkerLoopReport, WorkerRuntimeError> {
        let handler = registry.get(handler_name).ok_or_else(|| {
            WorkerRuntimeError::Configuration(format!(
                "unknown worker handler {handler_name}; registered handlers: {}",
                registry.names().join(", ")
            ))
        })?;
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..self.config.worker_concurrency {
            tasks.spawn(run_registered_handler_until_stopped(
                Arc::clone(&handler),
                self.heartbeat.clone(),
                Uuid::new_v4(),
                Some((Arc::clone(&self.store), self.config.worker_id)),
                stop.clone(),
                self.config.poll_interval,
            ));
        }

        let mut total = WorkerLoopReport::default();
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(Ok(report)) => merge_worker_loop_report(&mut total, report),
                Ok(Err(error)) => {
                    tasks.abort_all();
                    return Err(error);
                }
                Err(error) => {
                    tasks.abort_all();
                    return Err(WorkerRuntimeError::Configuration(format!(
                        "worker handler task failed: {error}"
                    )));
                }
            }
        }
        Ok(total)
    }

    pub async fn run_registered_until_shutdown(
        &self,
        registry: &WorkerHandlerRegistry,
        handler_name: &str,
    ) -> Result<WorkerLoopReport, WorkerRuntimeError> {
        let (sender, receiver) = tokio::sync::watch::channel(false);
        let signal_task = tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            let _ = sender.send(true);
        });
        let result = self
            .run_registered_until_stopped(registry, handler_name, receiver)
            .await;
        signal_task.abort();
        result
    }

    pub async fn run_until_shutdown<H: OutboxHandler + ?Sized>(
        &self,
        handler: &H,
    ) -> Result<WorkerLoopReport, WorkerRuntimeError> {
        Ok(self
            .worker
            .run_until_shutdown(handler, self.config.poll_interval)
            .await?)
    }

    pub async fn run_until_stopped<H: OutboxHandler + ?Sized>(
        &self,
        handler: &H,
        stop: tokio::sync::watch::Receiver<bool>,
    ) -> Result<WorkerLoopReport, WorkerRuntimeError> {
        Ok(self
            .worker
            .run_until_stopped(handler, stop, self.config.poll_interval)
            .await?)
    }

    pub async fn rebuild_projection<H: ProjectionHandler + ?Sized>(
        &self,
        projection_name: &str,
        handler: &H,
    ) -> Result<ProjectionRebuildReport, WorkerRuntimeError> {
        Ok(self
            .worker
            .rebuild_projection(projection_name, handler)
            .await?)
    }

    pub fn incident_handler(&self) -> IncidentOutboxHandler {
        IncidentOutboxHandler {
            worker: self.worker.clone(),
            projection: IncidentProjection::new(Arc::clone(&self.store)),
        }
    }

    pub fn customer_access_audit_summary_projection(&self) -> CustomerAccessAuditSummaryProjection {
        CustomerAccessAuditSummaryProjection::new(Arc::clone(&self.store))
    }

    pub fn stripe_semantic_handler(&self) -> StripeSemanticHandler {
        StripeSemanticHandler::new(
            Arc::new(SqlxCommercialStore::new(&self.store)),
            self.config.clone(),
        )
    }

    pub async fn rebuild_and_switch<H, S>(
        &self,
        projection_name: &str,
        handler: &H,
        switcher: &S,
    ) -> Result<ProjectionSwitchReport, WorkerRuntimeError>
    where
        H: ProjectionHandler + ?Sized,
        S: ProjectionReadDatabaseSwitcher + ?Sized,
    {
        switcher
            .prepare_replacement(projection_name)
            .await
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let rebuild = match self.rebuild_projection(projection_name, handler).await {
            Ok(rebuild) => rebuild,
            Err(error) => {
                let _ = switcher
                    .abort_replacement(projection_name, "projection rebuild failed")
                    .await;
                return Err(error);
            }
        };
        if let Err(error) = switcher.activate_replacement(projection_name).await {
            let _ = switcher
                .abort_replacement(projection_name, "projection activation failed")
                .await;
            return Err(PersistenceError::InvalidData(error.to_string()).into());
        }
        Ok(ProjectionSwitchReport {
            rebuild,
            activated: true,
        })
    }

    /// Prepare a replacement, resolve its candidate read route from the
    /// server-only router, rebuild the context there, and activate it only
    /// after the candidate projector succeeds. The candidate pool is owned and
    /// closed by this method on every path.
    pub async fn rebuild_candidate_and_switch<H, P, S>(
        &self,
        request: ProjectionCandidateRebuildRequest,
        router: &HoldingDatabaseRouter,
        preparer: &P,
        handler: &H,
        switcher: &S,
    ) -> Result<ProjectionSwitchReport, WorkerRuntimeError>
    where
        H: CandidateProjectionRebuilder + ?Sized,
        P: CandidateReadDatabasePreparer + ?Sized,
        S: ProjectionReadDatabaseSwitcher + ?Sized,
    {
        request.validate()?;
        if switcher.replacement_holding_id() != request.holding_id
            || switcher.replacement_candidate_database_key() != request.candidate_read_database_key
            || switcher.replacement_source_version() != request.source_version
        {
            return Err(PersistenceError::InvalidData(
                "candidate replacement binding does not match the switcher".into(),
            )
            .into());
        }
        switcher
            .prepare_replacement(&request.projection_name)
            .await
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if preparer.prepare_candidate(&request).await.is_err() {
            let _ = switcher
                .abort_replacement(&request.projection_name, "candidate preparation failed")
                .await;
            return Err(
                PersistenceError::InvalidData("candidate read preparation failed".into()).into(),
            );
        }
        let candidate = match router
            .route_replacement_read(
                &request.holding_id,
                &request.data_profile_key,
                &request.candidate_read_database_key,
            )
            .await
        {
            Ok(candidate) => candidate,
            Err(_) => {
                let _ = switcher
                    .abort_replacement(&request.projection_name, "candidate route failed")
                    .await;
                return Err(
                    PersistenceError::InvalidData("candidate read route failed".into()).into(),
                );
            }
        };
        let rebuild = match handler
            .rebuild_candidate(&request.projection_name, &candidate, request.source_version)
            .await
        {
            Ok(rebuild) => rebuild,
            Err(error) => {
                let _ = switcher
                    .abort_replacement(&request.projection_name, "candidate rebuild failed")
                    .await;
                candidate.close().await;
                return Err(PersistenceError::InvalidData(error.to_string()).into());
            }
        };
        if let Err(error) = switcher
            .activate_replacement(&request.projection_name)
            .await
        {
            let _ = switcher
                .abort_replacement(&request.projection_name, "candidate activation failed")
                .await;
            candidate.close().await;
            return Err(PersistenceError::InvalidData(error.to_string()).into());
        }
        candidate.close().await;
        Ok(ProjectionSwitchReport {
            rebuild,
            activated: true,
        })
    }

    pub async fn replay_dead_letter(
        &self,
        event_id: Uuid,
        available_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), WorkerRuntimeError> {
        self.store
            .replay_dead_letter(event_id, available_at)
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct IncidentRegisteredHandler {
    store: Arc<SqlxControlPlane>,
    batch_size: u32,
    claim_timeout: chrono::Duration,
    retry_policy: RetryPolicy,
}

#[derive(Debug, Clone)]
struct PlatformCompletionRuntimeRegisteredHandler {
    store: Arc<SqlxControlPlane>,
    batch_size: u32,
    claim_timeout: chrono::Duration,
    retry_policy: RetryPolicy,
}

#[async_trait::async_trait]
impl NamedWorkerHandler for IncidentRegisteredHandler {
    fn name(&self) -> &str {
        INCIDENT_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let worker = OutboxWorker::new(Arc::clone(&self.store), worker_id)
            .with_batch_size(self.batch_size)
            .with_claim_timeout(self.claim_timeout)
            .with_retry_policy(self.retry_policy);
        let handler = IncidentOutboxHandler {
            worker: worker.clone(),
            projection: IncidentProjection::new(Arc::clone(&self.store)),
        };
        Ok(worker.run_once(&handler).await?.into())
    }
}

#[async_trait::async_trait]
impl NamedWorkerHandler for PlatformCompletionRuntimeRegisteredHandler {
    fn name(&self) -> &str {
        PLATFORM_COMPLETION_RUNTIME_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let adapter = PlatformCompletionRuntimeAdapter::new(Arc::clone(&self.store))
            .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
        let handler = adapter.worker_handler(
            worker_id,
            self.batch_size,
            self.claim_timeout,
            self.retry_policy,
        )?;
        handler.run_once(worker_id).await
    }
}

#[derive(Debug, Clone)]
struct StripeRegisteredHandler {
    store: Arc<SqlxControlPlane>,
    config: WorkerRuntimeConfig,
}

#[derive(Debug, Clone)]
struct StripeBillingPolicyRegisteredHandler {
    store: Arc<SqlxControlPlane>,
    batch_size: u32,
}

#[async_trait::async_trait]
impl NamedWorkerHandler for StripeRegisteredHandler {
    fn name(&self) -> &str {
        STRIPE_SEMANTIC_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let mut config = self.config.clone();
        config.worker_id = worker_id;
        let handler =
            StripeSemanticHandler::new(Arc::new(SqlxCommercialStore::new(&self.store)), config);
        let report = handler.process_once().await?;
        Ok(WorkerHandlerReport {
            claimed: report.claimed,
            processed: report.processed,
            retried: report.retried,
            dead_lettered: report.dead_lettered,
            has_work: report.claimed != 0,
            ..WorkerHandlerReport::default()
        })
    }
}

#[async_trait::async_trait]
impl NamedWorkerHandler for StripeBillingPolicyRegisteredHandler {
    fn name(&self) -> &str {
        STRIPE_BILLING_POLICY_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() || self.batch_size == 0 {
            return Err(WorkerRuntimeError::Configuration(
                "Stripe billing policy worker configuration is invalid".into(),
            ));
        }
        let store = SqlxCommercialStore::new(&self.store);
        let report = store
            .advance_due_billing_policies(
                &format!("erpx-worker:{worker_id}"),
                chrono::Utc::now(),
                self.batch_size,
            )
            .await?;
        Ok(WorkerHandlerReport {
            claimed: u64::from(report.considered),
            processed: u64::from(report.advanced),
            has_work: report.considered != 0,
            ..WorkerHandlerReport::default()
        })
    }
}

#[derive(Debug, Clone)]
struct AuditRegisteredHandler {
    store: Arc<SqlxControlPlane>,
}

#[async_trait::async_trait]
impl NamedWorkerHandler for AuditRegisteredHandler {
    fn name(&self) -> &str {
        CUSTOMER_ACCESS_AUDIT_HANDLER_NAME
    }

    async fn run_once(&self, _worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let projection = CustomerAccessAuditSummaryProjection::new(Arc::clone(&self.store));
        let report = projection.rebuild().await.map_err(|error| {
            WorkerRuntimeError::Persistence(PersistenceError::InvalidData(error.to_string()))
        })?;
        Ok(WorkerHandlerReport {
            processed: report.summaries_written,
            ..WorkerHandlerReport::default()
        })
    }
}

/// Narrow worker boundary for a future PostgreSQL-backed restore-test queue.
/// The worker receives only holding-scoped opaque claims; it never receives
/// credentials, URLs, storage clients, or a command to execute a backup.
#[async_trait::async_trait]
pub trait RestoreTestScheduleSource: Send + Sync {
    async fn claim_due_for_holding(
        &self,
        holding_id: &str,
        worker_id: Uuid,
        now: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<RestoreTestClaim>, RestoreTestScheduleError>;
}

#[async_trait::async_trait]
impl RestoreTestScheduleSource for RestoreTestScheduler {
    async fn claim_due_for_holding(
        &self,
        holding_id: &str,
        worker_id: Uuid,
        now: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<RestoreTestClaim>, RestoreTestScheduleError> {
        RestoreTestScheduler::claim_due_for_holding(self, holding_id, worker_id, now, batch_size)
    }
}

#[derive(Debug, Clone)]
pub struct RestoreTestScheduleHandler<S> {
    source: S,
    holding_id: String,
    batch_size: u32,
}

impl<S> RestoreTestScheduleHandler<S>
where
    S: RestoreTestScheduleSource,
{
    pub fn new(
        source: S,
        holding_id: impl Into<String>,
        batch_size: u32,
    ) -> Result<Self, WorkerRuntimeError> {
        let holding_id = holding_id.into();
        validate_worker_reference(&holding_id, "holding_id")?;
        Ok(Self {
            source,
            holding_id,
            batch_size: batch_size.clamp(1, MAX_RESTORE_TEST_SCHEDULE_BATCH_SIZE),
        })
    }

    pub const fn batch_size(&self) -> u32 {
        self.batch_size
    }

    pub fn holding_id(&self) -> &str {
        &self.holding_id
    }
}

#[async_trait::async_trait]
impl<S> NamedWorkerHandler for RestoreTestScheduleHandler<S>
where
    S: RestoreTestScheduleSource + 'static,
{
    fn name(&self) -> &str {
        RESTORE_TEST_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let claims = self
            .source
            .claim_due_for_holding(&self.holding_id, worker_id, Utc::now(), self.batch_size)
            .await
            .map_err(redacted_restore_test_schedule_error)?;
        let claimed = claims.len() as u64;
        Ok(WorkerHandlerReport {
            claimed,
            has_work: claimed != 0,
            ..WorkerHandlerReport::default()
        })
    }
}

fn redacted_restore_test_schedule_error(error: RestoreTestScheduleError) -> WorkerRuntimeError {
    let kind = match error {
        RestoreTestScheduleError::InvalidRequest(_) => "invalid_request",
        RestoreTestScheduleError::IdempotencyConflict => "idempotency_conflict",
        RestoreTestScheduleError::NotFound => "not_found",
        RestoreTestScheduleError::ClaimLost => "claim_lost",
        RestoreTestScheduleError::NotDue => "not_due",
        RestoreTestScheduleError::Unavailable => "unavailable",
    };
    tracing::error!(error_kind = kind, "restore-test claim failed");
    WorkerRuntimeError::Configuration("restore-test claim failed".into())
}

/// A minimal worker-local seam keeps the production handler tied to the
/// typed support repository while allowing deterministic unit tests for
/// scheduling and reporting without manufacturing database state.
#[async_trait::async_trait]
pub trait SupportCapabilityExpiryStore: Send + Sync + Clone + 'static {
    async fn sweep_expired_capabilities(
        &self,
        now: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<CapabilityMetadata>, SupportPersistenceError>;
}

#[async_trait::async_trait]
impl SupportCapabilityExpiryStore for SupportPersistence {
    async fn sweep_expired_capabilities(
        &self,
        now: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<CapabilityMetadata>, SupportPersistenceError> {
        SupportPersistence::sweep_expired_capabilities(self, now, batch_size).await
    }
}

#[derive(Debug, Clone)]
pub struct SupportCapabilityExpiryHandler<S = SupportPersistence> {
    store: S,
    batch_size: u32,
}

impl<S> SupportCapabilityExpiryHandler<S> {
    pub fn new(store: S, batch_size: u32) -> Self {
        Self {
            store,
            batch_size: batch_size.clamp(1, SUPPORT_CAPABILITY_EXPIRY_MAX_BATCH_SIZE),
        }
    }

    pub const fn batch_size(&self) -> u32 {
        self.batch_size
    }
}

#[async_trait::async_trait]
impl<S> NamedWorkerHandler for SupportCapabilityExpiryHandler<S>
where
    S: SupportCapabilityExpiryStore,
{
    fn name(&self) -> &str {
        SUPPORT_CAPABILITY_EXPIRY_HANDLER_NAME
    }

    async fn run_once(&self, _worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let expired = self
            .store
            .sweep_expired_capabilities(Utc::now(), self.batch_size())
            .await
            .map_err(redacted_support_expiry_error)?;
        let affected = expired.len() as u64;
        Ok(WorkerHandlerReport {
            processed: affected,
            has_work: affected != 0,
            ..WorkerHandlerReport::default()
        })
    }
}

fn redacted_support_expiry_error(error: SupportPersistenceError) -> WorkerRuntimeError {
    let kind = match error {
        SupportPersistenceError::Database(_) => "database",
        SupportPersistenceError::Serialization(_) => "serialization",
        SupportPersistenceError::IdempotencyConflict { .. } => "idempotency",
        SupportPersistenceError::NotFound { .. } => "not_found",
        SupportPersistenceError::OptimisticConcurrency { .. } => "concurrency",
        SupportPersistenceError::InvalidData(_) => "invalid_data",
        SupportPersistenceError::AttachmentScopeDenied
        | SupportPersistenceError::AttachmentBindingDenied
        | SupportPersistenceError::AttachmentClosedTicket
        | SupportPersistenceError::AttachmentNotActive
        | SupportPersistenceError::AttachmentNotExpired
        | SupportPersistenceError::AttachmentLimitExceeded => "attachment_policy",
        SupportPersistenceError::AccessRequestTicketBindingDenied => "access_policy",
    };
    tracing::error!(error_kind = kind, "support capability expiry sweep failed");
    WorkerRuntimeError::Configuration("support capability expiry sweep failed".into())
}

async fn run_registered_handler_until_stopped(
    handler: Arc<dyn NamedWorkerHandler>,
    heartbeat: WorkerHeartbeat,
    worker_id: Uuid,
    heartbeat_store: Option<(Arc<SqlxControlPlane>, Uuid)>,
    mut stop: tokio::sync::watch::Receiver<bool>,
    poll_interval: StdDuration,
) -> Result<WorkerLoopReport, WorkerRuntimeError> {
    let mut total = WorkerLoopReport::default();
    loop {
        if *stop.borrow() {
            break;
        }
        let report = handler.run_once(worker_id).await?;
        let observed_at = Utc::now();
        let _ = heartbeat.record_at(observed_at);
        if let Some((store, heartbeat_id)) = &heartbeat_store {
            store
                .record_worker_heartbeat(*heartbeat_id, observed_at)
                .await?;
        }
        total.passes = total.passes.saturating_add(1);
        total.claimed = total.claimed.saturating_add(report.claimed);
        total.processed = total.processed.saturating_add(report.processed);
        total.published = total.published.saturating_add(report.published);
        total.retried = total.retried.saturating_add(report.retried);
        total.dead_lettered = total.dead_lettered.saturating_add(report.dead_lettered);
        if !report.has_work {
            tokio::select! {
                _ = tokio::time::sleep(poll_interval) => {}
                changed = stop.changed() => {
                    if changed.is_err() || *stop.borrow() {
                        break;
                    }
                }
            }
        }
    }
    Ok(total)
}

fn merge_worker_loop_report(total: &mut WorkerLoopReport, report: WorkerLoopReport) {
    total.passes = total.passes.saturating_add(report.passes);
    total.claimed = total.claimed.saturating_add(report.claimed);
    total.processed = total.processed.saturating_add(report.processed);
    total.published = total.published.saturating_add(report.published);
    total.retried = total.retried.saturating_add(report.retried);
    total.dead_lettered = total.dead_lettered.saturating_add(report.dead_lettered);
}

#[derive(Debug, Clone)]
pub struct IncidentProjection {
    store: Arc<SqlxControlPlane>,
}

impl IncidentProjection {
    pub fn new(store: Arc<SqlxControlPlane>) -> Self {
        Self { store }
    }
}

#[async_trait::async_trait]
impl ProjectionHandler for IncidentProjection {
    fn handles_event(&self, projection_name: &str, event: &OutboxEnvelope) -> bool {
        projection_name == INCIDENT_HANDLER_NAME && event.aggregate_type == "incident"
    }

    async fn clear(&self, projection_name: &str) -> Result<(), WorkerError> {
        if projection_name != INCIDENT_HANDLER_NAME {
            return Err(WorkerError::Permanent(format!(
                "unsupported projection: {projection_name}"
            )));
        }
        self.store
            .clear_incident_projection_read_model()
            .await
            .map_err(|error| WorkerError::Retryable(error.to_string()))
    }

    async fn project(
        &self,
        projection_name: &str,
        event: &OutboxEnvelope,
    ) -> Result<(), WorkerError> {
        if projection_name != INCIDENT_HANDLER_NAME {
            return Err(WorkerError::Permanent(format!(
                "unsupported projection: {projection_name}"
            )));
        }
        if event.aggregate_type != "incident" {
            return Ok(());
        }
        let incident: Incident = serde_json::from_value(event.payload.clone())
            .map_err(|error| WorkerError::Permanent(format!("invalid incident event: {error}")))?;
        self.store
            .upsert_incident_projection_read_model(&incident, event.aggregate_version)
            .await
            .map_err(|error| WorkerError::Retryable(error.to_string()))
    }
}

pub const CUSTOMER_ACCESS_AUDIT_SUMMARY_PROJECTION_NAME: &str = "customer_access_audit_summary";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CustomerAccessAuditSummaryProjectionReport {
    pub summaries_written: u64,
}

#[derive(Debug, Clone)]
pub struct CustomerAccessAuditSummaryProjection {
    store: Arc<SqlxControlPlane>,
}

impl CustomerAccessAuditSummaryProjection {
    pub fn new(store: Arc<SqlxControlPlane>) -> Self {
        Self { store }
    }

    pub async fn clear(&self) -> Result<(), WorkerError> {
        self.store
            .clear_customer_access_audit_summary_read_model()
            .await
            .map_err(|_| redacted_audit_projection_error("clear"))
    }

    pub async fn rebuild(&self) -> Result<CustomerAccessAuditSummaryProjectionReport, WorkerError> {
        let summaries_written = self
            .store
            .rebuild_customer_access_audit_summary_read_model()
            .await
            .map_err(|_| redacted_audit_projection_error("rebuild"))?;
        Ok(CustomerAccessAuditSummaryProjectionReport { summaries_written })
    }
}

fn redacted_audit_projection_error(operation: &str) -> WorkerError {
    let operation = match operation {
        "clear" | "rebuild" => operation,
        _ => "operation",
    };
    WorkerError::Retryable(format!(
        "customer access-audit summary projection {operation} failed"
    ))
}

#[derive(Debug, Clone)]
pub struct IncidentOutboxHandler {
    worker: OutboxWorker,
    projection: IncidentProjection,
}

#[async_trait::async_trait]
impl OutboxHandler for IncidentOutboxHandler {
    async fn handle(&self, event: &OutboxEnvelope) -> Result<(), WorkerError> {
        self.worker
            .project_once(INCIDENT_HANDLER_NAME, event, &self.projection)
            .await
            .map(|_| ())
            .map_err(|error| WorkerError::Retryable(error.to_string()))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StripeSemanticRunReport {
    pub passes: u64,
    pub claimed: u64,
    pub processed: u64,
    pub applied: u64,
    pub duplicate_or_out_of_order: u64,
    pub no_op: u64,
    pub retried: u64,
    pub dead_lettered: u64,
}

impl StripeSemanticRunReport {
    fn merge(&mut self, other: &Self) {
        self.passes += other.passes;
        self.claimed += other.claimed;
        self.processed += other.processed;
        self.applied += other.applied;
        self.duplicate_or_out_of_order += other.duplicate_or_out_of_order;
        self.no_op += other.no_op;
        self.retried += other.retried;
        self.dead_lettered += other.dead_lettered;
    }
}

#[derive(Debug, Clone)]
pub struct StripeSemanticHandler {
    store: Arc<SqlxCommercialStore>,
    worker_id: Uuid,
    batch_size: u32,
    claim_timeout: chrono::Duration,
    poll_interval: StdDuration,
    retry_policy: StripeInboxRetryPolicy,
}

impl StripeSemanticHandler {
    pub fn new(store: Arc<SqlxCommercialStore>, config: WorkerRuntimeConfig) -> Self {
        Self {
            store,
            worker_id: config.worker_id,
            batch_size: bounded_stripe_semantic_batch_size(config.batch_size),
            claim_timeout: bounded_stripe_semantic_claim_timeout(config.claim_timeout),
            poll_interval: config.poll_interval,
            retry_policy: StripeInboxRetryPolicy::default(),
        }
    }

    pub async fn process_once(&self) -> Result<StripeSemanticRunReport, WorkerRuntimeError> {
        let claim_now = chrono::Utc::now();
        let claims = self
            .store
            .claim_stripe_events(
                self.worker_id,
                self.batch_size,
                claim_now,
                self.claim_timeout,
                self.retry_policy,
            )
            .await?;
        let mut report = StripeSemanticRunReport {
            passes: 1,
            claimed: claims.len() as u64,
            ..Default::default()
        };
        for claim in claims {
            let event_id = claim.event_id.clone();
            // Long semantic batches must use the current lease clock for the
            // acknowledgement/retry transition. Reusing the claim timestamp
            // could acknowledge a lease that has already expired.
            let transition_at = chrono::Utc::now();
            match StripeSemanticDispatcher::dispatch(self.store.as_ref(), &claim).await {
                Ok(outcome) => {
                    self.store
                        .mark_stripe_event_processed(&event_id, self.worker_id, transition_at)
                        .await?;
                    report.processed += 1;
                    match outcome {
                        erpx_infrastructure::StripeSemanticApplyResult::Applied => {
                            report.applied += 1;
                        }
                        erpx_infrastructure::StripeSemanticApplyResult::DuplicateOrOutOfOrder => {
                            report.duplicate_or_out_of_order += 1;
                        }
                        erpx_infrastructure::StripeSemanticApplyResult::NoOp => {
                            report.no_op += 1;
                        }
                    }
                }
                Err(StripeSemanticDispatchError::Parse(error)) => {
                    self.store
                        .dead_letter_stripe_event(
                            &event_id,
                            self.worker_id,
                            &error.to_string(),
                            transition_at,
                        )
                        .await?;
                    report.dead_lettered += 1;
                }
                Err(StripeSemanticDispatchError::Apply(StripeSemanticApplyError::Retryable(
                    error,
                ))) => {
                    let transition = self
                        .store
                        .retry_stripe_event(
                            &event_id,
                            self.worker_id,
                            &error,
                            transition_at,
                            self.retry_policy,
                        )
                        .await?;
                    match transition {
                        StripeInboxTransition::RetryScheduled { .. } => report.retried += 1,
                        StripeInboxTransition::DeadLettered { .. } => report.dead_lettered += 1,
                        StripeInboxTransition::Processed { .. } => {}
                    }
                }
                Err(StripeSemanticDispatchError::Apply(StripeSemanticApplyError::Permanent(
                    error,
                ))) => {
                    self.store
                        .dead_letter_stripe_event(&event_id, self.worker_id, &error, transition_at)
                        .await?;
                    report.dead_lettered += 1;
                }
            }
        }
        Ok(report)
    }

    pub async fn run_until_shutdown(&self) -> Result<StripeSemanticRunReport, WorkerRuntimeError> {
        let mut total = StripeSemanticRunReport::default();
        loop {
            let pass = self.process_once().await?;
            let idle = pass.claimed == 0;
            total.merge(&pass);
            if idle {
                tokio::select! {
                    result = tokio::signal::ctrl_c() => {
                        result.map_err(|error| WorkerRuntimeError::Configuration(
                            format!("failed to listen for shutdown: {error}"),
                        ))?;
                        break;
                    }
                    _ = tokio::time::sleep(self.poll_interval) => {}
                }
            }
        }
        Ok(total)
    }

    pub async fn run_until_stopped(
        &self,
        mut stop: tokio::sync::watch::Receiver<bool>,
    ) -> Result<StripeSemanticRunReport, WorkerRuntimeError> {
        let mut total = StripeSemanticRunReport::default();
        loop {
            if *stop.borrow() {
                break;
            }
            let pass = self.process_once().await?;
            let idle = pass.claimed == 0;
            total.merge(&pass);
            if idle {
                tokio::select! {
                    result = stop.changed() => {
                        result.map_err(|error| WorkerRuntimeError::Configuration(
                            format!("failed to receive worker stop signal: {error}"),
                        ))?;
                    }
                    _ = tokio::time::sleep(self.poll_interval) => {}
                }
            }
        }
        Ok(total)
    }
}

fn bounded_stripe_semantic_batch_size(batch_size: u32) -> u32 {
    batch_size.clamp(1, STRIPE_SEMANTIC_MAX_BATCH_SIZE)
}

fn bounded_stripe_semantic_claim_timeout(timeout: chrono::Duration) -> chrono::Duration {
    timeout
        .max(chrono::Duration::seconds(1))
        .min(chrono::Duration::seconds(
            STRIPE_SEMANTIC_MAX_CLAIM_TIMEOUT_SECONDS,
        ))
}

/// The executable uses this guard until the modular-monolith composition root
/// registers real context handlers. It never acknowledges or mutates an event.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnconfiguredHandler;

#[async_trait::async_trait]
impl OutboxHandler for UnconfiguredHandler {
    async fn handle(
        &self,
        _event: &erpx_infrastructure::OutboxEnvelope,
    ) -> Result<(), WorkerError> {
        Err(WorkerError::Retryable(
            "no ERPX context handler is registered".into(),
        ))
    }
}

fn optional_uuid(name: &str) -> Result<Option<Uuid>, WorkerRuntimeError> {
    match env::var(name) {
        Ok(value) => Uuid::parse_str(&value).map(Some).map_err(|error| {
            WorkerRuntimeError::Configuration(format!("{name} is not a UUID: {error}"))
        }),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(WorkerRuntimeError::Configuration(format!(
            "could not read {name}: {error}"
        ))),
    }
}

fn optional_u32(name: &str) -> Result<Option<u32>, WorkerRuntimeError> {
    optional_number(name)
}

fn optional_u64(name: &str) -> Result<Option<u64>, WorkerRuntimeError> {
    optional_number(name)
}

fn optional_number<T>(name: &str) -> Result<Option<T>, WorkerRuntimeError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match env::var(name) {
        Ok(value) => value.parse::<T>().map(Some).map_err(|error| {
            WorkerRuntimeError::Configuration(format!("{name} is invalid: {error}"))
        }),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(WorkerRuntimeError::Configuration(format!(
            "could not read {name}: {error}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_domain::commercial::BillingLifecycle;
    use erpx_domain::support::{SupportCapabilityBinding, SupportMode};
    use erpx_infrastructure::{
        ControlPlaneDatabaseConfig, ControlPlanePools, StripeInboxEvent, StripeInboxStatus,
    };
    use erpx_migration::migrate_control_plane;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use std::{
        str::FromStr,
        sync::atomic::{AtomicUsize, Ordering},
    };

    struct DisposableStripeWriteDatabase {
        admin_url: String,
        name: String,
        url: String,
    }

    fn database_url_with_name(base_url: &str, database: &str) -> String {
        let authority_start = base_url.find("://").map(|offset| offset + 3).unwrap_or(0);
        let path_start = base_url[authority_start..]
            .find('/')
            .map(|offset| authority_start + offset)
            .unwrap_or(base_url.len());
        let query_start = base_url[path_start..]
            .find('?')
            .map(|offset| path_start + offset)
            .unwrap_or(base_url.len());
        let mut url = String::with_capacity(base_url.len() + database.len() + 1);
        url.push_str(&base_url[..path_start]);
        url.push('/');
        url.push_str(database);
        url.push_str(&base_url[query_start..]);
        url
    }

    impl DisposableStripeWriteDatabase {
        async fn create(base_url: &str) -> Self {
            PgConnectOptions::from_str(base_url)
                .expect("valid disposable Stripe write database URL");
            let admin_url = std::env::var("ERPX_TEST_STRIPE_SEMANTIC_ADMIN_DATABASE_URL")
                .unwrap_or_else(|_| database_url_with_name(base_url, "postgres"));
            let admin_options = PgConnectOptions::from_str(&admin_url)
                .expect("valid disposable Stripe PostgreSQL admin database URL");
            let admin = PgPoolOptions::new()
                .max_connections(1)
                .connect_with(admin_options)
                .await
                .expect("connect disposable Stripe PostgreSQL admin database");
            let name = format!("erpx_worker_stripe_semantic_{}", Uuid::new_v4().simple());
            let quoted_name = format!("\"{}\"", name.replace('"', "\"\""));
            sqlx::query(&format!("CREATE DATABASE {quoted_name}"))
                .execute(&admin)
                .await
                .expect("create disposable Stripe PostgreSQL write database");
            admin.close().await;
            Self {
                admin_url,
                url: database_url_with_name(base_url, &name),
                name,
            }
        }

        async fn cleanup(self) {
            let admin_options = PgConnectOptions::from_str(&self.admin_url)
                .expect("valid disposable Stripe PostgreSQL cleanup URL");
            let admin = PgPoolOptions::new()
                .max_connections(1)
                .connect_with(admin_options)
                .await
                .expect("connect disposable Stripe PostgreSQL cleanup database");
            sqlx::query(
                "SELECT pg_terminate_backend(pid)
                 FROM pg_stat_activity
                 WHERE datname = $1 AND pid <> pg_backend_pid()",
            )
            .bind(&self.name)
            .execute(&admin)
            .await
            .expect("terminate disposable Stripe PostgreSQL connections");
            let quoted_name = format!("\"{}\"", self.name.replace('"', "\"\""));
            sqlx::query(&format!("DROP DATABASE IF EXISTS {quoted_name}"))
                .execute(&admin)
                .await
                .expect("drop disposable Stripe PostgreSQL write database");
            admin.close().await;
        }
    }

    #[derive(Debug)]
    struct FakeNamedHandler {
        name: &'static str,
        calls: Arc<AtomicUsize>,
        report: WorkerHandlerReport,
        succeeds: bool,
    }

    #[derive(Debug, Clone, Copy)]
    enum FakeExpiryOutcome {
        Expired(usize),
        Error,
    }

    #[derive(Debug, Clone)]
    struct FakeExpiryStore {
        outcome: FakeExpiryOutcome,
        observed_batch_size: Arc<Mutex<Option<u32>>>,
    }

    #[test]
    fn stripe_semantic_worker_bounds_batch_and_claim_lease_inputs() {
        assert_eq!(bounded_stripe_semantic_batch_size(0), 1);
        assert_eq!(
            bounded_stripe_semantic_batch_size(STRIPE_SEMANTIC_MAX_BATCH_SIZE + 1),
            STRIPE_SEMANTIC_MAX_BATCH_SIZE
        );
        assert_eq!(
            bounded_stripe_semantic_claim_timeout(chrono::Duration::seconds(0)),
            chrono::Duration::seconds(1)
        );
        assert_eq!(
            bounded_stripe_semantic_claim_timeout(chrono::Duration::hours(1)),
            chrono::Duration::seconds(STRIPE_SEMANTIC_MAX_CLAIM_TIMEOUT_SECONDS)
        );
    }

    #[test]
    fn candidate_rebuild_request_is_fully_bound_and_bounded() {
        let request = ProjectionCandidateRebuildRequest::new(
            erpx_operations_sqlx::EXCEPTION_PROJECTION,
            "holding-1",
            "holding-1",
            "holding_candidate_read_1",
            42,
        )
        .expect("valid candidate rebuild request");
        assert_eq!(request.source_version, 42);
        assert_eq!(request.holding_id, "holding-1");
        assert_eq!(request.data_profile_key, "holding-1");
        assert!(
            ProjectionCandidateRebuildRequest::new(
                "operations/read",
                "holding-1",
                "holding-1",
                "holding_candidate_read_1",
                42,
            )
            .is_err()
        );
        assert!(
            ProjectionCandidateRebuildRequest::new(
                erpx_operations_sqlx::EXCEPTION_PROJECTION,
                "holding-1",
                "holding-1",
                "holding candidate",
                42,
            )
            .is_err()
        );
        assert!(
            ProjectionCandidateRebuildRequest::new(
                erpx_operations_sqlx::EXCEPTION_PROJECTION,
                "holding-1",
                "holding-1",
                "holding_candidate_read_1",
                0,
            )
            .is_err()
        );
    }

    #[test]
    fn sales_projection_requests_are_bounded_and_holding_bound() {
        assert!(SalesProjectionRequest::new("holding-1", "holding-1", None, 0).is_err());
        assert!(
            SalesProjectionRequest::new(
                "holding-1",
                "holding-1",
                None,
                SALES_PROJECTION_MAX_PAGE_SIZE + 1,
            )
            .is_err()
        );
        assert!(SalesProjectionRequest::new("holding/1", "holding-1", None, 1).is_err());
        assert!(SalesProjectionRequest::new("holding-1", "holding 1", None, 1).is_err());

        let cursor = SalesSourceEventCursor {
            aggregate_type: "sales_order".into(),
            aggregate_id: Uuid::new_v4(),
            aggregate_version: 1,
        };
        let request = SalesProjectionRequest::new("holding-1", "holding-1", Some(cursor), 500)
            .expect("bounded Sales projection request");
        assert_eq!(request.holding_id, "holding-1");
        assert_eq!(request.data_profile_key, "holding-1");
        assert_eq!(request.page_size, SALES_PROJECTION_MAX_PAGE_SIZE);
    }

    #[test]
    fn sales_projection_drains_only_with_a_bounded_continuation() {
        assert_eq!(next_sales_cursor(false, None).expect("terminal page"), None);
        assert!(next_sales_cursor(true, None).is_err());

        let cursor = SalesSourceEventCursor {
            aggregate_type: "lead".into(),
            aggregate_id: Uuid::from_u128(700),
            aggregate_version: 2,
        };
        assert_eq!(
            next_sales_cursor(true, Some(cursor.clone())).expect("continued page"),
            Some(cursor)
        );
        assert_eq!(SALES_PROJECTION_MAX_PAGES, 2_000);
    }

    #[test]
    fn sales_projection_accepts_exactly_one_million_events_at_any_page_size() {
        assert_eq!(
            next_sales_event_count(SALES_PROJECTION_MAX_EVENTS - 1, 1)
                .expect("one million Sales events is supported"),
            SALES_PROJECTION_MAX_EVENTS
        );
        assert!(next_sales_event_count(SALES_PROJECTION_MAX_EVENTS, 1).is_err());
    }

    #[test]
    fn sales_projection_acknowledgement_requires_reconciled_read_state() {
        let clean = SalesProjectionReconciliation::default();
        assert!(require_sales_reconciliation(clean).is_ok());

        let damaged = SalesProjectionReconciliation {
            missing_aggregate_count: 1,
            ..clean
        };
        let error = require_sales_reconciliation(damaged).expect_err("mismatch must fail closed");
        assert!(
            error
                .to_string()
                .contains("Sales source/read reconciliation found a mismatch")
        );
    }

    #[test]
    fn asset_maintenance_projection_requests_are_bounded_and_holding_bound() {
        assert!(AssetMaintenanceProjectionRequest::new("holding-1", "holding-1", None, 0).is_err());
        assert!(
            AssetMaintenanceProjectionRequest::new(
                "holding-1",
                "holding-1",
                None,
                ASSET_MAINTENANCE_PROJECTION_MAX_PAGE_SIZE + 1,
            )
            .is_err()
        );
        assert!(AssetMaintenanceProjectionRequest::new("holding/1", "holding-1", None, 1).is_err());
        assert!(AssetMaintenanceProjectionRequest::new("holding-1", "holding 1", None, 1).is_err());

        let cursor = AssetMaintenanceSourceEventCursor {
            aggregate_type: "asset".into(),
            aggregate_id: Uuid::new_v4(),
            aggregate_version: 1,
        };
        let request =
            AssetMaintenanceProjectionRequest::new("holding-1", "holding-1", Some(cursor), 500)
                .expect("bounded Asset/Maintenance projection request");
        assert_eq!(request.holding_id, "holding-1");
        assert_eq!(request.data_profile_key, "holding-1");
        assert_eq!(
            request.page_size,
            ASSET_MAINTENANCE_PROJECTION_MAX_PAGE_SIZE
        );
        assert!(
            AssetMaintenanceProjectionRequest::new(
                "holding-1",
                "holding-1",
                Some(AssetMaintenanceSourceEventCursor {
                    aggregate_type: "asset".into(),
                    aggregate_id: Uuid::nil(),
                    aggregate_version: 1,
                }),
                1,
            )
            .is_err()
        );
    }

    #[test]
    fn asset_maintenance_projection_accepts_exactly_one_million_events() {
        assert_eq!(
            next_asset_event_count(ASSET_MAINTENANCE_PROJECTION_MAX_EVENTS - 1, 1)
                .expect("one million Asset/Maintenance events is supported"),
            ASSET_MAINTENANCE_PROJECTION_MAX_EVENTS
        );
        assert!(next_asset_event_count(ASSET_MAINTENANCE_PROJECTION_MAX_EVENTS, 1).is_err());
    }

    #[async_trait::async_trait]
    impl SupportCapabilityExpiryStore for FakeExpiryStore {
        async fn sweep_expired_capabilities(
            &self,
            _now: DateTime<Utc>,
            batch_size: u32,
        ) -> Result<Vec<CapabilityMetadata>, SupportPersistenceError> {
            *self.observed_batch_size.lock().expect("batch mutex") = Some(batch_size);
            match self.outcome {
                FakeExpiryOutcome::Expired(count) => Ok((0..count)
                    .map(|index| CapabilityMetadata {
                        id: Uuid::from_u128(index as u128 + 1),
                        binding: SupportCapabilityBinding::new(
                            Uuid::from_u128(1),
                            "agent",
                            "device",
                            Uuid::from_u128(2),
                            Uuid::from_u128(3),
                            SupportMode::DiagnosticRead,
                            vec!["support.read".into()],
                            "holding:1",
                            "support",
                        )
                        .expect("valid binding"),
                        issued_at: Utc::now(),
                        expires_at: Utc::now(),
                        reissued_from: None,
                        status: "revoked".into(),
                        revoked_at: Some(Utc::now()),
                        revocation_reason: Some("expired".into()),
                        version: 2,
                        secret_hash_length: 32,
                    })
                    .collect()),
                FakeExpiryOutcome::Error => Err(SupportPersistenceError::InvalidData(
                    "postgres://secret-must-not-leak".into(),
                )),
            }
        }
    }

    fn fake_expiry_store(outcome: FakeExpiryOutcome) -> FakeExpiryStore {
        FakeExpiryStore {
            outcome,
            observed_batch_size: Arc::new(Mutex::new(None)),
        }
    }

    #[async_trait::async_trait]
    impl NamedWorkerHandler for FakeNamedHandler {
        fn name(&self) -> &str {
            self.name
        }

        async fn run_once(
            &self,
            _worker_id: Uuid,
        ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if self.succeeds {
                Ok(self.report)
            } else {
                Err(WorkerRuntimeError::Configuration("injected failure".into()))
            }
        }
    }

    #[tokio::test]
    async fn defaults_are_bounded_and_fail_closed_handler_never_acknowledges() {
        let config = WorkerRuntimeConfig::default();
        assert_eq!(config.batch_size, 100);
        assert!(config.poll_interval > StdDuration::ZERO);
        let result = UnconfiguredHandler
            .handle(&erpx_infrastructure::OutboxEnvelope {
                id: Uuid::nil(),
                event_type: "test".into(),
                event_version: 1,
                aggregate_type: "test".into(),
                aggregate_id: Uuid::nil(),
                aggregate_version: 1,
                active_scope: Some(serde_json::json!("platform")),
                actor: Some(serde_json::json!("test")),
                correlation_id: Uuid::nil(),
                causation_id: None,
                idempotency_key: None,
                payload: serde_json::json!({}),
                occurred_at: chrono::Utc::now(),
                attempts: 1,
                max_attempts: 3,
                claimed_by: None,
                claimed_at: None,
                available_at: chrono::Utc::now(),
                published_at: None,
                dead_lettered_at: None,
                last_error: None,
                last_error_context: None,
            })
            .await;
        assert!(matches!(result, Err(WorkerError::Retryable(_))));
    }

    #[test]
    fn access_audit_projection_has_a_stable_name_and_redacted_bounded_errors() {
        assert_eq!(
            CUSTOMER_ACCESS_AUDIT_SUMMARY_PROJECTION_NAME,
            "customer_access_audit_summary"
        );
        let error = redacted_audit_projection_error("rebuild");
        assert_eq!(
            error.to_string(),
            "retryable handler failure: customer access-audit summary projection rebuild failed"
        );
        assert!(error.to_string().len() <= 256);
        assert!(!error.to_string().contains("postgres://"));
        assert!(!error.to_string().contains("payload"));
    }

    #[tokio::test]
    async fn registry_accepts_injected_handlers_and_propagates_failures() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = WorkerHandlerRegistry::default();
        registry
            .register(Arc::new(FakeNamedHandler {
                name: "future_handler",
                calls: Arc::clone(&calls),
                report: WorkerHandlerReport {
                    claimed: 1,
                    processed: 1,
                    has_work: true,
                    ..WorkerHandlerReport::default()
                },
                succeeds: true,
            }))
            .expect("fake handler registration");
        assert_eq!(registry.names(), ["future_handler"]);
        let handler = registry.get("future_handler").expect("registered handler");
        let report = handler
            .run_once(Uuid::from_u128(9))
            .await
            .expect("fake handler pass");
        assert_eq!(report.processed, 1);
        assert_eq!(calls.load(Ordering::Relaxed), 1);

        let duplicate = registry.register(Arc::new(FakeNamedHandler {
            name: "future_handler",
            calls: Arc::new(AtomicUsize::new(0)),
            report: WorkerHandlerReport::default(),
            succeeds: true,
        }));
        assert!(matches!(
            duplicate,
            Err(WorkerRuntimeError::Configuration(_))
        ));

        let failing = Arc::new(FakeNamedHandler {
            name: "failing_handler",
            calls: Arc::new(AtomicUsize::new(0)),
            report: WorkerHandlerReport::default(),
            succeeds: false,
        });
        let mut failing_registry = WorkerHandlerRegistry::default();
        failing_registry
            .register(failing)
            .expect("failing handler registration");
        let stop = tokio::sync::watch::channel(false).1;
        let result = run_registered_handler_until_stopped(
            failing_registry
                .get("failing_handler")
                .expect("failing handler"),
            WorkerHeartbeat::new(Uuid::from_u128(11)),
            Uuid::from_u128(10),
            None,
            stop,
            StdDuration::from_millis(1),
        )
        .await;
        assert!(matches!(result, Err(WorkerRuntimeError::Configuration(_))));
    }

    #[tokio::test]
    async fn support_expiry_handler_has_stable_name_and_bounded_batch() {
        let store = fake_expiry_store(FakeExpiryOutcome::Expired(0));
        let handler = SupportCapabilityExpiryHandler::new(store, u32::MAX);
        assert_eq!(handler.name(), SUPPORT_CAPABILITY_EXPIRY_HANDLER_NAME);
        assert_eq!(
            handler.batch_size(),
            SUPPORT_CAPABILITY_EXPIRY_MAX_BATCH_SIZE
        );
    }

    #[tokio::test]
    async fn support_expiry_handler_reports_affected_rows_and_configured_batch() {
        let store = fake_expiry_store(FakeExpiryOutcome::Expired(3));
        let observed_batch_size = Arc::clone(&store.observed_batch_size);
        let handler = SupportCapabilityExpiryHandler::new(store, 17);
        let report = handler
            .run_once(Uuid::from_u128(4))
            .await
            .expect("expiry pass");
        assert_eq!(report.claimed, 0);
        assert_eq!(report.processed, 3);
        assert_eq!(report.published, 0);
        assert_eq!(report.retried, 0);
        assert_eq!(report.dead_lettered, 0);
        assert!(report.has_work);
        assert_eq!(*observed_batch_size.lock().expect("batch mutex"), Some(17));
    }

    #[tokio::test]
    async fn support_expiry_handler_redacts_store_errors() {
        let handler =
            SupportCapabilityExpiryHandler::new(fake_expiry_store(FakeExpiryOutcome::Error), 17);
        let error = handler
            .run_once(Uuid::from_u128(5))
            .await
            .expect_err("expiry error");
        assert_eq!(
            error.to_string(),
            "worker configuration error: support capability expiry sweep failed"
        );
        assert!(!error.to_string().contains("postgres://"));
    }

    #[test]
    fn support_expiry_redacts_access_request_binding_errors() {
        let error = redacted_support_expiry_error(
            SupportPersistenceError::AccessRequestTicketBindingDenied,
        );
        assert_eq!(
            error.to_string(),
            "worker configuration error: support capability expiry sweep failed"
        );
        assert!(!error.to_string().contains("ticket"));
        assert!(!error.to_string().contains("customer"));
    }

    #[tokio::test]
    async fn registry_includes_support_expiry_handler() {
        let write = PgPoolOptions::new()
            .connect_lazy("postgres://fluke@127.0.0.1/erpx_control_write_test")
            .expect("lazy write pool");
        let read = PgPoolOptions::new()
            .connect_lazy("postgres://fluke@127.0.0.1/erpx_control_read_test")
            .expect("lazy read pool");
        let config = WorkerRuntimeConfig::default();
        let store = Arc::new(SqlxControlPlane::new(ControlPlanePools { write, read }));
        let runtime = WorkerRuntime {
            worker: OutboxWorker::new(Arc::clone(&store), config.worker_id),
            store,
            heartbeat: WorkerHeartbeat::new(config.worker_id),
            config,
        };
        let registry = runtime.handler_registry().expect("handler registry");
        assert!(
            registry
                .names()
                .iter()
                .any(|name| name == SUPPORT_CAPABILITY_EXPIRY_HANDLER_NAME)
        );
        assert!(
            registry
                .names()
                .iter()
                .any(|name| name == SUPPORT_ATTACHMENT_EXPIRY_HANDLER_NAME)
        );
        assert!(
            registry
                .names()
                .iter()
                .any(|name| name == INCIDENT_HANDLER_NAME)
        );
        assert!(
            !registry
                .names()
                .iter()
                .any(|name| name == SUPPORT_WORKFLOW_HANDLER_NAME)
        );
        assert!(
            registry
                .names()
                .iter()
                .any(|name| name == PLATFORM_COMPLETION_RUNTIME_HANDLER_NAME)
        );
        assert!(
            registry
                .names()
                .iter()
                .any(|name| name == STRIPE_BILLING_POLICY_HANDLER_NAME)
        );
        assert!(
            !registry
                .names()
                .iter()
                .any(|name| name == RESTORE_TEST_HANDLER_NAME)
        );
        let composed = runtime
            .handler_registry_with_restore_test_source(
                RestoreTestScheduler::default(),
                "holding-test",
            )
            .expect("composed handler registry");
        assert!(
            composed
                .names()
                .iter()
                .any(|name| name == RESTORE_TEST_HANDLER_NAME)
        );
        let workflow_write = PgPoolOptions::new()
            .connect_lazy("postgres://fluke@127.0.0.1/erpx_support_workflow_write_test")
            .expect("lazy support workflow write pool");
        let workflow_read = PgPoolOptions::new()
            .connect_lazy("postgres://fluke@127.0.0.1/erpx_support_workflow_read_test")
            .expect("lazy support workflow read pool");
        let support_registry = runtime
            .handler_registry_with_support_workflow(SupportWorkflowStore::new(
                workflow_write,
                workflow_read,
            ))
            .expect("support workflow registry");
        assert!(
            support_registry
                .names()
                .iter()
                .any(|name| name == SUPPORT_WORKFLOW_HANDLER_NAME)
        );
    }

    #[tokio::test]
    async fn status_projection_publisher_is_opt_in_and_uses_both_control_pools() {
        let write = PgPoolOptions::new()
            .connect_lazy("postgres://fluke@127.0.0.1/erpx_control_write_status_test")
            .expect("lazy write pool");
        let read = PgPoolOptions::new()
            .connect_lazy("postgres://fluke@127.0.0.1/erpx_control_read_status_test")
            .expect("lazy read pool");
        let config = WorkerRuntimeConfig::default();
        let store = Arc::new(SqlxControlPlane::new(ControlPlanePools { write, read }));
        let runtime = WorkerRuntime {
            worker: OutboxWorker::new(Arc::clone(&store), config.worker_id),
            store,
            heartbeat: WorkerHeartbeat::new(config.worker_id),
            config,
        };
        let default_registry = runtime.handler_registry().expect("default registry");
        assert!(
            !default_registry
                .names()
                .iter()
                .any(|name| name == HOLDING_PROJECTION_STATUS_PROJECTION_HANDLER_NAME)
        );
        let composed = runtime
            .handler_registry_with_control_plane_holding_projection_status_projection()
            .expect("status projection registry");
        assert!(
            composed
                .names()
                .iter()
                .any(|name| name == HOLDING_PROJECTION_STATUS_PROJECTION_HANDLER_NAME)
        );
    }

    #[tokio::test]
    async fn replacement_switcher_is_bounded_and_fail_closed_on_projection_mismatch() {
        let write = PgPoolOptions::new()
            .connect_lazy("postgres://fluke@127.0.0.1/erpx_switch_write_test")
            .expect("lazy write pool");
        let read = PgPoolOptions::new()
            .connect_lazy("postgres://fluke@127.0.0.1/erpx_switch_read_test")
            .expect("lazy read pool");
        let control = Arc::new(SqlxControlPlane::new(ControlPlanePools { write, read }));
        assert!(
            SqlxHoldingProjectionReadDatabaseSwitcher::new(
                Arc::clone(&control),
                "holding-switch",
                "candidate-read",
                0,
                "operations",
            )
            .is_err()
        );
        assert!(
            SqlxHoldingProjectionReadDatabaseSwitcher::new(
                Arc::clone(&control),
                "holding-switch",
                "candidate/read",
                1,
                "operations",
            )
            .is_err()
        );

        let switcher = SqlxHoldingProjectionReadDatabaseSwitcher::new(
            control,
            "holding-switch",
            "candidate-read",
            42,
            "operations",
        )
        .expect("bounded switcher");
        assert_eq!(switcher.holding_id(), "holding-switch");
        assert_eq!(switcher.candidate_read_database_key(), "candidate-read");
        assert_eq!(switcher.source_version(), 42);
        let error = switcher
            .prepare_replacement("finance")
            .await
            .expect_err("projection mismatch");
        assert!(matches!(error, WorkerError::Permanent(_)));
        assert!(!error.to_string().contains("postgres://"));
        let abort_error = switcher
            .abort_replacement("finance", "projection rebuild failed")
            .await
            .expect_err("projection mismatch abort");
        assert!(matches!(abort_error, WorkerError::Permanent(_)));
        assert!(!abort_error.to_string().contains("postgres://"));
    }

    #[tokio::test]
    async fn restore_test_handler_is_bounded_and_fail_closed_without_due_work() {
        let handler = RestoreTestScheduleHandler::new(
            RestoreTestScheduler::default(),
            "holding-test",
            u32::MAX,
        )
        .expect("restore handler");
        assert_eq!(handler.name(), RESTORE_TEST_HANDLER_NAME);
        assert_eq!(handler.batch_size(), MAX_RESTORE_TEST_SCHEDULE_BATCH_SIZE);
        let report = handler
            .run_once(Uuid::from_u128(18))
            .await
            .expect("empty restore pass");
        assert_eq!(report.claimed, 0);
        assert!(!report.has_work);
        assert_eq!(handler.holding_id(), "holding-test");
    }

    fn database_health(
        holding_id: &str,
        side: DatabaseSide,
        database_key: &str,
        reachable: bool,
    ) -> HoldingDatabaseHealth {
        HoldingDatabaseHealth {
            holding_id: holding_id.into(),
            side,
            database_key: database_key.into(),
            schema_version: "holding-0001".into(),
            reachable,
            checked_at: Utc::now(),
            details: serde_json::json!({"source": "injected-test"}),
        }
    }

    fn storage_health(holding_id: &str, reachable: bool) -> HoldingStorageHealth {
        HoldingStorageHealth {
            holding_id: holding_id.into(),
            storage_namespace: "holding-storage".into(),
            reachable,
            checked_at: Utc::now(),
            details: serde_json::json!({"source": "injected-test"}),
        }
    }

    fn backup_run(holding_id: &str, status: &str, verified: bool) -> HoldingBackupRun {
        HoldingBackupRun {
            id: Uuid::from_u128(7),
            holding_id: holding_id.into(),
            kind: "restore_test".into(),
            status: status.into(),
            backup_ref: "backup-chain-1".into(),
            restore_target_ref: Some("restore-target-1".into()),
            pitr_recovery_point: None,
            started_at: Utc::now(),
            completed_at: (status == "completed").then(Utc::now),
            verified,
            error: None,
        }
    }

    #[test]
    fn readiness_state_strings_and_composition_are_fail_closed() {
        assert_eq!(ReadinessState::Ready.as_str(), "ready");
        assert_eq!(ReadinessState::NotProbed.as_str(), "not_probed");
        assert_eq!(ReadinessState::Unavailable.as_str(), "unavailable");
        assert_eq!(
            ReadinessState::from_status("unprobed"),
            ReadinessState::NotProbed
        );
        assert_eq!(
            WorkerReadinessComposition::not_probed().overall(),
            ReadinessState::NotProbed
        );
        assert_eq!(
            ReadinessState::from_status("unavailable"),
            ReadinessState::Unavailable
        );
        assert!(!WorkerReadinessComposition::not_probed().is_ready());

        let now = Utc::now();
        let write = database_health("holding-test", DatabaseSide::Write, "holding-write", true);
        let read = database_health("holding-test", DatabaseSide::Read, "holding-read", true);
        let storage = storage_health("holding-test", true);
        let backup = backup_run("holding-test", "completed", true);
        let composition = WorkerReadinessComposition::from_metadata(
            WorkerHeartbeatReadiness::from_last_seen(
                Some(now - chrono::Duration::seconds(1)),
                now,
                chrono::Duration::seconds(30),
            ),
            WorkerReadinessMetadata {
                holding_id: "holding-test",
                registry_state: Some("active"),
                write_health: Some(&write),
                read_health: Some(&read),
                storage_health: Some(&storage),
                projection_state: Some("activated"),
                latest_backup: Some(&backup),
            },
        );
        assert!(composition.is_ready());
        assert_eq!(composition.overall(), ReadinessState::Ready);
        let failed = WorkerReadinessComposition {
            holding_routing: ReadinessState::Unavailable,
            ..composition
        };
        assert_eq!(failed.overall(), ReadinessState::Unavailable);
    }

    #[test]
    fn heartbeat_requires_a_recent_injected_observation() {
        let now = Utc::now();
        assert_eq!(
            WorkerHeartbeatReadiness::not_probed().state,
            ReadinessState::NotProbed
        );
        assert_eq!(
            WorkerHeartbeatReadiness::from_last_seen(
                Some(now - chrono::Duration::seconds(5)),
                now,
                chrono::Duration::seconds(10),
            )
            .state,
            ReadinessState::Ready
        );
        assert_eq!(
            WorkerHeartbeatReadiness::from_last_seen(
                Some(now - chrono::Duration::seconds(11)),
                now,
                chrono::Duration::seconds(10),
            )
            .state,
            ReadinessState::Unavailable
        );
        assert_eq!(
            WorkerHeartbeatReadiness::from_last_seen(
                Some(now + chrono::Duration::seconds(1)),
                now,
                chrono::Duration::seconds(10),
            )
            .state,
            ReadinessState::Unavailable
        );
    }

    #[test]
    fn worker_heartbeat_observation_starts_unprobed_and_requires_recent_mark() {
        let worker_id = Uuid::from_u128(7);
        let heartbeat = WorkerHeartbeat::new(worker_id);
        let now = Utc::now();
        let missing = heartbeat.observe(now, chrono::Duration::minutes(1));
        assert_eq!(missing.worker_id, worker_id);
        assert_eq!(missing.state, ReadinessState::NotProbed);
        assert!(heartbeat.record_at(now - chrono::Duration::seconds(10)));
        assert_eq!(
            heartbeat.observe(now, chrono::Duration::minutes(1)).state,
            ReadinessState::Ready
        );
        assert_eq!(
            heartbeat.observe(now, chrono::Duration::seconds(5)).state,
            ReadinessState::Unavailable
        );
        assert!(heartbeat.record_at(now + chrono::Duration::seconds(1)));
        assert_eq!(
            heartbeat.observe(now, chrono::Duration::minutes(1)).state,
            ReadinessState::Unavailable
        );
    }

    #[test]
    fn infrastructure_observation_states_map_fail_closed_into_worker_composition() {
        assert_eq!(
            readiness_state_from_observation(ReadinessObservationState::Ready),
            ReadinessState::Ready
        );
        assert_eq!(
            readiness_state_from_observation(ReadinessObservationState::NotProbed),
            ReadinessState::NotProbed
        );
        assert_eq!(
            readiness_state_from_observation(ReadinessObservationState::Unavailable),
            ReadinessState::Unavailable
        );
    }

    #[test]
    fn holding_projection_and_backup_classifiers_do_not_promote_missing_or_failed_metadata() {
        let write = database_health("holding-test", DatabaseSide::Write, "holding-write", true);
        let read = database_health("holding-test", DatabaseSide::Read, "holding-read", true);
        let storage = storage_health("holding-test", true);
        assert_eq!(
            holding_routing_readiness(
                "holding-test",
                Some("active"),
                Some(&write),
                Some(&read),
                None,
            ),
            ReadinessState::NotProbed
        );
        let failed_storage = storage_health("holding-test", false);
        assert_eq!(
            holding_routing_readiness(
                "holding-test",
                Some("active"),
                Some(&write),
                Some(&read),
                Some(&failed_storage),
            ),
            ReadinessState::Unavailable
        );
        assert_eq!(
            holding_routing_readiness(
                "holding-test",
                Some("active"),
                Some(&write),
                Some(&read),
                Some(&storage),
            ),
            ReadinessState::Ready
        );
        assert_eq!(
            projection_freshness_readiness(None),
            ReadinessState::NotProbed
        );
        assert_eq!(
            projection_freshness_readiness(Some("rebuilding")),
            ReadinessState::Unavailable
        );
        assert_eq!(
            projection_freshness_readiness(Some("activated")),
            ReadinessState::Ready
        );
        let started = backup_run("holding-test", "started", false);
        assert_eq!(
            backup_pitr_readiness("holding-test", Some(&started)),
            ReadinessState::Unavailable
        );
        let completed = backup_run("holding-test", "completed", true);
        assert_eq!(
            backup_pitr_readiness("holding-test", Some(&completed)),
            ReadinessState::Ready
        );
        assert_eq!(
            backup_pitr_readiness("other-holding", Some(&completed)),
            ReadinessState::Unavailable
        );
    }

    #[tokio::test]
    #[ignore = "requires one disposable PostgreSQL control write database"]
    async fn stripe_semantic_handler_applies_valid_events_and_dead_letters_unknown_types() {
        let configured_write_url = std::env::var("ERPX_TEST_COMMERCIAL_DATABASE_URL")
            .or_else(|_| std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL"))
            .expect(
                "ERPX_TEST_COMMERCIAL_DATABASE_URL or ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required",
            );
        let read_url = std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required");
        let disposable_write = DisposableStripeWriteDatabase::create(&configured_write_url).await;
        let write_url = disposable_write.url.clone();
        let test_result = tokio::spawn(async move {
            let config = ControlPlaneDatabaseConfig {
                write_url,
                read_url,
            };
            let pools = ControlPlanePools::connect(&config)
                .await
                .expect("control pools");
            migrate_control_plane(&pools.write)
                .await
                .expect("write migration");
            let control = Arc::new(SqlxControlPlane::new(pools));
            let store = Arc::new(SqlxCommercialStore::new(&control));
            let customer_id = Uuid::new_v4();
            let subscription_id = Uuid::new_v4();
            let valid_event_id = format!("evt_valid_{}", Uuid::new_v4());
            let invalid_event_id = format!("evt_invalid_{}", Uuid::new_v4());
            store
                .insert_stripe_event(&StripeInboxEvent {
                    event_id: valid_event_id.clone(),
                    event_type: "invoice.payment_failed".into(),
                    stripe_created_at: Some(chrono::Utc::now()),
                    payload: serde_json::json!({
                        "data": {"object": {"metadata": {
                            "erpx_customer_id": customer_id.to_string(),
                            "erpx_subscription_id": subscription_id.to_string()
                        }}}
                    }),
                    raw_body: br#"{"id":"evt_valid"}"#.to_vec(),
                })
                .await
                .expect("valid event insert");
            store
                .insert_stripe_event(&StripeInboxEvent {
                    event_id: invalid_event_id.clone(),
                    event_type: "charge.refunded".into(),
                    stripe_created_at: Some(chrono::Utc::now()),
                    payload: serde_json::json!({"data": {"object": {"token": "secret"}}}),
                    raw_body: br#"{"id":"evt_invalid"}"#.to_vec(),
                })
                .await
                .expect("invalid event insert");

            let worker_config = WorkerRuntimeConfig {
                batch_size: 10,
                poll_interval: StdDuration::from_millis(1),
                ..Default::default()
            };
            let handler = StripeSemanticHandler::new(store.clone(), worker_config);
            let report = handler.process_once().await.expect("semantic pass");
            assert_eq!(report.claimed, 2);
            assert_eq!(report.processed, 1);
            assert_eq!(report.applied, 1);
            assert_eq!(report.dead_lettered, 1);
            assert_eq!(
                store
                    .get_stripe_event_state(&valid_event_id)
                    .await
                    .expect("valid state")
                    .expect("valid row")
                    .status,
                StripeInboxStatus::Processed
            );
            assert_eq!(
                store
                    .get_stripe_event_state(&invalid_event_id)
                    .await
                    .expect("invalid state")
                    .expect("invalid row")
                    .status,
                StripeInboxStatus::DeadLetter
            );
            assert_eq!(
                store
                    .get_billing_lifecycle(customer_id, subscription_id)
                    .await
                    .expect("lifecycle")
                    .expect("lifecycle row")
                    .0
                    .state,
                erpx_domain::commercial::BillingState::PastDue
            );
            drop(handler);
            drop(store);
            drop(control);
        })
        .await;
        disposable_write.cleanup().await;
        test_result.expect("Stripe semantic fixture task panicked");
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL control write/read databases"]
    async fn stripe_billing_policy_worker_advances_due_rows_and_is_idempotent() {
        let configured_write_url = std::env::var("ERPX_TEST_COMMERCIAL_DATABASE_URL")
            .or_else(|_| std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL"))
            .expect("commercial or control write database URL is required");
        let read_url = std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("control read database URL is required");
        let disposable_write = DisposableStripeWriteDatabase::create(&configured_write_url).await;
        let write_url = disposable_write.url.clone();
        let test_result = tokio::spawn(async move {
            let config = ControlPlaneDatabaseConfig {
                write_url,
                read_url,
            };
            let pools = ControlPlanePools::connect(&config)
                .await
                .expect("control pools");
            migrate_control_plane(&pools.write)
                .await
                .expect("write migration");
            let control = Arc::new(SqlxControlPlane::new(pools));
            let store = SqlxCommercialStore::new(&control);
            let customer_id = Uuid::new_v4();
            let subscription_id = Uuid::new_v4();
            let base = Utc::now() - chrono::Duration::hours(1);
            store
                .upsert_billing_lifecycle(
                    customer_id,
                    subscription_id,
                    &BillingLifecycle::new(erpx_domain::commercial::BillingState::PastDue, base),
                    None,
                )
                .await
                .expect("seed due billing lifecycle");
            let handler = StripeBillingPolicyRegisteredHandler {
                store: Arc::clone(&control),
                batch_size: 10,
            };
            let first = handler
                .run_once(Uuid::new_v4())
                .await
                .expect("billing policy worker pass");
            assert_eq!(first.claimed, 1);
            assert_eq!(first.processed, 1);
            assert!(first.has_work);
            assert_eq!(
                store
                    .get_billing_lifecycle(customer_id, subscription_id)
                    .await
                    .expect("read billing lifecycle")
                    .expect("billing lifecycle")
                    .0
                    .state,
                erpx_domain::commercial::BillingState::Grace
            );
            let second = handler
                .run_once(Uuid::new_v4())
                .await
                .expect("empty billing policy worker pass");
            assert_eq!(second.claimed, 0);
            assert_eq!(second.processed, 0);
            assert!(!second.has_work);
        })
        .await;
        disposable_write.cleanup().await;
        test_result.expect("billing policy worker fixture task panicked");
    }
}
