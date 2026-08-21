#![doc = "ERPX SQLx persistence, outbox, routing, and projection adapters."]

mod agent_notifications;
mod asset_maintenance_persistence;
mod backup_retention;
mod branch_provisioning;
mod cluster_pitr;
mod commercial;
mod corporate_identity;
mod data_plane;
mod error;
mod finance_bank_credentials;
mod finance_bank_provider_import_jobs;
mod finance_banking_persistence;
mod finance_banking_provider;
mod finance_budget_persistence;
mod finance_chart_persistence;
mod finance_chart_projection;
mod finance_close_control_evidence;
mod finance_close_persistence;
mod finance_close_projection;
mod finance_close_read;
mod finance_persistence;
mod finance_subledger_persistence;
mod finance_tax_persistence;
mod g13_migration;
mod g13_target;
mod holding_administration;
mod holding_projection_scheduler;
mod holding_projection_status_projection;
mod hr_persistence;
mod hr_projection;
mod legal_persistence;
mod legal_projection;
mod marketing_callback_secret_manager;
mod marketing_callback_secrets;
mod marketing_content_application;
mod marketing_content_object_access;
mod marketing_content_persistence;
mod marketing_crm_persistence;
mod marketing_delivery_persistence;
mod marketing_delivery_scheduler;
mod marketing_journey_execution_persistence;
mod marketing_journey_integration_persistence;
mod marketing_journey_persistence;
mod marketing_persistence;
mod marketing_privacy_application;
mod marketing_provider_onboarding;
mod object_storage;
mod outbox;
mod pitr;
mod platform_completion;
mod pools;
mod pos_persistence;
mod pos_projection;
mod provisioning;
mod quality_persistence;
mod repositories;
mod role_review;
mod runtime_readiness;
mod sales_persistence;
mod scm_persistence;
mod shared_enterprise_services;
mod stripe_client;
mod support_persistence;
mod support_workflow;
mod worker;

#[cfg(test)]
#[path = "../../adapters/schema_test.rs"]
mod schema_test;
#[cfg(test)]
mod test_support;

pub use asset_maintenance_persistence::{
    AssetMaintenanceAggregate, AssetMaintenanceProjectionApplication,
    AssetMaintenanceProjectionRebuildPreparation, AssetMaintenanceReadCursor,
    AssetMaintenanceReadListRow, AssetMaintenanceReadPage, AssetMaintenanceReadScope,
    AssetMaintenanceSourceEvent, AssetMaintenanceSourceEventCursor,
    AssetMaintenanceSourceEventPage, AssetMaintenanceWriteReceipt, CapacityHandoffReceipt,
    MAX_ASSET_MAINTENANCE_PROJECTION_EVENTS, MAX_ASSET_MAINTENANCE_READ_PAGE_SIZE,
    SqlxAssetMaintenanceStore,
};
pub use backup_retention::{
    HoldingBackupInventoryObservation, HoldingBackupInventoryReport, HoldingBackupRetentionClaim,
    HoldingBackupRetentionObject, HoldingBackupRetentionReceipt, HoldingBackupRetentionReport,
    HoldingBackupRetentionRequest, MAX_HOLDING_BACKUP_INVENTORY_ENTRIES,
    MAX_HOLDING_BACKUP_RETENTION_AGE_SECONDS, MAX_HOLDING_BACKUP_RETENTION_ATTEMPTS,
    MAX_HOLDING_BACKUP_RETENTION_LEASE_SECONDS, MAX_HOLDING_BACKUP_RETENTION_PAGE_SIZE,
    MAX_HOLDING_BACKUP_RETENTION_ROWS, MIN_HOLDING_BACKUP_RETENTION_AGE_SECONDS,
    MIN_HOLDING_BACKUP_RETENTION_LEASE_SECONDS, validate_holding_backup_retention_request_at,
};
pub use branch_provisioning::{
    BRANCH_INFRASTRUCTURE_PROVISION, BRANCH_PROVISIONING_LEASE_SECONDS, BranchDatabaseRoute,
    BranchProvisioningClaim, BranchProvisioningJob, BranchProvisioningState,
    DatabaseProfileRegistration, ProvisionBranchRequest, SqlxBranchProvisioningStore,
    branch_database_keys,
};
pub use cluster_pitr::{
    ClusterPitrChain, ClusterPitrError, ClusterPitrRepository, ClusterRecoveryPoint,
    ClusterRestoreRehearsal, ClusterRestoreRehearsalResult, MAX_CLUSTER_PITR_PAGE_SIZE,
};
pub use commercial::{
    BillingPolicyAdvanceReport, MAX_BILLING_POLICY_BATCH_SIZE,
    MAX_STRIPE_CATALOG_RECONCILIATION_PAGES, MAX_STRIPE_CATALOG_RETRY_ATTEMPTS,
    MAX_STRIPE_INBOX_ATTEMPTS, MAX_STRIPE_INBOX_REPLAYS, PINNED_STRIPE_API_VERSION,
    STRIPE_API_BASE_URL, SqlxCommercialStore, StripeAccountId, StripeCatalogBoundaryError,
    StripeCatalogHttpClient, StripeCatalogIdempotencyBinding, StripeCatalogItemIdentifiers,
    StripeCatalogOperation, StripeCatalogOperationResult, StripeCatalogProvider,
    StripeCatalogProviderBoundary, StripeCatalogProviderSnapshot, StripeCatalogPublicationPlan,
    StripeCatalogPublicationPlanError, StripeCatalogPublicationResponse,
    StripeCatalogPublicationStep, StripeCatalogPublishRequest, StripeCatalogReconcileRequest,
    StripeCatalogRequestError, StripeCatalogRetryPolicy, StripeCatalogSnapshotError,
    StripeCheckoutBinding, StripeEntitlementFeatureId, StripeEntitlementUpdate, StripeEventId,
    StripeHttpCatalogProvider, StripeIdentifierError, StripeInboxClaim, StripeInboxEvent,
    StripeInboxInsert, StripeInboxReplayPolicy, StripeInboxRetryPolicy, StripeInboxState,
    StripeInboxStatus, StripeInboxTransition, StripeInvoiceProjection, StripeMeterId,
    StripePriceId, StripeProductId, StripeProviderConfig, StripeProviderConfigError,
    StripeProviderError, StripeProviderErrorKind, StripeReplayAuthorizationContext,
    StripeSemanticApplier, StripeSemanticApplyError, StripeSemanticApplyResult,
    StripeSemanticDispatchError, StripeSemanticDispatcher, StripeSemanticEvent, StripeSemanticNoOp,
    StripeSemanticParseError, StripeWebhookEvent, StripeWebhookEventParseError,
    StripeWebhookPersistenceContext, StripeWebhookPersistenceContextError, redact_provider_error,
};
pub use corporate_identity::{
    CorporateIdentityHistoryEntry, CorporateIdentityRollback, SqlxCorporateIdentityStore,
    VerifiedBrandDomain, corporate_identity_publish_decision_ref,
};
pub use data_plane::{
    ApprovedDesiredConfiguration, DataPlaneEnrollmentConsumption, DataPlaneEnrollmentIssue,
    HoldingInfrastructureSummary, RootAccessGrant, RootAccessQueueRecord,
    SqlxDataPlaneControlStore, desired_configuration_decision_ref, root_access_decision_ref,
};
pub use erpx_domain::commercial::{
    CatalogError, CatalogReconciliationReport, ProviderCatalogPrice, ProviderCatalogRevision,
};
pub use error::PersistenceError;
pub use finance_bank_credentials::{
    BankProviderCredentialError, BankProviderCredentialKey, BankProviderCredentialResolver,
    MAX_BANK_PROVIDER_CREDENTIAL_BYTES, MAX_BANK_PROVIDER_CREDENTIAL_KEY_REF_BYTES,
    ResolvedBankProviderCredential, UnconfiguredBankProviderCredentialResolver,
};
pub use finance_bank_provider_import_jobs::{
    FINANCE_BANK_PROVIDER_IMPORT_MAX_ATTEMPTS, FinanceBankProviderImportAuthorization,
    FinanceBankProviderImportError, FinanceBankProviderImportJobStatus,
    FinanceBankProviderImportRequest, SqlxFinanceBankProviderImportJobSource,
};
pub use finance_banking_persistence::{
    BankAccountWriteReceipt, BankMatchDecisionWriteReceipt, BankMatchProposalWriteReceipt,
    BankReconciliationWriteReceipt, FINANCE_BANKING_READ_PROJECTION,
    FinanceBankingProjectionApplication, FinanceBankingProjectionRebuildPreparation,
    FinanceBankingProjectionReconciliation, FinanceBankingReadAggregate, FinanceBankingReadCursor,
    FinanceBankingReadPage, FinanceBankingReadRow, FinanceBankingSourceEvent,
    FinanceBankingSourceEventCursor, FinanceBankingSourceEventPage,
    MAX_FINANCE_BANKING_READ_PAGE_SIZE, MAX_FINANCE_BANKING_SOURCE_EVENT_PAGE_SIZE,
    MAX_FINANCE_BANKING_TRANSACTION_PAGE_SIZE, SqlxFinanceBankingStore,
};
pub use finance_banking_provider::{
    BankProviderAccountBinding, BankProviderAccountRef, BankProviderAdapter, BankProviderCursor,
    BankProviderDomainRecords, BankProviderError, BankProviderRequestBinding,
    BankProviderTlsEvidence, BankProviderTlsVersion, BankStatementFacts, BankStatementPage,
    BankStatementPageRequest, MAX_PROVIDER_ACCOUNT_REFERENCE_BYTES, MAX_PROVIDER_CURSOR_BYTES,
    MAX_PROVIDER_NAME_BYTES, MAX_PROVIDER_PAGE_SIZE, MAX_PROVIDER_SERVER_NAME_BYTES,
    MAX_PROVIDER_TLS_CLOCK_SKEW, MAX_PROVIDER_TLS_EVIDENCE_AGE,
    MAX_PROVIDER_TRANSACTION_REFERENCE_BYTES, MAX_PROVIDER_TRUST_ANCHOR_REFERENCE_BYTES,
    ProviderBankTransaction,
};
pub use finance_budget_persistence::{BudgetCommitmentRequest, SqlxFinanceBudgetStore};
pub use finance_chart_persistence::{
    FINANCE_CHART_AGGREGATE, FINANCE_CHART_SAVED_EVENT, FinanceChartOutboxEvent,
    FinanceChartSavedEvent, FinanceChartWriteReceipt, SqlxFinanceChartStore,
};
pub use finance_chart_projection::{
    FINANCE_CHART_PROJECTION_NAME, FinanceAccountReadRow, FinanceChartProjectionApplication,
    FinanceChartProjectionRebuildPreparation, FinanceChartReadCursor, FinanceChartReadPage,
    FinanceChartReadRow, FinanceChartReadScope, FinanceChartSourceEvent,
    FinanceChartSourceEventCursor, FinanceChartSourceEventPage, FinanceChartSourceMetadata,
    MAX_FINANCE_CHART_READ_PAGE_SIZE, MAX_FINANCE_CHART_SOURCE_EVENT_PAGE_SIZE,
    SqlxFinanceChartProjectionStore, SqlxFinanceChartReadStore,
};
pub use finance_close_control_evidence::{
    FinanceCloseControlEvidenceSourceRecord, FinanceCloseControlEvidenceWriteReceipt,
    SqlxFinanceCloseControlEvidenceStore,
};
pub use finance_close_persistence::{
    CloseReviewSaveReceipt, FinanceCloseControlEvidence, SqlxFinanceCloseStore,
};
pub use finance_close_projection::{
    FINANCE_CLOSE_PROJECTION_NAME, FinanceCloseProjectionApplication,
    FinanceCloseProjectionRebuildPreparation, FinanceCloseSourceEvent,
    FinanceCloseSourceEventCursor, FinanceCloseSourceEventPage,
    MAX_FINANCE_CLOSE_SOURCE_EVENT_PAGE_SIZE, SqlxFinanceCloseProjectionStore,
};
pub use finance_close_read::{
    FINANCE_CLOSE_READ_STALE_AFTER_SECONDS, FinanceCloseAccountSummary, FinanceCloseControlSummary,
    FinanceCloseReadAvailability, FinanceCloseReadCursor, FinanceCloseReadError,
    FinanceCloseReadFreshness, FinanceCloseReadListRow, FinanceCloseReadMetadata,
    FinanceCloseReadPage, FinanceCloseReadResult, FinanceCloseReadScope, FinanceCloseReadView,
    FinanceCloseReport, FinanceCloseReviewProjection, FinanceCloseReviewStatus,
    FinanceCloseStatementSummary, MAX_FINANCE_CLOSE_READ_PAGE_SIZE,
    MAX_FINANCE_CLOSE_REPORT_ACCOUNTS, MAX_FINANCE_CLOSE_REPORT_CONTROLS,
    MAX_FINANCE_CLOSE_REPORT_FACTS, SqlxFinanceCloseReadAdapter,
};
pub use finance_persistence::{
    FinanceProjectionApplication, FinanceProjectionRebuildPreparation, FinanceReadJournal,
    FinanceSourceEvent, FinanceSourceEventCursor, FinanceSourceEventPage,
    MAX_FINANCE_CONSOLIDATED_CHARTS, MAX_FINANCE_READ_JOURNAL_PAGE_SIZE,
    MAX_FINANCE_SOURCE_EVENT_PAGE_SIZE, SqlxFinanceStore,
};
pub use finance_subledger_persistence::{
    FINANCE_SUBLEDGER_READ_PROJECTION, FinanceAllocationBatchReceipt, FinanceSubledgerAggregate,
    FinanceSubledgerProjectionApplication, FinanceSubledgerProjectionRebuildPreparation,
    FinanceSubledgerReadCursor, FinanceSubledgerReadPage, FinanceSubledgerReadRow,
    FinanceSubledgerSourceEvent, FinanceSubledgerSourceEventCursor,
    FinanceSubledgerSourceEventPage, FinanceSubledgerWriteReceipt,
    MAX_FINANCE_SUBLEDGER_READ_PAGE_SIZE, MAX_FINANCE_SUBLEDGER_SOURCE_EVENT_PAGE_SIZE,
    SqlxFinanceSubledgerStore,
};
pub use finance_tax_persistence::{
    FINANCE_THAI_TAX_FACT_AGGREGATE, FINANCE_THAI_TAX_FACT_EVENT, FINANCE_THAI_TAX_PROJECTION_NAME,
    FinanceThaiTaxProjectionApplication, FinanceThaiTaxProjectionRebuildPreparation,
    FinanceThaiTaxProjectionReconciliation, FinanceThaiTaxSourceEvent,
    FinanceThaiTaxSourceEventCursor, FinanceThaiTaxSourceEventPage,
    FinanceThaiTaxStatutoryReconciliation, MAX_FINANCE_THAI_TAX_RECONCILIATION_REPORTS,
    MAX_FINANCE_THAI_TAX_REPORT_FACTS, MAX_FINANCE_THAI_TAX_SOURCE_EVENT_PAGE_SIZE,
    SqlxFinanceThaiTaxStore, ThaiTaxFactReadRow, ThaiTaxFactWriteReceipt, ThaiTaxReportRead,
};
pub use g13_migration::{
    G13_CUTOVER_PERMISSION, G13_EXECUTE_PERMISSION, G13AuthorizationProof, G13MigrationStatus,
    G13MigrationWorkClaim, G13MigrationWorkReceipt, G13PersistenceError, G13SourceSnapshotRecord,
    SqlxG13MigrationStore,
};
pub use g13_target::{
    G13ReadProjectionResult, G13RetentionResult, G13ShadowReplayResult, G13TargetImportError,
    G13TargetImportResult, G13TargetImporter, G13TargetReconciliation, G13TargetRollbackResult,
};
pub use holding_administration::{
    HoldingAdminContextIssue, HoldingBillingGraceExtensionGrant, HoldingControlHistoryCursor,
    HoldingControlHistoryEntry, HoldingControlHistoryPage, HoldingInvoicePage,
    HoldingInvoiceSummary, HoldingRegistryFilter, HoldingRegistryPage, HoldingRegistrySummary,
    HoldingStripeSubscriptionBinding, MAX_HOLDING_CONTROL_HISTORY_PAGE_SIZE,
    MAX_HOLDING_SUMMARY_PAGE_SIZE, SqlxHoldingAdministrationStore,
};
pub use holding_projection_scheduler::{
    ClaimKeyDecision, ClaimKeyRegistry, DeadLetterRecord, FRESH_INITIAL_HOLDING_PROJECTIONS,
    HOLDING_PROJECTION_PERMISSION, HoldingHoldingBinding, HoldingProjectionSchedulerError,
    HoldingProjectionSchedulerRepository, HoldingProjectionStatus,
    HoldingProjectionStatusChangePage, HoldingProjectionStatusPage,
    HoldingProjectionStatusReadRepository, MAX_BACKOFF_SECONDS, MAX_CLAIM_KEY_LENGTH,
    MAX_JOB_KEY_LENGTH, MAX_LEASE_SECONDS, MAX_PROJECTION_ATTEMPTS, MAX_PROJECTION_BATCH_SIZE,
    MAX_PROJECTION_ERROR_BYTES, MAX_PROJECTION_NAME_LENGTH, MAX_PROJECTION_PAGE_SIZE,
    MAX_PROJECTION_PAYLOAD_BYTES, MAX_PROJECTION_STATUS_FRESHNESS_SECONDS,
    MAX_PROJECTION_STATUS_PAGE_SIZE, MAX_SCOPE_REFERENCE_LENGTH, MIN_BACKOFF_SECONDS,
    MIN_LEASE_SECONDS, ProjectionClaimRecord, ProjectionClaimRequest, ProjectionFailureOutcome,
    ProjectionJobKind, ProjectionJobRecord, ProjectionJobState, ProjectionLease, ProjectionPolicy,
    ProjectionScheduleRequest, ProjectionSchedulerAuthorization, ProjectionStatusChangeCursor,
    ProjectionStatusCursor, RebuildProjectionRequest, RebuildRecord, RetryRecord,
    StaleLeaseReclaimRecord,
};
pub use holding_projection_status_projection::{
    HOLDING_PROJECTION_STATUS_CHECKPOINT, HOLDING_PROJECTION_STATUS_READ_PROJECTION,
    SqlxHoldingProjectionStatusProjectionStore,
};
pub use hr_persistence::{HrWriteReceipt, SqlxHrPersistence};
pub use hr_projection::{
    HR_PROJECTION_NAME, HrEmployeeReadPage, HrEmployeeReadRow, HrExpenseReadCursor,
    HrExpenseReadPage, HrExpenseReadRow, HrLeaveReadCursor, HrLeaveReadPage, HrLeaveReadRow,
    HrPayrollLineReadRow, HrPayrollRunReadCursor, HrPayrollRunReadPage, HrPayrollRunReadRow,
    HrProjectionApplication, HrProjectionRebuildPreparation, HrReadScope, HrReassignmentReadCursor,
    HrReassignmentReadPage, HrReassignmentReadRow, HrSourceEvent, HrSourceEventCursor,
    HrSourceEventPage, MAX_HR_READ_PAGE_SIZE, MAX_HR_READ_PAYROLL_LINES_PER_RUN,
    MAX_HR_SOURCE_EVENT_PAGE_SIZE, SqlxHrProjectionStore, SqlxHrReadStore,
};
pub use legal_persistence::{
    LEGAL_ACCESS_HISTORY_AGGREGATE, LEGAL_ACCESS_RECORDED_EVENT, LEGAL_CONTRACT_AGGREGATE,
    LEGAL_CONTRACT_SAVED_EVENT, LEGAL_DOCUMENT_AGGREGATE, LEGAL_DOCUMENT_SAVED_EVENT,
    LegalAccessHistoryEvent, LegalAggregateType, LegalContractEvent, LegalContractPartyEvent,
    LegalDocumentEvent, LegalDocumentVersionEvent, LegalEventType, LegalOutboxEvent,
    LegalOutboxPayload, LegalRetentionSweepReport, LegalWriteReceipt, SqlxLegalPersistence,
};
pub use legal_projection::{
    LEGAL_PROJECTION_NAME, LEGAL_READ_STALE_AFTER_SECONDS, LegalAccessHistoryReadPage,
    LegalAccessHistoryReadRow, LegalAccessReadCursor, LegalContractApprovalReadRow,
    LegalContractPartyReadRow, LegalContractReadCursor, LegalContractReadPage,
    LegalContractReadRow, LegalContractRetentionReadRow, LegalDocumentReadCursor,
    LegalDocumentReadPage, LegalDocumentReadRow, LegalDocumentVersionReadRow,
    LegalProjectionApplication, LegalProjectionRebuildPreparation, LegalReadFreshness,
    LegalReadScope, LegalSourceEvent, LegalSourceEventCursor, LegalSourceEventPage,
    LegalSourceMetadata, MAX_LEGAL_PROJECTION_EVENTS, MAX_LEGAL_READ_CHILD_ROWS,
    MAX_LEGAL_READ_PAGE_SIZE, MAX_LEGAL_SOURCE_EVENT_PAGE_SIZE, SqlxLegalProjectionStore,
    SqlxLegalReadStore,
};
pub use marketing_callback_secret_manager::{
    DEFAULT_MARKETING_SECRET_MANAGER_TIMEOUT, HttpMarketingCallbackSecretManager,
    MAX_MARKETING_SECRET_MANAGER_ENDPOINT_BYTES, MAX_MARKETING_SECRET_MANAGER_RESPONSE_BYTES,
    MarketingCallbackSecretManagerConfigError,
};
pub use marketing_callback_secrets::{
    EnvMarketingCallbackSecretResolver, MAX_MARKETING_CALLBACK_SECRET_BYTES,
    MarketingCallbackSecretError, MarketingCallbackSecretResolver, ResolvedMarketingCallbackSecret,
    UnconfiguredMarketingCallbackSecretResolver,
};
pub use marketing_content_application::SqlxMarketingContentWriteAdapter;
pub use marketing_content_object_access::{
    MAX_MARKETING_ASSET_SIGNED_URL_SECONDS, MarketingContentObjectError,
    MarketingContentObjectStore, MarketingObjectAccessContext, MarketingObjectPermission,
};
pub use marketing_content_persistence::{
    MARKETING_CONTENT_ASSET_AGGREGATE, MARKETING_CONTENT_ASSET_SAVED_EVENT,
    MARKETING_CONTENT_DOMAIN_AGGREGATE, MARKETING_CONTENT_DOMAIN_BINDING_AGGREGATE,
    MARKETING_CONTENT_DOMAIN_BOUND_EVENT, MARKETING_CONTENT_DOMAIN_SAVED_EVENT,
    MARKETING_CONTENT_DOMAIN_VERIFICATION_AGGREGATE,
    MARKETING_CONTENT_DOMAIN_VERIFICATION_ISSUED_EVENT, MARKETING_CONTENT_DOMAIN_VERIFIED_EVENT,
    MARKETING_CONTENT_FORM_AGGREGATE, MARKETING_CONTENT_FORM_SAVED_EVENT,
    MARKETING_CONTENT_FORM_SUBMISSION_AGGREGATE, MARKETING_CONTENT_FORM_SUBMISSION_REDACTED_EVENT,
    MARKETING_CONTENT_FORM_SUBMISSION_RETENTION, MARKETING_CONTENT_FORM_SUBMITTED_EVENT,
    MARKETING_CONTENT_PREVIEW_AGGREGATE, MARKETING_CONTENT_PREVIEW_EXPIRED_EVENT,
    MARKETING_CONTENT_PREVIEW_ISSUED_EVENT, MARKETING_CONTENT_PREVIEW_REVOKED_EVENT,
    MARKETING_CONTENT_PRIVACY_MAX_BATCH_SIZE, MARKETING_CONTENT_PROJECTION,
    MARKETING_CONTENT_TEMPLATE_AGGREGATE, MARKETING_CONTENT_TEMPLATE_SAVED_EVENT,
    MARKETING_CONTENT_WORKFLOW_PROJECTION, MAX_MARKETING_CONTENT_READ_PAGE_SIZE,
    MarketingContentPrivacySweepReport, MarketingContentProjectionRebuildReport,
    MarketingContentReadRow, MarketingContentWorkflowProjectionRebuildReport,
    MarketingContentWriteReceipt, SqlxMarketingContentStore,
};
pub use marketing_crm_persistence::{
    CrmAttributionWriteReceipt, CrmWriteReceipt, SqlxMarketingCrmStore,
};
pub use marketing_delivery_persistence::{
    MARKETING_JOURNEY_OWNER_NOTIFICATION_CHANNEL_EVENT, MAX_MARKETING_DELIVERY_CLAIM_BATCH_SIZE,
    MAX_MARKETING_DELIVERY_LEASE_SECONDS, MAX_MARKETING_DELIVERY_PROJECTION_PAGE_SIZE,
    MIN_MARKETING_DELIVERY_LEASE_SECONDS, MarketingDeliveryClaim,
    MarketingDeliveryProjectionReconciliation, MarketingDeliveryWriteReceipt,
    MarketingProviderCallbackReceipt, SqlxMarketingDeliveryStore,
};
pub use marketing_delivery_scheduler::{
    MARKETING_DELIVERY_SCHEDULER_DEFAULT_BACKOFF_BASE_SECONDS,
    MARKETING_DELIVERY_SCHEDULER_DEFAULT_BACKOFF_MAX_SECONDS,
    MARKETING_DELIVERY_SCHEDULER_DEFAULT_CADENCE_SECONDS, MARKETING_DELIVERY_SCHEDULER_JOB_KEY,
    MARKETING_DELIVERY_SCHEDULER_MAX_ATTEMPTS, MARKETING_DELIVERY_SCHEDULER_MAX_CADENCE_SECONDS,
    MARKETING_DELIVERY_SCHEDULER_MAX_ERROR_BYTES, MARKETING_DELIVERY_SCHEDULER_MAX_JOB_KEY_BYTES,
    MARKETING_DELIVERY_SCHEDULER_MAX_LEASE_SECONDS, MARKETING_DELIVERY_SCHEDULER_MIN_LEASE_SECONDS,
    MarketingDeliveryScheduleRequest, MarketingDeliverySchedulerClaim,
    MarketingDeliverySchedulerError, MarketingDeliverySchedulerFailureOutcome,
    MarketingDeliverySchedulerJob, MarketingDeliverySchedulerLease,
    MarketingDeliverySchedulerPolicy, MarketingDeliverySchedulerState,
    SqlxMarketingDeliverySchedulerRepository,
};
pub use marketing_journey_execution_persistence::{
    MARKETING_JOURNEY_EXECUTION_AGGREGATE, MARKETING_JOURNEY_EXECUTION_FACT_EVENT,
    MARKETING_JOURNEY_EXECUTION_STARTED_EVENT, MAX_MARKETING_JOURNEY_EXECUTION_CLAIM_BATCH_SIZE,
    MAX_MARKETING_JOURNEY_EXECUTION_LEASE_SECONDS, MIN_MARKETING_JOURNEY_EXECUTION_LEASE_SECONDS,
    MarketingJourneyExecutionClaim, MarketingJourneyExecutionFailure,
    MarketingJourneyExecutionMutation, MarketingJourneyExecutionWriteReceipt,
    SqlxMarketingJourneyExecutionStore,
};
pub use marketing_journey_integration_persistence::{
    MARKETING_JOURNEY_INTEGRATION_AGGREGATE, MARKETING_JOURNEY_INTEGRATION_FAILED_EVENT,
    MARKETING_JOURNEY_INTEGRATION_INVOKED_EVENT, MARKETING_JOURNEY_INTEGRATION_RETRY_EVENT,
    MARKETING_JOURNEY_INTEGRATION_SUCCEEDED_EVENT,
    MAX_MARKETING_JOURNEY_INTEGRATION_CLAIM_BATCH_SIZE,
    MAX_MARKETING_JOURNEY_INTEGRATION_LEASE_SECONDS,
    MAX_MARKETING_JOURNEY_INTEGRATION_PROJECTION_PAGE_SIZE,
    MAX_MARKETING_JOURNEY_INTEGRATION_REBUILD_EVENTS,
    MIN_MARKETING_JOURNEY_INTEGRATION_LEASE_SECONDS, MarketingJourneyIntegrationClaim,
    MarketingJourneyIntegrationProjectionRebuildPreparation,
    MarketingJourneyIntegrationProjectionReconciliation, MarketingJourneyIntegrationProviderResult,
    MarketingJourneyIntegrationReadRow, MarketingJourneyIntegrationWriteReceipt,
    SqlxMarketingJourneyIntegrationStore,
};
pub use marketing_journey_persistence::{
    MARKETING_JOURNEY_AGGREGATE, MARKETING_JOURNEY_CREATED_EVENT,
    MARKETING_JOURNEY_STATUS_CHANGED_EVENT, MarketingJourneyWriteReceipt,
    SqlxMarketingJourneyStore,
};
pub use marketing_persistence::{
    AudienceMembershipPage, CrmReadPage, CrmReadRow, MARKETING_JOURNEY_CONVERSION_AGGREGATE,
    MARKETING_JOURNEY_CONVERSION_EVENT, MARKETING_JOURNEY_OWNER_NOTIFICATION_AGGREGATE,
    MARKETING_JOURNEY_OWNER_NOTIFICATION_EVENT, MAX_CRM_READ_PAGE_SIZE, MAX_MARKETING_PAGE_SIZE,
    MAX_MARKETING_SOURCE_EVENT_PAGE_SIZE, MarketingJourneyConversionReadPage,
    MarketingJourneyConversionReadRow, MarketingJourneyOwnerNotificationDeliveryReadPage,
    MarketingJourneyOwnerNotificationDeliveryReadRow, MarketingJourneyOwnerNotificationReadPage,
    MarketingJourneyOwnerNotificationReadRow, MarketingMutationReceipt,
    MarketingProjectionApplication, MarketingProjectionRebuildPreparation, MarketingSourceEvent,
    MarketingSourceEventCursor, MarketingSourceEventPage, ProfileUpsertReceipt, SqlxMarketingStore,
};
pub use marketing_privacy_application::SqlxMarketingProfilePrivacyAdapter;
pub use marketing_provider_onboarding::SqlxMarketingProviderOnboardingStore;
pub use object_storage::{
    MinioObjectStore, MinioObjectStoreConfig, ObjectInventoryEntry, ObjectStorageError,
    SignedObjectUrl,
};
pub use outbox::{
    ControlPlaneWriteTransaction, DeadLetterReplayCommand, DeadLetterReplayReceipt, EventMetadata,
    IdempotencyReservation, IdempotencyState, MAX_OUTBOX_CLAIM_BATCH_SIZE, OutboxEnvelope,
    ProjectionCheckpointDecision, ProjectionCheckpointRecord,
};
pub use pitr::{
    LocalPitrRestoreTargetResolver, PhysicalPitrRestoreExecutor, PhysicalPitrRestoreTarget,
    PitrBackupAdapter, PitrBackupAdapterConfig, PitrBackupResult, PitrBackupTarget,
    PitrRestoreTargetResolver, PostgresPhysicalPitrRestoreExecutor,
    UnconfiguredPhysicalPitrRestoreExecutor, UnconfiguredPitrRestoreTargetResolver,
};
pub use platform_completion::*;
pub use pools::{ControlPlaneDatabaseConfig, ControlPlanePoolSettings, ControlPlanePools};
pub use pos_persistence::{PosWriteReceipt, SqlxPosPersistence};
pub use pos_projection::{
    MAX_POS_READ_CHILD_ROWS_PER_SALE, MAX_POS_READ_PAGE_SIZE, MAX_POS_SOURCE_EVENT_PAGE_SIZE,
    POS_PROJECTION_NAME, PosProjectionApplication, PosProjectionRebuildPreparation, PosReadScope,
    PosSaleReadCursor, PosSaleReadLine, PosSaleReadPage, PosSaleReadPayment, PosSaleReadRow,
    PosSourceEvent, PosSourceEventCursor, PosSourceEventPage, SqlxPosProjectionStore,
};
pub use provisioning::{
    ApprovedDatabaseProfileConnection, BackupAdapterError, BackupExecutionRequest,
    BackupExecutionResult, BackupExecutor, BackupExecutorError, BackupFailureClass,
    BackupObjectTransport, BackupPitrObservation, BackupProcessCommand, BackupProcessRunner,
    BackupRunKind, DatabaseReadinessObservation, DatabaseSide, HoldingBackupJobClaim,
    HoldingBackupJobReceipt, HoldingBackupJobRequest, HoldingBackupJobSummary, HoldingBackupRun,
    HoldingBackupTarget, HoldingDataRouteScope, HoldingDatabaseHealth, HoldingDatabaseProvisioner,
    HoldingDatabaseProvisioningConfig, HoldingDatabaseRouter, HoldingOrganizationBootstrap,
    HoldingOwnerSeed, HoldingProvisioner, HoldingReadDatabaseReplacement, HoldingReadinessMetadata,
    HoldingReadinessObservation, HoldingRestoreTestActivation, HoldingRestoreTestTarget,
    HoldingRoutingObservation, HoldingSourceProjection, HoldingSourceRebuildLock,
    HoldingStorageHealth, MAX_HOLDING_BACKUP_JOB_ATTEMPTS, MAX_HOLDING_BACKUP_JOB_LEASE_SECONDS,
    MAX_HOLDING_BACKUP_JOB_PAGE_SIZE, MAX_HOLDING_BACKUP_JOB_PROJECTION_BATCH_SIZE,
    MAX_HOLDING_PITR_INVENTORY_ENTRIES, MAX_RESTORE_TEST_ATTEMPTS,
    MAX_RESTORE_TEST_SCHEDULE_BATCH_SIZE, MIN_HOLDING_BACKUP_JOB_LEASE_SECONDS,
    MinioBackupObjectTransport, PhysicalHoldingDatabases, PostgresMinioBackupAdapter,
    PostgresMinioBackupConfig, PostgresRestoredDatabaseVerifier, ProjectionFreshnessObservation,
    ProvisioningState, ReadinessComponentObservation, ReadinessObservationPolicy,
    ReadinessObservationState, RecoveryHistoryClassification, RestoreExecutor,
    RestoreExecutorError, RestoreTestClaim, RestoreTestClaimResult, RestoreTestExecutionRequest,
    RestoreTestExecutionResult, RestoreTestHistoryEntry, RestoreTestOutcome, RestoreTestSchedule,
    RestoreTestScheduleError, RestoreTestScheduleReceipt, RestoreTestScheduleRequest,
    RestoreTestScheduleState, RestoreTestScheduler, RestoredDatabaseVerifier, RoutedDatabaseSide,
    RoutedHoldingDatabase, SystemBackupProcessRunner, ValidatedBackupExecutionResult,
    ValidatedRestoreTestResult, holding_holding_scope_id, holding_owner_assignment_id,
    initialize_holding_organization_in_pools, projection_rebuild_activation_decision_ref,
};
pub use quality_persistence::{
    MAX_QUALITY_PROJECTION_EVENTS, MAX_QUALITY_READ_PAGE_SIZE, MAX_QUALITY_SOURCE_PAGE_SIZE,
    QualityInspectionReadCursor, QualityInspectionReadPage, QualityInspectionReadRow,
    QualityProjectionApplication, QualityProjectionRebuildPreparation, QualityReadScope,
    QualitySourceEvent, QualitySourceEventCursor, QualitySourceEventPage, QualityWriteReceipt,
    SqlxQualityStore, quality_request_hash,
};
pub use repositories::{
    FinanceBankProviderAccountBinding, HoldingStoreRegistration, HoldingStoreRegistrationDetail,
    HoldingStoreRegistrationPage, MARKETING_PUBLIC_ABUSE_WINDOW_SECONDS,
    MAX_HOLDING_REGISTRY_PAGE_SIZE, MAX_MARKETING_PROVIDER_ACCOUNTS_PER_HOLDING,
    MAX_MARKETING_PUBLIC_ABUSE_LIMIT, MAX_PLATFORM_AUDIT_PAGE_SIZE,
    MAX_PLATFORM_INTEGRATION_OBSERVATIONS, MarketingProviderAccountBinding,
    MarketingProviderChannelRouteBinding, MarketingPublicAbuseWindowRequest,
    ObjectAccessBindingRequest, PlatformAuditEntry, PlatformAuditPage,
    PlatformIntegrationObservation, ProjectionAuthorizationRequest, SodDecision, SqlxControlPlane,
    SqlxHoldingStoreRegistry, SupportCapabilityAuthorizationRequest,
};
pub use role_review::{
    ClaimedRoleExpiryNotification, ROLE_EXPIRY_NOTIFICATION_EVENT_TYPE, RoleRecoveryReceipt,
    RoleReviewPersistence, RoleReviewPersistenceError, RoleReviewSnapshotReceipt,
};
pub use runtime_readiness::{
    ReadinessState, WorkerHeartbeat, WorkerHeartbeatObservation, WorkerHeartbeatReadiness,
    WorkerLiveReadinessObservation, WorkerLiveReadinessPolicy, WorkerReadinessComposition,
    WorkerReadinessMetadata, WorkerReadinessObserver, aggregate_readiness_states,
    backup_pitr_readiness, holding_routing_readiness, projection_freshness_readiness,
};
pub use sales_persistence::{
    MAX_SALES_PROJECTION_EVENTS, MAX_SALES_READ_PAGE_SIZE, MAX_SALES_SOURCE_EVENT_PAGE_SIZE,
    SalesAggregate, SalesProjectionApplication, SalesProjectionRebuildPreparation, SalesReadCursor,
    SalesReadListRow, SalesReadPage, SalesReadScope, SalesSourceEvent, SalesSourceEventCursor,
    SalesSourceEventPage, SalesWriteReceipt, SqlxSalesStore,
};
pub use scm_persistence::{
    MAX_SCM_READ_PAGE_SIZE, ScmAggregate, ScmProjectionApplication,
    ScmProjectionRebuildPreparation, ScmReadCursor, ScmReadListRow, ScmReadPage, ScmReadRow,
    ScmReadScope, ScmSourceEventCursor, ScmWriteReceipt, SqlxScmStore,
};
pub use shared_enterprise_services::{SharedProjectionRead, SqlxSharedEnterpriseServiceStore};
pub use stripe_client::{
    DEFAULT_STRIPE_HTTP_TIMEOUT, DEFAULT_STRIPE_REQUEST_BODY_BYTES,
    DEFAULT_STRIPE_RESPONSE_BODY_BYTES, MAX_STRIPE_HTTP_TIMEOUT, MAX_STRIPE_REQUEST_BODY_BYTES,
    MAX_STRIPE_RESPONSE_BODY_BYTES, StripeApiKey, StripeCheckoutLineItem, StripeCheckoutMode,
    StripeCheckoutSessionCreateRequest, StripeCheckoutSessionResponse, StripeCredentialError,
    StripeCustomerPortalSessionCreateRequest, StripeCustomerPortalSessionResponse,
    StripeEntitlementFeatureCreateRequest, StripeEntitlementFeatureListRequest,
    StripeEntitlementFeatureResponse, StripeHttpClient, StripeHttpConfigurationError,
    StripeHttpEndpoint, StripeHttpError, StripeHttpLimits, StripeHttpMethod,
    StripeHttpRequestError, StripeListResponse, StripeMeterAggregation,
    StripeMeterAggregationFormula, StripeMeterAggregationResponse, StripeMeterCreateRequest,
    StripeMeterListRequest, StripeMeterResponse, StripeMeterStatus, StripePriceCreateRequest,
    StripePriceInterval, StripePriceListRequest, StripePriceRecurring,
    StripePriceRecurringResponse, StripePriceResponse, StripePriceUsageType,
    StripeProductCreateRequest, StripeProductListRequest, StripeProductResponse,
    StripeProductUpdateRequest, StripeRequestEnvelope, StripeResponseEnvelope,
};
pub use support_persistence::{
    CapabilityMetadata, CustomerVisibleSupportAttachmentPage,
    IdempotencyKey as SupportIdempotencyKey, IncidentPage, MAX_INCIDENT_LIST_PAGE_SIZE,
    MAX_SUPPORT_ATTACHMENT_LIST_PAGE_SIZE, MAX_SUPPORT_SLA_NOTIFICATION_CLAIM_BATCH_SIZE,
    MAX_SUPPORT_TICKET_LIST_PAGE_SIZE, Persisted as PersistedSupport, PersistedSupportAttachment,
    SupportApproval, SupportAttachmentPage, SupportPersistence, SupportPersistenceError,
    SupportTicketListCursor, SupportTicketPage,
};
pub use support_workflow::{
    MAX_SUPPORT_WORKFLOW_REBUILD_EVENTS, MAX_SUPPORT_WORKFLOW_RECONCILIATION_EVENTS,
    SUPPORT_CAPABILITY_ISSUED_EVENT, SUPPORT_CAPABILITY_REVOKED_EVENT, SUPPORT_DIAGNOSTICS_EVENT,
    SUPPORT_SESSION_APPROVED_EVENT, SUPPORT_SESSION_REVOKED_EVENT,
    SUPPORT_WORKFLOW_PROJECTION_NAME, SupportAccessProjectionRow, SupportDiagnosticsProjectionRow,
    SupportWorkflowPersistenceError, SupportWorkflowReconciliation, SupportWorkflowStore,
};
pub use worker::{
    OutboxHandler, OutboxWorker, ProjectionHandler, ProjectionRebuildReport, RetryDisposition,
    RetryPolicy, WorkerError, WorkerErrorContext, WorkerLoopReport, WorkerRunReport,
};

pub use agent_notifications::SqlxAgentNotificationPort;
#[cfg(test)]
pub use test_support::{InMemoryControlPlane, ProjectionRunner};
