use std::future::Future;
use std::sync::Arc;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    net::SocketAddr,
};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use erpx_application::{
    AcknowledgeMarketingProviderVerificationCommand, ApplicationError, AuditEvent, AuditSink,
    AuthoritativeRoleLifecycleService, AuthoritativeSupportService,
    AuthorizedMarketingProfilePrivacyPort, BindDomainCommand,
    ConfigureMarketingProviderAccountCommand, Customer360DetailReadStore,
    CustomerGlobalReadAuditDetailCursor, CustomerGlobalReadAuditDetailQuery,
    CustomerGlobalReadAuditDetailService, CustomerGlobalReadAuditQueryContext,
    CustomerReadMetadata, CustomerReadView, CustomerSecurityAdminScope,
    DEFAULT_MARKETING_PROVIDER_PRIORITY, EraseMarketingProfileCommand,
    IssueDomainVerificationCommand, IssuePreviewCommand, MarketingContentWritePort,
    MarketingProfileErasureCorrelation, MarketingProviderChannelRoute, PlatformCustomerService,
    RevokePreviewCommand, RoleAssignmentStore, RoleLifecycleService as RoleLifecycleUseCase,
    SetMarketingProviderAccountStateCommand, SubmitPublishedFormCommand,
    SupportIntersectionRequest, SupportIntersectionService, SupportStore, SystemClock,
    VerifyDomainCommand, authorize_and_erase_marketing_profile,
};
use erpx_domain::{
    access::{
        EffectiveAccessRequest, HOLDING_OWNER_ROLE, RiskTier, RoleAssignment,
        RoleExpiryNotificationKind, RoleRecoveryRequest, RoleRecoveryTarget, RoleReviewItem, Scope,
        evaluate_effective_access_chain,
    },
    commercial::{
        ActivateBranchModuleCommand, ActivateCompanyModuleCommand, AllocateCompanyCommand,
        BillingState, CommercialCommandContext, EntitlementAllocationBook, EntitlementDecision,
        authorize_module,
    },
    contracts::{ActorAttribution, ActorKind, CONTRACT_VERSION, CommandContext, TenantScope},
    customer::{
        Customer360DetailReadContext, CustomerGlobalReadContext, CustomerNoteCursor,
        CustomerStatus, GlobalReadActor, GlobalReadAuditRecord, GlobalReadScope, HealthState,
    },
    finance::{
        AccountKind, AccountingPeriod, ChartOfAccounts, FinanceAccessPath,
        FinanceAuthorizationOperation, FinanceAuthorizationRequest, JournalEntry,
    },
    finance_banking::{
        BankAccount, BankMatchDecision, BankMatchProposal, BankReconciliation, BankStatementImport,
    },
    finance_chart::FinanceChart,
    finance_close::FinanceCloseReview,
    finance_subledgers::{Allocation, AllocationBatch, Invoice, Payment},
    finance_tax::ThaiTaxFact,
    hr::{EmployeeRecord, ExpenseClaim, LeaveRequest, PayrollRun},
    legal::{LegalAccessHistoryEntry, LegalContract, LegalDocument},
    marketing::MarketingProfileErasure,
    marketing_abuse::{
        AnonymousMarketingAbuseInput, AnonymousMarketingAbusePolicy, MarketingAbuseDecision,
        MarketingAbuseSignal,
    },
    marketing_callback::{
        MarketingProviderCallbackBinding, MarketingProviderCallbackKind,
        MarketingProviderCallbackRequest, ProviderAuthenticationDecision,
        VerifiedProviderAuthentication,
    },
    marketing_callback_secret::MarketingCallbackSecretReference,
    marketing_content::{ContentFormRef, ContentTemplateRef, CustomDomainBinding, PreviewGrant},
    marketing_crm::{AcceptedLeadHandoff, AttributionModel, CrmOpportunity},
    marketing_delivery::DeliveryState,
    operations::completion::OperationsManagerChainProof,
    operations::{
        ExceptionLifecycleAction, ExceptionLifecycleCommand, ExceptionPrincipalReference,
        ExceptionSlaMetadata, ExceptionStatus, MAX_EXCEPTION_SLA_MINUTES,
        MAX_OPERATIONS_SOURCE_EVENT_PAYLOAD_BYTES, OperationsSourceDomain, OperationsSourceEvent,
        OrganizationStatus,
    },
    platform_completion::{
        BackupControlCommand, PitrRecoveryPoint, PlatformAuthorizationProof, PlatformBackupKind,
        TenantBinding, TenantLifecycleAction, TenantLifecycleCommand,
    },
    pos::Sale,
    quality::QualityInspection,
    support::{
        Incident, IncidentStatus, SupportCapability, SupportCapabilityBinding,
        SupportCapabilitySecret, SupportDomainPolicy, SupportMode, TicketStatus,
    },
};
use erpx_identity::{
    AccessTokenClaims, AccessTokenSigner, DirectoryConnector, DirectoryFeedKind, DirectoryProvider,
    DirectoryRoleAccess, DirectoryRoleMapping, DirectoryScope, DirectoryTlsMode,
    DirectoryTlsPolicy, IdentityStoreError, PasskeyCeremonyService, PasswordService,
    RecoveryCodeSet, SqlxDirectoryStore, SqlxIdentityStore, TotpKeyResolver, WebAuthnProvider,
    WebAuthnProviderConfig, WebAuthnRsProvider, apply_identity_migration,
};
use erpx_infrastructure::{
    AssetMaintenanceAggregate, AssetMaintenanceReadCursor, AssetMaintenanceReadScope,
    BankProviderAccountBinding, BankProviderAccountRef, BankProviderCredentialKey,
    CloseReviewSaveReceipt, ControlPlaneDatabaseConfig, ControlPlanePools, DeadLetterReplayCommand,
    DiagnosticsLimits, DiagnosticsSnapshot, DiagnosticsSnapshotInput,
    EnvMarketingCallbackSecretResolver, FinanceAllocationBatchReceipt, FinanceBankingReadCursor,
    FinanceBankingReadRow, FinanceChartReadCursor, FinanceChartReadScope,
    FinanceCloseReadAvailability, FinanceCloseReadCursor, FinanceCloseReadFreshness,
    FinanceCloseReadListRow, FinanceCloseReadMetadata, FinanceCloseReadResult,
    FinanceCloseReadScope, FinanceCloseReadView, FinanceSubledgerAggregate,
    FinanceSubledgerReadCursor, FinanceSubledgerReadRow, FinanceSubledgerWriteReceipt,
    FinanceThaiTaxProjectionReconciliation, HrExpenseReadCursor, HrLeaveReadCursor,
    HrPayrollRunReadCursor, HrReadScope, HrReassignmentReadCursor,
    HttpMarketingCallbackSecretManager, LegalAccessReadCursor, LegalContractReadCursor,
    LegalDocumentReadCursor, LegalReadScope, MAX_ASSET_MAINTENANCE_READ_PAGE_SIZE,
    MAX_CRM_READ_PAGE_SIZE, MAX_FINANCE_BANKING_READ_PAGE_SIZE, MAX_FINANCE_CHART_READ_PAGE_SIZE,
    MAX_FINANCE_CLOSE_READ_PAGE_SIZE, MAX_FINANCE_READ_JOURNAL_PAGE_SIZE,
    MAX_FINANCE_SUBLEDGER_READ_PAGE_SIZE, MAX_HR_READ_PAGE_SIZE, MAX_HR_READ_PAYROLL_LINES_PER_RUN,
    MAX_INCIDENT_LIST_PAGE_SIZE, MAX_LEGAL_READ_PAGE_SIZE, MAX_MARKETING_CONTENT_READ_PAGE_SIZE,
    MAX_OPERATIONS_EVENT_PAGE_SIZE, MAX_OPERATIONS_EXCEPTION_PAGE_SIZE, MAX_ORGANIZATION_PAGE_SIZE,
    MAX_PLATFORM_AUDIT_PAGE_SIZE, MAX_POS_READ_PAGE_SIZE, MAX_PROJECTION_STATUS_FRESHNESS_SECONDS,
    MAX_PROJECTION_STATUS_PAGE_SIZE, MAX_QUALITY_READ_PAGE_SIZE, MAX_SALES_READ_PAGE_SIZE,
    MAX_SCM_READ_PAGE_SIZE, MAX_TENANT_REGISTRY_PAGE_SIZE, MarketingCallbackSecretResolver,
    MarketingPublicAbuseWindowRequest, OPERATIONS_READ_PROJECTION, ObjectAccessBindingRequest,
    OperationsExceptionCursor, OperationsJsonReadCursor, OrganizationNodeCommand,
    OrganizationNodeKind, PersistenceError, PlatformControlStore, PosReadScope, PosSaleReadCursor,
    ProjectionPolicy, ProjectionSchedulerAuthorization, ProjectionStatusCursor,
    QualityInspectionReadCursor, QualityReadScope, ReadinessObservationState,
    RoleReviewPersistence, RoutedDatabaseSide, RoutedTenantDatabase, SalesAggregate,
    SalesReadCursor, SalesReadScope, ScmAggregate, ScmReadCursor, ScmReadScope,
    SqlxAssetMaintenanceStore, SqlxCommercialStore, SqlxControlPlane, SqlxFinanceBankingStore,
    SqlxFinanceChartReadStore, SqlxFinanceChartStore, SqlxFinanceCloseReadAdapter,
    SqlxFinanceCloseStore, SqlxFinanceStore, SqlxFinanceSubledgerStore, SqlxFinanceThaiTaxStore,
    SqlxG13MigrationStore, SqlxHrPersistence, SqlxHrReadStore, SqlxLegalPersistence,
    SqlxLegalReadStore, SqlxMarketingContentStore, SqlxMarketingContentWriteAdapter,
    SqlxMarketingCrmStore, SqlxMarketingDeliveryStore, SqlxMarketingProfilePrivacyAdapter,
    SqlxMarketingProviderOnboardingStore, SqlxMarketingStore, SqlxOperationsStore,
    SqlxOrganizationStore, SqlxQualityStore, SqlxSalesStore, SqlxScmStore, SqlxTenantStoreRegistry,
    StripeInboxInsert, StripeWebhookEvent, StripeWebhookPersistenceContext,
    SupportCapabilityAuthorizationRequest, SupportIdempotencyKey, SupportPersistence,
    SupportPersistenceError, SupportTicketListCursor, TenantDatabaseProvisioner,
    TenantDatabaseProvisioningConfig, TenantDatabaseRouter, TenantHoldingBinding, TenantOwnerSeed,
    TenantProjectionSchedulerRepository, TenantProjectionStatusReadRepository, TenantProvisioner,
    TenantSourceProjection, TenantStoreRegistration, UnconfiguredMarketingCallbackSecretResolver,
    quality_request_hash,
};
use erpx_proto::platform::{
    AcknowledgeMarketingProviderVerificationRequest,
    AcknowledgeMarketingProviderVerificationResponse, ActivateBranchEntitlementRequest,
    ActivateBranchEntitlementResponse, ActivateCompanyEntitlementRequest,
    ActivateCompanyEntitlementResponse, ActivateProjectionRebuildRequest,
    ActivateProjectionRebuildResponse, AddTicketInternalNoteRequest, AddTicketInternalNoteResponse,
    AddTicketReplyRequest, AddTicketReplyResponse, AllocateCompanyEntitlementRequest,
    AllocateCompanyEntitlementResponse, ApproveSupportSessionRequest,
    BeginTenantProvisioningRequest, BeginTenantProvisioningResponse, BindMarketingDomainRequest,
    BindMarketingDomainResponse, CheckEntitlementRequest, CheckEntitlementResponse,
    ConfigureMarketingProviderAccountRequest, ConfigureMarketingProviderAccountResponse,
    CreateIncidentRequest, CreateIncidentResponse, CreateOperationsExceptionFromSourceRequest,
    CreateOperationsExceptionFromSourceResponse, CreateQualityInspectionRequest,
    CreateQualityInspectionResponse, CreateTicketRecurrenceRequest, CreateTicketRecurrenceResponse,
    CreateTicketRequest, CreateTicketResponse, CrmReadRow, Customer360SectionFact,
    Customer360SectionState, CustomerAccessAuditEntry, CustomerNote as CustomerNoteMessage,
    CustomerSummary, DirectoryConnectorSummary, DirectoryIdentityLinkSummary,
    DirectoryImportStatus as DirectoryImportStatusMessage, DirectoryRoleMappingMessage,
    EnqueueFinanceBankProviderImportRequest, EnqueueFinanceBankProviderImportResponse,
    EntitlementCommandReceipt, EraseMarketingProfileRequest, EraseMarketingProfileResponse,
    FinanceConsolidatedTrialBalanceLine, FinanceJournal, FinanceTrialBalanceLine,
    GetAssetMaintenanceAggregateRequest, GetAssetMaintenanceAggregateResponse,
    GetCustomer360DetailRequest, GetCustomer360DetailResponse, GetCustomerAccessAuditRequest,
    GetCustomerAccessAuditResponse, GetCustomerProjectionRequest, GetCustomerProjectionResponse,
    GetCustomerRequest, GetCustomerResponse, GetDirectoryImportStatusRequest,
    GetDirectoryImportStatusResponse, GetFinanceBankProviderImportJobStatusRequest,
    GetFinanceBankProviderImportJobStatusResponse, GetFinanceBankingReadRowRequest,
    GetFinanceBankingReadRowResponse, GetFinanceCloseReviewRequest, GetFinanceCloseReviewResponse,
    GetFinanceConsolidatedTrialBalanceRequest, GetFinanceConsolidatedTrialBalanceResponse,
    GetFinanceTrialBalanceRequest, GetFinanceTrialBalanceResponse, GetIncidentRequest,
    GetIncidentResponse, GetMarketingDeliveryReadRequest, GetMarketingDeliveryReadResponse,
    GetOperationsSummaryRequest, GetOperationsSummaryResponse, GetPlatformSettingsRequest,
    GetPlatformSettingsResponse, GetPlatformTenantRequest, GetPlatformTenantResponse,
    GetQualityInspectionRequest, GetQualityInspectionResponse, GetSalesAggregateRequest,
    GetSalesAggregateResponse, GetScmAggregateRequest, GetScmAggregateResponse,
    GetSupportDiagnosticsRequest, GetSupportDiagnosticsResponse, GetThaiTaxReportRequest,
    GetThaiTaxReportResponse, GetTicketRequest, GetTicketResponse, ImportBankStatementRequest,
    ImportBankStatementResponse, Incident as IncidentMessage,
    IssueMarketingDomainVerificationRequest, IssueMarketingDomainVerificationResponse,
    IssueMarketingPreviewRequest, IssueMarketingPreviewResponse,
    ListAssetMaintenanceAggregatesRequest, ListAssetMaintenanceAggregatesResponse,
    ListCrmReadRowsRequest, ListCrmReadRowsResponse, ListCustomersRequest, ListCustomersResponse,
    ListDirectoryConnectorsRequest, ListDirectoryConnectorsResponse,
    ListDirectoryIdentityLinksRequest, ListDirectoryIdentityLinksResponse,
    ListDirectoryRoleMappingsRequest, ListDirectoryRoleMappingsResponse, ListExpiringRolesRequest,
    ListExpiringRolesResponse, ListFinanceBankingReadRowsRequest,
    ListFinanceBankingReadRowsResponse, ListFinanceChartsRequest, ListFinanceChartsResponse,
    ListFinanceCloseReviewsRequest, ListFinanceCloseReviewsResponse, ListFinanceJournalsRequest,
    ListFinanceJournalsResponse, ListFinanceSubledgerReadRowsRequest,
    ListFinanceSubledgerReadRowsResponse, ListHrEmployeesRequest, ListHrEmployeesResponse,
    ListHrExpenseClaimsRequest, ListHrExpenseClaimsResponse, ListHrLeaveRequestsRequest,
    ListHrLeaveRequestsResponse, ListHrPayrollRunsRequest, ListHrPayrollRunsResponse,
    ListHrReassignmentsRequest, ListHrReassignmentsResponse, ListIncidentsRequest,
    ListIncidentsResponse, ListLegalAccessHistoryRequest, ListLegalAccessHistoryResponse,
    ListLegalContractsRequest, ListLegalContractsResponse, ListLegalDocumentsRequest,
    ListLegalDocumentsResponse, ListMarketingContentReadRowsRequest,
    ListMarketingContentReadRowsResponse, ListOperationsExceptionsRequest,
    ListOperationsExceptionsResponse, ListOperationsWorkQueuePoliciesRequest,
    ListOperationsWorkQueuePoliciesResponse, ListOrganizationNodesRequest,
    ListOrganizationNodesResponse, ListPlatformAuditRequest, ListPlatformAuditResponse,
    ListPlatformIntegrationsRequest, ListPlatformIntegrationsResponse, ListPlatformTenantsRequest,
    ListPlatformTenantsResponse, ListPosSalesRequest, ListPosSalesResponse,
    ListProjectionStatusesRequest, ListProjectionStatusesResponse, ListQualityInspectionsRequest,
    ListQualityInspectionsResponse, ListRoleReviewRequest, ListRoleReviewResponse,
    ListSalesAggregatesRequest, ListSalesAggregatesResponse, ListScmAggregatesRequest,
    ListScmAggregatesResponse, ListTenantBackupJobsRequest, ListTenantBackupJobsResponse,
    ListTicketsRequest, ListTicketsResponse, MarketingContentReadRow, MarketingDeliveryRead,
    OperationsException, OperationsSourceCount, OperationsSummary,
    OperationsWorkQueuePolicy as OperationsWorkQueuePolicyMessage,
    PlatformAuditEntry as PlatformAuditEntryMessage, PlatformIntegrationStatus,
    PlatformTenantSummary, PostFinanceJournalRequest, PostFinanceJournalResponse,
    ProjectionStatus as ProjectionStatusMessage, ProvisionTenantResourcesRequest,
    ProvisionTenantResourcesResponse, PublishOperationsWorkQueuePolicyRequest,
    PublishOperationsWorkQueuePolicyResponse, RecordCrmAttributionRequest,
    RecordCrmAttributionResponse, RecordLegalAccessRequest, RecordLegalAccessResponse,
    RecordThaiTaxFactRequest, RecordThaiTaxFactResponse, RecoverFinalRoleRequest,
    RecoverFinalRoleResponse, RegisterBankAccountRequest, RegisterBankAccountResponse,
    RenewRoleRequest, RenewRoleResponse, ReplayDeadLetterOutboxRequest,
    ReplayDeadLetterOutboxResponse, RequestSupportSessionRequest, RequestSupportSessionResponse,
    RevokeMarketingPreviewRequest, RevokeMarketingPreviewResponse, RevokeSupportSessionRequest,
    RoleAssignment as RoleAssignmentMessage, RoleReviewItem as RoleReviewItemMessage,
    SaveAssetMaintenanceAggregateRequest, SaveAssetMaintenanceAggregateResponse,
    SaveBankMatchDecisionRequest, SaveBankMatchDecisionResponse, SaveBankMatchProposalRequest,
    SaveBankMatchProposalResponse, SaveBankReconciliationRequest, SaveBankReconciliationResponse,
    SaveCrmHandoffRequest, SaveCrmHandoffResponse, SaveCrmOpportunityRequest,
    SaveCrmOpportunityResponse, SaveDirectoryConnectorRequest, SaveDirectoryConnectorResponse,
    SaveDirectoryRoleMappingRequest, SaveDirectoryRoleMappingResponse,
    SaveFinanceAllocationBatchRequest, SaveFinanceAllocationBatchResponse, SaveFinanceChartRequest,
    SaveFinanceChartResponse, SaveFinanceCloseReviewRequest, SaveFinanceCloseReviewResponse,
    SaveFinancePeriodRequest, SaveFinancePeriodResponse, SaveFinanceSubledgerRequest,
    SaveFinanceSubledgerResponse, SaveHrEmployeeRequest, SaveHrEmployeeResponse,
    SaveHrExpenseRequest, SaveHrExpenseResponse, SaveHrLeaveRequest, SaveHrLeaveResponse,
    SaveHrPayrollRequest, SaveHrPayrollResponse, SaveLegalContractRequest,
    SaveLegalContractResponse, SaveLegalDocumentRequest, SaveLegalDocumentResponse,
    SaveOrganizationNodeRequest, SaveOrganizationNodeResponse, SavePosSaleRequest,
    SavePosSaleResponse, SaveSalesAggregateRequest, SaveSalesAggregateResponse,
    SaveScmAggregateRequest, SaveScmAggregateResponse, ScheduleTenantBackupRequest,
    ScheduleTenantBackupResponse, SetMarketingProviderAccountStateRequest,
    SetMarketingProviderAccountStateResponse, StartDirectoryImportRequest,
    StartDirectoryImportResponse, StartProjectionRebuildRequest, StartProjectionRebuildResponse,
    SubmitMarketingFormRequest, SubmitMarketingFormResponse, SupportDiagnosticField,
    SupportDiagnosticsAccess, SupportSession as SupportSessionMessage,
    TenantBackupJobSummary as TenantBackupJobSummaryMessage, Ticket as TicketMessage, TicketReply,
    TransitionHrEmployeeRequest, TransitionHrEmployeeResponse,
    TransitionOperationsExceptionRequest, TransitionOperationsExceptionResponse,
    TransitionQualityInspectionRequest, TransitionQualityInspectionResponse,
    TransitionSalesAggregateRequest, TransitionSalesAggregateResponse,
    TransitionScmAggregateRequest, TransitionScmAggregateResponse, TransitionTenantStateRequest,
    TransitionTenantStateResponse, UpdateIncidentRequest, UpdateIncidentResponse,
    UpdateTicketRequest, UpdateTicketResponse, VerifyMarketingDomainRequest,
    VerifyMarketingDomainResponse,
    asset_maintenance_service_server::{AssetMaintenanceService, AssetMaintenanceServiceServer},
    crm_service_server::{CrmService, CrmServiceServer},
    directory_service_server::{DirectoryService as DirectoryRpc, DirectoryServiceServer},
    entitlement_service_server::{EntitlementService, EntitlementServiceServer},
    finance_service_server::{FinanceService, FinanceServiceServer},
    hr_service_server::{HrService, HrServiceServer},
    legal_service_server::{LegalService, LegalServiceServer},
    marketing_content_service_server::{MarketingContentService, MarketingContentServiceServer},
    migration_service_server::MigrationServiceServer,
    operations_service_server::{OperationsService, OperationsServiceServer},
    organization_service_server::{OrganizationService, OrganizationServiceServer},
    platform_customer_service_server::{
        PlatformCustomerService as CustomerRpc, PlatformCustomerServiceServer,
    },
    platform_operations_service_server::{
        PlatformOperationsService as PlatformOperationsRpc, PlatformOperationsServiceServer,
    },
    pos_service_server::{PosService, PosServiceServer},
    quality_service_server::{QualityService, QualityServiceServer},
    role_lifecycle_service_server::{
        RoleLifecycleService as RoleLifecycleRpc, RoleLifecycleServiceServer,
    },
    sales_service_server::{SalesService, SalesServiceServer},
    scm_service_server::{ScmService, ScmServiceServer},
    support_service_server::{SupportService as SupportRpc, SupportServiceServer},
};
use erpx_worker::{
    FinanceBankProviderImportAuthorization, FinanceBankProviderImportJobStatus,
    FinanceBankProviderImportRequest, ReadinessState, SqlxFinanceBankProviderImportJobSource,
    WorkerHeartbeatReadiness, WorkerLiveReadinessPolicy, WorkerReadinessComposition,
};
use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tonic::{Request, Response, Status};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

mod boundary;
mod errors;
mod identity_http;
mod migration;
mod platform_completion;
mod storage;
use boundary::{command_domain, query as query_context, transport};
use platform_completion::{
    PlatformBackupApiRequest, PlatformCompletionApi, PlatformLifecycleApiRequest,
    PlatformReadAuthorization,
};
use storage::{
    ObjectAccessError, ObjectAuthorizationError, StorageComposition, TenantObjectDownloadRequest,
    TenantObjectUploadRequest,
};

const PLATFORM_CUSTOMER_READ_PERMISSION: &str = "platform.customer.read";
const PLATFORM_CUSTOMER_ACCESS_AUDIT_PERMISSION: &str = "platform.customer.access_audit.read";
const PLATFORM_TENANT_READ_PERMISSION: &str = "platform.tenant.read";
const PLATFORM_TENANT_PROVISION_PERMISSION: &str = "platform.tenant.provision";
const PLATFORM_TENANT_BACKUP_PERMISSION: &str = "platform.tenant.backup";
const PLATFORM_TENANT_SUSPEND_PERMISSION: &str = "platform.tenant.suspend";
const PLATFORM_TENANT_RECOVERY_PERMISSION: &str = "platform.tenant.recovery";
const PLATFORM_PROJECTION_REBUILD_PERMISSION: &str = "platform.projection.rebuild";
const PLATFORM_DIRECTORY_READ_PERMISSION: &str = "platform.directory.read";
const PLATFORM_DIRECTORY_MANAGE_PERMISSION: &str = "platform.directory.manage";
const MAX_DIRECTORY_IDENTITY_LINK_PAGE_SIZE: u32 = 500;
const PLATFORM_OUTBOX_REPLAY_PERMISSION: &str = "platform.outbox.replay";
const ENTITLEMENT_ALLOCATE_PERMISSION: &str = "commercial.entitlement.allocate";
const ENTITLEMENT_ACTIVATE_PERMISSION: &str = "commercial.entitlement.activate";
const INCIDENT_CREATE_PERMISSION: &str = "support.incident.create";
const INCIDENT_UPDATE_PERMISSION: &str = "support.incident.update";
const SUPPORT_TICKET_REPLY_PERMISSION: &str = "support.ticket.reply";
const SUPPORT_TICKET_INTERNAL_NOTE_PERMISSION: &str = "support.ticket.note";
const MAX_INCIDENT_LEADS: usize = 64;
const MAX_INCIDENT_SERVICES: usize = 64;
const MAX_INCIDENT_CUSTOMERS: usize = 256;
const MAX_INCIDENT_REFERENCE_BYTES: usize = 128;
const MAX_ROLE_REVIEW_DAYS: u32 = 365;

macro_rules! grpc_handler {
    ($request:ident, $body:block) => {{
        let correlation_id = boundary::request_correlation(&$request);
        let result = (async move $body).await;
        result.map_err(|status| errors::enrich(status, Some(correlation_id)))
    }};
}

fn validate_read_purpose(value: &str) -> Result<&str, Status> {
    let purpose = value.trim();
    if purpose.is_empty() {
        return Err(Status::invalid_argument("read purpose is required"));
    }
    if purpose.len() > 128 || purpose.chars().any(char::is_control) {
        return Err(Status::invalid_argument("read purpose is invalid"));
    }
    Ok(purpose)
}

fn platform_tenant_page_size(value: u32) -> Result<u32, Status> {
    match value {
        0 => Ok(25),
        value if value <= MAX_TENANT_REGISTRY_PAGE_SIZE => Ok(value),
        _ => Err(Status::invalid_argument(
            "platform tenant page size exceeds the allowed bound",
        )),
    }
}

fn finance_banking_read_page_size(value: u32) -> Result<u32, Status> {
    match value {
        0 => Ok(25),
        value if value <= MAX_FINANCE_BANKING_READ_PAGE_SIZE => Ok(value),
        _ => Err(Status::invalid_argument(
            "Finance banking read page size exceeds the allowed bound",
        )),
    }
}

fn finance_subledger_read_page_size(value: u32) -> Result<u32, Status> {
    match value {
        0 => Ok(25),
        value if value <= MAX_FINANCE_SUBLEDGER_READ_PAGE_SIZE => Ok(value),
        _ => Err(Status::invalid_argument(
            "Finance subledger read page size exceeds the allowed bound",
        )),
    }
}

fn projection_status_page_size(value: u32) -> Result<u32, Status> {
    match value {
        0 => Ok(25),
        value if value <= MAX_PROJECTION_STATUS_PAGE_SIZE => Ok(value),
        _ => Err(Status::invalid_argument(
            "projection status page size exceeds the allowed bound",
        )),
    }
}

fn platform_audit_page_size(value: u32) -> Result<u32, Status> {
    match value {
        0 => Ok(25),
        value if value <= MAX_PLATFORM_AUDIT_PAGE_SIZE => Ok(value),
        _ => Err(Status::invalid_argument(
            "platform audit page size exceeds the allowed bound",
        )),
    }
}

fn projection_rebuild_name(value: &str) -> Result<&str, Status> {
    let value = validate_transport_reference(value, "projection_name")?;
    if value != OPERATIONS_READ_PROJECTION {
        return Err(Status::invalid_argument(
            "projection_name is not supported by this rebuild control",
        ));
    }
    Ok(value)
}

fn projection_rebuild_source_version(value: u64) -> Result<u64, Status> {
    if value == 0 {
        return Err(Status::invalid_argument("source_version must be positive"));
    }
    Ok(value)
}

fn projection_rebuild_generation(value: u64) -> Result<u64, Status> {
    if value == 0 {
        Err(Status::invalid_argument("generation must be positive"))
    } else {
        Ok(value)
    }
}

async fn operations_source_high_watermark(
    router: &TenantDatabaseRouter,
    tenant_id: &str,
    holding_ref: &str,
) -> Result<u64, PersistenceError> {
    let write = router
        .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
        .await?;
    let read = match router
        .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
        .await
    {
        Ok(read) => read,
        Err(error) => {
            write.close().await;
            return Err(error);
        }
    };
    let result = match SqlxOperationsStore::from_pools(
        write.database_pool(),
        read.database_pool(),
        holding_ref.to_owned(),
    ) {
        Ok(store) => store.source_event_high_watermark().await,
        Err(error) => Err(error),
    };
    write.close().await;
    read.close().await;
    result
}

fn projection_status_freshness(published_at: DateTime<Utc>, now: DateTime<Utc>) -> &'static str {
    let age = now.signed_duration_since(published_at);
    if age < Duration::zero() {
        "unknown"
    } else if age <= Duration::seconds(MAX_PROJECTION_STATUS_FRESHNESS_SECONDS) {
        "fresh"
    } else {
        "stale"
    }
}

fn validate_transport_reference<'a>(
    value: &'a str,
    field: &'static str,
) -> Result<&'a str, Status> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(Status::invalid_argument(format!("{field} is invalid")));
    }
    Ok(value)
}

fn validate_customer_projection(value: &str) -> Result<&'static str, Status> {
    match value {
        "organization" => Ok("organization"),
        "contacts" => Ok("contacts"),
        "commercial" => Ok("commercial"),
        "entitlements" => Ok("entitlements"),
        "onboarding" => Ok("onboarding"),
        "health" => Ok("health"),
        "support" => Ok("support"),
        "access-audit" => Ok("access-audit"),
        "notes" => Ok("notes"),
        _ => Err(Status::invalid_argument(
            "customer projection is unsupported",
        )),
    }
}

struct EncodedCustomerProjection {
    payload: Vec<u8>,
    metadata: CustomerReadMetadata,
    supported: bool,
    present: bool,
    unsupported_reason: Option<String>,
}

async fn encode_customer_projection<T, F>(view: F) -> Result<EncodedCustomerProjection, Status>
where
    T: Serialize,
    F: Future<Output = Result<CustomerReadView<T>, erpx_application::ApplicationError>>,
{
    match view
        .await
        .map_err(|error| Status::internal(error.to_string()))?
    {
        CustomerReadView::Present { data, metadata } => Ok(EncodedCustomerProjection {
            payload: serde_json::to_vec(&data)
                .map_err(|_| Status::internal("customer projection serialization failed"))?,
            metadata,
            supported: true,
            present: true,
            unsupported_reason: None,
        }),
        CustomerReadView::Absent { metadata } => Ok(EncodedCustomerProjection {
            payload: Vec::new(),
            metadata,
            supported: false,
            present: false,
            unsupported_reason: Some("customer projection is not available".into()),
        }),
        CustomerReadView::Unsupported { metadata, reason } => Ok(EncodedCustomerProjection {
            payload: Vec::new(),
            metadata,
            supported: false,
            present: true,
            unsupported_reason: Some(reason),
        }),
    }
}

const fn customer_freshness_name(
    freshness: erpx_application::CustomerReadFreshness,
) -> &'static str {
    match freshness {
        erpx_application::CustomerReadFreshness::Fresh => "fresh",
        erpx_application::CustomerReadFreshness::Stale => "stale",
        erpx_application::CustomerReadFreshness::Unknown => "unknown",
    }
}

fn customer_detail_section_state<T>(view: &CustomerReadView<T>) -> &'static str {
    match view {
        CustomerReadView::Present { .. } => "present",
        CustomerReadView::Absent { .. } => "absent",
        CustomerReadView::Unsupported { .. } => "unsupported",
    }
}

fn customer_detail_section_states(
    model: &erpx_domain::customer::Customer360DetailReadModel,
) -> Vec<Customer360SectionState> {
    [
        ("account", customer_detail_section_state(&model.account)),
        (
            "organization",
            customer_detail_section_state(&model.organization),
        ),
        ("contacts", customer_detail_section_state(&model.contacts)),
        (
            "commercial",
            customer_detail_section_state(&model.commercial),
        ),
        (
            "entitlements",
            customer_detail_section_state(&model.entitlements),
        ),
        (
            "onboarding",
            customer_detail_section_state(&model.onboarding),
        ),
        ("health", customer_detail_section_state(&model.health)),
        ("support", customer_detail_section_state(&model.support)),
        (
            "access_audit",
            customer_detail_section_state(&model.access_audit),
        ),
        ("notes", customer_detail_section_state(&model.notes)),
    ]
    .into_iter()
    .map(|(section, state)| Customer360SectionState {
        section: section.into(),
        state: state.into(),
    })
    .collect()
}

const CUSTOMER_DETAIL_FACT_MAX_BYTES: usize = 256;

fn customer_detail_fact(
    section: &'static str,
    key: &'static str,
    value: impl Into<String>,
) -> Customer360SectionFact {
    let value = value.into();
    let value = if customer_detail_sensitive_marker(&value) {
        "redacted".to_string()
    } else if value.trim().is_empty()
        || value.len() > CUSTOMER_DETAIL_FACT_MAX_BYTES
        || value.chars().any(char::is_control)
    {
        "unavailable".to_string()
    } else {
        value
    };
    Customer360SectionFact {
        section: section.into(),
        key: key.into(),
        value,
    }
}

fn customer_detail_sensitive_marker(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "access_token",
        "refresh_token",
        "authorization",
        "password",
        "secret",
        "api_key",
        "private_key",
        "credential",
        "database_url",
        "connection_string",
    ]
    .iter()
    .any(|marker| value.contains(marker))
}

fn customer_detail_section_facts(
    model: &erpx_domain::customer::Customer360DetailReadModel,
) -> Vec<Customer360SectionFact> {
    let mut facts = Vec::new();
    if let CustomerReadView::Present { data, .. } = &model.account {
        facts.extend([
            customer_detail_fact("account", "account_code", data.account_code.clone()),
            customer_detail_fact(
                "account",
                "trading_name",
                data.identity.trading_name.clone(),
            ),
            customer_detail_fact("account", "country", data.country.clone()),
            customer_detail_fact(
                "account",
                "account_state",
                customer_status_name(data.account_state),
            ),
            customer_detail_fact(
                "account",
                "subscription_state",
                billing_state_name(data.subscription_state),
            ),
            customer_detail_fact(
                "account",
                "responsibility_contact_count",
                data.responsibility_contacts.len().to_string(),
            ),
            customer_detail_fact(
                "account",
                "customer_success_owner_present",
                data.customer_success_owner.is_some().to_string(),
            ),
        ]);
    }
    if let CustomerReadView::Present { data, .. } = &model.organization {
        let organization = &data.organization;
        facts.extend([
            customer_detail_fact(
                "organization",
                "holding_count",
                organization.holding_count.to_string(),
            ),
            customer_detail_fact(
                "organization",
                "company_count",
                organization.company_count.to_string(),
            ),
            customer_detail_fact(
                "organization",
                "branch_count",
                organization.branch_count.to_string(),
            ),
            customer_detail_fact(
                "organization",
                "department_count",
                organization.departments.len().to_string(),
            ),
            customer_detail_fact(
                "organization",
                "active_user_count",
                organization.active_user_count.to_string(),
            ),
            customer_detail_fact(
                "organization",
                "expiring_role_count",
                organization.expiring_role_count.to_string(),
            ),
            customer_detail_fact(
                "organization",
                "open_ticket_count",
                organization.open_ticket_count.to_string(),
            ),
            customer_detail_fact(
                "organization",
                "module_count",
                organization.module_count.to_string(),
            ),
        ]);
    }
    if let CustomerReadView::Present { data, .. } = &model.contacts {
        facts.extend([
            customer_detail_fact("contacts", "contact_count", data.contacts.len().to_string()),
            customer_detail_fact(
                "contacts",
                "customer_success_owner_present",
                data.customer_success_owner.is_some().to_string(),
            ),
        ]);
    }
    if let CustomerReadView::Present { data, .. } = &model.commercial {
        let commercial = &data.commercial;
        facts.extend([
            customer_detail_fact(
                "commercial",
                "payment_state",
                billing_state_name(commercial.payment_state),
            ),
            customer_detail_fact(
                "commercial",
                "invoice_count",
                commercial.invoices.len().to_string(),
            ),
            customer_detail_fact(
                "commercial",
                "quote_count",
                commercial.quotes.len().to_string(),
            ),
            customer_detail_fact(
                "commercial",
                "discount_count",
                commercial.discounts.len().to_string(),
            ),
            customer_detail_fact(
                "commercial",
                "lifetime_value_minor",
                commercial.lifetime_value_minor.to_string(),
            ),
            customer_detail_fact(
                "commercial",
                "module_allocation_count",
                commercial.module_allocations.len().to_string(),
            ),
        ]);
    }
    if let CustomerReadView::Present { data, .. } = &model.entitlements {
        facts.extend([
            customer_detail_fact(
                "entitlements",
                "active_module_count",
                data.active_module_count.to_string(),
            ),
            customer_detail_fact(
                "entitlements",
                "subscription_allocation_count",
                data.subscription_allocations.len().to_string(),
            ),
            customer_detail_fact(
                "entitlements",
                "organization_allocation_count",
                data.organization_allocations.len().to_string(),
            ),
        ]);
    }
    if let CustomerReadView::Present { data, .. } = &model.onboarding {
        facts.extend([
            customer_detail_fact(
                "onboarding",
                "state",
                customer_onboarding_state_name(data.onboarding.state),
            ),
            customer_detail_fact(
                "onboarding",
                "imports_completed",
                data.onboarding.imports_completed.to_string(),
            ),
            customer_detail_fact(
                "onboarding",
                "training_completed",
                data.onboarding.training_completed.to_string(),
            ),
            customer_detail_fact(
                "onboarding",
                "go_live_at",
                data.onboarding
                    .go_live_at
                    .map(|value| value.to_rfc3339())
                    .unwrap_or_default(),
            ),
            customer_detail_fact(
                "onboarding",
                "owner_present",
                data.onboarding.owner.is_some().to_string(),
            ),
        ]);
    }
    if let CustomerReadView::Present { data, .. } = &model.health {
        let health = &data.health;
        facts.extend([
            customer_detail_fact("health", "state", health_state_name(health.state)),
            customer_detail_fact(
                "health",
                "projection_lag_seconds",
                health.projection_lag_seconds.to_string(),
            ),
            customer_detail_fact(
                "health",
                "worker_failures",
                health.worker_failures.to_string(),
            ),
            customer_detail_fact("health", "storage_status", health.storage_status.clone()),
            customer_detail_fact("health", "backup_status", health.backup_status.clone()),
            customer_detail_fact(
                "health",
                "point_in_time_recovery_status",
                health.point_in_time_recovery_status.clone(),
            ),
            customer_detail_fact(
                "health",
                "integration_failure_count",
                health.integration_failure_count.to_string(),
            ),
            customer_detail_fact(
                "health",
                "api_usage_count",
                health.api_usage_count.to_string(),
            ),
            customer_detail_fact(
                "health",
                "last_activity_at",
                health
                    .last_activity_at
                    .map(|value| value.to_rfc3339())
                    .unwrap_or_default(),
            ),
        ]);
    }
    if let CustomerReadView::Present { data, .. } = &model.support {
        facts.push(customer_detail_fact(
            "support",
            "open_ticket_count",
            data.open_ticket_count.to_string(),
        ));
    }
    if let CustomerReadView::Present { data, .. } = &model.access_audit {
        facts.extend([
            customer_detail_fact("access_audit", "read_count", data.read_count.to_string()),
            customer_detail_fact(
                "access_audit",
                "last_access_at",
                data.last_access_at
                    .map(|value| value.to_rfc3339())
                    .unwrap_or_default(),
            ),
            customer_detail_fact(
                "access_audit",
                "customer_visible",
                data.customer_visible.to_string(),
            ),
        ]);
    }
    if let CustomerReadView::Present { data, .. } = &model.notes {
        facts.extend([
            customer_detail_fact("notes", "note_count", data.notes.len().to_string()),
            customer_detail_fact(
                "notes",
                "has_next_page",
                data.next_before.is_some().to_string(),
            ),
        ]);
    }
    facts
}

const fn customer_onboarding_state_name(
    state: erpx_domain::customer::OnboardingState,
) -> &'static str {
    match state {
        erpx_domain::customer::OnboardingState::NotStarted => "not_started",
        erpx_domain::customer::OnboardingState::InProgress => "in_progress",
        erpx_domain::customer::OnboardingState::ReadyForGoLive => "ready_for_go_live",
        erpx_domain::customer::OnboardingState::Live => "live",
        erpx_domain::customer::OnboardingState::Blocked => "blocked",
    }
}

fn require_actor_match(
    authenticated_principal: &str,
    supplied_principal: &str,
    message: &'static str,
) -> Result<(), Status> {
    if authenticated_principal == supplied_principal {
        Ok(())
    } else {
        Err(Status::permission_denied(message))
    }
}

fn validate_authenticated_device_ref(expected: Uuid, supplied: &str) -> Result<(), Status> {
    if supplied.trim() == expected.to_string() {
        Ok(())
    } else {
        Err(Status::invalid_argument(
            "device_ref must match the authenticated session device",
        ))
    }
}

fn require_platform_scope(scope: &Scope) -> Result<(), Status> {
    if matches!(scope, Scope::Platform) {
        Ok(())
    } else {
        Err(Status::permission_denied(
            "platform customer reads require platform scope",
        ))
    }
}

fn command_mfa_code(context: Option<&erpx_proto::common::v1::CommandContext>) -> &str {
    context.map_or("", |context| context.mfa_code.as_str())
}

fn platform_authorization_proof(
    actor_ref: &str,
    permission: &str,
    scope: Scope,
    at: DateTime<Utc>,
) -> PlatformAuthorizationProof {
    PlatformAuthorizationProof {
        principal_ref: actor_ref.to_owned(),
        permission: permission.to_owned(),
        scope,
        valid_from: at - Duration::minutes(1),
        valid_until: at + Duration::minutes(4),
        mfa_verified: true,
        local_break_glass: false,
        break_glass_ticket_ref: None,
    }
}

fn platform_read_authorization(actor_ref: &str, at: DateTime<Utc>) -> PlatformReadAuthorization {
    PlatformReadAuthorization {
        actor_ref: actor_ref.to_owned(),
        proof: PlatformAuthorizationProof {
            principal_ref: actor_ref.to_owned(),
            permission: PLATFORM_TENANT_READ_PERMISSION.into(),
            scope: Scope::Platform,
            valid_from: at - Duration::minutes(1),
            valid_until: at + Duration::minutes(4),
            mfa_verified: false,
            local_break_glass: false,
            break_glass_ticket_ref: None,
        },
        at,
    }
}

fn parse_directory_provider(value: &str) -> Result<DirectoryProvider, Status> {
    match value.trim() {
        "generic_ldap" => Ok(DirectoryProvider::GenericLdap),
        "active_directory_domain_services" => Ok(DirectoryProvider::ActiveDirectoryDomainServices),
        "univention_corporate_server" => Ok(DirectoryProvider::UniventionCorporateServer),
        _ => Err(Status::invalid_argument(
            "directory provider is unsupported",
        )),
    }
}

fn parse_directory_feed(value: &str) -> Result<DirectoryFeedKind, Status> {
    match value.trim() {
        "polling" => Ok(DirectoryFeedKind::Polling),
        "ad_dirsync" => Ok(DirectoryFeedKind::AdDirSync),
        "rfc4533" => Ok(DirectoryFeedKind::Rfc4533),
        "ucs_provisioning_service" => Ok(DirectoryFeedKind::UcsProvisioningService),
        _ => Err(Status::invalid_argument("directory feed is unsupported")),
    }
}

fn parse_directory_tls(value: &str) -> Result<DirectoryTlsMode, Status> {
    match value.trim() {
        "ldaps" => Ok(DirectoryTlsMode::Ldaps),
        "start_tls" => Ok(DirectoryTlsMode::StartTls),
        "https" => Ok(DirectoryTlsMode::Https),
        _ => Err(Status::invalid_argument(
            "directory TLS mode is unsupported",
        )),
    }
}

fn parse_directory_scope(scope_key: &str, holding_ref: &str) -> Result<DirectoryScope, Status> {
    if scope_key.trim() == "platform" && holding_ref.trim().is_empty() {
        return Ok(DirectoryScope::Platform);
    }
    let holding = if holding_ref.trim().is_empty() {
        scope_key.trim()
    } else {
        holding_ref.trim()
    }
    .strip_prefix("holding:")
    .unwrap_or_else(|| {
        if holding_ref.trim().is_empty() {
            scope_key.trim()
        } else {
            holding_ref.trim()
        }
    });
    if scope_key.trim().starts_with("holding:")
        && !holding_ref.trim().is_empty()
        && scope_key.trim() != format!("holding:{holding}")
    {
        return Err(Status::invalid_argument(
            "directory holding scope and holding_ref do not match",
        ));
    }
    let holding = holding.trim().to_owned();
    if holding.is_empty() || holding.len() > 255 || holding.chars().any(char::is_control) {
        return Err(Status::invalid_argument(
            "directory holding scope is invalid",
        ));
    }
    Ok(DirectoryScope::Holding {
        holding_ref: holding,
    })
}

fn directory_scope_access(scope: &Scope, connector_scope: &DirectoryScope) -> Result<(), Status> {
    match scope {
        Scope::Platform => Ok(()),
        Scope::Holding { holding_id } if connector_scope.holding_id() == Some(*holding_id) => {
            Ok(())
        }
        Scope::Company { .. } | Scope::Branch { .. } | Scope::Subtree { .. } => {
            Err(Status::permission_denied(
                "directory administration requires platform or holding scope",
            ))
        }
        Scope::Holding { .. } => Err(Status::permission_denied(
            "directory connector is outside the requested holding scope",
        )),
    }
}

fn directory_scope_from_key(scope_key: &str) -> Result<DirectoryScope, Status> {
    parse_directory_scope(scope_key, "")
}

fn directory_connector_summary(
    snapshot: erpx_identity::DirectoryConnectorSnapshot,
) -> DirectoryConnectorSummary {
    DirectoryConnectorSummary {
        connector_id: snapshot.id.to_string(),
        provider: match snapshot.provider {
            DirectoryProvider::GenericLdap => "generic_ldap",
            DirectoryProvider::ActiveDirectoryDomainServices => "active_directory_domain_services",
            DirectoryProvider::UniventionCorporateServer => "univention_corporate_server",
        }
        .into(),
        scope_key: snapshot.scope_key,
        endpoint: snapshot.endpoint,
        base_dn: snapshot.base_dn,
        tls_mode: match snapshot.tls_mode {
            DirectoryTlsMode::Ldaps => "ldaps",
            DirectoryTlsMode::StartTls => "start_tls",
            DirectoryTlsMode::Https => "https",
        }
        .into(),
        feed_kind: match snapshot.feed {
            DirectoryFeedKind::Polling => "polling",
            DirectoryFeedKind::AdDirSync => "ad_dirsync",
            DirectoryFeedKind::Rfc4533 => "rfc4533",
            DirectoryFeedKind::UcsProvisioningService => "ucs_provisioning_service",
        }
        .into(),
        enabled: snapshot.enabled,
        max_import_users: snapshot.max_import_users,
        revision: snapshot.revision,
        status: snapshot.status,
        last_success_at: snapshot
            .last_success_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        freshness: snapshot
            .last_success_at
            .map(|value| projection_status_freshness(value, Utc::now()))
            .unwrap_or("unknown")
            .into(),
    }
}

fn directory_import_status_message(
    status: erpx_identity::DirectoryImportStatus,
) -> DirectoryImportStatusMessage {
    DirectoryImportStatusMessage {
        import_id: status.import_id.to_string(),
        connector_id: status.connector_id.to_string(),
        state: status.state,
        max_users: status.max_users,
        users_seen: status.users_seen,
        pages_applied: status.pages_applied,
        page_count: status.page_count,
        cursor_kind: status.cursor_kind,
        started_at: status.started_at.to_rfc3339(),
        finalized_at: status
            .finalized_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        updated_at: status.updated_at.to_rfc3339(),
        failure_category: status.failure_category.unwrap_or_default(),
    }
}

#[derive(Clone)]
struct AppState {
    auth: Arc<AuthContext>,
    store: Arc<SqlxControlPlane>,
    commercial: Arc<SqlxCommercialStore>,
    tenant_router: Option<Arc<TenantDatabaseRouter>>,
    storage: Arc<StorageComposition>,
    support_persistence: Arc<SupportPersistence>,
    customers: Arc<PlatformCustomerService<SqlxControlPlane>>,
    authoritative_support:
        Arc<AuthoritativeSupportService<SqlxControlPlane, SystemClock, SqlxControlPlane>>,
    roles: Arc<RoleLifecycleUseCase<SqlxControlPlane, SystemClock>>,
    authoritative_roles:
        Arc<AuthoritativeRoleLifecycleService<SqlxControlPlane, SystemClock, SqlxControlPlane>>,
    platform_completion: Arc<PlatformCompletionApi>,
    migration: Arc<SqlxG13MigrationStore>,
}

#[derive(Clone)]
enum ReadinessTarget {
    NotConfigured,
    InvalidConfiguration,
    Configured {
        tenant_id: Option<String>,
        router: Arc<TenantDatabaseRouter>,
    },
}

const MAX_READINESS_TENANTS: usize = 1_000;

#[derive(Clone)]
struct ServerReadinessAdapter {
    store: Arc<SqlxControlPlane>,
    registry: Arc<SqlxTenantStoreRegistry>,
    worker_id: Option<Uuid>,
    target: ReadinessTarget,
    policy: WorkerLiveReadinessPolicy,
}

impl ServerReadinessAdapter {
    fn from_env(store: Arc<SqlxControlPlane>, registry: Arc<SqlxTenantStoreRegistry>) -> Self {
        let worker_id = std::env::var("ERPX_READINESS_WORKER_ID")
            .ok()
            .and_then(|value| Uuid::parse_str(&value).ok())
            .filter(|worker_id| !worker_id.is_nil());
        let tenant_id = std::env::var("ERPX_READINESS_TENANT_ID")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let admin_database_url = std::env::var("ERPX_TENANT_ADMIN_DATABASE_URL")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let database_url_template = std::env::var("ERPX_TENANT_DATABASE_URL_TEMPLATE")
            .ok()
            .filter(|value| !value.trim().is_empty());

        let target = match (tenant_id, admin_database_url, database_url_template) {
            (tenant_id, Some(admin_database_url), Some(database_url_template)) => {
                match TenantDatabaseRouter::new(
                    store.clone(),
                    TenantDatabaseProvisioningConfig {
                        admin_database_url,
                        database_url_template,
                    },
                ) {
                    Ok(router) => ReadinessTarget::Configured {
                        tenant_id,
                        router: Arc::new(router),
                    },
                    Err(_) => ReadinessTarget::InvalidConfiguration,
                }
            }
            (None, None, None) => ReadinessTarget::NotConfigured,
            _ => ReadinessTarget::InvalidConfiguration,
        };

        Self {
            store,
            registry,
            worker_id,
            target,
            policy: WorkerLiveReadinessPolicy::default(),
        }
    }

    fn unavailable_composition() -> WorkerReadinessComposition {
        WorkerReadinessComposition {
            worker_heartbeat: ReadinessState::Unavailable,
            tenant_routing: ReadinessState::Unavailable,
            projection_freshness: ReadinessState::Unavailable,
            backup_pitr: ReadinessState::Unavailable,
        }
    }

    async fn observe(&self) -> WorkerReadinessComposition {
        let ReadinessTarget::Configured { tenant_id, router } = &self.target else {
            return Self::unavailable_composition();
        };
        let now = Utc::now();
        let worker_heartbeat = match self.worker_id {
            Some(worker_id) => match self.store.load_worker_heartbeat(worker_id).await {
                Ok(Some(last_seen)) => {
                    WorkerHeartbeatReadiness::from_last_seen(
                        Some(last_seen),
                        now,
                        self.policy.heartbeat_max_age,
                    )
                    .state
                }
                Ok(None) => ReadinessState::NotProbed,
                Err(_) => {
                    tracing::warn!("worker readiness observation failed");
                    ReadinessState::Unavailable
                }
            },
            None => ReadinessState::Unavailable,
        };

        let tenant_ids = match tenant_id {
            Some(tenant_id) => vec![tenant_id.clone()],
            None => match self
                .registry
                .list_active_bounded(MAX_READINESS_TENANTS)
                .await
            {
                Ok(registrations) => registrations
                    .into_iter()
                    .map(|registration| registration.tenant_id)
                    .collect(),
                Err(_) => {
                    // Do not expose database URLs, SQLx details, credentials, or
                    // registry references through the unauthenticated endpoint.
                    tracing::warn!("active tenant readiness enumeration failed");
                    return WorkerReadinessComposition {
                        worker_heartbeat,
                        tenant_routing: ReadinessState::Unavailable,
                        projection_freshness: ReadinessState::Unavailable,
                        backup_pitr: ReadinessState::Unavailable,
                    };
                }
            },
        };

        let mut observations = Vec::with_capacity(tenant_ids.len());
        for tenant_id in tenant_ids {
            match router
                .observe_tenant_readiness(&tenant_id, now, self.policy.tenant)
                .await
            {
                Ok(observation) => observations.push(observation),
                Err(_) => {
                    // Do not expose database URLs, SQLx details, credentials, or
                    // registry references through the unauthenticated endpoint.
                    tracing::warn!("tenant readiness observation failed");
                    return WorkerReadinessComposition {
                        worker_heartbeat,
                        tenant_routing: ReadinessState::Unavailable,
                        projection_freshness: ReadinessState::Unavailable,
                        backup_pitr: ReadinessState::Unavailable,
                    };
                }
            }
        }

        WorkerReadinessComposition {
            worker_heartbeat,
            tenant_routing: aggregate_readiness_states(
                observations
                    .iter()
                    .map(|observation| readiness_state_from_observation(observation.routing.state)),
            ),
            projection_freshness: aggregate_readiness_states(observations.iter().map(
                |observation| {
                    readiness_state_from_observation(observation.projection_freshness.state)
                },
            )),
            backup_pitr: aggregate_readiness_states(observations.iter().map(|observation| {
                readiness_state_from_observation(observation.backup_pitr.state)
            })),
        }
    }
}

fn readiness_state_from_observation(state: ReadinessObservationState) -> ReadinessState {
    match state {
        ReadinessObservationState::Ready => ReadinessState::Ready,
        ReadinessObservationState::NotProbed => ReadinessState::NotProbed,
        ReadinessObservationState::Unavailable => ReadinessState::Unavailable,
    }
}

fn aggregate_readiness_states(states: impl IntoIterator<Item = ReadinessState>) -> ReadinessState {
    let mut observed = false;
    let mut not_probed = false;
    for state in states {
        observed = true;
        match state {
            ReadinessState::Ready => {}
            ReadinessState::NotProbed => not_probed = true,
            ReadinessState::Unavailable => return ReadinessState::Unavailable,
        }
    }
    if !observed || not_probed {
        ReadinessState::NotProbed
    } else {
        ReadinessState::Ready
    }
}

#[derive(Clone)]
struct AuthContext {
    identity: Arc<SqlxIdentityStore>,
    directory: Arc<SqlxDirectoryStore>,
    signer: Arc<AccessTokenSigner>,
    passkeys: Option<Arc<PasskeyCeremonyService<Arc<dyn WebAuthnProvider>>>>,
    totp_keys: Option<Arc<TotpKeyResolver>>,
}

impl AuthContext {
    async fn authorize<T>(&self, request: &Request<T>) -> Result<AccessTokenClaims, Status> {
        let token = request
            .metadata()
            .get("x-erpx-access-token")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Status::unauthenticated("x-erpx-access-token is required"))?;
        let claims = self
            .identity
            .authorize_access_token(&self.signer, token, Utc::now())
            .await
            .map_err(map_identity_auth_error)?;
        let actor = request
            .metadata()
            .get("x-erpx-actor")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Status::unauthenticated("x-erpx-actor is required"))?;
        let session = request
            .metadata()
            .get("x-erpx-session")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Status::unauthenticated("x-erpx-session is required"))?;
        if claims.principal_ref != actor || claims.session_id.to_string() != session {
            return Err(Status::permission_denied(
                "access token attribution does not match transport identity",
            ));
        }
        Ok(claims)
    }

    /// High-risk platform commands require a fresh, one-time TOTP step-up.
    /// The code is verified against the identity write store and is never
    /// included in an audit event, idempotency response, or error message.
    async fn authorize_mfa(
        &self,
        claims: &AccessTokenClaims,
        presented_code: &str,
    ) -> Result<(), Status> {
        let keys = self
            .totp_keys
            .as_ref()
            .ok_or_else(|| Status::permission_denied("fresh MFA step-up is required"))?;
        self.identity
            .verify_totp(
                &claims.principal_ref,
                presented_code,
                keys.as_ref(),
                Utc::now(),
            )
            .await
            .map_err(|_| Status::permission_denied("fresh MFA step-up is required"))
    }

    async fn authorize_role(
        &self,
        store: &SqlxControlPlane,
        principal_ref: &str,
        requested_scope: &Scope,
        required_permission: &str,
    ) -> Result<(), Status> {
        let directory_scope = requested_scope.canonical_key();
        let holding_scope = directory_holding_scope_key(requested_scope);
        match self
            .identity
            .directory_role_access(
                principal_ref,
                required_permission,
                &directory_scope,
                holding_scope.as_deref(),
                Utc::now(),
            )
            .await
            .map_err(|_| Status::internal("directory authorization lookup failed"))?
        {
            DirectoryRoleAccess::Allowed => return Ok(()),
            DirectoryRoleAccess::Denied => {
                return Err(Status::permission_denied(
                    "directory-managed role permission is not active for this scope",
                ));
            }
            DirectoryRoleAccess::Unmanaged => {}
        }
        if !store
            .principal_has_role(
                principal_ref,
                requested_scope,
                required_permission,
                Utc::now(),
            )
            .await
            .map_err(|_| Status::internal("authorization lookup failed"))?
        {
            return Err(Status::permission_denied(
                "effective role permission is not active for this scope",
            ));
        }
        Ok(())
    }

    async fn authorize_independent_role(
        &self,
        store: &SqlxControlPlane,
        principal_ref: &str,
        requested_scope: &Scope,
        required_permission: &str,
        requester_ref: &str,
    ) -> Result<(), Status> {
        if principal_ref.trim().is_empty() || principal_ref == requester_ref {
            return Err(Status::permission_denied(
                "independent approver identity is required",
            ));
        }
        if !self
            .identity
            .principal_is_active(principal_ref)
            .await
            .map_err(map_identity_auth_error)?
        {
            return Err(Status::permission_denied("approver identity is inactive"));
        }
        let directory_scope = requested_scope.canonical_key();
        let holding_scope = directory_holding_scope_key(requested_scope);
        match self
            .identity
            .directory_role_access(
                principal_ref,
                required_permission,
                &directory_scope,
                holding_scope.as_deref(),
                Utc::now(),
            )
            .await
            .map_err(|_| Status::internal("directory approver authorization lookup failed"))?
        {
            DirectoryRoleAccess::Allowed => return Ok(()),
            DirectoryRoleAccess::Denied => {
                return Err(Status::permission_denied(
                    "directory-managed approver role is not active for this scope",
                ));
            }
            DirectoryRoleAccess::Unmanaged => {}
        }
        if !store
            .principal_has_role(
                principal_ref,
                requested_scope,
                required_permission,
                Utc::now(),
            )
            .await
            .map_err(|_| Status::internal("approver role lookup failed"))?
        {
            return Err(Status::permission_denied(
                "approver lacks the required active role",
            ));
        }
        Ok(())
    }

    async fn authorize_customer_support_approver(
        &self,
        store: &SqlxControlPlane,
        principal_ref: &str,
        requested_scope: &Scope,
    ) -> Result<(), Status> {
        let directory_scope = requested_scope.canonical_key();
        let holding_scope = directory_holding_scope_key(requested_scope);
        match self
            .identity
            .directory_role_access(
                principal_ref,
                "support.session.approve",
                &directory_scope,
                holding_scope.as_deref(),
                Utc::now(),
            )
            .await
            .map_err(|_| Status::internal("directory support approver lookup failed"))?
        {
            DirectoryRoleAccess::Allowed => return Ok(()),
            DirectoryRoleAccess::Denied => {
                return Err(Status::permission_denied(
                    "directory-managed support approver role is not active for this scope",
                ));
            }
            DirectoryRoleAccess::Unmanaged => {}
        }
        if !store
            .principal_has_role_key(
                principal_ref,
                requested_scope,
                &["holding_owner", "customer_security_head"],
                Utc::now(),
            )
            .await
            .map_err(|_| Status::internal("customer approval lookup failed"))?
        {
            return Err(Status::permission_denied(
                "customer approval requires an active Holding Owner or Customer Security Head",
            ));
        }
        Ok(())
    }

    async fn authorize_customer_security_admin(
        &self,
        store: &SqlxControlPlane,
        principal_ref: &str,
        customer_id: Uuid,
        requested_scope: &Scope,
    ) -> Result<(), Status> {
        if !store
            .customer_scope_contains(customer_id, requested_scope)
            .await
            .map_err(|_| Status::internal("customer scope lookup failed"))?
        {
            return Err(Status::permission_denied("customer_scope_not_granted"));
        }
        let directory_scope = requested_scope.canonical_key();
        let holding_scope = directory_holding_scope_key(requested_scope);
        match self
            .identity
            .directory_role_access(
                principal_ref,
                PLATFORM_CUSTOMER_ACCESS_AUDIT_PERMISSION,
                &directory_scope,
                holding_scope.as_deref(),
                Utc::now(),
            )
            .await
            .map_err(|_| Status::internal("directory customer security lookup failed"))?
        {
            DirectoryRoleAccess::Allowed => return Ok(()),
            DirectoryRoleAccess::Denied => {
                return Err(Status::permission_denied(
                    "directory-managed customer security role is not active for this scope",
                ));
            }
            DirectoryRoleAccess::Unmanaged => {}
        }
        if !store
            .principal_has_role_key(
                principal_ref,
                requested_scope,
                &["holding_owner", "customer_security_head"],
                Utc::now(),
            )
            .await
            .map_err(|_| Status::internal("customer security role lookup failed"))?
        {
            return Err(Status::permission_denied(
                "customer access-audit reads require an active customer security administrator",
            ));
        }
        Ok(())
    }
}

impl AppState {
    /// The single write-side policy used by customer-scoped commands. Callers
    /// supply only the resource identity and policy permission; billing,
    /// entitlement, role-chain, and SoD state is loaded from control storage.
    async fn authorize_entitled_command(
        &self,
        claims: &AccessTokenClaims,
        context: &erpx_domain::contracts::CommandContext,
        customer_id: Uuid,
        module_key: &str,
        required_permission: &str,
    ) -> Result<(), Status> {
        require_actor_match(
            &claims.principal_ref,
            &context.actor.principal_ref,
            "command actor must match the authenticated principal",
        )?;
        let scope = access_scope(&context.tenant_scope);
        if !self
            .store
            .customer_scope_contains(customer_id, &scope)
            .await
            .map_err(|_| Status::internal("customer scope lookup failed"))?
        {
            return Err(Status::permission_denied("customer_scope_not_granted"));
        }
        self.auth
            .authorize_role(
                &self.store,
                &claims.principal_ref,
                &scope,
                required_permission,
            )
            .await?;
        let subscription_id = context
            .subscription_id
            .ok_or_else(|| Status::invalid_argument("subscription_id is required"))?;
        let at = Utc::now();
        let Some((lifecycle, _)) = self
            .commercial
            .get_billing_lifecycle(customer_id, subscription_id)
            .await
            .map_err(|_| Status::internal("billing state lookup failed"))?
        else {
            return Err(Status::permission_denied("billing_state_not_found"));
        };
        let Some((book, _)) = self
            .commercial
            .get_entitlement_book(customer_id, module_key)
            .await
            .map_err(|_| Status::internal("entitlement lookup failed"))?
        else {
            return Err(Status::permission_denied("module_not_entitled"));
        };
        let Some((assignment, parents)) = self
            .store
            .effective_role_assignment_chain(&claims.principal_ref, &scope, required_permission, at)
            .await
            .map_err(|_| Status::internal("role authorization lookup failed"))?
        else {
            return Err(Status::permission_denied("effective_role_missing"));
        };
        if !self
            .store
            .sod_decision_active(&claims.principal_ref, module_key, &scope, at)
            .await
            .map_err(|_| Status::internal("segregation-of-duties lookup failed"))?
        {
            return Err(Status::permission_denied("segregation_of_duties_failed"));
        }
        let entitlement = book.module_entitlement();
        evaluate_effective_access_chain(
            &assignment,
            &parents,
            &EffectiveAccessRequest {
                principal_ref: claims.principal_ref.clone(),
                at,
                requested_scope: scope.clone(),
                module_key: module_key.into(),
                required_permission: required_permission.into(),
                requested_approval_minor: 0,
                approval_limit_minor: assignment.approval_limit_minor,
                identity_active: true,
                session_active: true,
                billing_state: lifecycle.state,
                entitlement: Some(entitlement),
                entitlement_book: Some(book.clone()),
                is_mutation: true,
                segregation_of_duties_passed: true,
                support_capability_required: false,
                support_capability_active: false,
            },
        )
        .map_err(|error| Status::permission_denied(error.to_string()))?;
        match scope {
            Scope::Company { company_id, .. } => {
                match book.authorize(lifecycle.state, company_id, None, true, at, true) {
                    EntitlementDecision::Allowed => Ok(()),
                    EntitlementDecision::Denied(reason) => Err(Status::permission_denied(reason)),
                }
            }
            Scope::Branch {
                company_id,
                branch_id,
                ..
            } => match book.authorize(lifecycle.state, company_id, Some(branch_id), true, at, true)
            {
                EntitlementDecision::Allowed => Ok(()),
                EntitlementDecision::Denied(reason) => Err(Status::permission_denied(reason)),
            },
            Scope::Platform | Scope::Holding { .. } | Scope::Subtree { .. } => Ok(()),
        }
    }

    /// Authorize a tenant/customer diagnostics read against current
    /// write-side state. The transport supplies an immutable capability
    /// binding, but the capability is not sufficient by itself: the active
    /// identity session/device, support session, agent role chain, scope,
    /// module allocation, billing state, and SoD decision are all rechecked.
    async fn authorize_support_diagnostics(
        &self,
        claims: &AccessTokenClaims,
        requested_scope: &Scope,
        access: &SupportDiagnosticsAccess,
    ) -> Result<Uuid, Status> {
        let parsed = parse_support_diagnostics_access(access, requested_scope)?;
        if !self
            .store
            .customer_scope_contains(parsed.customer_id, requested_scope)
            .await
            .map_err(|_| Status::internal("customer scope lookup failed"))?
        {
            return Err(Status::permission_denied("customer_scope_not_granted"));
        }

        let at = Utc::now();
        let active_device_id = self
            .auth
            .identity
            .active_session_device_id(claims.session_id, at)
            .await
            .map_err(map_identity_auth_error)?;
        validate_authenticated_device_ref(active_device_id, &parsed.device_ref)?;

        let capability_active = self
            .store
            .authorize_support_capability(&SupportCapabilityAuthorizationRequest {
                capability_id: parsed.capability_id,
                customer_account_id: parsed.customer_id,
                agent_ref: claims.principal_ref.clone(),
                device_ref: parsed.device_ref.clone(),
                session_id: parsed.support_session_id,
                ticket_id: parsed.support_ticket_id,
                mode: "diagnostic_read".into(),
                permission: SUPPORT_DIAGNOSTICS_PERMISSION.into(),
                scope: parsed.support_scope.clone(),
                module: parsed.module.clone(),
                secret_hex: parsed.capability_secret_hex.clone(),
                at,
            })
            .await
            .map_err(|_| Status::internal("support capability lookup failed"))?;
        if !capability_active {
            return Err(Status::permission_denied(
                "support diagnostics capability is inactive",
            ));
        }

        let session = self
            .store
            .get_session(parsed.support_session_id)
            .await
            .map_err(|_| Status::internal("support session lookup failed"))?
            .ok_or_else(|| {
                Status::permission_denied("support diagnostics capability is inactive")
            })?;
        if session.customer_account_id != parsed.customer_id
            || session.agent_ref != claims.principal_ref
            || session.ticket_id != parsed.support_ticket_id
            || session.mode != SupportMode::DiagnosticRead
            || session.module != parsed.module
            || session.scope != parsed.support_scope
        {
            return Err(Status::permission_denied(
                "support diagnostics capability binding is invalid",
            ));
        }
        let capability = self
            .store
            .load_support_capability(parsed.capability_id)
            .await
            .map_err(|_| Status::internal("support capability lookup failed"))?
            .ok_or_else(|| {
                Status::permission_denied("support diagnostics capability is inactive")
            })?;

        let Some((lifecycle, _)) = self
            .commercial
            .get_billing_lifecycle(parsed.customer_id, parsed.subscription_id)
            .await
            .map_err(|_| Status::internal("billing state lookup failed"))?
        else {
            return Err(Status::permission_denied("billing_state_not_found"));
        };
        let Some((book, _)) = self
            .commercial
            .get_entitlement_book(parsed.customer_id, &parsed.module)
            .await
            .map_err(|_| Status::internal("entitlement lookup failed"))?
        else {
            return Err(Status::permission_denied("module_not_entitled"));
        };
        if !diagnostics_module_scope_is_entitled(requested_scope, lifecycle.state, &book, at) {
            return Err(Status::permission_denied("module_scope_not_active"));
        }
        let Some((agent_role, agent_parent_role)) = self
            .store
            .effective_role_assignment(
                &claims.principal_ref,
                &Scope::Platform,
                SUPPORT_DIAGNOSTICS_AGENT_PERMISSION,
                at,
            )
            .await
            .map_err(|_| Status::internal("support role lookup failed"))?
        else {
            return Err(Status::permission_denied("support agent role is inactive"));
        };
        let sod_passed = self
            .store
            .sod_decision_active(&claims.principal_ref, &parsed.module, &Scope::Platform, at)
            .await
            .map_err(|_| Status::internal("segregation-of-duties lookup failed"))?;

        let canonical_scope = parsed.approved_scope.canonical_key();
        let mut evaluation_session = session;
        evaluation_session.scope = canonical_scope.clone();
        let mut evaluation_capability = capability;
        evaluation_capability.binding.scope = canonical_scope.clone();
        let entitlement = book.module_entitlement();
        let domain_policy = SupportDomainPolicy::new(
            SUPPORT_DIAGNOSTICS_AGENT_PERMISSION,
            std::iter::once(parsed.module.clone()).collect(),
            true,
            false,
            false,
            sod_passed,
        );
        SupportIntersectionService
            .authorize(&SupportIntersectionRequest {
                at,
                agent_identity_ref: &claims.principal_ref,
                agent_identity_active: true,
                agent_device_ref: &parsed.device_ref,
                agent_device_active: true,
                session: &evaluation_session,
                capability: &evaluation_capability,
                capability_secret: &parsed.capability_secret,
                agent_role: &agent_role,
                agent_parent_role: agent_parent_role.as_ref(),
                represented_user_identity_active: false,
                represented_user_role: None,
                represented_user_parent_role: None,
                requested_scope,
                scope_key: &canonical_scope,
                module_key: &parsed.module,
                required_permission: SUPPORT_DIAGNOSTICS_PERMISSION,
                billing_state: lifecycle.state,
                entitlement: Some(&entitlement),
                domain_policy: &domain_policy,
                is_mutation: false,
            })
            .map_err(|_| Status::permission_denied("support diagnostics authorization denied"))?;
        Ok(parsed.customer_id)
    }

    /// Role lifecycle is a control-plane mutation, but it still uses the
    /// authoritative write-side policy. Customer-scoped renewals require the
    /// customer subscription, module entitlement, role chain, scope binding,
    /// and SoD decision. Platform-scoped renewals use the explicit identity
    /// control-plane policy and therefore have no tenant commercial state.
    async fn authorize_role_lifecycle_command(
        &self,
        claims: &AccessTokenClaims,
        context: &erpx_domain::contracts::CommandContext,
        customer_id: Option<Uuid>,
        module_key: &str,
    ) -> Result<(), Status> {
        require_actor_match(
            &claims.principal_ref,
            &context.actor.principal_ref,
            "command actor must match the authenticated principal",
        )?;
        let scope = access_scope(&context.tenant_scope);
        match (&scope, customer_id) {
            (Scope::Platform, Some(_)) => Err(Status::invalid_argument(
                "customer_id is not valid for platform role renewal",
            )),
            (Scope::Platform, None) => {
                self.auth
                    .authorize_role(
                        &self.store,
                        &claims.principal_ref,
                        &scope,
                        "role.assignment.renew",
                    )
                    .await?;
                let at = Utc::now();
                let Some((assignment, parent)) = self
                    .store
                    .effective_role_assignment(
                        &claims.principal_ref,
                        &scope,
                        "role.assignment.renew",
                        at,
                    )
                    .await
                    .map_err(|_| Status::internal("authorization lookup failed"))?
                else {
                    return Err(Status::permission_denied(
                        "effective role permission is not active for this scope",
                    ));
                };
                if !assignment.is_active_at(at)
                    || !assignment.scope.contains(&scope)
                    || parent
                        .as_ref()
                        .is_some_and(|parent| !parent.effective_at(at, None))
                {
                    return Err(Status::permission_denied(
                        "effective role chain is not active",
                    ));
                }
                if !self
                    .store
                    .sod_decision_active(&claims.principal_ref, "identity", &scope, at)
                    .await
                    .map_err(|_| Status::internal("segregation-of-duties lookup failed"))?
                {
                    return Err(Status::permission_denied("segregation_of_duties_failed"));
                }
                Ok(())
            }
            (_, None) => Err(Status::invalid_argument(
                "customer_id is required for tenant role renewal",
            )),
            (_, Some(customer_id)) => {
                let module_key = module_key.trim();
                if module_key.is_empty() {
                    return Err(Status::invalid_argument(
                        "module_key is required for tenant role renewal",
                    ));
                }
                self.authorize_entitled_command(
                    claims,
                    context,
                    customer_id,
                    module_key,
                    "role.assignment.renew",
                )
                .await
            }
        }
    }
}

fn access_scope(scope: &TenantScope) -> Scope {
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

fn entitlement_holding_id(scope: &Scope) -> Result<Uuid, Status> {
    match scope {
        Scope::Platform => Err(Status::permission_denied(
            "commercial entitlement commands require a tenant scope",
        )),
        Scope::Holding { holding_id }
        | Scope::Company { holding_id, .. }
        | Scope::Branch { holding_id, .. }
        | Scope::Subtree { holding_id, .. } => Ok(*holding_id),
    }
}

fn directory_holding_scope_key(scope: &Scope) -> Option<String> {
    let holding_id = match scope {
        Scope::Platform => return None,
        Scope::Holding { holding_id }
        | Scope::Company { holding_id, .. }
        | Scope::Branch { holding_id, .. }
        | Scope::Subtree { holding_id, .. } => holding_id,
    };
    Some(format!("holding:{holding_id}"))
}

fn finance_banking_read_tenant_scope(scope: &Scope) -> Result<TenantScope, Status> {
    match scope {
        Scope::Holding { holding_id } => Ok(TenantScope::Holding {
            holding_id: *holding_id,
        }),
        Scope::Company {
            holding_id,
            company_id,
        } => Ok(TenantScope::Company {
            holding_id: *holding_id,
            company_id: *company_id,
        }),
        Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(TenantScope::Branch {
            holding_id: *holding_id,
            company_id: *company_id,
            branch_id: *branch_id,
        }),
        Scope::Platform | Scope::Subtree { .. } => Err(Status::permission_denied(
            "Finance banking reads require an explicit holding, company, or branch scope",
        )),
    }
}

fn finance_subledger_read_tenant_scope(scope: &Scope) -> Result<TenantScope, Status> {
    match scope {
        Scope::Holding { holding_id } => Ok(TenantScope::Holding {
            holding_id: *holding_id,
        }),
        Scope::Company {
            holding_id,
            company_id,
        } => Ok(TenantScope::Company {
            holding_id: *holding_id,
            company_id: *company_id,
        }),
        Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(TenantScope::Branch {
            holding_id: *holding_id,
            company_id: *company_id,
            branch_id: *branch_id,
        }),
        Scope::Platform | Scope::Subtree { .. } => Err(Status::permission_denied(
            "Finance subledger reads require an explicit holding, company, or branch scope",
        )),
    }
}

fn finance_company_scope_contains(scope: &Scope, holding_id: Uuid, company_id: Uuid) -> bool {
    match scope {
        Scope::Platform => false,
        Scope::Holding {
            holding_id: scoped_holding,
        } => *scoped_holding == holding_id,
        Scope::Company {
            holding_id: scoped_holding,
            company_id: scoped_company,
        } => *scoped_holding == holding_id && *scoped_company == company_id,
        Scope::Branch {
            holding_id: scoped_holding,
            company_id: scoped_company,
            ..
        } => *scoped_holding == holding_id && *scoped_company == company_id,
        Scope::Subtree {
            holding_id: scoped_holding,
            company_ids,
            ..
        } => *scoped_holding == holding_id && company_ids.contains(&company_id),
    }
}

fn finance_trial_balance_account_kind(value: &str) -> Result<AccountKind, Status> {
    match value {
        "asset" => Ok(AccountKind::Asset),
        "liability" => Ok(AccountKind::Liability),
        "equity" => Ok(AccountKind::Equity),
        "revenue" => Ok(AccountKind::Revenue),
        "expense" => Ok(AccountKind::Expense),
        _ => Err(Status::failed_precondition(
            "Finance chart projection contains an invalid account category",
        )),
    }
}

fn finance_trial_balance_account_kind_text(value: AccountKind) -> &'static str {
    match value {
        AccountKind::Asset => "asset",
        AccountKind::Liability => "liability",
        AccountKind::Equity => "equity",
        AccountKind::Revenue => "revenue",
        AccountKind::Expense => "expense",
    }
}

fn finance_trial_balance_freshness(
    projected_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> &'static str {
    let Some(projected_at) = projected_at else {
        return "unknown";
    };
    let age = now.signed_duration_since(projected_at);
    if age < Duration::zero() {
        "unknown"
    } else if age <= Duration::minutes(5) {
        "fresh"
    } else {
        "stale"
    }
}

fn finance_chart_read_scope(
    caller_scope: &Scope,
    holding_id: &str,
    company_id: &str,
) -> Result<FinanceChartReadScope, Status> {
    if matches!(caller_scope, Scope::Platform | Scope::Subtree { .. }) {
        return Err(Status::permission_denied(
            "Finance chart reads require an explicit holding, company, or branch scope",
        ));
    }
    let holding_id = parse_uuid(holding_id, "holding_id")?;
    let company_id = parse_uuid(company_id, "company_id")?;
    let requested_scope = Scope::Company {
        holding_id,
        company_id,
    };
    if !caller_scope.contains(&requested_scope) {
        return Err(Status::permission_denied(
            "Finance chart read scope is not contained",
        ));
    }
    Ok(FinanceChartReadScope {
        holding_id,
        company_id,
    })
}

fn finance_close_read_scope(
    scope: &Scope,
    holding_id: Uuid,
    company_id: Uuid,
) -> Result<FinanceCloseReadScope, Status> {
    match scope {
        Scope::Holding {
            holding_id: scoped_holding,
        } if *scoped_holding == holding_id => FinanceCloseReadScope::new(holding_id, company_id)
            .map_err(|_| Status::invalid_argument("Finance close scope is invalid")),
        Scope::Company {
            holding_id: scoped_holding,
            company_id: scoped_company,
        } if *scoped_holding == holding_id && *scoped_company == company_id => {
            FinanceCloseReadScope::new(holding_id, company_id)
                .map_err(|_| Status::invalid_argument("Finance close scope is invalid"))
        }
        Scope::Branch {
            holding_id: scoped_holding,
            company_id: scoped_company,
            ..
        } if *scoped_holding == holding_id && *scoped_company == company_id => {
            FinanceCloseReadScope::new(holding_id, company_id)
                .map_err(|_| Status::invalid_argument("Finance close scope is invalid"))
        }
        Scope::Platform | Scope::Subtree { .. } => Err(Status::permission_denied(
            "Finance close reads require an explicit holding, company, or branch scope",
        )),
        _ => Err(Status::permission_denied(
            "Finance close read scope does not match the requested company",
        )),
    }
}

fn query_access_scope(context: &erpx_proto::common::v1::QueryContext) -> Result<Scope, Status> {
    let scope = context
        .tenant_scope
        .as_ref()
        .and_then(|scope| scope.scope.as_ref())
        .ok_or_else(|| Status::permission_denied("query tenant scope is required"))?;
    match scope {
        erpx_proto::common::v1::tenant_scope::Scope::Platform(_) => Ok(Scope::Platform),
        erpx_proto::common::v1::tenant_scope::Scope::Holding(value) => Ok(Scope::Holding {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
        }),
        erpx_proto::common::v1::tenant_scope::Scope::Company(value) => Ok(Scope::Company {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
        }),
        erpx_proto::common::v1::tenant_scope::Scope::Branch(value) => Ok(Scope::Branch {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
            branch_id: parse_uuid(&value.branch_id, "branch_id")?,
        }),
        erpx_proto::common::v1::tenant_scope::Scope::Subtree(value) => Ok(Scope::Subtree {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_ids: value
                .company_ids
                .iter()
                .map(|id| parse_uuid(id, "company_id"))
                .collect::<Result<Vec<_>, _>>()?,
            branch_ids: value
                .branch_ids
                .iter()
                .map(|id| parse_uuid(id, "branch_id"))
                .collect::<Result<Vec<_>, _>>()?,
        }),
    }
}

fn explicit_product_scope(scope: Scope, product: &'static str) -> Result<Scope, Status> {
    if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
        return Err(Status::permission_denied(format!(
            "{product} access requires an explicit holding, company, or branch scope"
        )));
    }
    Ok(scope)
}

fn customer_security_admin_scope(
    customer_id: Uuid,
    scope: &Scope,
) -> Result<CustomerSecurityAdminScope, Status> {
    let result = match scope {
        Scope::Holding { holding_id } => CustomerSecurityAdminScope {
            customer_id,
            platform: false,
            holding_id: Some(*holding_id),
            company_id: None,
            branch_id: None,
        },
        Scope::Company {
            holding_id,
            company_id,
        } => CustomerSecurityAdminScope {
            customer_id,
            platform: false,
            holding_id: Some(*holding_id),
            company_id: Some(*company_id),
            branch_id: None,
        },
        Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => CustomerSecurityAdminScope {
            customer_id,
            platform: false,
            holding_id: Some(*holding_id),
            company_id: Some(*company_id),
            branch_id: Some(*branch_id),
        },
        Scope::Platform => CustomerSecurityAdminScope {
            customer_id,
            platform: true,
            holding_id: None,
            company_id: None,
            branch_id: None,
        },
        Scope::Subtree { .. } => {
            return Err(Status::permission_denied(
                "customer access-audit reads require an explicit holding, company, or branch scope",
            ));
        }
    };
    Ok(result)
}

fn global_read_scope_for_customer_scope(scope: &Scope) -> Result<GlobalReadScope, Status> {
    let (holding_id, company_id, branch_id) = match scope {
        Scope::Holding { holding_id } => (Some(*holding_id), None, None),
        Scope::Company {
            holding_id,
            company_id,
        } => (Some(*holding_id), Some(*company_id), None),
        Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => (Some(*holding_id), Some(*company_id), Some(*branch_id)),
        Scope::Platform => {
            return Ok(GlobalReadScope {
                label: "platform".into(),
                holding_id: None,
                company_id: None,
                branch_id: None,
            });
        }
        Scope::Subtree { .. } => {
            return Err(Status::permission_denied(
                "customer access-audit reads require an explicit holding, company, or branch scope",
            ));
        }
    };
    Ok(GlobalReadScope {
        label: support_scope_reference(scope),
        holding_id,
        company_id,
        branch_id,
    })
}

fn support_scope_reference(scope: &Scope) -> String {
    match scope {
        Scope::Platform => "platform".into(),
        Scope::Holding { holding_id } => format!("holding/{holding_id}"),
        Scope::Company {
            holding_id,
            company_id,
        } => format!("company/{holding_id}/{company_id}"),
        Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("branch/{holding_id}/{company_id}/{branch_id}"),
        Scope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => {
            let mut company_ids = company_ids.clone();
            let mut branch_ids = branch_ids.clone();
            company_ids.sort_unstable();
            branch_ids.sort_unstable();
            let companies = company_ids
                .iter()
                .map(Uuid::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let branches = branch_ids
                .iter()
                .map(Uuid::to_string)
                .collect::<Vec<_>>()
                .join(",");
            format!("subtree/{holding_id}/companies/{companies}/branches/{branches}")
        }
    }
}

fn parse_support_scope(value: &str) -> Result<Scope, Status> {
    let parts = value.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        ["platform"] => Ok(Scope::Platform),
        ["holding", holding_id] => Ok(Scope::Holding {
            holding_id: parse_uuid(holding_id, "support holding_id")?,
        }),
        ["company", holding_id, company_id] => Ok(Scope::Company {
            holding_id: parse_uuid(holding_id, "support holding_id")?,
            company_id: parse_uuid(company_id, "support company_id")?,
        }),
        ["branch", holding_id, company_id, branch_id] => Ok(Scope::Branch {
            holding_id: parse_uuid(holding_id, "support holding_id")?,
            company_id: parse_uuid(company_id, "support company_id")?,
            branch_id: parse_uuid(branch_id, "support branch_id")?,
        }),
        [
            "subtree",
            holding_id,
            "companies",
            companies,
            "branches",
            branches,
        ] => {
            let parse_ids = |value: &str, field: &str| {
                if value.is_empty() {
                    return Ok(Vec::new());
                }
                value
                    .split(',')
                    .map(|id| parse_uuid(id, field))
                    .collect::<Result<Vec<_>, _>>()
            };
            Ok(Scope::Subtree {
                holding_id: parse_uuid(holding_id, "support holding_id")?,
                company_ids: parse_ids(companies, "support company_id")?,
                branch_ids: parse_ids(branches, "support branch_id")?,
            })
        }
        _ => Err(Status::invalid_argument(
            "support scope must use a canonical platform/holding/company/branch reference",
        )),
    }
}

fn map_identity_auth_error(error: IdentityStoreError) -> Status {
    match error {
        IdentityStoreError::Database(_) => Status::internal("identity authorization failed"),
        IdentityStoreError::InvalidCredentials
        | IdentityStoreError::PrincipalInactive
        | IdentityStoreError::DeviceRevoked
        | IdentityStoreError::PasskeyAlreadyRegistered
        | IdentityStoreError::PasskeyNotFound
        | IdentityStoreError::PasskeyRevoked
        | IdentityStoreError::SessionInactive
        | IdentityStoreError::Primitive(_) => {
            Status::unauthenticated("access token is invalid or revoked")
        }
        IdentityStoreError::InvalidRecord(_) => Status::internal("identity record is invalid"),
    }
}

fn map_dead_letter_replay_error(error: PersistenceError) -> Status {
    match error {
        PersistenceError::NotFound { .. } => {
            Status::not_found("dead-lettered outbox event was not found")
        }
        PersistenceError::IdempotencyConflict { .. } => {
            Status::aborted("dead-letter replay idempotency key conflicts with another request")
        }
        PersistenceError::InvalidData(message)
            if message.contains("not dead-lettered")
                || message.contains("attempt budget")
                || message.contains("cannot be replayed")
                || message.contains("already in progress") =>
        {
            Status::failed_precondition("dead-letter replay is not currently available")
        }
        PersistenceError::InvalidData(_) => {
            Status::invalid_argument("dead-letter replay request is invalid")
        }
        PersistenceError::Database(_)
        | PersistenceError::Migration(_)
        | PersistenceError::Serialization(_)
        | PersistenceError::OptimisticConcurrency { .. }
        | PersistenceError::ClaimLost { .. }
        | PersistenceError::Configuration(_) => {
            Status::unavailable("dead-letter replay persistence is unavailable")
        }
    }
}

type ApprovalMac = Hmac<Sha256>;

fn verify_customer_approval(
    token: &str,
    session_id: Uuid,
    customer_id: Uuid,
) -> Result<(), Status> {
    let secret = std::env::var("ERPX_SUPPORT_APPROVAL_SECRET").map_err(|_| {
        Status::failed_precondition("ERPX_SUPPORT_APPROVAL_SECRET is not configured")
    })?;
    if secret.len() < 32 {
        return Err(Status::failed_precondition(
            "ERPX_SUPPORT_APPROVAL_SECRET must contain at least 32 characters",
        ));
    }
    let mut mac = ApprovalMac::new_from_slice(secret.as_bytes())
        .map_err(|_| Status::internal("support approval signer is unavailable"))?;
    mac.update(format!("{session_id}:{customer_id}").as_bytes());
    let provided = hex::decode(token)
        .map_err(|_| Status::permission_denied("customer approval signature is invalid"))?;
    mac.verify_slice(&provided)
        .map_err(|_| Status::permission_denied("customer approval signature is invalid"))
}

fn parse_uuid(value: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(value).map_err(|_| Status::invalid_argument(format!("{field} must be a UUID")))
}

fn organization_page_size(value: u32) -> Result<u32, Status> {
    match value {
        0 => Ok(25),
        value if value <= MAX_ORGANIZATION_PAGE_SIZE => Ok(value),
        _ => Err(Status::invalid_argument(
            "organization page size exceeds the allowed bound",
        )),
    }
}

fn optional_organization_uuid(value: &str, field: &str) -> Result<Option<Uuid>, Status> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(parse_uuid(value, field)?))
    }
}

fn organization_status(value: &str) -> Result<OrganizationStatus, Status> {
    match value {
        "active" => Ok(OrganizationStatus::Active),
        "suspended" => Ok(OrganizationStatus::Suspended),
        "archived" => Ok(OrganizationStatus::Archived),
        _ => Err(Status::invalid_argument(
            "organization status must be active, suspended, or archived",
        )),
    }
}

fn organization_cursor(value: &OperationsJsonReadCursor) -> String {
    hex::encode(format!("{}|{}", value.updated_at.to_rfc3339(), value.aggregate_id).as_bytes())
}

fn parse_organization_cursor(value: &str) -> Result<Option<OperationsJsonReadCursor>, Status> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    if value.len() > 512 {
        return Err(Status::invalid_argument("organization cursor is invalid"));
    }
    let decoded = hex::decode(value)
        .map_err(|_| Status::invalid_argument("organization cursor is invalid"))?;
    let decoded = String::from_utf8(decoded)
        .map_err(|_| Status::invalid_argument("organization cursor is invalid"))?;
    let (updated_at, aggregate_id) = decoded
        .split_once('|')
        .ok_or_else(|| Status::invalid_argument("organization cursor is invalid"))?;
    let updated_at = DateTime::parse_from_rfc3339(updated_at)
        .map_err(|_| Status::invalid_argument("organization cursor is invalid"))?
        .with_timezone(&Utc);
    let aggregate_id = parse_uuid(aggregate_id, "organization cursor aggregate_id")?;
    Ok(Some(OperationsJsonReadCursor {
        updated_at,
        aggregate_id,
    }))
}

fn operations_exception_page_size(value: u32) -> Result<u32, Status> {
    match value {
        0 => Ok(25),
        value if value <= MAX_OPERATIONS_EXCEPTION_PAGE_SIZE => Ok(value),
        _ => Err(Status::invalid_argument(
            "operations exception page size exceeds the allowed bound",
        )),
    }
}

fn operations_exception_cursor(cursor: &OperationsExceptionCursor) -> String {
    hex::encode(format!("{}|{}", cursor.updated_at.to_rfc3339(), cursor.aggregate_id).as_bytes())
}

fn parse_operations_exception_cursor(
    value: &str,
) -> Result<Option<OperationsExceptionCursor>, Status> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    if value.len() > 512 {
        return Err(Status::invalid_argument(
            "operations exception cursor is invalid",
        ));
    }
    let decoded = hex::decode(value)
        .map_err(|_| Status::invalid_argument("operations exception cursor is invalid"))?;
    let decoded = String::from_utf8(decoded)
        .map_err(|_| Status::invalid_argument("operations exception cursor is invalid"))?;
    let (updated_at, aggregate_id) = decoded
        .split_once('|')
        .ok_or_else(|| Status::invalid_argument("operations exception cursor is invalid"))?;
    let updated_at = DateTime::parse_from_rfc3339(updated_at)
        .map_err(|_| Status::invalid_argument("operations exception cursor is invalid"))?
        .with_timezone(&Utc);
    let aggregate_id = parse_uuid(aggregate_id, "operations exception cursor aggregate_id")?;
    Ok(Some(OperationsExceptionCursor {
        updated_at,
        aggregate_id,
    }))
}

const fn operations_exception_status_name(status: ExceptionStatus) -> &'static str {
    match status {
        ExceptionStatus::Detected => "detected",
        ExceptionStatus::Assigned => "assigned",
        ExceptionStatus::Acknowledged => "acknowledged",
        ExceptionStatus::Escalated => "escalated",
        ExceptionStatus::Resolved => "resolved",
        ExceptionStatus::Closed => "closed",
    }
}

fn encode_hr_leave_cursor(cursor: &HrLeaveReadCursor) -> String {
    hex::encode(
        format!(
            "{}|{}",
            cursor.starts_on.format("%Y-%m-%d"),
            cursor.leave_id
        )
        .as_bytes(),
    )
}

fn decode_hr_leave_cursor(value: &str) -> Result<HrLeaveReadCursor, Status> {
    if value.is_empty() || value.len() > 256 {
        return Err(Status::invalid_argument("HR leave cursor is invalid"));
    }
    let decoded =
        hex::decode(value).map_err(|_| Status::invalid_argument("HR leave cursor is invalid"))?;
    let decoded = String::from_utf8(decoded)
        .map_err(|_| Status::invalid_argument("HR leave cursor is invalid"))?;
    let (date, leave_id) = decoded
        .split_once('|')
        .ok_or_else(|| Status::invalid_argument("HR leave cursor is invalid"))?;
    let starts_on = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| Status::invalid_argument("HR leave cursor is invalid"))?;
    let leave_id = parse_uuid(leave_id, "HR leave cursor")?;
    if leave_id.is_nil() {
        return Err(Status::invalid_argument("HR leave cursor is invalid"));
    }
    Ok(HrLeaveReadCursor {
        starts_on,
        leave_id,
    })
}

fn encode_hr_expense_cursor(cursor: &HrExpenseReadCursor) -> String {
    hex::encode(cursor.expense_id.to_string().as_bytes())
}

fn decode_hr_expense_cursor(value: &str) -> Result<HrExpenseReadCursor, Status> {
    if value.is_empty() || value.len() > 256 {
        return Err(Status::invalid_argument("HR expense cursor is invalid"));
    }
    let decoded =
        hex::decode(value).map_err(|_| Status::invalid_argument("HR expense cursor is invalid"))?;
    let decoded = String::from_utf8(decoded)
        .map_err(|_| Status::invalid_argument("HR expense cursor is invalid"))?;
    let expense_id = parse_uuid(&decoded, "HR expense cursor")?;
    if expense_id.is_nil() {
        return Err(Status::invalid_argument("HR expense cursor is invalid"));
    }
    Ok(HrExpenseReadCursor { expense_id })
}

fn encode_hr_payroll_cursor(cursor: &HrPayrollRunReadCursor) -> String {
    hex::encode(
        format!(
            "{}|{}",
            cursor.starts_on.format("%Y-%m-%d"),
            cursor.payroll_id
        )
        .as_bytes(),
    )
}

fn decode_hr_payroll_cursor(value: &str) -> Result<HrPayrollRunReadCursor, Status> {
    if value.is_empty() || value.len() > 256 {
        return Err(Status::invalid_argument("HR payroll cursor is invalid"));
    }
    let decoded =
        hex::decode(value).map_err(|_| Status::invalid_argument("HR payroll cursor is invalid"))?;
    let decoded = String::from_utf8(decoded)
        .map_err(|_| Status::invalid_argument("HR payroll cursor is invalid"))?;
    let (date, payroll_id) = decoded
        .split_once('|')
        .ok_or_else(|| Status::invalid_argument("HR payroll cursor is invalid"))?;
    let starts_on = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| Status::invalid_argument("HR payroll cursor is invalid"))?;
    let payroll_id = parse_uuid(payroll_id, "HR payroll cursor")?;
    if payroll_id.is_nil() {
        return Err(Status::invalid_argument("HR payroll cursor is invalid"));
    }
    Ok(HrPayrollRunReadCursor {
        starts_on,
        payroll_id,
    })
}

fn encode_hr_reassignment_cursor(cursor: &HrReassignmentReadCursor) -> String {
    hex::encode(
        format!(
            "{}|{}",
            cursor.reassigned_at.to_rfc3339(),
            cursor.workflow_ref
        )
        .as_bytes(),
    )
}

fn decode_hr_reassignment_cursor(value: &str) -> Result<HrReassignmentReadCursor, Status> {
    if value.is_empty() || value.len() > 256 {
        return Err(Status::invalid_argument(
            "HR reassignment cursor is invalid",
        ));
    }
    let decoded = hex::decode(value)
        .map_err(|_| Status::invalid_argument("HR reassignment cursor is invalid"))?;
    let decoded = String::from_utf8(decoded)
        .map_err(|_| Status::invalid_argument("HR reassignment cursor is invalid"))?;
    let (reassigned_at, workflow_ref) = decoded
        .split_once('|')
        .ok_or_else(|| Status::invalid_argument("HR reassignment cursor is invalid"))?;
    let reassigned_at = DateTime::parse_from_rfc3339(reassigned_at)
        .map_err(|_| Status::invalid_argument("HR reassignment cursor is invalid"))?
        .with_timezone(&Utc);
    if workflow_ref.is_empty()
        || workflow_ref.len() > 255
        || workflow_ref.chars().any(char::is_control)
    {
        return Err(Status::invalid_argument(
            "HR reassignment cursor is invalid",
        ));
    }
    Ok(HrReassignmentReadCursor {
        reassigned_at,
        workflow_ref: workflow_ref.to_owned(),
    })
}

const MAX_LEGAL_JSON_BYTES: usize = 2 * 1024 * 1024;

fn legal_json<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    field: &'static str,
) -> Result<T, Status> {
    if bytes.is_empty() || bytes.len() > MAX_LEGAL_JSON_BYTES {
        return Err(Status::invalid_argument(format!(
            "{field} is out of bounds"
        )));
    }
    serde_json::from_slice(bytes)
        .map_err(|_| Status::invalid_argument(format!("{field} is invalid")))
}

fn legal_expected_version(
    has_expected_version: bool,
    expected_version: u64,
    context: &erpx_domain::contracts::CommandContext,
) -> Result<Option<u64>, Status> {
    let expected = if has_expected_version {
        if expected_version == 0 {
            return Err(Status::invalid_argument(
                "expected_version must be positive",
            ));
        }
        Some(expected_version)
    } else {
        None
    };
    if expected != context.expected_aggregate_version {
        return Err(Status::invalid_argument(
            "expected_version does not match command context",
        ));
    }
    Ok(expected)
}

fn encode_legal_contract_cursor(cursor: &LegalContractReadCursor) -> String {
    hex::encode(cursor.contract_id.as_bytes())
}

fn decode_legal_contract_cursor(value: &str) -> Result<LegalContractReadCursor, Status> {
    if value.is_empty() || value.len() > 256 {
        return Err(Status::invalid_argument("Legal contract cursor is invalid"));
    }
    let bytes = hex::decode(value)
        .map_err(|_| Status::invalid_argument("Legal contract cursor is invalid"))?;
    let bytes: [u8; 16] = bytes
        .try_into()
        .map_err(|_| Status::invalid_argument("Legal contract cursor is invalid"))?;
    let contract_id = Uuid::from_bytes(bytes);
    if contract_id.is_nil() {
        return Err(Status::invalid_argument("Legal contract cursor is invalid"));
    }
    Ok(LegalContractReadCursor { contract_id })
}

fn encode_legal_document_cursor(cursor: &LegalDocumentReadCursor) -> String {
    hex::encode(cursor.document_id.as_bytes())
}

fn decode_legal_document_cursor(value: &str) -> Result<LegalDocumentReadCursor, Status> {
    if value.is_empty() || value.len() > 256 {
        return Err(Status::invalid_argument("Legal document cursor is invalid"));
    }
    let bytes = hex::decode(value)
        .map_err(|_| Status::invalid_argument("Legal document cursor is invalid"))?;
    let bytes: [u8; 16] = bytes
        .try_into()
        .map_err(|_| Status::invalid_argument("Legal document cursor is invalid"))?;
    let document_id = Uuid::from_bytes(bytes);
    if document_id.is_nil() {
        return Err(Status::invalid_argument("Legal document cursor is invalid"));
    }
    Ok(LegalDocumentReadCursor { document_id })
}

fn encode_legal_access_cursor(cursor: &LegalAccessReadCursor) -> String {
    hex::encode(format!("{}|{}", cursor.observed_at.to_rfc3339(), cursor.entry_id).as_bytes())
}

fn decode_legal_access_cursor(value: &str) -> Result<LegalAccessReadCursor, Status> {
    if value.is_empty() || value.len() > 256 {
        return Err(Status::invalid_argument("Legal access cursor is invalid"));
    }
    let bytes = hex::decode(value)
        .map_err(|_| Status::invalid_argument("Legal access cursor is invalid"))?;
    let value = String::from_utf8(bytes)
        .map_err(|_| Status::invalid_argument("Legal access cursor is invalid"))?;
    let (observed_at, entry_id) = value
        .split_once('|')
        .ok_or_else(|| Status::invalid_argument("Legal access cursor is invalid"))?;
    let observed_at = DateTime::parse_from_rfc3339(observed_at)
        .map_err(|_| Status::invalid_argument("Legal access cursor is invalid"))?
        .with_timezone(&Utc);
    let entry_id = parse_uuid(entry_id, "Legal access cursor")?;
    if entry_id.is_nil() {
        return Err(Status::invalid_argument("Legal access cursor is invalid"));
    }
    Ok(LegalAccessReadCursor {
        observed_at,
        entry_id,
    })
}

fn encode_pos_sales_cursor(cursor: &PosSaleReadCursor) -> String {
    hex::encode(format!("{}|{}", cursor.updated_at.to_rfc3339(), cursor.sale_id).as_bytes())
}

fn decode_pos_sales_cursor(value: &str) -> Result<PosSaleReadCursor, Status> {
    if value.is_empty() || value.len() > 256 {
        return Err(Status::invalid_argument("POS sales cursor is invalid"));
    }
    let decoded =
        hex::decode(value).map_err(|_| Status::invalid_argument("POS sales cursor is invalid"))?;
    let decoded = String::from_utf8(decoded)
        .map_err(|_| Status::invalid_argument("POS sales cursor is invalid"))?;
    let (updated_at, sale_id) = decoded
        .split_once('|')
        .ok_or_else(|| Status::invalid_argument("POS sales cursor is invalid"))?;
    let updated_at = DateTime::parse_from_rfc3339(updated_at)
        .map_err(|_| Status::invalid_argument("POS sales cursor is invalid"))?
        .with_timezone(&Utc);
    let sale_id = parse_uuid(sale_id, "POS sales cursor")?;
    if sale_id.is_nil() {
        return Err(Status::invalid_argument("POS sales cursor is invalid"));
    }
    Ok(PosSaleReadCursor {
        updated_at,
        sale_id,
    })
}

fn encode_quality_inspection_cursor(cursor: &QualityInspectionReadCursor) -> String {
    hex::encode(
        format!(
            "{}|{}",
            cursor.projected_at.to_rfc3339(),
            cursor.inspection_id
        )
        .as_bytes(),
    )
}

fn decode_quality_inspection_cursor(value: &str) -> Result<QualityInspectionReadCursor, Status> {
    if value.is_empty() || value.len() > 256 {
        return Err(Status::invalid_argument(
            "Quality inspections cursor is invalid",
        ));
    }
    let decoded = hex::decode(value)
        .map_err(|_| Status::invalid_argument("Quality inspections cursor is invalid"))?;
    let decoded = String::from_utf8(decoded)
        .map_err(|_| Status::invalid_argument("Quality inspections cursor is invalid"))?;
    let (projected_at, inspection_id) = decoded
        .split_once('|')
        .ok_or_else(|| Status::invalid_argument("Quality inspections cursor is invalid"))?;
    let projected_at = DateTime::parse_from_rfc3339(projected_at)
        .map_err(|_| Status::invalid_argument("Quality inspections cursor is invalid"))?
        .with_timezone(&Utc);
    let inspection_id = parse_uuid(inspection_id, "Quality inspections cursor")?;
    if inspection_id.is_nil() {
        return Err(Status::invalid_argument(
            "Quality inspections cursor is invalid",
        ));
    }
    Ok(QualityInspectionReadCursor {
        projected_at,
        inspection_id,
    })
}

fn encode_scm_read_cursor(cursor: &ScmReadCursor) -> String {
    hex::encode(
        format!(
            "{}|{}|{}",
            cursor.projected_at.to_rfc3339(),
            cursor.aggregate_type,
            cursor.aggregate_id
        )
        .as_bytes(),
    )
}

fn decode_scm_read_cursor(value: &str) -> Result<ScmReadCursor, Status> {
    if value.is_empty() || value.len() > 384 {
        return Err(Status::invalid_argument("SCM read cursor is invalid"));
    }
    let decoded =
        hex::decode(value).map_err(|_| Status::invalid_argument("SCM read cursor is invalid"))?;
    let decoded = String::from_utf8(decoded)
        .map_err(|_| Status::invalid_argument("SCM read cursor is invalid"))?;
    let mut parts = decoded.split('|');
    let projected_at = parts
        .next()
        .ok_or_else(|| Status::invalid_argument("SCM read cursor is invalid"))?;
    let aggregate_type = parts
        .next()
        .ok_or_else(|| Status::invalid_argument("SCM read cursor is invalid"))?;
    let aggregate_id = parts
        .next()
        .ok_or_else(|| Status::invalid_argument("SCM read cursor is invalid"))?;
    if parts.next().is_some() {
        return Err(Status::invalid_argument("SCM read cursor is invalid"));
    }
    let projected_at = DateTime::parse_from_rfc3339(projected_at)
        .map_err(|_| Status::invalid_argument("SCM read cursor is invalid"))?
        .with_timezone(&Utc);
    let aggregate_type = match aggregate_type {
        "product" => "product",
        "warehouse" => "warehouse",
        "purchase_request" => "purchase_request",
        "purchase_order" => "purchase_order",
        "goods_receipt" => "goods_receipt",
        "stock_movement" => "stock_movement",
        "stock_transfer" => "stock_transfer",
        "stock_reservation" => "stock_reservation",
        _ => return Err(Status::invalid_argument("SCM read cursor is invalid")),
    };
    let aggregate_id = parse_uuid(aggregate_id, "SCM read cursor aggregate ID")?;
    if aggregate_id.is_nil() {
        return Err(Status::invalid_argument("SCM read cursor is invalid"));
    }
    Ok(ScmReadCursor {
        projected_at,
        aggregate_type,
        aggregate_id,
    })
}

fn encode_sales_read_cursor(cursor: &SalesReadCursor) -> String {
    hex::encode(
        format!(
            "{}|{}|{}",
            cursor.projected_at.to_rfc3339(),
            cursor.aggregate_type,
            cursor.aggregate_id
        )
        .as_bytes(),
    )
}

fn decode_sales_read_cursor(value: &str) -> Result<SalesReadCursor, Status> {
    if value.is_empty() || value.len() > 384 {
        return Err(Status::invalid_argument("Sales read cursor is invalid"));
    }
    let decoded =
        hex::decode(value).map_err(|_| Status::invalid_argument("Sales read cursor is invalid"))?;
    let decoded = String::from_utf8(decoded)
        .map_err(|_| Status::invalid_argument("Sales read cursor is invalid"))?;
    let mut parts = decoded.split('|');
    let projected_at = parts
        .next()
        .ok_or_else(|| Status::invalid_argument("Sales read cursor is invalid"))?;
    let aggregate_type = parts
        .next()
        .ok_or_else(|| Status::invalid_argument("Sales read cursor is invalid"))?;
    let aggregate_id = parts
        .next()
        .ok_or_else(|| Status::invalid_argument("Sales read cursor is invalid"))?;
    if parts.next().is_some()
        || !matches!(
            aggregate_type,
            "lead"
                | "opportunity"
                | "quote"
                | "sales_order"
                | "delivery_handoff"
                | "invoice_handoff"
                | "collection_state"
        )
    {
        return Err(Status::invalid_argument("Sales read cursor is invalid"));
    }
    let projected_at = DateTime::parse_from_rfc3339(projected_at)
        .map_err(|_| Status::invalid_argument("Sales read cursor is invalid"))?
        .with_timezone(&Utc);
    let aggregate_id = parse_uuid(aggregate_id, "Sales read cursor aggregate ID")?;
    if aggregate_id.is_nil() {
        return Err(Status::invalid_argument("Sales read cursor is invalid"));
    }
    Ok(SalesReadCursor {
        projected_at,
        aggregate_type: aggregate_type.to_owned(),
        aggregate_id,
    })
}

fn encode_asset_maintenance_read_cursor(cursor: &AssetMaintenanceReadCursor) -> String {
    hex::encode(
        format!(
            "{}|{}|{}",
            cursor.projected_at.to_rfc3339(),
            cursor.aggregate_type,
            cursor.aggregate_id
        )
        .as_bytes(),
    )
}

fn decode_asset_maintenance_read_cursor(value: &str) -> Result<AssetMaintenanceReadCursor, Status> {
    if value.is_empty() || value.len() > 384 {
        return Err(Status::invalid_argument(
            "Asset/Maintenance read cursor is invalid",
        ));
    }
    let decoded = hex::decode(value)
        .map_err(|_| Status::invalid_argument("Asset/Maintenance read cursor is invalid"))?;
    let decoded = String::from_utf8(decoded)
        .map_err(|_| Status::invalid_argument("Asset/Maintenance read cursor is invalid"))?;
    let mut parts = decoded.split('|');
    let projected_at = parts
        .next()
        .ok_or_else(|| Status::invalid_argument("Asset/Maintenance read cursor is invalid"))?;
    let aggregate_type = parts
        .next()
        .ok_or_else(|| Status::invalid_argument("Asset/Maintenance read cursor is invalid"))?;
    let aggregate_id = parts
        .next()
        .ok_or_else(|| Status::invalid_argument("Asset/Maintenance read cursor is invalid"))?;
    if parts.next().is_some() {
        return Err(Status::invalid_argument(
            "Asset/Maintenance read cursor is invalid",
        ));
    }
    let projected_at = DateTime::parse_from_rfc3339(projected_at)
        .map_err(|_| Status::invalid_argument("Asset/Maintenance read cursor is invalid"))?
        .with_timezone(&Utc);
    let aggregate_type = match aggregate_type {
        "asset" => "asset",
        "maintenance_work_order" => "maintenance_work_order",
        _ => {
            return Err(Status::invalid_argument(
                "Asset/Maintenance read cursor is invalid",
            ));
        }
    };
    let aggregate_id = parse_uuid(aggregate_id, "Asset/Maintenance read cursor aggregate ID")?;
    if aggregate_id.is_nil() {
        return Err(Status::invalid_argument(
            "Asset/Maintenance read cursor is invalid",
        ));
    }
    Ok(AssetMaintenanceReadCursor {
        projected_at,
        aggregate_type,
        aggregate_id,
    })
}

fn encode_finance_close_read_cursor(cursor: &FinanceCloseReadCursor) -> String {
    hex::encode(format!("{}|{}", cursor.projected_at.to_rfc3339(), cursor.period_id).as_bytes())
}

fn decode_finance_close_read_cursor(value: &str) -> Result<FinanceCloseReadCursor, Status> {
    if value.is_empty() || value.len() > 256 {
        return Err(Status::invalid_argument(
            "Finance close read cursor is invalid",
        ));
    }
    let decoded = hex::decode(value)
        .map_err(|_| Status::invalid_argument("Finance close read cursor is invalid"))?;
    let decoded = String::from_utf8(decoded)
        .map_err(|_| Status::invalid_argument("Finance close read cursor is invalid"))?;
    let (projected_at, period_id) = decoded
        .split_once('|')
        .ok_or_else(|| Status::invalid_argument("Finance close read cursor is invalid"))?;
    let projected_at = DateTime::parse_from_rfc3339(projected_at)
        .map_err(|_| Status::invalid_argument("Finance close read cursor is invalid"))?
        .with_timezone(&Utc);
    let period_id = parse_uuid(period_id, "Finance close read cursor period ID")?;
    if period_id.is_nil() {
        return Err(Status::invalid_argument(
            "Finance close read cursor is invalid",
        ));
    }
    Ok(FinanceCloseReadCursor {
        projected_at,
        period_id,
    })
}

/// Encode only the support queue's validated keyset position. The cursor is
/// opaque at the transport boundary and contains no ticket payload, customer
/// secret, or authorization material.
fn encode_support_ticket_cursor(cursor: &SupportTicketListCursor) -> String {
    let value = format!(
        "{}|{}",
        cursor
            .updated_at()
            .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        cursor.id()
    );
    hex::encode(value.as_bytes())
}

fn decode_support_ticket_cursor(value: &str) -> Result<SupportTicketListCursor, Status> {
    if value.is_empty() || value.len() > 256 {
        return Err(Status::invalid_argument("support ticket cursor is invalid"));
    }
    let bytes = hex::decode(value)
        .map_err(|_| Status::invalid_argument("support ticket cursor is invalid"))?;
    let value = String::from_utf8(bytes)
        .map_err(|_| Status::invalid_argument("support ticket cursor is invalid"))?;
    let (timestamp, id) = value
        .split_once('|')
        .ok_or_else(|| Status::invalid_argument("support ticket cursor is invalid"))?;
    let timestamp = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| Status::invalid_argument("support ticket cursor is invalid"))?
        .with_timezone(&Utc);
    let id = parse_uuid(id, "support ticket cursor")?;
    SupportTicketListCursor::new(timestamp, id)
        .map_err(|_| Status::invalid_argument("support ticket cursor is invalid"))
}

fn encode_finance_banking_read_cursor(cursor: &FinanceBankingReadCursor) -> String {
    hex::encode(format!("{}|{}", cursor.aggregate_type(), cursor.aggregate_id()).as_bytes())
}

fn decode_finance_banking_read_cursor(value: &str) -> Result<FinanceBankingReadCursor, Status> {
    if value.is_empty() || value.len() > 256 {
        return Err(Status::invalid_argument(
            "Finance banking read cursor is invalid",
        ));
    }
    let bytes = hex::decode(value)
        .map_err(|_| Status::invalid_argument("Finance banking read cursor is invalid"))?;
    let value = String::from_utf8(bytes)
        .map_err(|_| Status::invalid_argument("Finance banking read cursor is invalid"))?;
    let (aggregate_type, aggregate_id) = value
        .split_once('|')
        .ok_or_else(|| Status::invalid_argument("Finance banking read cursor is invalid"))?;
    let aggregate_id = parse_uuid(aggregate_id, "Finance banking read cursor aggregate_id")?;
    FinanceBankingReadCursor::new(aggregate_type, aggregate_id)
        .map_err(|_| Status::invalid_argument("Finance banking read cursor is invalid"))
}

fn encode_finance_subledger_read_cursor(cursor: &FinanceSubledgerReadCursor) -> String {
    hex::encode(format!("{}|{}", cursor.aggregate_type(), cursor.aggregate_id()).as_bytes())
}

fn decode_finance_subledger_read_cursor(value: &str) -> Result<FinanceSubledgerReadCursor, Status> {
    if value.is_empty() || value.len() > 256 {
        return Err(Status::invalid_argument(
            "Finance subledger read cursor is invalid",
        ));
    }
    let bytes = hex::decode(value)
        .map_err(|_| Status::invalid_argument("Finance subledger read cursor is invalid"))?;
    let value = String::from_utf8(bytes)
        .map_err(|_| Status::invalid_argument("Finance subledger read cursor is invalid"))?;
    let (aggregate_type, aggregate_id) = value
        .split_once('|')
        .ok_or_else(|| Status::invalid_argument("Finance subledger read cursor is invalid"))?;
    let aggregate_id = parse_uuid(aggregate_id, "Finance subledger read cursor aggregate_id")?;
    FinanceSubledgerReadCursor::new(aggregate_type, aggregate_id)
        .map_err(|_| Status::invalid_argument("Finance subledger read cursor is invalid"))
}

fn encode_projection_status_cursor(cursor: &ProjectionStatusCursor) -> String {
    hex::encode(serde_json::to_vec(cursor).unwrap_or_default())
}

fn decode_projection_status_cursor(value: &str) -> Result<ProjectionStatusCursor, Status> {
    if value.is_empty() || value.len() > 512 {
        return Err(Status::invalid_argument(
            "projection status cursor is invalid",
        ));
    }
    let bytes = hex::decode(value)
        .map_err(|_| Status::invalid_argument("projection status cursor is invalid"))?;
    let cursor: ProjectionStatusCursor = serde_json::from_slice(&bytes)
        .map_err(|_| Status::invalid_argument("projection status cursor is invalid"))?;
    ProjectionStatusCursor::new(
        cursor.tenant_id,
        cursor.holding_ref,
        cursor.projection_name,
        cursor.job_key,
        cursor.job_id,
    )
    .map_err(|_| Status::invalid_argument("projection status cursor is invalid"))
}

fn finance_persistence_status(error: PersistenceError, correlation_id: Uuid) -> Status {
    let status = match error {
        PersistenceError::OptimisticConcurrency { .. }
        | PersistenceError::IdempotencyConflict { .. } => {
            Status::aborted("Finance command conflicts with the current aggregate state")
        }
        PersistenceError::NotFound { .. } => Status::not_found("Finance aggregate not found"),
        PersistenceError::InvalidData(_) => {
            Status::failed_precondition("Finance aggregate cannot be persisted")
        }
        PersistenceError::Configuration(_) => {
            Status::failed_precondition("Finance persistence is not configured")
        }
        PersistenceError::Database(_) | PersistenceError::Migration(_) => {
            Status::unavailable("Finance persistence is unavailable")
        }
        PersistenceError::Serialization(_) => Status::internal("Finance persistence failed"),
        PersistenceError::ClaimLost { .. } => Status::aborted("Finance command ownership changed"),
    };
    errors::enrich(status, Some(correlation_id))
}

fn commercial_command_context(
    context: &CommandContext,
) -> Result<CommercialCommandContext, Status> {
    CommercialCommandContext::new(
        context.command_id,
        context.idempotency_key.clone(),
        context.correlation_id,
        context.actor.principal_ref.clone(),
        context.audit.purpose.clone(),
    )
    .map_err(|_| Status::invalid_argument("commercial command context is invalid"))
}

fn commercial_persistence_status(error: PersistenceError, correlation_id: Uuid) -> Status {
    let status = match error {
        PersistenceError::OptimisticConcurrency { .. }
        | PersistenceError::IdempotencyConflict { .. } => {
            Status::aborted("commercial command conflicts with the current aggregate state")
        }
        PersistenceError::NotFound { .. } => Status::not_found("commercial aggregate not found"),
        PersistenceError::InvalidData(_) => {
            Status::failed_precondition("commercial aggregate cannot be persisted")
        }
        PersistenceError::Configuration(_) => {
            Status::failed_precondition("commercial persistence is not configured")
        }
        PersistenceError::Database(_) | PersistenceError::Migration(_) => {
            Status::unavailable("commercial persistence is unavailable")
        }
        PersistenceError::Serialization(_) => Status::internal("commercial persistence failed"),
        PersistenceError::ClaimLost { .. } => {
            Status::aborted("commercial command ownership changed")
        }
    };
    errors::enrich(status, Some(correlation_id))
}

fn entitlement_activation_time(value: &str) -> Result<DateTime<Utc>, Status> {
    if value.trim().is_empty() {
        return Err(Status::invalid_argument("activated_at is required"));
    }
    DateTime::parse_from_rfc3339(value.trim())
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| Status::invalid_argument("activated_at is invalid"))
}

fn require_finance_thai_tax_reconciliation(
    reconciliation: FinanceThaiTaxProjectionReconciliation,
) -> Result<(), Status> {
    if reconciliation.is_reconciled() {
        Ok(())
    } else {
        Err(Status::failed_precondition(
            "Thai tax source and read projection are not reconciled",
        ))
    }
}

fn product_persistence_status(
    error: PersistenceError,
    product: &'static str,
    correlation_id: Uuid,
) -> Status {
    let status = match error {
        PersistenceError::OptimisticConcurrency { .. }
        | PersistenceError::IdempotencyConflict { .. } => Status::aborted(format!(
            "{product} command conflicts with the current aggregate state"
        )),
        PersistenceError::NotFound { .. } => {
            Status::not_found(format!("{product} aggregate not found"))
        }
        PersistenceError::InvalidData(_) => {
            Status::failed_precondition(format!("{product} aggregate cannot be persisted"))
        }
        PersistenceError::Configuration(_) => {
            Status::failed_precondition(format!("{product} persistence is not configured"))
        }
        PersistenceError::Database(_) | PersistenceError::Migration(_) => {
            Status::unavailable(format!("{product} persistence is unavailable"))
        }
        PersistenceError::Serialization(_) => {
            Status::internal(format!("{product} persistence failed"))
        }
        PersistenceError::ClaimLost { .. } => {
            Status::aborted(format!("{product} command ownership changed"))
        }
    };
    errors::enrich(status, Some(correlation_id))
}

fn marketing_content_persistence_status(error: PersistenceError, correlation_id: Uuid) -> Status {
    let status = match error {
        PersistenceError::OptimisticConcurrency { .. }
        | PersistenceError::IdempotencyConflict { .. } => {
            Status::aborted("Marketing content command conflicts with current state")
        }
        PersistenceError::NotFound { .. } => {
            Status::not_found("Marketing content record not found")
        }
        PersistenceError::InvalidData(_) => {
            Status::failed_precondition("Marketing content command cannot be persisted")
        }
        PersistenceError::Configuration(_) => {
            Status::failed_precondition("Marketing content persistence is not configured")
        }
        PersistenceError::Database(_) | PersistenceError::Migration(_) => {
            Status::unavailable("Marketing content persistence is unavailable")
        }
        PersistenceError::Serialization(_) => {
            Status::internal("Marketing content persistence failed")
        }
        PersistenceError::ClaimLost { .. } => {
            Status::aborted("Marketing content command ownership changed")
        }
    };
    errors::enrich(status, Some(correlation_id))
}

fn marketing_delivery_state_text(state: DeliveryState) -> &'static str {
    match state {
        DeliveryState::Queued => "queued",
        DeliveryState::Sending => "sending",
        DeliveryState::RetryScheduled => "retry_scheduled",
        DeliveryState::ProviderAccepted => "provider_accepted",
        DeliveryState::Delivered => "delivered",
        DeliveryState::Suppressed => "suppressed",
        DeliveryState::FailedTerminal => "failed_terminal",
    }
}

const MAX_MARKETING_FORM_VALUES_BYTES: usize = 2 * 1024 * 1024;
const PUBLIC_MARKETING_FORM_REQUESTS_PER_MINUTE: u32 = 30;
const PUBLIC_MARKETING_PREVIEW_REQUESTS_PER_MINUTE: u32 = 120;

fn marketing_form_values(value: &[u8]) -> Result<BTreeMap<String, String>, Status> {
    if value.is_empty() || value.len() > MAX_MARKETING_FORM_VALUES_BYTES {
        return Err(Status::invalid_argument("values_json is out of bounds"));
    }
    serde_json::from_slice(value)
        .map_err(|_| Status::invalid_argument("values_json must be a string map"))
}

fn legal_persistence_status(error: PersistenceError, correlation_id: Uuid) -> Status {
    let status = match error {
        PersistenceError::OptimisticConcurrency { .. }
        | PersistenceError::IdempotencyConflict { .. } => {
            Status::aborted("Legal command conflicts with the current aggregate state")
        }
        PersistenceError::NotFound { .. } => Status::not_found("Legal record not found"),
        PersistenceError::InvalidData(_) => {
            Status::failed_precondition("Legal record cannot be persisted or read")
        }
        PersistenceError::Configuration(_) => {
            Status::failed_precondition("Legal persistence is not configured")
        }
        PersistenceError::Database(_) | PersistenceError::Migration(_) => {
            Status::unavailable("Legal persistence is unavailable")
        }
        PersistenceError::Serialization(_) => Status::internal("Legal persistence failed"),
        PersistenceError::ClaimLost { .. } => Status::aborted("Legal command ownership changed"),
    };
    errors::enrich(status, Some(correlation_id))
}

fn finance_close_read_status(
    error: erpx_infrastructure::FinanceCloseReadError,
    correlation_id: Uuid,
) -> Status {
    let status = match error {
        erpx_infrastructure::FinanceCloseReadError::InvalidRequest => {
            Status::invalid_argument("Finance close read request is invalid")
        }
        erpx_infrastructure::FinanceCloseReadError::InvalidProjection => {
            Status::failed_precondition("Finance close read projection is invalid")
        }
        erpx_infrastructure::FinanceCloseReadError::PersistenceUnavailable => {
            Status::unavailable("Finance close read persistence is unavailable")
        }
    };
    errors::enrich(status, Some(correlation_id))
}

fn crm_persistence_status(error: PersistenceError, correlation_id: Uuid) -> Status {
    let status = match error {
        PersistenceError::InvalidData(_) => {
            Status::failed_precondition("CRM read projection is invalid")
        }
        PersistenceError::Configuration(_) => {
            Status::failed_precondition("CRM persistence is not configured")
        }
        PersistenceError::Database(_) | PersistenceError::Migration(_) => {
            Status::unavailable("CRM persistence is unavailable")
        }
        PersistenceError::Serialization(_) => Status::internal("CRM read failed"),
        PersistenceError::OptimisticConcurrency { .. }
        | PersistenceError::IdempotencyConflict { .. }
        | PersistenceError::NotFound { .. }
        | PersistenceError::ClaimLost { .. } => Status::internal("CRM read failed"),
    };
    errors::enrich(status, Some(correlation_id))
}

const MAX_FINANCE_BANKING_JSON_BYTES: usize = 2 * 1024 * 1024;

fn decode_finance_banking_json<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    field: &'static str,
) -> Result<T, Status> {
    if bytes.is_empty() || bytes.len() > MAX_FINANCE_BANKING_JSON_BYTES {
        return Err(Status::invalid_argument(format!(
            "{field} is empty or exceeds its size bound"
        )));
    }
    serde_json::from_slice(bytes)
        .map_err(|_| Status::invalid_argument(format!("{field} is invalid")))
}

fn banking_read_row_message(
    row: FinanceBankingReadRow,
) -> Result<erpx_proto::platform::FinanceBankingReadRow, Status> {
    Ok(erpx_proto::platform::FinanceBankingReadRow {
        aggregate_type: row.aggregate_type,
        aggregate_id: row.aggregate_id.to_string(),
        holding_id: row.holding_id.to_string(),
        company_id: row.company_id.to_string(),
        source_version: row.source_version,
        aggregate_json: serde_json::to_vec(&row.aggregate)
            .map_err(|_| Status::internal("Finance banking read encoding failed"))?,
    })
}

fn finance_bank_provider_import_status_message(
    status: FinanceBankProviderImportJobStatus,
) -> Result<erpx_proto::platform::FinanceBankProviderImportJobStatus, Status> {
    if !matches!(
        status.state.as_str(),
        "pending" | "claimed" | "retryable" | "completed" | "dead_letter"
    ) || status.attempts > status.max_attempts
    {
        return Err(Status::internal("Finance bank import status is invalid"));
    }
    Ok(erpx_proto::platform::FinanceBankProviderImportJobStatus {
        job_id: status.job_id.to_string(),
        tenant_id: status.tenant_id,
        holding_ref: status.holding_ref,
        import_id: status.import_id.to_string(),
        state: status.state,
        source_fence: status.source_fence,
        attempts: u32::from(status.attempts),
        max_attempts: u32::from(status.max_attempts),
        available_at: status.available_at.to_rfc3339(),
        lease_until: status
            .lease_until
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        last_failure_class: status.last_failure_class.unwrap_or_default(),
        completed_at: status
            .completed_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        updated_at: status.updated_at.to_rfc3339(),
    })
}

const MAX_FINANCE_SUBLEDGER_JSON_BYTES: usize = 2 * 1024 * 1024;

fn finance_subledger_read_row_message(
    row: FinanceSubledgerReadRow,
) -> Result<erpx_proto::platform::FinanceSubledgerReadRow, Status> {
    let aggregate_json = serde_json::to_vec(&row.aggregate)
        .map_err(|_| Status::internal("Finance subledger read encoding failed"))?;
    if aggregate_json.len() > MAX_FINANCE_SUBLEDGER_JSON_BYTES {
        return Err(Status::internal(
            "Finance subledger read exceeded its safety bound",
        ));
    }
    Ok(erpx_proto::platform::FinanceSubledgerReadRow {
        aggregate_type: row.aggregate_type,
        aggregate_id: row.aggregate_id.to_string(),
        company_id: row.company_id.to_string(),
        branch_id: row
            .branch_id
            .map(|value| value.to_string())
            .unwrap_or_default(),
        source_version: row.aggregate_version,
        aggregate_json,
    })
}

fn finance_subledger_row_scope(row: &FinanceSubledgerReadRow) -> Scope {
    access_scope(row.aggregate.scope())
}

fn finance_banking_row_scope(row: &FinanceBankingReadRow) -> Scope {
    match &row.aggregate {
        erpx_infrastructure::FinanceBankingReadAggregate::Account(account) => {
            access_scope(account.scope())
        }
        erpx_infrastructure::FinanceBankingReadAggregate::StatementImport(import) => {
            access_scope(import.scope())
        }
        erpx_infrastructure::FinanceBankingReadAggregate::Reconciliation(reconciliation) => {
            access_scope(reconciliation.scope())
        }
    }
}

/// Map a support aggregate to the transport-safe ticket shape. The domain
/// projection deliberately removes internal notes before this boundary, so
/// every ticket response can be reused by customer-facing and platform
/// clients without accidentally disclosing the internal audience.
fn ticket_message(ticket: &erpx_domain::support::SupportTicket) -> TicketMessage {
    let visible = ticket.customer_visible_view();
    TicketMessage {
        id: visible.id.to_string(),
        customer_id: visible.customer_account_id.to_string(),
        status: format!("{:?}", visible.status).to_lowercase(),
        severity: visible.severity,
        description: visible.description,
        customer_replies: visible
            .messages
            .into_iter()
            .map(|message| TicketReply {
                id: message.id.to_string(),
                author_ref: message.author_ref,
                body: message.body,
                created_at: message.created_at.to_rfc3339(),
            })
            .collect(),
    }
}

fn parse_mode(value: &str) -> Result<SupportMode, Status> {
    match value {
        "diagnostic_read" => Ok(SupportMode::DiagnosticRead),
        "tenant_mutation" => Ok(SupportMode::TenantMutation),
        "impersonation" => Ok(SupportMode::Impersonation),
        _ => Err(Status::invalid_argument("unsupported support mode")),
    }
}

const SUPPORT_DIAGNOSTICS_PERMISSION: &str = "diagnostics:read";
const SUPPORT_DIAGNOSTICS_AGENT_PERMISSION: &str = "support.session.request";

/// Parsed customer-support diagnostics authority. The raw capability secret
/// is kept only in this request-local value and has no Debug representation;
/// it is never copied into an audit record or diagnostics payload.
struct ParsedSupportDiagnosticsAccess {
    customer_id: Uuid,
    capability_id: Uuid,
    capability_secret_hex: String,
    capability_secret: SupportCapabilitySecret,
    support_session_id: Uuid,
    support_ticket_id: Uuid,
    device_ref: String,
    support_scope: String,
    approved_scope: Scope,
    module: String,
    subscription_id: Uuid,
}

fn validate_diagnostics_access_field(
    value: &str,
    field: &'static str,
    maximum_bytes: usize,
) -> Result<(), Status> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Status::permission_denied(format!(
            "support diagnostics {field} is required"
        )));
    }
    if value.len() > maximum_bytes || value.chars().any(char::is_control) {
        return Err(Status::permission_denied(format!(
            "support diagnostics {field} is invalid"
        )));
    }
    Ok(())
}

fn platform_diagnostics_access_is_empty(access: &SupportDiagnosticsAccess) -> Result<(), Status> {
    if !access.customer_id.is_empty()
        || !access.capability_id.is_empty()
        || !access.capability_secret.is_empty()
        || !access.support_session_id.is_empty()
        || !access.support_ticket_id.is_empty()
        || !access.support_mode.is_empty()
        || !access.support_scope.is_empty()
        || !access.module.is_empty()
        || !access.subscription_id.is_empty()
        || !access.device_ref.is_empty()
    {
        return Err(Status::invalid_argument(
            "platform diagnostics cannot include customer support capability material",
        ));
    }
    Ok(())
}

fn parse_support_diagnostics_access(
    access: &SupportDiagnosticsAccess,
    requested_scope: &Scope,
) -> Result<ParsedSupportDiagnosticsAccess, Status> {
    validate_diagnostics_access_field(&access.customer_id, "customer_id", 64)?;
    validate_diagnostics_access_field(&access.capability_id, "capability_id", 64)?;
    validate_diagnostics_access_field(&access.capability_secret, "capability_secret", 64)?;
    validate_diagnostics_access_field(&access.support_session_id, "support_session_id", 64)?;
    validate_diagnostics_access_field(&access.support_ticket_id, "support_ticket_id", 64)?;
    validate_diagnostics_access_field(&access.support_mode, "support_mode", 32)?;
    validate_diagnostics_access_field(&access.support_scope, "support_scope", 4096)?;
    validate_diagnostics_access_field(&access.module, "module", 128)?;
    validate_diagnostics_access_field(&access.subscription_id, "subscription_id", 64)?;
    validate_diagnostics_access_field(&access.device_ref, "device_ref", 64)?;

    let customer_id = parse_uuid(&access.customer_id, "customer_id")?;
    let capability_id = parse_uuid(&access.capability_id, "capability_id")?;
    let support_session_id = parse_uuid(&access.support_session_id, "support_session_id")?;
    let support_ticket_id = parse_uuid(&access.support_ticket_id, "support_ticket_id")?;
    let subscription_id = parse_uuid(&access.subscription_id, "subscription_id")?;
    let device_id = parse_uuid(&access.device_ref, "device_ref")?;
    if device_id.to_string() != access.device_ref {
        return Err(Status::permission_denied(
            "support diagnostics device binding is invalid",
        ));
    }

    let support_mode = parse_mode(&access.support_mode).map_err(|_| {
        Status::permission_denied("support diagnostics requires diagnostic_read capability mode")
    })?;
    if support_mode != SupportMode::DiagnosticRead {
        return Err(Status::permission_denied(
            "support diagnostics requires diagnostic_read capability mode",
        ));
    }
    let approved_scope = parse_support_scope(&access.support_scope)?;
    if access.support_scope != support_scope_reference(&approved_scope) {
        return Err(Status::permission_denied(
            "support diagnostics scope is not canonical",
        ));
    }
    if matches!(approved_scope, Scope::Platform)
        || matches!(requested_scope, Scope::Platform)
        || !approved_scope.contains(requested_scope)
    {
        return Err(Status::permission_denied(
            "support diagnostics scope is not approved",
        ));
    }

    let capability_secret = SupportCapabilitySecret::from_hex(&access.capability_secret)
        .map_err(|_| Status::permission_denied("support diagnostics capability is inactive"))?;
    Ok(ParsedSupportDiagnosticsAccess {
        customer_id,
        capability_id,
        capability_secret_hex: access.capability_secret.clone(),
        capability_secret,
        support_session_id,
        support_ticket_id,
        device_ref: access.device_ref.clone(),
        support_scope: access.support_scope.clone(),
        approved_scope,
        module: access.module.clone(),
        subscription_id,
    })
}

fn diagnostics_module_scope_is_entitled(
    scope: &Scope,
    billing: BillingState,
    book: &EntitlementAllocationBook,
    at: DateTime<Utc>,
) -> bool {
    match scope {
        Scope::Holding { .. } => {
            authorize_module(billing, Some(&book.module_entitlement()), true, at, false)
                == EntitlementDecision::Allowed
        }
        Scope::Company { company_id, .. } => {
            book.authorize(billing, *company_id, None, true, at, false)
                == EntitlementDecision::Allowed
        }
        Scope::Branch {
            company_id,
            branch_id,
            ..
        } => {
            book.authorize(billing, *company_id, Some(*branch_id), true, at, false)
                == EntitlementDecision::Allowed
        }
        // A subtree can contain branches without a company-to-branch mapping
        // in this control-plane book. Deny that ambiguous shape rather than
        // treating purchased capacity as permission for arbitrary branches.
        Scope::Subtree {
            company_ids,
            branch_ids,
            ..
        } if !company_ids.is_empty() && branch_ids.is_empty() => company_ids.iter().all(|id| {
            book.authorize(billing, *id, None, true, at, false) == EntitlementDecision::Allowed
        }),
        Scope::Platform | Scope::Subtree { .. } => false,
    }
}

fn support_diagnostics_transport_fields(
    snapshot: &DiagnosticsSnapshot,
) -> Vec<SupportDiagnosticField> {
    let mut fields = Vec::with_capacity(12);
    let mut push = |key: &'static str, value: String| {
        fields.push(SupportDiagnosticField {
            key: key.into(),
            value,
        });
    };
    if let Some(version) = snapshot
        .versions()
        .iter()
        .find(|version| version.component() == "server")
    {
        push("diagnostic_version", version.version().to_owned());
    }
    push(
        "diagnostic_schema_version",
        snapshot.schema_version().to_owned(),
    );
    if let Some(lag) = snapshot.projection_lag_seconds() {
        push("projection_lag_seconds", lag.to_string());
    }
    if let Some(health) = snapshot.worker_health() {
        push("worker_health", health.state().to_owned());
        if let Some(age) = health.age_seconds() {
            push("worker_health_age_seconds", age.to_string());
        }
    }
    if let Some(health) = snapshot.backup_health() {
        push("backup_health", health.health().state().to_owned());
        push("backup_verified", health.verified().to_string());
    }
    push("failed_job_count", snapshot.failed_jobs().len().to_string());
    push(
        "integration_error_count",
        snapshot.integration_errors().len().to_string(),
    );
    push(
        "redacted_configuration_count",
        snapshot.redacted_configuration_count().to_string(),
    );
    let truncated = snapshot.truncated();
    push(
        "truncated_item_count",
        (truncated.versions()
            + truncated.failed_jobs()
            + truncated.integration_errors()
            + truncated.correlation_ids()
            + truncated.configuration())
        .to_string(),
    );
    fields
}

fn parse_ticket_status(value: &str) -> Result<TicketStatus, Status> {
    match value {
        "new" => Ok(TicketStatus::New),
        "triaged" => Ok(TicketStatus::Triaged),
        "waiting_for_customer" => Ok(TicketStatus::WaitingForCustomer),
        "access_requested" => Ok(TicketStatus::AccessRequested),
        "approved" => Ok(TicketStatus::Approved),
        "in_progress" => Ok(TicketStatus::InProgress),
        "resolved" => Ok(TicketStatus::Resolved),
        "customer_confirmed" => Ok(TicketStatus::CustomerConfirmed),
        "closed" => Ok(TicketStatus::Closed),
        _ => Err(Status::invalid_argument("unsupported ticket status")),
    }
}

fn parse_incident_status(value: &str) -> Result<IncidentStatus, Status> {
    match value {
        "investigating" => Ok(IncidentStatus::Investigating),
        "identified" => Ok(IncidentStatus::Identified),
        "monitoring" => Ok(IncidentStatus::Monitoring),
        "resolved" => Ok(IncidentStatus::Resolved),
        "postmortem_complete" => Ok(IncidentStatus::PostmortemComplete),
        _ => Err(Status::invalid_argument("unsupported incident status")),
    }
}

const fn incident_status_name(status: IncidentStatus) -> &'static str {
    match status {
        IncidentStatus::Investigating => "investigating",
        IncidentStatus::Identified => "identified",
        IncidentStatus::Monitoring => "monitoring",
        IncidentStatus::Resolved => "resolved",
        IncidentStatus::PostmortemComplete => "postmortem_complete",
    }
}

fn validate_incident_text(
    value: &str,
    field: &str,
    maximum_bytes: usize,
) -> Result<String, Status> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Status::invalid_argument(format!("{field} is required")));
    }
    if value.len() > maximum_bytes || value.chars().any(char::is_control) {
        return Err(Status::invalid_argument(format!("{field} is invalid")));
    }
    Ok(value.to_owned())
}

fn validate_incident_text_list(
    values: &[String],
    field: &'static str,
    maximum_items: usize,
) -> Result<Vec<String>, Status> {
    if values.len() > maximum_items {
        return Err(Status::invalid_argument(format!(
            "{field} has too many entries"
        )));
    }
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            validate_incident_text(
                value,
                &format!("{field}[{index}]"),
                MAX_INCIDENT_REFERENCE_BYTES,
            )
        })
        .collect()
}

fn incident_request_hash(value: &serde_json::Value) -> Result<String, Status> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| Status::internal("incident request hashing failed"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn incident_persistence_status(error: SupportPersistenceError) -> Status {
    match error {
        SupportPersistenceError::IdempotencyConflict { .. } => {
            Status::aborted("incident idempotency key conflicts with an existing command")
        }
        SupportPersistenceError::OptimisticConcurrency { .. } => {
            Status::aborted("incident aggregate version conflict")
        }
        SupportPersistenceError::NotFound { .. } => Status::not_found("incident not found"),
        SupportPersistenceError::InvalidData(_) => {
            Status::invalid_argument("incident command is invalid")
        }
        SupportPersistenceError::Database(_) => {
            Status::unavailable("incident persistence unavailable")
        }
        SupportPersistenceError::Serialization(_) => {
            Status::internal("incident persistence failed")
        }
        SupportPersistenceError::AttachmentScopeDenied
        | SupportPersistenceError::AttachmentBindingDenied
        | SupportPersistenceError::AccessRequestTicketBindingDenied
        | SupportPersistenceError::AttachmentClosedTicket
        | SupportPersistenceError::AttachmentNotActive
        | SupportPersistenceError::AttachmentNotExpired
        | SupportPersistenceError::AttachmentLimitExceeded => {
            Status::failed_precondition("support attachment policy denied the operation")
        }
    }
}

fn incident_message(incident: Incident, version: u64) -> IncidentMessage {
    IncidentMessage {
        id: incident.id.to_string(),
        status: incident_status_name(incident.status).into(),
        commander_ref: incident.commander_ref,
        leads: incident.leads,
        affected_services: incident.affected_services,
        affected_customer_accounts: incident
            .affected_customer_accounts
            .into_iter()
            .map(|id| id.to_string())
            .collect(),
        version,
    }
}

const fn billing_state_name(state: BillingState) -> &'static str {
    match state {
        BillingState::Trialing => "trialing",
        BillingState::Active => "active",
        BillingState::PastDue => "past_due",
        BillingState::Grace => "grace",
        BillingState::ReadOnly => "read_only",
        BillingState::Canceled => "canceled",
    }
}

fn entitlement_command_receipt(
    receipt: &erpx_domain::commercial::CommercialCommandReceipt,
) -> EntitlementCommandReceipt {
    EntitlementCommandReceipt {
        aggregate_type: receipt.aggregate_type.clone(),
        aggregate_id: receipt.aggregate_id.to_string(),
        version: receipt.aggregate_version,
        replayed: receipt.replayed,
        command_id: receipt.command_id.to_string(),
    }
}

const fn customer_status_name(status: CustomerStatus) -> &'static str {
    match status {
        CustomerStatus::Prospect => "prospect",
        CustomerStatus::Onboarding => "onboarding",
        CustomerStatus::Active => "active",
        CustomerStatus::Suspended => "suspended",
        CustomerStatus::Churned => "churned",
    }
}

const fn health_state_name(state: HealthState) -> &'static str {
    match state {
        HealthState::Healthy => "healthy",
        HealthState::Degraded => "degraded",
        HealthState::Down => "down",
        HealthState::Unknown => "unknown",
    }
}

const fn support_mode_name(mode: SupportMode) -> &'static str {
    match mode {
        SupportMode::DiagnosticRead => "diagnostic_read",
        SupportMode::TenantMutation => "tenant_mutation",
        SupportMode::Impersonation => "impersonation",
    }
}

const fn risk_tier_name(tier: RiskTier) -> &'static str {
    match tier {
        RiskTier::PlatformRoot => "platform_root",
        RiskTier::Administrator => "administrator",
        RiskTier::High => "high",
        RiskTier::Standard => "standard",
        RiskTier::Contractor => "contractor",
        RiskTier::SupportDiagnostic => "support_diagnostic",
        RiskTier::SupportMutation => "support_mutation",
        RiskTier::Emergency => "emergency",
    }
}

fn to_summary(customer: erpx_domain::customer::Customer360) -> CustomerSummary {
    CustomerSummary {
        id: customer.account.id.to_string(),
        account_code: customer.account.account_code,
        trading_name: customer.account.trading_name,
        legal_name: customer.account.legal_name.unwrap_or_default(),
        country: customer.account.country,
        status: customer_status_name(customer.account.status).into(),
        billing_state: billing_state_name(customer.account.billing_state).into(),
        health: health_state_name(customer.health.state).into(),
        holding_count: customer.holding_count,
        company_count: customer.company_count,
        branch_count: customer.branch_count,
        active_user_count: customer.active_user_count,
        expiring_role_count: customer.expiring_role_count,
        open_ticket_count: customer.open_ticket_count,
        module_count: customer.module_count,
        projection_lag_seconds: customer.health.projection_lag_seconds,
    }
}

#[derive(Clone)]
struct CustomerRpcImpl {
    state: AppState,
}

#[tonic::async_trait]
impl CustomerRpc for CustomerRpcImpl {
    async fn list_customers(
        &self,
        request: Request<ListCustomersRequest>,
    ) -> Result<Response<ListCustomersResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            require_platform_scope(&scope)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_CUSTOMER_READ_PERMISSION,
                )
                .await?;
            let customers = self
                .state
                .customers
                .list(
                    (!request.get_ref().search.is_empty())
                        .then_some(request.get_ref().search.as_str()),
                )
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            let session_ref = request
                .metadata()
                .get("x-erpx-session")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            for customer in &customers {
                let audit = GlobalReadAuditRecord::new(
                    GlobalReadActor {
                        principal_ref: context.actor.clone(),
                        session_ref: session_ref.clone(),
                    },
                    customer.account.id,
                    "platform.customer_360",
                    GlobalReadScope {
                        label: "platform".into(),
                        holding_id: None,
                        company_id: None,
                        branch_id: None,
                    },
                    "platform_customer_console_list",
                    Utc::now(),
                    context.correlation_id,
                )
                .map_err(|error| Status::internal(error.to_string()))?;
                self.state
                    .store
                    .record_global_read(&audit)
                    .await
                    .map_err(|_| Status::internal("global-read audit persistence failed"))?;
            }
            Ok(Response::new(ListCustomersResponse {
                customers: customers.into_iter().map(to_summary).collect(),
            }))
        })
    }

    async fn get_customer(
        &self,
        request: Request<GetCustomerRequest>,
    ) -> Result<Response<GetCustomerResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            require_platform_scope(&scope)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_CUSTOMER_READ_PERMISSION,
                )
                .await?;
            let actor_ref = context.actor.clone();
            let id = parse_uuid(&request.get_ref().customer_id, "customer_id")?;
            let purpose = validate_read_purpose(&request.get_ref().purpose)?;
            let customer = self
                .state
                .customers
                .inspect(&actor_ref, id)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            let session_ref = request
                .metadata()
                .get("x-erpx-session")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let correlation_id = context.correlation_id;
            let audit = GlobalReadAuditRecord::new(
                GlobalReadActor {
                    principal_ref: actor_ref,
                    session_ref,
                },
                id,
                "platform.customer_360",
                GlobalReadScope {
                    label: "platform".into(),
                    holding_id: None,
                    company_id: None,
                    branch_id: None,
                },
                purpose,
                Utc::now(),
                correlation_id,
            )
            .map_err(|error| Status::internal(error.to_string()))?;
            self.state
                .store
                .record_global_read(&audit)
                .await
                .map_err(|_| Status::internal("global-read audit persistence failed"))?;
            Ok(Response::new(GetCustomerResponse {
                customer: Some(to_summary(customer)),
                correlation_id: correlation_id.to_string(),
            }))
        })
    }

    async fn get_customer_projection(
        &self,
        request: Request<GetCustomerProjectionRequest>,
    ) -> Result<Response<GetCustomerProjectionResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            require_platform_scope(&scope)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_CUSTOMER_READ_PERMISSION,
                )
                .await?;
            let id = parse_uuid(&request.get_ref().customer_id, "customer_id")?;
            let purpose = validate_read_purpose(&request.get_ref().purpose)?;
            let projection = validate_customer_projection(&request.get_ref().projection)?;
            let view = match projection {
                "organization" => {
                    encode_customer_projection(self.state.store.get_customer_organization(id))
                        .await?
                }
                "contacts" => {
                    encode_customer_projection(self.state.store.get_customer_contacts(id)).await?
                }
                "commercial" => {
                    encode_customer_projection(self.state.store.get_customer_commercial(id)).await?
                }
                "entitlements" => {
                    encode_customer_projection(self.state.store.get_customer_entitlements(id))
                        .await?
                }
                "onboarding" => {
                    encode_customer_projection(self.state.store.get_customer_onboarding(id)).await?
                }
                "health" => {
                    encode_customer_projection(self.state.store.get_customer_health(id)).await?
                }
                "support" => {
                    encode_customer_projection(self.state.store.get_customer_support_summary(id))
                        .await?
                }
                "access-audit" => {
                    encode_customer_projection(
                        self.state.store.get_customer_access_audit_summary(id),
                    )
                    .await?
                }
                "notes" => {
                    encode_customer_projection(self.state.store.get_customer_notes(id)).await?
                }
                _ => unreachable!("projection validated above"),
            };
            let session_ref = request
                .metadata()
                .get("x-erpx-session")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let audit = GlobalReadAuditRecord::new(
                GlobalReadActor {
                    principal_ref: context.actor,
                    session_ref,
                },
                id,
                "platform.customer_360",
                GlobalReadScope {
                    label: "platform".into(),
                    holding_id: None,
                    company_id: None,
                    branch_id: None,
                },
                purpose,
                Utc::now(),
                context.correlation_id,
            )
            .map_err(|error| Status::internal(error.to_string()))?;
            self.state
                .store
                .record_global_read(&audit)
                .await
                .map_err(|_| Status::internal("global-read audit persistence failed"))?;
            Ok(Response::new(GetCustomerProjectionResponse {
                customer_id: id.to_string(),
                projection: projection.into(),
                payload_json: view.payload,
                projection_updated_at: view
                    .metadata
                    .projection_updated_at
                    .map(|value| value.to_rfc3339())
                    .unwrap_or_default(),
                freshness: customer_freshness_name(view.metadata.freshness).into(),
                read_only: view.metadata.read_only,
                correlation_id: context.correlation_id.to_string(),
                supported: view.supported,
                unsupported_reason: view.unsupported_reason.unwrap_or_default(),
                present: view.present,
            }))
        })
    }

    async fn get_customer360_detail(
        &self,
        request: Request<GetCustomer360DetailRequest>,
    ) -> Result<Response<GetCustomer360DetailResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            let global_scope = global_read_scope_for_customer_scope(&scope)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_CUSTOMER_READ_PERMISSION,
                )
                .await?;
            let session_ref = request
                .metadata()
                .get("x-erpx-session")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let input = request.into_inner();
            let customer_id = parse_uuid(&input.customer_id, "customer_id")?;
            let purpose = validate_read_purpose(&input.purpose)?;
            let notes_before = match (
                input.notes_before_occurred_at.trim(),
                input.notes_before_id.trim(),
            ) {
                ("", "") => None,
                ("", _) | (_, "") => {
                    return Err(Status::invalid_argument(
                        "notes cursor requires both occurred_at and id",
                    ));
                }
                (occurred_at, note_id) => Some(CustomerNoteCursor {
                    created_at: DateTime::parse_from_rfc3339(occurred_at)
                        .map_err(|_| {
                            Status::invalid_argument("notes_before_occurred_at must be RFC3339")
                        })?
                        .with_timezone(&Utc),
                    note_id: parse_uuid(note_id, "notes_before_id")?,
                }),
            };
            let notes_page_size = if input.notes_page_size == 0 {
                50
            } else {
                input.notes_page_size
            };
            if notes_page_size > 100 {
                return Err(Status::invalid_argument(
                    "notes_page_size must be at most 100",
                ));
            }
            let detail_context = Customer360DetailReadContext {
                customer_id,
                scope,
                global_read: CustomerGlobalReadContext {
                    actor: GlobalReadActor {
                        principal_ref: context.actor.clone(),
                        session_ref,
                    },
                    module: "platform.customer_360.detail".into(),
                    purpose: purpose.into(),
                    correlation_id: context.correlation_id,
                },
            };
            let view = self
                .state
                .store
                .get_customer_360_detail(&detail_context, notes_before, notes_page_size)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            let metadata = view.metadata().clone();
            let (supported, unsupported_reason, present, section_states) = match &view {
                CustomerReadView::Present { data, .. } => (
                    true,
                    String::new(),
                    true,
                    customer_detail_section_states(data),
                ),
                CustomerReadView::Absent { .. } => (true, String::new(), false, Vec::new()),
                CustomerReadView::Unsupported { reason, .. } => {
                    (false, reason.clone(), false, Vec::new())
                }
            };
            let section_facts = match &view {
                CustomerReadView::Present { data, .. } => customer_detail_section_facts(data),
                CustomerReadView::Absent { .. } | CustomerReadView::Unsupported { .. } => {
                    Vec::new()
                }
            };
            let (notes, notes_next_before_occurred_at, notes_next_before_id) = match &view {
                CustomerReadView::Present { data, .. } => match &data.notes {
                    CustomerReadView::Present { data, .. } => {
                        let notes = data
                            .notes
                            .iter()
                            .map(|note| CustomerNoteMessage {
                                id: note.id.to_string(),
                                author_ref: note.author_ref.clone(),
                                created_at: note.created_at.to_rfc3339(),
                                body: note.body.clone(),
                            })
                            .collect();
                        let (next_before_occurred_at, next_before_id) = data
                            .next_before
                            .map(|cursor| {
                                (cursor.created_at.to_rfc3339(), cursor.note_id.to_string())
                            })
                            .unwrap_or_default();
                        (notes, next_before_occurred_at, next_before_id)
                    }
                    CustomerReadView::Absent { .. } | CustomerReadView::Unsupported { .. } => {
                        (Vec::new(), String::new(), String::new())
                    }
                },
                CustomerReadView::Absent { .. } | CustomerReadView::Unsupported { .. } => {
                    (Vec::new(), String::new(), String::new())
                }
            };
            self.state
                .store
                .record_global_read(
                    &GlobalReadAuditRecord::new(
                        detail_context.global_read.actor.clone(),
                        customer_id,
                        detail_context.global_read.module.clone(),
                        global_scope,
                        purpose,
                        Utc::now(),
                        context.correlation_id,
                    )
                    .map_err(|error| Status::internal(error.to_string()))?,
                )
                .await
                .map_err(|_| Status::internal("global-read audit persistence failed"))?;
            Ok(Response::new(GetCustomer360DetailResponse {
                customer_id: customer_id.to_string(),
                payload_json: Vec::new(),
                projection_updated_at: metadata
                    .projection_updated_at
                    .map(|value| value.to_rfc3339())
                    .unwrap_or_default(),
                freshness: customer_freshness_name(metadata.freshness).into(),
                read_only: metadata.read_only,
                correlation_id: context.correlation_id.to_string(),
                supported,
                unsupported_reason,
                present,
                notes,
                notes_next_before_occurred_at,
                notes_next_before_id,
                section_states,
                section_facts,
            }))
        })
    }

    async fn get_customer_access_audit(
        &self,
        request: Request<GetCustomerAccessAuditRequest>,
    ) -> Result<Response<GetCustomerAccessAuditResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            let id = parse_uuid(&request.get_ref().customer_id, "customer_id")?;
            let purpose = validate_read_purpose(&request.get_ref().purpose)?.to_owned();
            if matches!(scope, Scope::Platform) {
                self.state
                    .auth
                    .authorize_role(
                        &self.state.store,
                        &claims.principal_ref,
                        &Scope::Platform,
                        PLATFORM_CUSTOMER_READ_PERMISSION,
                    )
                    .await?;
            } else {
                self.state
                    .auth
                    .authorize_customer_security_admin(
                        &self.state.store,
                        &claims.principal_ref,
                        id,
                        &scope,
                    )
                    .await?;
            }
            let session_ref = request
                .metadata()
                .get("x-erpx-session")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let global_read_audit = GlobalReadAuditRecord::new(
                GlobalReadActor {
                    principal_ref: context.actor.clone(),
                    session_ref: session_ref.clone(),
                },
                id,
                "platform.customer_access_audit",
                global_read_scope_for_customer_scope(&scope)?,
                purpose.clone(),
                Utc::now(),
                context.correlation_id,
            )
            .map_err(|error| Status::internal(error.to_string()))?;
            self.state
                .store
                .record_global_read(&global_read_audit)
                .await
                .map_err(|_| Status::internal("global-read audit persistence failed"))?;

            let before = match (
                request.get_ref().before_occurred_at.trim(),
                request.get_ref().before_audit_id.trim(),
            ) {
                ("", "") => None,
                (occurred_at, audit_id) if !occurred_at.is_empty() && !audit_id.is_empty() => {
                    Some(CustomerGlobalReadAuditDetailCursor {
                        occurred_at: DateTime::parse_from_rfc3339(occurred_at)
                            .map_err(|_| Status::invalid_argument("before_occurred_at is invalid"))?
                            .with_timezone(&Utc),
                        audit_id: parse_uuid(audit_id, "before_audit_id")?,
                    })
                }
                _ => {
                    return Err(Status::invalid_argument(
                        "before_occurred_at and before_audit_id must be supplied together",
                    ));
                }
            };
            let query = CustomerGlobalReadAuditDetailQuery {
                customer_id: id,
                security_admin_scope: customer_security_admin_scope(id, &scope)?,
                audit: CustomerGlobalReadAuditQueryContext {
                    requester: GlobalReadActor {
                        principal_ref: context.actor,
                        session_ref,
                    },
                    purpose,
                    correlation_id: context.correlation_id,
                },
                before,
                limit: request.get_ref().page_size,
            };
            let audit_sink: Arc<dyn AuditSink> = self.state.store.clone();
            let view = CustomerGlobalReadAuditDetailService::new(
                Arc::clone(&self.state.store),
                audit_sink,
            )
            .list(query)
            .await
            .map_err(|error| errors::application(error, context.correlation_id))?;

            let mut response = GetCustomerAccessAuditResponse {
                customer_id: id.to_string(),
                correlation_id: context.correlation_id.to_string(),
                ..Default::default()
            };
            match view {
                CustomerReadView::Present { data, metadata } => {
                    response.entries = data
                        .entries
                        .into_iter()
                        .map(|entry| {
                            Ok(CustomerAccessAuditEntry {
                                audit_id: entry.audit_id.to_string(),
                                actor_ref: entry.actor.principal_ref,
                                session_ref: entry.actor.session_ref.unwrap_or_default(),
                                customer_id: entry.customer_id.to_string(),
                                module: entry.module,
                                scope: serde_json::to_string(&entry.scope).map_err(|_| {
                                    Status::internal(
                                        "customer access-audit scope serialization failed",
                                    )
                                })?,
                                purpose: entry.purpose,
                                occurred_at: entry.occurred_at.to_rfc3339(),
                                correlation_id: entry.correlation_id.to_string(),
                            })
                        })
                        .collect::<Result<Vec<_>, Status>>()?;
                    if let Some(cursor) = data.next_before {
                        response.next_before_occurred_at = cursor.occurred_at.to_rfc3339();
                        response.next_before_audit_id = cursor.audit_id.to_string();
                    }
                    response.projection_updated_at = metadata
                        .projection_updated_at
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_default();
                    response.freshness = customer_freshness_name(metadata.freshness).into();
                    response.read_only = metadata.read_only;
                    response.present = true;
                }
                CustomerReadView::Absent { .. } | CustomerReadView::Unsupported { .. } => {}
            }
            Ok(Response::new(response))
        })
    }
}

#[derive(Debug)]
struct TenantOwnerSeedReceipt {
    seeded: bool,
    principal_ref: String,
    recovery_code: Option<String>,
}

/// Seed the identity, recovery code, finite holding-owner assignment, and
/// owner-seed state in one control write transaction. The plaintext recovery
/// code is retained only in the returned response value and is never logged or
/// written to any database/read projection.
async fn seed_tenant_owner(
    store: &SqlxControlPlane,
    seed: &TenantOwnerSeed,
    grantor_ref: &str,
    at: DateTime<Utc>,
) -> Result<TenantOwnerSeedReceipt, Status> {
    if seed.state == "seeded" {
        return Ok(TenantOwnerSeedReceipt {
            seeded: true,
            principal_ref: seed.principal_ref.clone(),
            recovery_code: None,
        });
    }
    if seed.state != "pending" {
        return Err(Status::failed_precondition(
            "tenant owner seed is in an invalid state",
        ));
    }
    let recovery_batch = RecoveryCodeSet::issue(
        seed.principal_ref.clone(),
        1,
        &PasswordService::default(),
        at,
    )
    .map_err(|_| Status::internal("owner recovery material could not be generated"))?;
    let recovery_code = recovery_batch.code(0).map(str::to_owned);
    let role_assignment = RoleAssignment {
        id: seed.role_assignment_id,
        principal_ref: seed.principal_ref.clone(),
        role_key: HOLDING_OWNER_ROLE.into(),
        permissions: vec![
            "organization.admin".into(),
            "organization.read".into(),
            "operations.read".into(),
            "operations.exception.manage".into(),
            "role.assignment.read".into(),
            "role.assignment.approve".into(),
            "role.assignment.review".into(),
            "role.assignment.renew".into(),
            "role.assignment.recover".into(),
        ],
        functional_domain: "organization".into(),
        authority_level: 100,
        approval_limit_minor: 0,
        scope: Scope::Holding {
            holding_id: seed.holding_id,
        },
        valid_from: at,
        valid_until: at + Duration::days(90),
        timezone: "Asia/Bangkok".into(),
        risk_tier: RiskTier::Administrator,
        grant_reason: "initial holding owner bootstrap".into(),
        ticket_reference: None,
        grantor: grantor_ref.into(),
        approver: Some(seed.approver_ref.clone()),
        renewal_reviewer: None,
        parent_assignment_id: None,
        assignment_version: 1,
    };
    role_assignment
        .validate_for_grant(None)
        .map_err(|_| Status::failed_precondition("initial owner assignment is invalid"))?;
    let role_payload = serde_json::to_value(&role_assignment)
        .map_err(|_| Status::internal("owner assignment could not be serialized"))?;

    let mut transaction = store
        .pools()
        .write
        .begin()
        .await
        .map_err(|_| Status::unavailable("tenant owner seed transaction unavailable"))?;
    let seed_row = sqlx::query(
        "SELECT holding_ref, holding_id, principal_ref, approver_ref,
                role_assignment_id, state, recovery_issued_at
         FROM erpx_control.tenant_owner_seeds
         WHERE tenant_id = $1
         FOR UPDATE",
    )
    .bind(&seed.tenant_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| Status::unavailable("tenant owner seed transaction unavailable"))?
    .ok_or_else(|| Status::failed_precondition("tenant owner seed is missing"))?;
    let stored_state: String = seed_row
        .try_get("state")
        .map_err(|_| Status::internal("tenant owner seed record is invalid"))?;
    let stored_recovery_issued_at: Option<DateTime<Utc>> =
        seed_row
            .try_get("recovery_issued_at")
            .map_err(|_| Status::internal("tenant owner seed record is invalid"))?;
    let immutable_match = seed_row
        .try_get::<String, _>("holding_ref")
        .map(|value| value == seed.holding_ref)
        .unwrap_or(false)
        && seed_row
            .try_get::<Uuid, _>("holding_id")
            .map(|value| value == seed.holding_id)
            .unwrap_or(false)
        && seed_row
            .try_get::<String, _>("principal_ref")
            .map(|value| value == seed.principal_ref)
            .unwrap_or(false)
        && seed_row
            .try_get::<String, _>("approver_ref")
            .map(|value| value == seed.approver_ref)
            .unwrap_or(false)
        && seed_row
            .try_get::<Uuid, _>("role_assignment_id")
            .map(|value| value == seed.role_assignment_id)
            .unwrap_or(false);
    if !immutable_match {
        return Err(Status::failed_precondition(
            "tenant owner seed immutable references changed",
        ));
    }
    if stored_state == "seeded" || stored_recovery_issued_at.is_some() {
        transaction
            .commit()
            .await
            .map_err(|_| Status::unavailable("tenant owner seed transaction unavailable"))?;
        return Ok(TenantOwnerSeedReceipt {
            seeded: stored_state == "seeded",
            principal_ref: seed.principal_ref.clone(),
            recovery_code: None,
        });
    }

    let generated_principal_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:principal:v1:{}", seed.principal_ref).as_bytes(),
    );
    sqlx::query(
        "INSERT INTO erpx_identity.principals
            (id, principal_ref, status, created_at, updated_at)
         VALUES ($1, $2, 'recovery_required', $3, $3)
         ON CONFLICT (principal_ref) DO NOTHING",
    )
    .bind(generated_principal_id)
    .bind(&seed.principal_ref)
    .bind(at)
    .execute(&mut *transaction)
    .await
    .map_err(|_| Status::unavailable("owner identity write unavailable"))?;
    let principal_row = sqlx::query(
        "SELECT id, status
         FROM erpx_identity.principals
         WHERE principal_ref = $1
         FOR UPDATE",
    )
    .bind(&seed.principal_ref)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| Status::unavailable("owner identity write unavailable"))?;
    let principal_id: Uuid = principal_row
        .try_get("id")
        .map_err(|_| Status::internal("owner identity record is invalid"))?;
    let principal_status: String = principal_row
        .try_get("status")
        .map_err(|_| Status::internal("owner identity record is invalid"))?;
    if principal_status == "suspended" {
        return Err(Status::failed_precondition("owner identity is suspended"));
    }
    let needs_recovery = principal_status == "recovery_required";
    if needs_recovery {
        sqlx::query("DELETE FROM erpx_identity.recovery_codes WHERE principal_id = $1")
            .bind(principal_id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| Status::unavailable("owner recovery write unavailable"))?;
        for record in recovery_batch.set().records() {
            sqlx::query(
                "INSERT INTO erpx_identity.recovery_codes
                    (id, principal_id, code_hash, issued_at, consumed_at)
                 VALUES ($1, $2, $3, $4, NULL)",
            )
            .bind(record.id())
            .bind(principal_id)
            .bind(record.hash().as_str())
            .bind(recovery_batch.set().issued_at())
            .execute(&mut *transaction)
            .await
            .map_err(|_| Status::unavailable("owner recovery write unavailable"))?;
        }
    }

    sqlx::query(
        "INSERT INTO erpx_control.role_assignments
            (id, principal_ref, role_key, parent_assignment_id, valid_from,
             valid_until, assignment_version, payload)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(role_assignment.id)
    .bind(&role_assignment.principal_ref)
    .bind(&role_assignment.role_key)
    .bind(role_assignment.parent_assignment_id)
    .bind(role_assignment.valid_from)
    .bind(role_assignment.valid_until)
    .bind(
        i32::try_from(role_assignment.assignment_version)
            .map_err(|_| Status::internal("owner assignment version is invalid"))?,
    )
    .bind(role_payload)
    .execute(&mut *transaction)
    .await
    .map_err(|_| Status::unavailable("owner role write unavailable"))?;
    let stored_role_payload: Value =
        sqlx::query("SELECT payload FROM erpx_control.role_assignments WHERE id = $1")
            .bind(role_assignment.id)
            .fetch_one(&mut *transaction)
            .await
            .and_then(|row| row.try_get("payload"))
            .map_err(|_| Status::unavailable("owner role read unavailable"))?;
    let stored_assignment: RoleAssignment = serde_json::from_value(stored_role_payload)
        .map_err(|_| Status::failed_precondition("owner role payload is invalid"))?;
    if stored_assignment.canonical_bytes().ok() != role_assignment.canonical_bytes().ok() {
        return Err(Status::failed_precondition(
            "owner role assignment conflicts with existing state",
        ));
    }
    sqlx::query(
        "UPDATE erpx_control.tenant_owner_seeds
         SET state = 'seeded',
             recovery_issued_at = CASE WHEN $2 THEN $3 ELSE NULL END,
             updated_at = $4
         WHERE tenant_id = $1",
    )
    .bind(&seed.tenant_id)
    .bind(needs_recovery)
    .bind(if needs_recovery { Some(at) } else { None })
    .bind(at)
    .execute(&mut *transaction)
    .await
    .map_err(|_| Status::unavailable("tenant owner seed update unavailable"))?;
    transaction
        .commit()
        .await
        .map_err(|_| Status::unavailable("tenant owner seed transaction unavailable"))?;
    Ok(TenantOwnerSeedReceipt {
        seeded: true,
        principal_ref: seed.principal_ref.clone(),
        recovery_code: if needs_recovery { recovery_code } else { None },
    })
}

#[derive(Clone)]
struct PlatformOperationsRpcImpl {
    state: AppState,
}

#[tonic::async_trait]
impl PlatformOperationsRpc for PlatformOperationsRpcImpl {
    async fn list_tenants(
        &self,
        request: Request<ListPlatformTenantsRequest>,
    ) -> Result<Response<ListPlatformTenantsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            require_platform_scope(&scope)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_TENANT_READ_PERMISSION,
                )
                .await?;

            let page_size = platform_tenant_page_size(request.get_ref().page_size)?;
            let after_tenant_id = (!request.get_ref().after_tenant_id.trim().is_empty())
                .then_some(request.get_ref().after_tenant_id.trim());
            let registry = SqlxTenantStoreRegistry::new(self.state.store.clone());
            let page = registry
                .list_page(after_tenant_id, page_size)
                .await
                .map_err(|_| Status::internal("platform tenant registry read failed"))?;
            let provisioner = TenantProvisioner::new(self.state.store.clone());
            let mut tenants = Vec::with_capacity(page.registrations.len());
            for registration in page.registrations {
                let readiness = provisioner
                    .observe_tenant_readiness_metadata(
                        &registration.tenant_id,
                        Utc::now(),
                        Default::default(),
                    )
                    .await
                    .map_err(|_| Status::internal("platform tenant readiness read failed"))?;
                let binding = TenantBinding::new(
                    registration.tenant_id.clone(),
                    registration.holding_ref.clone(),
                )
                .map_err(|_| Status::internal("platform tenant binding is invalid"))?;
                let reconciliation = self
                    .state
                    .platform_completion
                    .read_reconciliation_ui(
                        &binding,
                        platform_read_authorization(&claims.principal_ref, Utc::now()),
                    )
                    .await?
                    .into_inner();
                let projection = reconciliation
                    .as_ref()
                    .map(|report| {
                        if report.stale {
                            "stale"
                        } else {
                            report.state.as_str()
                        }
                    })
                    .unwrap_or("unknown");
                tenants.push(PlatformTenantSummary {
                    tenant_id: registration.tenant_id,
                    state: registration.state,
                    registry_version: registration.registry_version,
                    routing: format!(
                        "write:{};read:{}",
                        readiness.write_database.state.as_str(),
                        readiness.read_database.state.as_str()
                    ),
                    projection: projection.into(),
                    backup_pitr: readiness.backup_pitr.state.as_str().into(),
                    storage: readiness.storage.state.as_str().into(),
                });
            }
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.tenant.list".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "platform operations tenant list".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("platform tenant audit failed"))?;
            Ok(Response::new(ListPlatformTenantsResponse {
                tenants,
                has_more: page.has_more,
                next_after_tenant_id: page.next_after_tenant_id.unwrap_or_default(),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn get_tenant(
        &self,
        request: Request<GetPlatformTenantRequest>,
    ) -> Result<Response<GetPlatformTenantResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            require_platform_scope(&scope)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_TENANT_READ_PERMISSION,
                )
                .await?;
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let registry = SqlxTenantStoreRegistry::new(self.state.store.clone());
            let registration = registry
                .get_detail(tenant_id)
                .await
                .map_err(|_| Status::internal("platform tenant registry read failed"))?
                .ok_or_else(|| Status::not_found("platform tenant not found"))?;
            let readiness = TenantProvisioner::new(self.state.store.clone())
                .observe_tenant_readiness_metadata(
                    &registration.tenant_id,
                    Utc::now(),
                    Default::default(),
                )
                .await
                .map_err(|_| Status::internal("platform tenant readiness read failed"))?;
            let binding = TenantBinding::new(
                registration.tenant_id.clone(),
                registration.holding_ref.clone(),
            )
            .map_err(|_| Status::internal("platform tenant binding is invalid"))?;
            let reconciliation = self
                .state
                .platform_completion
                .read_reconciliation_ui(
                    &binding,
                    platform_read_authorization(&claims.principal_ref, Utc::now()),
                )
                .await?
                .into_inner();
            let projection = reconciliation
                .as_ref()
                .map(|report| {
                    if report.stale {
                        "stale"
                    } else {
                        report.state.as_str()
                    }
                })
                .unwrap_or("unknown")
                .to_owned();
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.tenant.get".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "platform operations tenant detail".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("platform tenant audit failed"))?;
            Ok(Response::new(GetPlatformTenantResponse {
                tenant: Some(PlatformTenantSummary {
                    tenant_id: registration.tenant_id,
                    state: registration.state,
                    registry_version: registration.registry_version,
                    routing: format!(
                        "write:{};read:{}",
                        readiness.write_database.state.as_str(),
                        readiness.read_database.state.as_str()
                    ),
                    projection,
                    backup_pitr: readiness.backup_pitr.state.as_str().into(),
                    storage: readiness.storage.state.as_str().into(),
                }),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn begin_tenant_provisioning(
        &self,
        request: Request<BeginTenantProvisioningRequest>,
    ) -> Result<Response<BeginTenantProvisioningResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &context.actor.principal_ref,
                    &scope,
                    PLATFORM_TENANT_PROVISION_PERMISSION,
                )
                .await?;
            let owner_principal_ref = validate_transport_reference(
                &request.get_ref().owner_principal_ref,
                "owner_principal_ref",
            )?
            .to_owned();
            let owner_approver_ref = validate_transport_reference(
                &request.get_ref().owner_approver_ref,
                "owner_approver_ref",
            )?
            .to_owned();
            self.state
                .auth
                .authorize_independent_role(
                    &self.state.store,
                    &owner_approver_ref,
                    &Scope::Platform,
                    PLATFORM_TENANT_PROVISION_PERMISSION,
                    &context.actor.principal_ref,
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(
                    &claims,
                    command_mfa_code(request.get_ref().context.as_ref()),
                )
                .await?;
            let input = request.into_inner();
            let registration = TenantStoreRegistration {
                tenant_id: validate_transport_reference(&input.tenant_id, "tenant_id")?.into(),
                holding_ref: validate_transport_reference(&input.holding_ref, "holding_ref")?
                    .into(),
                write_database_key: validate_transport_reference(
                    &input.write_database_key,
                    "write_database_key",
                )?
                .into(),
                read_database_key: validate_transport_reference(
                    &input.read_database_key,
                    "read_database_key",
                )?
                .into(),
                storage_namespace: validate_transport_reference(
                    &input.storage_namespace,
                    "storage_namespace",
                )?
                .into(),
                credential_key_ref: validate_transport_reference(
                    &input.credential_key_ref,
                    "credential_key_ref",
                )?
                .into(),
                backup_chain_ref: validate_transport_reference(
                    &input.backup_chain_ref,
                    "backup_chain_ref",
                )?
                .into(),
                state: "provisioning".into(),
                registry_version: 1,
            };
            let tenant_id = registration.tenant_id.clone();
            TenantProvisioner::new(self.state.store.clone())
                .begin(registration)
                .await
                .map_err(|_| Status::failed_precondition("tenant provisioning request rejected"))?;
            TenantProvisioner::new(self.state.store.clone())
                .register_owner_seed(
                    tenant_id.as_str(),
                    &owner_principal_ref,
                    &owner_approver_ref,
                )
                .await
                .map_err(|_| Status::failed_precondition("tenant owner seed was rejected"))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: context.actor.principal_ref,
                    customer_account_id: None,
                    action: "platform.tenant.provision.begin".into(),
                    purpose: context.audit.purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("platform audit persistence unavailable"))?;
            Ok(Response::new(BeginTenantProvisioningResponse {
                tenant_id,
                state: "provisioning".into(),
                registry_version: 1,
                accepted: true,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn provision_tenant_resources(
        &self,
        request: Request<ProvisionTenantResourcesRequest>,
    ) -> Result<Response<ProvisionTenantResourcesResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &context.actor.principal_ref,
                    &scope,
                    PLATFORM_TENANT_PROVISION_PERMISSION,
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(
                    &claims,
                    command_mfa_code(request.get_ref().context.as_ref()),
                )
                .await?;
            let input = request.into_inner();
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let owner_seed = TenantProvisioner::new(self.state.store.clone())
                .get_owner_seed(tenant_id)
                .await
                .map_err(|_| Status::unavailable("tenant owner seed unavailable"))?
                .ok_or_else(|| Status::failed_precondition("tenant owner seed is missing"))?;
            let database_config = TenantDatabaseProvisioningConfig::from_env().map_err(|_| {
                Status::unavailable("tenant provisioning configuration unavailable")
            })?;
            let provisioner =
                TenantDatabaseProvisioner::new(self.state.store.clone(), database_config).map_err(
                    |_| Status::unavailable("tenant provisioning configuration unavailable"),
                )?;
            provisioner
                .provision_databases(tenant_id)
                .await
                .map_err(|_| Status::unavailable("tenant database provisioning unavailable"))?;
            provisioner
                .initialize_tenant_organization(
                    tenant_id,
                    owner_seed.holding_id,
                    &owner_seed.principal_ref,
                )
                .await
                .map_err(|_| {
                    Status::unavailable("tenant organization initialization unavailable")
                })?;
            let storage = self.state.storage.clone();
            let storage_health = provisioner
                .provision_storage_namespace(tenant_id, move |namespace| async move {
                    storage.ensure_namespace(&namespace).await
                })
                .await
                .map_err(|_| Status::unavailable("tenant storage provisioning unavailable"))?;
            let registration = SqlxTenantStoreRegistry::new(self.state.store.clone())
                .get(tenant_id)
                .await
                .map_err(|_| Status::unavailable("tenant registry unavailable"))?
                .ok_or_else(|| Status::not_found("tenant not found"))?;
            let owner_receipt = seed_tenant_owner(
                &self.state.store,
                &owner_seed,
                &context.actor.principal_ref,
                Utc::now(),
            )
            .await?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: context.actor.principal_ref.clone(),
                    customer_account_id: None,
                    action: "platform.tenant.owner.seed".into(),
                    purpose: "tenant owner bootstrap".into(),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("platform audit persistence unavailable"))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: context.actor.principal_ref,
                    customer_account_id: None,
                    action: "platform.tenant.provision.resources".into(),
                    purpose: context.audit.purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("platform audit persistence unavailable"))?;
            Ok(Response::new(ProvisionTenantResourcesResponse {
                tenant_id: tenant_id.into(),
                state: registration.state,
                databases_ready: true,
                storage_ready: storage_health.reachable,
                correlation_id: context.correlation_id.to_string(),
                owner_seeded: owner_receipt.seeded,
                owner_principal_ref: owner_receipt.principal_ref,
                owner_recovery_code: owner_receipt.recovery_code.unwrap_or_default(),
            }))
        })
    }

    async fn activate_tenant(
        &self,
        request: Request<TransitionTenantStateRequest>,
    ) -> Result<Response<TransitionTenantStateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &context.actor.principal_ref,
                    &scope,
                    PLATFORM_TENANT_PROVISION_PERMISSION,
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(
                    &claims,
                    command_mfa_code(request.get_ref().context.as_ref()),
                )
                .await?;
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let registration = SqlxTenantStoreRegistry::new(self.state.store.clone())
                .get(tenant_id)
                .await
                .map_err(|_| Status::unavailable("tenant registry unavailable"))?
                .ok_or_else(|| Status::not_found("tenant not found"))?;
            let at = Utc::now();
            let receipt = self
                .state
                .platform_completion
                .transition_lifecycle(PlatformLifecycleApiRequest {
                    command: TenantLifecycleCommand {
                        binding: TenantBinding::new(
                            tenant_id.to_owned(),
                            registration.holding_ref.clone(),
                        )
                        .map_err(|_| Status::failed_precondition("tenant binding is invalid"))?,
                        action: TenantLifecycleAction::Activate,
                        actor_ref: context.actor.principal_ref.clone(),
                        correlation_id: context.correlation_id,
                        idempotency_key: context.idempotency_key.clone(),
                        expected_registry_version: Some(registration.registry_version),
                        at,
                    },
                    proof: platform_authorization_proof(
                        &context.actor.principal_ref,
                        PLATFORM_TENANT_PROVISION_PERMISSION,
                        scope,
                        at,
                    ),
                })
                .await?
                .into_inner();
            Ok(Response::new(TransitionTenantStateResponse {
                tenant_id: receipt.tenant_id,
                state: receipt.state.as_str().into(),
                registry_version: receipt.registry_version,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn suspend_tenant(
        &self,
        request: Request<TransitionTenantStateRequest>,
    ) -> Result<Response<TransitionTenantStateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &context.actor.principal_ref,
                    &scope,
                    PLATFORM_TENANT_SUSPEND_PERMISSION,
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(
                    &claims,
                    command_mfa_code(request.get_ref().context.as_ref()),
                )
                .await?;
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let registration = SqlxTenantStoreRegistry::new(self.state.store.clone())
                .get(tenant_id)
                .await
                .map_err(|_| Status::unavailable("tenant registry unavailable"))?
                .ok_or_else(|| Status::not_found("tenant not found"))?;
            let at = Utc::now();
            let receipt = self
                .state
                .platform_completion
                .transition_lifecycle(PlatformLifecycleApiRequest {
                    command: TenantLifecycleCommand {
                        binding: TenantBinding::new(
                            tenant_id.to_owned(),
                            registration.holding_ref.clone(),
                        )
                        .map_err(|_| Status::failed_precondition("tenant binding is invalid"))?,
                        action: TenantLifecycleAction::Suspend,
                        actor_ref: context.actor.principal_ref.clone(),
                        correlation_id: context.correlation_id,
                        idempotency_key: context.idempotency_key.clone(),
                        expected_registry_version: Some(registration.registry_version),
                        at,
                    },
                    proof: platform_authorization_proof(
                        &context.actor.principal_ref,
                        PLATFORM_TENANT_SUSPEND_PERMISSION,
                        scope,
                        at,
                    ),
                })
                .await?
                .into_inner();
            Ok(Response::new(TransitionTenantStateResponse {
                tenant_id: receipt.tenant_id,
                state: receipt.state.as_str().into(),
                registry_version: receipt.registry_version,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn enter_tenant_recovery(
        &self,
        request: Request<TransitionTenantStateRequest>,
    ) -> Result<Response<TransitionTenantStateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &context.actor.principal_ref,
                    &scope,
                    PLATFORM_TENANT_RECOVERY_PERMISSION,
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(
                    &claims,
                    command_mfa_code(request.get_ref().context.as_ref()),
                )
                .await?;
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let registration = SqlxTenantStoreRegistry::new(self.state.store.clone())
                .get(tenant_id)
                .await
                .map_err(|_| Status::unavailable("tenant registry unavailable"))?
                .ok_or_else(|| Status::not_found("tenant not found"))?;
            let at = Utc::now();
            let receipt = self
                .state
                .platform_completion
                .transition_lifecycle(PlatformLifecycleApiRequest {
                    command: TenantLifecycleCommand {
                        binding: TenantBinding::new(
                            tenant_id.to_owned(),
                            registration.holding_ref.clone(),
                        )
                        .map_err(|_| Status::failed_precondition("tenant binding is invalid"))?,
                        action: TenantLifecycleAction::EnterRecovery,
                        actor_ref: context.actor.principal_ref.clone(),
                        correlation_id: context.correlation_id,
                        idempotency_key: context.idempotency_key.clone(),
                        expected_registry_version: Some(registration.registry_version),
                        at,
                    },
                    proof: platform_authorization_proof(
                        &context.actor.principal_ref,
                        PLATFORM_TENANT_RECOVERY_PERMISSION,
                        scope,
                        at,
                    ),
                })
                .await?
                .into_inner();
            Ok(Response::new(TransitionTenantStateResponse {
                tenant_id: receipt.tenant_id,
                state: receipt.state.as_str().into(),
                registry_version: receipt.registry_version,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn schedule_tenant_backup(
        &self,
        request: Request<ScheduleTenantBackupRequest>,
    ) -> Result<Response<ScheduleTenantBackupResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &context.actor.principal_ref,
                    &scope,
                    PLATFORM_TENANT_BACKUP_PERMISSION,
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(
                    &claims,
                    command_mfa_code(request.get_ref().context.as_ref()),
                )
                .await?;
            let input = request.into_inner();
            let available_at = if input.available_at.trim().is_empty() {
                Utc::now()
            } else {
                DateTime::parse_from_rfc3339(&input.available_at)
                    .map_err(|_| Status::invalid_argument("available_at is invalid"))?
                    .with_timezone(&Utc)
            };
            let max_attempts = u16::try_from(input.max_attempts)
                .map_err(|_| Status::invalid_argument("max_attempts is invalid"))?;
            let pitr_recovery_point = input
                .pitr_recovery_point
                .map(|point| -> Result<PitrRecoveryPoint, Status> {
                    let target_time = (!point.target_time.trim().is_empty())
                        .then(|| {
                            DateTime::parse_from_rfc3339(&point.target_time)
                                .map(|value| value.with_timezone(&Utc))
                                .map_err(|_| {
                                    Status::invalid_argument("PITR target_time is invalid")
                                })
                        })
                        .transpose()?;
                    Ok(PitrRecoveryPoint {
                        target_time,
                        target_lsn: (!point.target_lsn.trim().is_empty())
                            .then(|| point.target_lsn.trim().to_owned()),
                        wal_archive_ref: validate_transport_reference(
                            &point.wal_archive_ref,
                            "wal_archive_ref",
                        )?
                        .into(),
                        physical_restore_target_ref: validate_transport_reference(
                            &point.physical_restore_target_ref,
                            "physical_restore_target_ref",
                        )?
                        .into(),
                    })
                })
                .transpose()?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let kind = match validate_transport_reference(&input.kind, "kind")? {
                "full" => PlatformBackupKind::Full,
                "restore_test" => PlatformBackupKind::RestoreTest,
                "pitr" => PlatformBackupKind::Pitr,
                _ => return Err(Status::invalid_argument("backup kind is unsupported")),
            };
            let source_projection = validate_transport_reference(
                if input.source_projection.trim().is_empty() {
                    "operations"
                } else {
                    &input.source_projection
                },
                "source_projection",
            )?;
            let at = Utc::now();
            let receipt = self
                .state
                .platform_completion
                .schedule_backup(PlatformBackupApiRequest {
                    command: BackupControlCommand {
                        binding: TenantBinding::new(tenant_id.to_owned(), holding_ref.to_owned())
                            .map_err(|_| {
                            Status::failed_precondition("tenant binding is invalid")
                        })?,
                        kind,
                        backup_ref: validate_transport_reference(&input.backup_ref, "backup_ref")?
                            .to_owned(),
                        restore_target_ref: (!input.restore_target_ref.trim().is_empty())
                            .then(|| input.restore_target_ref.trim().to_owned()),
                        candidate_read_database_key: (!input
                            .candidate_read_database_key
                            .trim()
                            .is_empty())
                        .then(|| input.candidate_read_database_key.trim().to_owned()),
                        source_version: (input.source_version != 0).then_some(input.source_version),
                        source_projection: source_projection.to_owned(),
                        pitr_recovery_point,
                        available_at,
                        max_attempts,
                        actor_ref: context.actor.principal_ref.clone(),
                        correlation_id: context.correlation_id,
                        idempotency_key: validate_transport_reference(
                            &input.idempotency_key,
                            "idempotency_key",
                        )?
                        .to_owned(),
                        at,
                    },
                    proof: platform_authorization_proof(
                        &context.actor.principal_ref,
                        PLATFORM_TENANT_BACKUP_PERMISSION,
                        scope,
                        at,
                    ),
                })
                .await?
                .into_inner();
            Ok(Response::new(ScheduleTenantBackupResponse {
                job_id: receipt.id.to_string(),
                tenant_id: receipt.tenant_id,
                kind: receipt.kind,
                state: receipt.state,
                available_at: receipt.available_at.to_rfc3339(),
                max_attempts: u32::from(receipt.max_attempts),
                duplicate: receipt.duplicate,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn list_tenant_backup_jobs(
        &self,
        request: Request<ListTenantBackupJobsRequest>,
    ) -> Result<Response<ListTenantBackupJobsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            require_platform_scope(&scope)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_TENANT_READ_PERMISSION,
                )
                .await?;
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let page_size = platform_tenant_page_size(request.get_ref().page_size)?;
            let jobs = TenantProvisioner::new(self.state.store.clone())
                .list_backup_jobs(tenant_id, page_size)
                .await
                .map_err(|_| Status::internal("backup job history read failed"))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.tenant.backup.list".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "platform tenant backup history".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("platform audit persistence unavailable"))?;
            Ok(Response::new(ListTenantBackupJobsResponse {
                jobs: jobs
                    .into_iter()
                    .map(|job| TenantBackupJobSummaryMessage {
                        job_id: job.id.to_string(),
                        tenant_id: job.tenant_id,
                        kind: job.kind,
                        state: job.state,
                        available_at: job.available_at.to_rfc3339(),
                        attempts: u32::from(job.attempts),
                        max_attempts: u32::from(job.max_attempts),
                        verified: job.verified,
                        error_present: job.last_error_present,
                        created_at: job.created_at.to_rfc3339(),
                        updated_at: job.updated_at.to_rfc3339(),
                    })
                    .collect(),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn start_projection_rebuild(
        &self,
        request: Request<StartProjectionRebuildRequest>,
    ) -> Result<Response<StartProjectionRebuildResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &context.actor.principal_ref,
                    &scope,
                    PLATFORM_PROJECTION_REBUILD_PERMISSION,
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(
                    &claims,
                    command_mfa_code(request.get_ref().context.as_ref()),
                )
                .await?;
            let input = request.into_inner();
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let projection_name = projection_rebuild_name(&input.projection_name)?;
            let candidate_read_database_key = validate_transport_reference(
                &input.candidate_read_database_key,
                "candidate_read_database_key",
            )?;
            let source_version = projection_rebuild_source_version(input.source_version)?;
            let registry = Arc::new(SqlxTenantStoreRegistry::new(self.state.store.clone()));
            let registration = match registry.get(tenant_id).await {
                Ok(Some(registration)) if registration.state == "active" => registration,
                Ok(Some(_)) => {
                    return Err(Status::failed_precondition(
                        "projection rebuild tenant is not active",
                    ));
                }
                Ok(None) => return Err(Status::not_found("projection rebuild tenant not found")),
                Err(_) => return Err(Status::unavailable("tenant registry unavailable")),
            };
            let router = self
                .state
                .tenant_router
                .as_deref()
                .ok_or_else(|| Status::unavailable("tenant routing is not configured"))?;
            let observed_source_version =
                operations_source_high_watermark(router, tenant_id, &registration.holding_ref)
                    .await
                    .map_err(|_| Status::unavailable("Operations source is unavailable"))?;
            if observed_source_version != source_version {
                return Err(Status::failed_precondition(
                    "projection source_version is no longer current",
                ));
            }
            let binding = TenantHoldingBinding::new(
                registration.tenant_id.clone(),
                registration.holding_ref.clone(),
            )
            .map_err(|_| Status::failed_precondition("tenant projection binding is invalid"))?;
            let scheduler_authorization = ProjectionSchedulerAuthorization::new(
                claims.principal_ref.clone(),
                context.audit.purpose.clone(),
                context.correlation_id,
            )
            .map_err(|_| Status::internal("projection scheduler authorization is invalid"))?;
            let scheduler = TenantProjectionSchedulerRepository::new(
                self.state.store.pools().write.clone(),
                registry,
                scheduler_authorization,
            );
            let policy = ProjectionPolicy::new(MAX_OPERATIONS_EVENT_PAGE_SIZE, 1, 5, 5, 300)
                .map_err(|_| Status::internal("projection scheduler policy is invalid"))?;
            let (generation, _rebuild) = scheduler
                .begin_rebuild_after_platform_authorization(
                    binding,
                    projection_name,
                    candidate_read_database_key,
                    source_version,
                    policy,
                    Utc::now(),
                )
                .await
                .map_err(|_| Status::unavailable("projection rebuild could not be scheduled"))?;
            Ok(Response::new(StartProjectionRebuildResponse {
                tenant_id: tenant_id.into(),
                projection_name: projection_name.into(),
                state: "rebuilding".into(),
                source_version,
                correlation_id: context.correlation_id.to_string(),
                generation,
            }))
        })
    }

    async fn activate_projection_rebuild(
        &self,
        request: Request<ActivateProjectionRebuildRequest>,
    ) -> Result<Response<ActivateProjectionRebuildResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &context.actor.principal_ref,
                    &scope,
                    PLATFORM_PROJECTION_REBUILD_PERMISSION,
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(
                    &claims,
                    command_mfa_code(request.get_ref().context.as_ref()),
                )
                .await?;
            let input = request.into_inner();
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let projection_name = projection_rebuild_name(&input.projection_name)?;
            let candidate_read_database_key = validate_transport_reference(
                &input.candidate_read_database_key,
                "candidate_read_database_key",
            )?;
            let generation = projection_rebuild_generation(input.generation)?;
            let router = self
                .state
                .tenant_router
                .as_deref()
                .ok_or_else(|| Status::unavailable("tenant routing is not configured"))?;
            let registry = Arc::new(SqlxTenantStoreRegistry::new(Arc::clone(&self.state.store)));
            let registration = registry
                .get(tenant_id)
                .await
                .map_err(|_| Status::unavailable("tenant registry unavailable"))?
                .ok_or_else(|| Status::failed_precondition("tenant is not registered"))?;
            if projection_name != OPERATIONS_READ_PROJECTION {
                return Err(Status::invalid_argument(
                    "projection activation is not supported for this projection",
                ));
            }
            let source_version = router
                .replacement_read_source_version(tenant_id, candidate_read_database_key, generation)
                .await
                .map_err(|_| Status::failed_precondition("projection generation is not ready"))?
                .ok_or_else(|| Status::failed_precondition("projection generation is not ready"))?;
            let write = router
                .route(
                    tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| Status::unavailable("tenant write database unavailable"))?;
            let mut source_lock = match write.acquire_source_rebuild_lock().await {
                Ok(lock) => lock,
                Err(_) => return Err(Status::unavailable("projection source fence unavailable")),
            };
            let current_source_version = source_lock
                .source_event_high_watermark(
                    TenantSourceProjection::Operations,
                    &registration.holding_ref,
                )
                .await
                .map_err(|_| Status::unavailable("Operations source is unavailable"));
            let current_source_version = match current_source_version {
                Ok(version) => version,
                Err(error) => {
                    if let Err(error) = source_lock.rollback().await {
                        tracing::error!(?error, "projection source-fence rollback failed");
                    }
                    write.close().await;
                    return Err(error);
                }
            };
            if current_source_version != source_version {
                if let Err(error) = source_lock.rollback().await {
                    tracing::error!(?error, "projection source-fence rollback failed");
                }
                write.close().await;
                router
                    .fail_replacement_read_database(
                        tenant_id,
                        candidate_read_database_key,
                        generation,
                        "source moved before manual activation",
                    )
                    .await
                    .map_err(|_| Status::unavailable("projection stale transition failed"))?;
                let binding = TenantHoldingBinding::new(
                    registration.tenant_id.clone(),
                    registration.holding_ref.clone(),
                )
                .map_err(|_| Status::failed_precondition("tenant projection binding is invalid"))?;
                let scheduler_authorization = ProjectionSchedulerAuthorization::new(
                    claims.principal_ref.clone(),
                    context.audit.purpose.clone(),
                    context.correlation_id,
                )
                .map_err(|_| Status::internal("projection scheduler authorization is invalid"))?;
                let scheduler = TenantProjectionSchedulerRepository::new(
                    self.state.store.pools().write.clone(),
                    registry,
                    scheduler_authorization,
                );
                scheduler
                    .terminalize_stale_rebuild(
                        &binding,
                        projection_name,
                        candidate_read_database_key,
                        source_version,
                        generation,
                        Utc::now(),
                    )
                    .await
                    .map_err(|_| Status::unavailable("stale projection job transition failed"))?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: None,
                        action: "platform.projection_rebuild.stale_source".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("platform audit persistence unavailable"))?;
                return Err(Status::failed_precondition(
                    "projection source_version is no longer current",
                ));
            }
            if router
                .activate_replacement_read_database(
                    tenant_id,
                    candidate_read_database_key,
                    generation,
                )
                .await
                .is_err()
            {
                if let Err(error) = source_lock.rollback().await {
                    tracing::error!(?error, "projection source-fence rollback failed");
                }
                write.close().await;
                return Err(Status::failed_precondition(
                    "projection activation rejected",
                ));
            }
            if source_lock.commit().await.is_err() {
                write.close().await;
                return Err(Status::unavailable(
                    "projection activation acknowledgement failed",
                ));
            }
            write.close().await;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.projection_rebuild.activate".into(),
                    purpose: context.audit.purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("platform audit persistence unavailable"))?;
            Ok(Response::new(ActivateProjectionRebuildResponse {
                tenant_id: tenant_id.into(),
                projection_name: projection_name.into(),
                state: "activated".into(),
                generation,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn list_projection_statuses(
        &self,
        request: Request<ListProjectionStatusesRequest>,
    ) -> Result<Response<ListProjectionStatusesResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            require_platform_scope(&scope)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_TENANT_READ_PERMISSION,
                )
                .await?;

            let input = request.get_ref();
            let tenant_id = (!input.tenant_id.trim().is_empty())
                .then(|| validate_transport_reference(&input.tenant_id, "tenant_id"))
                .transpose()?;
            let holding_ref = (!input.holding_ref.trim().is_empty())
                .then(|| validate_transport_reference(&input.holding_ref, "holding_ref"))
                .transpose()?;
            if holding_ref.is_some() && tenant_id.is_none() {
                return Err(Status::invalid_argument("holding_ref requires tenant_id"));
            }
            let projection_name = (!input.projection_name.trim().is_empty())
                .then(|| validate_transport_reference(&input.projection_name, "projection_name"))
                .transpose()?;
            let after = if input.after_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_projection_status_cursor(&input.after_cursor)?)
            };
            let page_size = projection_status_page_size(input.page_size)?;
            let purpose = proto_context
                .audit
                .as_ref()
                .map(|audit| audit.purpose.clone())
                .unwrap_or_else(|| "platform projection status read".into());
            let status_read =
                TenantProjectionStatusReadRepository::new(self.state.store.pools().read.clone());
            let page = status_read
                .list_status_page(
                    tenant_id,
                    holding_ref,
                    projection_name,
                    after.as_ref(),
                    page_size,
                )
                .await
                .map_err(|_| Status::internal("projection status read failed"))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.projection_status.list".into(),
                    purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("platform audit persistence unavailable"))?;
            let freshness_now = Utc::now();
            let statuses = page
                .statuses
                .into_iter()
                .map(|status| {
                    let freshness = projection_status_freshness(status.published_at, freshness_now);
                    ProjectionStatusMessage {
                        tenant_id: status.tenant_id,
                        holding_ref: status.holding_ref,
                        projection_name: status.projection_name,
                        job_key: status.job_key,
                        job_id: status.job_id.to_string(),
                        kind: match status.kind {
                            erpx_infrastructure::ProjectionJobKind::Incremental => "incremental",
                            erpx_infrastructure::ProjectionJobKind::Rebuild => "rebuild",
                        }
                        .into(),
                        state: match status.state {
                            erpx_infrastructure::ProjectionJobState::Pending => "pending",
                            erpx_infrastructure::ProjectionJobState::Claimed => "claimed",
                            erpx_infrastructure::ProjectionJobState::Retryable => "retryable",
                            erpx_infrastructure::ProjectionJobState::Succeeded => "succeeded",
                            erpx_infrastructure::ProjectionJobState::DeadLetter => "dead_letter",
                            erpx_infrastructure::ProjectionJobState::RebuildPending => {
                                "rebuild_pending"
                            }
                            erpx_infrastructure::ProjectionJobState::RebuildSucceeded => {
                                "rebuild_succeeded"
                            }
                        }
                        .into(),
                        available_at: status.available_at.to_rfc3339(),
                        attempts: u32::from(status.attempts),
                        max_attempts: u32::from(status.max_attempts),
                        lease_until: status
                            .lease_until
                            .map(|value| value.to_rfc3339())
                            .unwrap_or_default(),
                        last_error: status.last_error.unwrap_or_default(),
                        dead_lettered_at: status
                            .dead_lettered_at
                            .map(|value| value.to_rfc3339())
                            .unwrap_or_default(),
                        rebuild_id: status
                            .rebuild_id
                            .map(|value| value.to_string())
                            .unwrap_or_default(),
                        version: status.version,
                        created_at: status.created_at.to_rfc3339(),
                        updated_at: status.updated_at.to_rfc3339(),
                        published_at: status.published_at.to_rfc3339(),
                        freshness: freshness.into(),
                    }
                })
                .collect();
            Ok(Response::new(ListProjectionStatusesResponse {
                statuses,
                has_more: page.has_more,
                next_cursor: page
                    .next_cursor
                    .as_ref()
                    .map(encode_projection_status_cursor)
                    .unwrap_or_default(),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn list_platform_audit(
        &self,
        request: Request<ListPlatformAuditRequest>,
    ) -> Result<Response<ListPlatformAuditResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            require_platform_scope(&scope)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_TENANT_READ_PERMISSION,
                )
                .await?;

            let input = request.get_ref();
            let before = match (
                input.before_recorded_at.trim(),
                input.before_audit_id.trim(),
            ) {
                ("", "") => None,
                (recorded_at, audit_id) if !recorded_at.is_empty() && !audit_id.is_empty() => {
                    Some((
                        DateTime::parse_from_rfc3339(recorded_at)
                            .map_err(|_| Status::invalid_argument("before_recorded_at is invalid"))?
                            .with_timezone(&Utc),
                        parse_uuid(audit_id, "before_audit_id")?,
                    ))
                }
                _ => {
                    return Err(Status::invalid_argument(
                        "before_recorded_at and before_audit_id must be supplied together",
                    ));
                }
            };
            let page_size = platform_audit_page_size(input.page_size)?;
            let purpose = proto_context
                .audit
                .as_ref()
                .map(|audit| audit.purpose.clone())
                .unwrap_or_else(|| "platform audit read".into());
            let page = self
                .state
                .store
                .list_platform_audit_page(before, page_size)
                .await
                .map_err(|_| Status::internal("platform audit read failed"))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.audit.list".into(),
                    purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("platform audit persistence unavailable"))?;
            Ok(Response::new(ListPlatformAuditResponse {
                entries: page
                    .entries
                    .into_iter()
                    .map(|entry| PlatformAuditEntryMessage {
                        audit_id: entry.audit_id.to_string(),
                        actor_ref: entry.actor_ref,
                        action: entry.action,
                        purpose: entry.purpose,
                        correlation_id: entry.correlation_id.to_string(),
                        recorded_at: entry.recorded_at.to_rfc3339(),
                        customer_scoped: entry.customer_scoped,
                    })
                    .collect(),
                has_more: page.has_more,
                next_before_recorded_at: page
                    .next_before
                    .as_ref()
                    .map(|(recorded_at, _)| recorded_at.to_rfc3339())
                    .unwrap_or_default(),
                next_before_audit_id: page
                    .next_before
                    .as_ref()
                    .map(|(_, audit_id)| audit_id.to_string())
                    .unwrap_or_default(),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn replay_dead_letter_outbox(
        &self,
        request: Request<ReplayDeadLetterOutboxRequest>,
    ) -> Result<Response<ReplayDeadLetterOutboxResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor does not match authenticated principal",
            )?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_OUTBOX_REPLAY_PERMISSION,
                )
                .await?;

            let input = request.get_ref();
            let event_id = parse_uuid(&input.event_id, "event_id")?;
            let available_at = DateTime::parse_from_rfc3339(&input.available_at)
                .map_err(|_| Status::invalid_argument("available_at is invalid"))?
                .with_timezone(&Utc);
            self.state
                .auth
                .authorize_mfa(&claims, &input.mfa_code)
                .await?;

            let receipt = self
                .state
                .store
                .replay_dead_letter_with_audit(DeadLetterReplayCommand {
                    event_id,
                    available_at,
                    actor_ref: &context.actor.principal_ref,
                    purpose: &context.audit.purpose,
                    correlation_id: context.correlation_id,
                    idempotency_key: &context.idempotency_key,
                    command_id: context.command_id,
                })
                .await
                .map_err(map_dead_letter_replay_error)?;
            Ok(Response::new(ReplayDeadLetterOutboxResponse {
                event_id: receipt.event_id.to_string(),
                state: if receipt.replayed {
                    "already_requeued".into()
                } else {
                    "requeued".into()
                },
                correlation_id: receipt.correlation_id.to_string(),
                replayed: receipt.replayed,
            }))
        })
    }

    async fn list_platform_integrations(
        &self,
        request: Request<ListPlatformIntegrationsRequest>,
    ) -> Result<Response<ListPlatformIntegrationsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            require_platform_scope(&scope)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_TENANT_READ_PERMISSION,
                )
                .await?;
            let checked_at = Utc::now().to_rfc3339();
            let control_plane_state = if self.state.store.check_ready().await.is_ok() {
                "ready"
            } else {
                "not_ready"
            };
            let identity_state = if self.state.auth.identity.check_ready().await.is_ok() {
                "ready"
            } else {
                "not_ready"
            };
            let storage = self.state.storage.integration_health();
            let integrations = vec![
                PlatformIntegrationStatus {
                    integration_key: "control_plane".into(),
                    state: control_plane_state.into(),
                    capability: "SQLx control-plane write/read pools".into(),
                    checked_at: checked_at.clone(),
                },
                PlatformIntegrationStatus {
                    integration_key: "identity".into(),
                    state: identity_state.into(),
                    capability: "SQLx identity, session, and recovery boundary".into(),
                    checked_at: checked_at.clone(),
                },
                PlatformIntegrationStatus {
                    integration_key: "tenant_routing".into(),
                    state: if self.state.tenant_router.is_some() {
                        "configured"
                    } else {
                        "not_configured"
                    }
                    .into(),
                    capability: "physical tenant write/read database routing".into(),
                    checked_at: checked_at.clone(),
                },
                PlatformIntegrationStatus {
                    integration_key: "object_storage".into(),
                    state: storage.0.into(),
                    capability: "MinIO-compatible tenant object storage".into(),
                    checked_at: checked_at.clone(),
                },
                PlatformIntegrationStatus {
                    integration_key: "billing".into(),
                    state: "not_probed".into(),
                    capability: "Stripe catalog, webhook, and entitlement boundary".into(),
                    checked_at,
                },
            ];
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.integrations.list".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "platform integrations read".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("platform audit persistence unavailable"))?;
            Ok(Response::new(ListPlatformIntegrationsResponse {
                integrations,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn get_platform_settings(
        &self,
        request: Request<GetPlatformSettingsRequest>,
    ) -> Result<Response<GetPlatformSettingsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            require_platform_scope(&scope)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_TENANT_READ_PERMISSION,
                )
                .await?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.settings.get".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "platform settings read".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("platform audit persistence unavailable"))?;
            Ok(Response::new(GetPlatformSettingsResponse {
                settings: Some(erpx_proto::platform::PlatformSettingsSummary {
                    contract_version: u32::from(CONTRACT_VERSION).to_string(),
                    transport: "tonic gRPC and gRPC-Web".into(),
                    tenant_isolation: "physical per-holding write/read PostgreSQL databases".into(),
                    aggregate_authority: "PostgreSQL aggregate state with outbox projection".into(),
                    billing_authority: "Stripe payment and subscription state".into(),
                    storage_authority: "MinIO-compatible object storage".into(),
                    // Folio remains the production command owner until an
                    // explicitly approved, rollback-rehearsed G13 run enters
                    // the owner-switch phase. This read model must not imply
                    // that local ERPX composition has performed cutover.
                    command_owner: "Folio (ERPX migration-gated)".into(),
                }),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }
}

#[derive(Clone)]
struct DirectoryRpcImpl {
    state: AppState,
}

#[tonic::async_trait]
impl DirectoryRpc for DirectoryRpcImpl {
    async fn list_directory_connectors(
        &self,
        request: Request<ListDirectoryConnectorsRequest>,
    ) -> Result<Response<ListDirectoryConnectorsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            if matches!(
                scope,
                Scope::Company { .. } | Scope::Branch { .. } | Scope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "directory administration requires platform or holding scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_DIRECTORY_READ_PERMISSION,
                )
                .await?;
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            let requested_scope = if request.get_ref().scope_key.trim().is_empty() {
                None
            } else {
                Some(directory_scope_from_key(
                    request.get_ref().scope_key.trim(),
                )?)
            };
            if let Some(requested_scope) = requested_scope.as_ref() {
                directory_scope_access(&scope, requested_scope)?;
            }
            let connectors = directory
                .list_connectors()
                .await
                .map_err(|_| Status::internal("directory connector read failed"))?
                .into_iter()
                .filter(|connector| {
                    let Ok(connector_scope) = directory_scope_from_key(&connector.scope_key) else {
                        return false;
                    };
                    directory_scope_access(&scope, &connector_scope).is_ok()
                        && requested_scope
                            .as_ref()
                            .is_none_or(|requested| &connector_scope == requested)
                })
                .map(directory_connector_summary)
                .collect::<Vec<_>>();
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.directory.list".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "directory connector administration read".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("directory audit failed"))?;
            Ok(Response::new(ListDirectoryConnectorsResponse {
                connectors,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn save_directory_connector(
        &self,
        request: Request<SaveDirectoryConnectorRequest>,
    ) -> Result<Response<SaveDirectoryConnectorResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(&proto_context))?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(
                scope,
                Scope::Company { .. } | Scope::Branch { .. } | Scope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "directory administration requires platform or holding scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_DIRECTORY_MANAGE_PERMISSION,
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(&claims, &request.get_ref().mfa_code)
                .await?;
            let input = request.get_ref();
            let connector_id = if input.connector_id.trim().is_empty() {
                Uuid::new_v4()
            } else {
                Uuid::parse_str(input.connector_id.trim())
                    .map_err(|_| Status::invalid_argument("connector_id is invalid"))?
            };
            let expected_revision = input.expected_revision;
            let revision = expected_revision
                .checked_add(1)
                .ok_or_else(|| Status::invalid_argument("connector revision overflow"))?;
            let connector = DirectoryConnector {
                id: connector_id,
                provider: parse_directory_provider(&input.provider)?,
                scope: parse_directory_scope(&input.scope_key, &input.holding_ref)?,
                endpoint: input.endpoint.trim().into(),
                base_dn: input.base_dn.trim().into(),
                tls: DirectoryTlsPolicy {
                    mode: parse_directory_tls(&input.tls_mode)?,
                    ca_certificate_sha256: input.ca_certificate_sha256.trim().into(),
                    server_name: input.server_name.trim().into(),
                    minimum_tls_version: input.minimum_tls_version.trim().into(),
                },
                feed: parse_directory_feed(&input.feed_kind)?,
                credential_ref: input.credential_ref.trim().into(),
                enabled: input.enabled,
                max_import_users: input.max_import_users,
                revision,
            };
            directory_scope_access(&scope, &connector.scope)?;
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            directory
                .save_connector(&connector, Utc::now())
                .await
                .map_err(|error| Status::invalid_argument(error.to_string()))?;
            let snapshot = directory
                .list_connectors()
                .await
                .map_err(|_| Status::internal("directory connector read failed"))?
                .into_iter()
                .find(|snapshot| snapshot.id == connector_id)
                .ok_or_else(|| Status::internal("saved directory connector was not readable"))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.directory.save".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "directory connector administration write".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("directory audit failed"))?;
            Ok(Response::new(SaveDirectoryConnectorResponse {
                connector: Some(directory_connector_summary(snapshot)),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn start_directory_import(
        &self,
        request: Request<StartDirectoryImportRequest>,
    ) -> Result<Response<StartDirectoryImportResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(&proto_context))?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(
                scope,
                Scope::Company { .. } | Scope::Branch { .. } | Scope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "directory administration requires platform or holding scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_DIRECTORY_MANAGE_PERMISSION,
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(&claims, &request.get_ref().mfa_code)
                .await?;
            let connector_id = Uuid::parse_str(request.get_ref().connector_id.trim())
                .map_err(|_| Status::invalid_argument("connector_id is invalid"))?;
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            let connector = directory
                .connector(connector_id)
                .await
                .map_err(|_| Status::not_found("directory connector not found"))?;
            directory_scope_access(&scope, &connector.scope)?;
            let plan = directory
                .start_import(
                    &connector,
                    request.get_ref().total_users,
                    request.get_ref().page_size,
                    Utc::now(),
                )
                .await
                .map_err(|error| Status::invalid_argument(error.to_string()))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.directory.import.start".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "directory full import start".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("directory audit failed"))?;
            Ok(Response::new(StartDirectoryImportResponse {
                connector_id: connector_id.to_string(),
                page_count: plan.page_count(),
                status: "queued".into(),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn get_directory_import_status(
        &self,
        request: Request<GetDirectoryImportStatusRequest>,
    ) -> Result<Response<GetDirectoryImportStatusResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            if matches!(
                scope,
                Scope::Company { .. } | Scope::Branch { .. } | Scope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "directory administration requires platform or holding scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_DIRECTORY_READ_PERMISSION,
                )
                .await?;
            let input = request.get_ref();
            let connector_id = Uuid::parse_str(input.connector_id.trim())
                .map_err(|_| Status::invalid_argument("connector_id is invalid"))?;
            let import_id = if input.import_id.trim().is_empty() {
                None
            } else {
                Some(
                    Uuid::parse_str(input.import_id.trim())
                        .map_err(|_| Status::invalid_argument("import_id is invalid"))?,
                )
            };
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            let connector = directory
                .connector(connector_id)
                .await
                .map_err(|_| Status::not_found("directory connector not found"))?;
            directory_scope_access(&scope, &connector.scope)?;
            let status = directory
                .import_status(connector_id, import_id)
                .await
                .map_err(|_| Status::internal("directory import status read failed"))?
                .ok_or_else(|| Status::not_found("directory import status not found"))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.directory.import.status".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "directory full import status read".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("directory audit failed"))?;
            Ok(Response::new(GetDirectoryImportStatusResponse {
                status: Some(directory_import_status_message(status)),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn list_directory_role_mappings(
        &self,
        request: Request<ListDirectoryRoleMappingsRequest>,
    ) -> Result<Response<ListDirectoryRoleMappingsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            if matches!(
                scope,
                Scope::Company { .. } | Scope::Branch { .. } | Scope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "directory administration requires platform or holding scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_DIRECTORY_READ_PERMISSION,
                )
                .await?;
            let connector_id = Uuid::parse_str(request.get_ref().connector_id.trim())
                .map_err(|_| Status::invalid_argument("connector_id is invalid"))?;
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            let connector = directory
                .connector(connector_id)
                .await
                .map_err(|_| Status::not_found("directory connector not found"))?;
            directory_scope_access(&scope, &connector.scope)?;
            let mappings = directory
                .list_role_mappings(connector_id)
                .await
                .map_err(|_| Status::internal("directory role mapping read failed"))?
                .into_iter()
                .map(
                    |mapping: DirectoryRoleMapping| DirectoryRoleMappingMessage {
                        connector_id: mapping.connector_id.to_string(),
                        group_ref: mapping.group_ref,
                        role_ref: mapping.role_ref,
                        scope_key: mapping.scope.canonical_key().unwrap_or_default(),
                        requires_mfa: mapping.requires_mfa,
                        active: mapping.active,
                        revision: mapping.revision,
                    },
                )
                .collect();
            Ok(Response::new(ListDirectoryRoleMappingsResponse {
                mappings,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn list_directory_identity_links(
        &self,
        request: Request<ListDirectoryIdentityLinksRequest>,
    ) -> Result<Response<ListDirectoryIdentityLinksResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            if matches!(
                scope,
                Scope::Company { .. } | Scope::Branch { .. } | Scope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "directory administration requires platform or holding scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_DIRECTORY_READ_PERMISSION,
                )
                .await?;
            let input = request.get_ref();
            let connector_id = Uuid::parse_str(input.connector_id.trim())
                .map_err(|_| Status::invalid_argument("connector_id is invalid"))?;
            let page_size = match input.page_size {
                0 => 100,
                value if value <= MAX_DIRECTORY_IDENTITY_LINK_PAGE_SIZE => value,
                _ => {
                    return Err(Status::invalid_argument(
                        "directory identity-link page size exceeds the allowed bound",
                    ));
                }
            };
            let after = if input.after_identity_link.trim().is_empty() {
                None
            } else {
                Some(input.after_identity_link.trim())
            };
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            let connector = directory
                .connector(connector_id)
                .await
                .map_err(|_| Status::not_found("directory connector not found"))?;
            directory_scope_access(&scope, &connector.scope)?;
            let (links, has_more) = directory
                .list_identity_links(connector_id, page_size, after)
                .await
                .map_err(|_| Status::internal("directory identity-link read failed"))?;
            let next_after_identity_link = if has_more {
                links
                    .last()
                    .and_then(|link| link.identity_link.clone())
                    .ok_or_else(|| Status::internal("directory identity-link cursor missing"))?
            } else {
                String::new()
            };
            let links = links
                .into_iter()
                .map(|link| DirectoryIdentityLinkSummary {
                    connector_id: link.connector_id.to_string(),
                    external_subject: link.external_object_id,
                    immutable_identity_key: link.identity_link.unwrap_or_default(),
                    principal_ref: link.principal_ref.unwrap_or_else(|| {
                        link.identity_id
                            .map(|id| id.to_string())
                            .unwrap_or_default()
                    }),
                    active: link.active,
                    source_revision: link.source_revision,
                    observed_at: link.observed_at.to_rfc3339(),
                    revoked_at: link
                        .revoked_at
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_default(),
                    immutable: link.immutable_authority.is_some() && link.immutable_value.is_some(),
                })
                .collect();
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.directory.identity_links.list".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "directory immutable identity-link read".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("directory audit failed"))?;
            Ok(Response::new(ListDirectoryIdentityLinksResponse {
                links,
                has_more,
                next_after_identity_link,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn save_directory_role_mapping(
        &self,
        request: Request<SaveDirectoryRoleMappingRequest>,
    ) -> Result<Response<SaveDirectoryRoleMappingResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(&proto_context))?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(
                scope,
                Scope::Company { .. } | Scope::Branch { .. } | Scope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "directory administration requires platform or holding scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    PLATFORM_DIRECTORY_MANAGE_PERMISSION,
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(&claims, &request.get_ref().mfa_code)
                .await?;
            let input = request.get_ref();
            let connector_id = Uuid::parse_str(input.connector_id.trim())
                .map_err(|_| Status::invalid_argument("connector_id is invalid"))?;
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            let connector = directory
                .connector(connector_id)
                .await
                .map_err(|_| Status::not_found("directory connector not found"))?;
            directory_scope_access(&scope, &connector.scope)?;
            let revision = input
                .expected_revision
                .checked_add(1)
                .ok_or_else(|| Status::invalid_argument("role mapping revision overflow"))?;
            let mapping = DirectoryRoleMapping {
                connector_id,
                group_ref: input.group_ref.trim().into(),
                role_ref: input.role_ref.trim().into(),
                scope: parse_directory_scope(&input.scope_key, "")?,
                requires_mfa: input.requires_mfa,
                active: input.active,
                revision,
            };
            directory_scope_access(&scope, &mapping.scope)?;
            connector
                .scope
                .permits(&mapping.scope)
                .then_some(())
                .ok_or_else(|| {
                    Status::permission_denied(
                        "directory role mapping is outside the connector scope",
                    )
                })?;
            directory
                .save_role_mapping(&mapping, Utc::now())
                .await
                .map_err(|error| Status::invalid_argument(error.to_string()))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.directory.role_mapping.save".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "directory role mapping administration write".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("directory audit failed"))?;
            Ok(Response::new(SaveDirectoryRoleMappingResponse {
                mapping: Some(DirectoryRoleMappingMessage {
                    connector_id: mapping.connector_id.to_string(),
                    group_ref: mapping.group_ref,
                    role_ref: mapping.role_ref,
                    scope_key: mapping.scope.canonical_key().unwrap_or_default(),
                    requires_mfa: mapping.requires_mfa,
                    active: mapping.active,
                    revision: mapping.revision,
                }),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }
}

struct OrganizationRpcImpl {
    state: AppState,
}

fn organization_target_scope(
    kind: OrganizationNodeKind,
    node_id: Uuid,
    holding_id: Uuid,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
) -> Result<Scope, Status> {
    match kind {
        OrganizationNodeKind::Holding => {
            if node_id != holding_id {
                return Err(Status::invalid_argument(
                    "holding node_id must equal holding_id",
                ));
            }
            Ok(Scope::Holding { holding_id })
        }
        OrganizationNodeKind::Company => Ok(Scope::Company {
            holding_id,
            company_id: node_id,
        }),
        OrganizationNodeKind::Headquarters => Ok(Scope::Company {
            holding_id,
            company_id: company_id
                .filter(|value| !value.is_nil())
                .ok_or_else(|| Status::invalid_argument("company_id is required"))?,
        }),
        OrganizationNodeKind::Branch => Ok(Scope::Branch {
            holding_id,
            company_id: company_id
                .filter(|value| !value.is_nil())
                .ok_or_else(|| Status::invalid_argument("company_id is required"))?,
            branch_id: node_id,
        }),
        OrganizationNodeKind::Department | OrganizationNodeKind::Team => {
            let company_id = company_id
                .filter(|value| !value.is_nil())
                .ok_or_else(|| Status::invalid_argument("company_id is required"))?;
            Ok(branch_id.map_or(
                Scope::Company {
                    holding_id,
                    company_id,
                },
                |branch_id| Scope::Branch {
                    holding_id,
                    company_id,
                    branch_id,
                },
            ))
        }
    }
}

fn organization_persistence_status(error: PersistenceError, correlation_id: Uuid) -> Status {
    let status = match error {
        PersistenceError::OptimisticConcurrency { .. }
        | PersistenceError::IdempotencyConflict { .. } => {
            Status::aborted("organization command conflicts with the current state")
        }
        PersistenceError::NotFound { .. } => Status::not_found("organization node not found"),
        PersistenceError::InvalidData(_) => {
            Status::failed_precondition("organization node cannot be persisted or read")
        }
        PersistenceError::Configuration(_) => {
            Status::failed_precondition("organization persistence is not configured")
        }
        PersistenceError::Database(_) | PersistenceError::Migration(_) => {
            Status::unavailable("organization persistence is unavailable")
        }
        PersistenceError::Serialization(_) => Status::internal("organization persistence failed"),
        PersistenceError::ClaimLost { .. } => {
            Status::aborted("organization command ownership changed")
        }
    };
    errors::enrich(status, Some(correlation_id))
}

fn organization_node_message(
    node: erpx_infrastructure::OrganizationReadNode,
) -> erpx_proto::platform::OrganizationNode {
    erpx_proto::platform::OrganizationNode {
        kind: node.kind.as_str().into(),
        node_id: node.node_id.to_string(),
        holding_id: node.holding_id.to_string(),
        company_id: node
            .company_id
            .map_or_else(String::new, |value| value.to_string()),
        branch_id: node
            .branch_id
            .map_or_else(String::new, |value| value.to_string()),
        department_id: node
            .department_id
            .map_or_else(String::new, |value| value.to_string()),
        code: node.code,
        name: node.name,
        registration_number: node.registration_number,
        status: node.status,
        aggregate_version: node.aggregate_version,
        updated_at: node.updated_at.to_rfc3339(),
    }
}

#[tonic::async_trait]
impl OrganizationService for OrganizationRpcImpl {
    async fn save_organization_node(
        &self,
        request: Request<SaveOrganizationNodeRequest>,
    ) -> Result<Response<SaveOrganizationNodeResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform) {
                return Err(Status::permission_denied(
                    "organization commands require tenant scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "organization.admin",
                )
                .await?;
            let input = request.into_inner();
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let kind = OrganizationNodeKind::parse(input.kind.trim())
                .map_err(|_| Status::invalid_argument("organization kind is unsupported"))?;
            let node_id = parse_uuid(&input.node_id, "node_id")?;
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let company_id = optional_organization_uuid(&input.company_id, "company_id")?;
            let branch_id = optional_organization_uuid(&input.branch_id, "branch_id")?;
            let department_id = optional_organization_uuid(&input.department_id, "department_id")?;
            let target_scope =
                organization_target_scope(kind, node_id, holding_id, company_id, branch_id)?;
            if !scope.contains(&target_scope) {
                return Err(Status::permission_denied(
                    "organization command scope is outside the authorized scope",
                ));
            }
            let expected_version = if input.has_expected_version {
                if input.expected_version == 0 {
                    return Err(Status::invalid_argument(
                        "expected_version must be positive",
                    ));
                }
                Some(input.expected_version)
            } else {
                None
            };
            if expected_version != context.expected_aggregate_version {
                return Err(Status::invalid_argument(
                    "expected_version does not match command context",
                ));
            }
            let status = organization_status(input.status.trim())?;
            let registry = SqlxTenantStoreRegistry::new(self.state.store.clone());
            let registration = registry
                .get(tenant_id)
                .await
                .map_err(|_| Status::unavailable("tenant registry is unavailable"))?
                .ok_or_else(|| Status::not_found("tenant is not registered"))?;
            if registration.state != "active" {
                return Err(Status::failed_precondition("tenant is not active"));
            }
            let expected_holding_id =
                erpx_infrastructure::tenant_holding_scope_id(&registration.holding_ref);
            if expected_holding_id != holding_id {
                return Err(Status::permission_denied(
                    "organization holding does not match the tenant route",
                ));
            }
            let router = self
                .state
                .tenant_router
                .as_ref()
                .ok_or_else(|| Status::unavailable("tenant routing is not configured"))?;
            let write = router
                .route(
                    tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| Status::unavailable("tenant write route is unavailable"))?;
            let read = match router
                .route(
                    tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Read,
                )
                .await
            {
                Ok(read) => read,
                Err(_) => {
                    write.close().await;
                    return Err(Status::unavailable("tenant read route is unavailable"));
                }
            };
            let store = SqlxOrganizationStore::from_pools(
                write.database_pool(),
                read.database_pool(),
                registration.holding_ref.clone(),
            )
            .map_err(|error| organization_persistence_status(error, context.correlation_id))?;
            let result = store
                .save(&OrganizationNodeCommand {
                    kind,
                    node_id,
                    holding_ref: registration.holding_ref,
                    holding_id,
                    company_id,
                    branch_id,
                    department_id,
                    code: input.code,
                    name: input.name,
                    registration_number: input.registration_number,
                    status,
                    expected_version,
                    idempotency_key: context.idempotency_key,
                    actor: context.actor,
                    correlation_id: context.correlation_id,
                    at: Utc::now(),
                })
                .await
                .map_err(|error| organization_persistence_status(error, context.correlation_id));
            write.close().await;
            read.close().await;
            let receipt = result?;
            Ok(Response::new(SaveOrganizationNodeResponse {
                kind: receipt.kind.as_str().into(),
                node_id: receipt.node_id.to_string(),
                aggregate_version: receipt.aggregate_version,
                event_id: receipt.event_id.to_string(),
                applied: receipt.applied,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn list_organization_nodes(
        &self,
        request: Request<ListOrganizationNodesRequest>,
    ) -> Result<Response<ListOrganizationNodesResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_id = parse_uuid(&request.get_ref().holding_id, "holding_id")?;
            let holding_scope = Scope::Holding { holding_id };
            if !scope.contains(&holding_scope) {
                return Err(Status::permission_denied(
                    "organization read scope is outside the authorized scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "organization.read",
                )
                .await?;
            let kind = if request.get_ref().kind.trim().is_empty() {
                None
            } else {
                Some(
                    OrganizationNodeKind::parse(request.get_ref().kind.trim()).map_err(|_| {
                        Status::invalid_argument("organization kind is unsupported")
                    })?,
                )
            };
            let cursor = parse_organization_cursor(&request.get_ref().after_cursor)?;
            let page_size = organization_page_size(request.get_ref().page_size)?;
            let registry = SqlxTenantStoreRegistry::new(self.state.store.clone());
            let registration = registry
                .get(tenant_id)
                .await
                .map_err(|_| Status::unavailable("tenant registry is unavailable"))?
                .ok_or_else(|| Status::not_found("tenant is not registered"))?;
            if registration.state != "active"
                || erpx_infrastructure::tenant_holding_scope_id(&registration.holding_ref)
                    != holding_id
            {
                return Err(Status::failed_precondition(
                    "tenant organization route is unavailable",
                ));
            }
            let router = self
                .state
                .tenant_router
                .as_ref()
                .ok_or_else(|| Status::unavailable("tenant routing is not configured"))?;
            let read = router
                .route(
                    tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Read,
                )
                .await
                .map_err(|_| Status::unavailable("tenant read route is unavailable"))?;
            let store = SqlxOrganizationStore::from_pools(
                read.database_pool(),
                read.database_pool(),
                registration.holding_ref,
            )
            .map_err(|error| organization_persistence_status(error, context.correlation_id))?;
            let result = store.list(kind, cursor, page_size).await;
            read.close().await;
            let page = result
                .map_err(|error| organization_persistence_status(error, context.correlation_id))?;
            Ok(Response::new(ListOrganizationNodesResponse {
                nodes: page
                    .nodes
                    .into_iter()
                    .map(organization_node_message)
                    .collect(),
                has_more: page.has_more,
                next_cursor: page
                    .next_cursor
                    .as_ref()
                    .map(organization_cursor)
                    .unwrap_or_default(),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }
}

#[derive(Clone)]
struct OperationsRpcImpl {
    state: AppState,
}

impl OperationsRpcImpl {
    async fn active_exception_principal(
        &self,
        principal_ref: &str,
        scope: &Scope,
        permission: &str,
        at: DateTime<Utc>,
    ) -> Result<ExceptionPrincipalReference, Status> {
        let principal_ref = validate_transport_reference(principal_ref, "principal_ref")?;
        let assignment = self
            .state
            .store
            .effective_role_assignment_chain(principal_ref, scope, permission, at)
            .await
            .map_err(|_| Status::internal("exception principal authorization lookup failed"))?
            .map(|(assignment, _)| assignment)
            .ok_or_else(|| Status::permission_denied("exception principal is not active"))?;
        Ok(ExceptionPrincipalReference {
            principal_ref: principal_ref.into(),
            valid_from: assignment.valid_from,
            valid_until: assignment.valid_until,
        })
    }

    /// Resolve the assignee's complete role-assignment chain and bind the
    /// requested manager to its authoritative root. A manager is not merely
    /// another active Operations reader: the root assignment must carry
    /// exception-management authority and every delegation edge must remain
    /// active, scoped, and permission-preserving at the command timestamp.
    async fn active_manager_chain(
        &self,
        subject_principal_ref: &str,
        manager_principal_ref: &str,
        scope: &Scope,
        at: DateTime<Utc>,
    ) -> Result<(ExceptionPrincipalReference, OperationsManagerChainProof), Status> {
        let subject_principal_ref =
            validate_transport_reference(subject_principal_ref, "assignee")?;
        let manager_principal_ref = validate_transport_reference(manager_principal_ref, "manager")?;
        let (assignment, parents) = self
            .state
            .store
            .effective_role_assignment_chain(subject_principal_ref, scope, "operations.read", at)
            .await
            .map_err(|_| Status::internal("manager chain authorization lookup failed"))?
            .ok_or_else(|| Status::permission_denied("assignee role chain is not active"))?;
        let mut assignments = Vec::with_capacity(parents.len() + 1);
        assignments.push(assignment);
        assignments.extend(parents);
        let manager_assignment = assignments
            .last()
            .ok_or_else(|| Status::permission_denied("manager role chain is empty"))?;
        if manager_assignment.principal_ref != manager_principal_ref {
            return Err(Status::permission_denied(
                "manager is not the authoritative parent of the assignee",
            ));
        }
        let manager_id = manager_assignment.id;
        let manager_valid_from = manager_assignment.valid_from;
        let manager_valid_until = manager_assignment.valid_until;
        let proof = OperationsManagerChainProof {
            subject_principal_ref: subject_principal_ref.into(),
            manager_principal_ref: manager_principal_ref.into(),
            scope: scope.clone(),
            evaluated_at: at,
            authority_source_ref: format!("role-assignment-chain:{manager_id}"),
            assignments,
        };
        proof
            .validate(at)
            .map_err(|_| Status::permission_denied("manager role chain is not authoritative"))?;
        Ok((
            ExceptionPrincipalReference {
                principal_ref: manager_principal_ref.into(),
                valid_from: manager_valid_from,
                valid_until: manager_valid_until,
            },
            proof,
        ))
    }
}

fn operations_persistence_status(error: PersistenceError, correlation_id: Uuid) -> Status {
    let status = match error {
        PersistenceError::OptimisticConcurrency { .. }
        | PersistenceError::IdempotencyConflict { .. } => {
            Status::aborted("operations state conflicts with the current state")
        }
        PersistenceError::NotFound { .. } => Status::not_found("operations row not found"),
        PersistenceError::InvalidData(_) => {
            Status::failed_precondition("operations projection is invalid")
        }
        PersistenceError::Configuration(_) => {
            Status::failed_precondition("operations persistence is not configured")
        }
        PersistenceError::Database(_) | PersistenceError::Migration(_) => {
            Status::unavailable("operations persistence is unavailable")
        }
        PersistenceError::Serialization(_) => Status::internal("operations projection failed"),
        PersistenceError::ClaimLost { .. } => {
            Status::aborted("operations projection ownership changed")
        }
    };
    errors::enrich(status, Some(correlation_id))
}

fn operations_scope_and_holding(
    proto_context: &erpx_proto::common::v1::QueryContext,
    holding_id: Uuid,
) -> Result<(erpx_domain::contracts::TenantScope, Scope), Status> {
    let scope = query_access_scope(proto_context)?;
    let holding_scope = Scope::Holding { holding_id };
    if !scope.contains(&holding_scope) {
        return Err(Status::permission_denied(
            "operations read scope is outside the authorized scope",
        ));
    }
    Ok((
        erpx_domain::contracts::TenantScope::Holding { holding_id },
        scope,
    ))
}

async fn routed_operations_store(
    state: &AppState,
    tenant_id: &str,
    holding_id: Uuid,
) -> Result<
    (
        SqlxOperationsStore,
        RoutedTenantDatabase,
        RoutedTenantDatabase,
    ),
    Status,
> {
    let registry = SqlxTenantStoreRegistry::new(state.store.clone());
    let registration = registry
        .get(tenant_id)
        .await
        .map_err(|_| Status::unavailable("tenant registry is unavailable"))?
        .ok_or_else(|| Status::not_found("tenant is not registered"))?;
    if registration.state != "active"
        || erpx_infrastructure::tenant_holding_scope_id(&registration.holding_ref) != holding_id
    {
        return Err(Status::failed_precondition(
            "operations tenant route is unavailable",
        ));
    }
    let router = state
        .tenant_router
        .as_ref()
        .ok_or_else(|| Status::unavailable("tenant routing is not configured"))?;
    let write = router
        .route(
            tenant_id,
            &registration.holding_ref,
            RoutedDatabaseSide::Write,
        )
        .await
        .map_err(|_| Status::unavailable("tenant write route is unavailable"))?;
    let read = match router
        .route(
            tenant_id,
            &registration.holding_ref,
            RoutedDatabaseSide::Read,
        )
        .await
    {
        Ok(read) => read,
        Err(_) => {
            write.close().await;
            return Err(Status::unavailable("tenant read route is unavailable"));
        }
    };
    let store = match SqlxOperationsStore::from_pools(
        write.database_pool(),
        read.database_pool(),
        registration.holding_ref,
    ) {
        Ok(store) => store,
        Err(error) => {
            write.close().await;
            read.close().await;
            return Err(operations_persistence_status(error, Uuid::new_v4()));
        }
    };
    Ok((store, write, read))
}

#[tonic::async_trait]
impl OperationsService for OperationsRpcImpl {
    async fn get_operations_summary(
        &self,
        request: Request<GetOperationsSummaryRequest>,
    ) -> Result<Response<GetOperationsSummaryResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let holding_id = parse_uuid(&request.get_ref().holding_id, "holding_id")?;
            let (_, scope) = operations_scope_and_holding(&proto_context, holding_id)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "operations.read",
                )
                .await?;
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let registry = SqlxTenantStoreRegistry::new(self.state.store.clone());
            let registration = registry
                .get(tenant_id)
                .await
                .map_err(|_| Status::unavailable("tenant registry is unavailable"))?
                .ok_or_else(|| Status::not_found("tenant is not registered"))?;
            if registration.state != "active"
                || erpx_infrastructure::tenant_holding_scope_id(&registration.holding_ref)
                    != holding_id
            {
                return Err(Status::failed_precondition(
                    "operations tenant route is unavailable",
                ));
            }
            let router = self
                .state
                .tenant_router
                .as_ref()
                .ok_or_else(|| Status::unavailable("tenant routing is not configured"))?;
            let write = router
                .route(
                    tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| Status::unavailable("tenant write route is unavailable"))?;
            let read = match router
                .route(
                    tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Read,
                )
                .await
            {
                Ok(read) => read,
                Err(_) => {
                    write.close().await;
                    return Err(Status::unavailable("tenant read route is unavailable"));
                }
            };
            let store = SqlxOperationsStore::from_pools(
                write.database_pool(),
                read.database_pool(),
                registration.holding_ref.clone(),
            )
            .map_err(|error| operations_persistence_status(error, context.correlation_id))?;
            let result = store.read_summary().await;
            write.close().await;
            read.close().await;
            let summary = result
                .map_err(|error| operations_persistence_status(error, context.correlation_id))?;
            Ok(Response::new(GetOperationsSummaryResponse {
                summary: Some(OperationsSummary {
                    tenant_id: tenant_id.into(),
                    holding_id: holding_id.to_string(),
                    projection_name: OPERATIONS_READ_PROJECTION.into(),
                    read_row_count: summary.read_row_count,
                    exception_count: summary.exception_count,
                    projection_checkpoint_count: summary.projection_checkpoint_count,
                    event_count: summary.event_count,
                    source_reference_count: summary.source_reference_count,
                    source_counts: summary
                        .source_domain_counts
                        .into_iter()
                        .map(|(source_domain, reference_count)| OperationsSourceCount {
                            source_domain,
                            reference_count,
                        })
                        .collect(),
                    latest_event_at: summary
                        .latest_event_at
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_default(),
                }),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn list_operations_exceptions(
        &self,
        request: Request<ListOperationsExceptionsRequest>,
    ) -> Result<Response<ListOperationsExceptionsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let holding_id = parse_uuid(&request.get_ref().holding_id, "holding_id")?;
            let (_, scope) = operations_scope_and_holding(&proto_context, holding_id)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "operations.read",
                )
                .await?;
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let cursor = parse_operations_exception_cursor(&request.get_ref().after_cursor)?;
            let page_size = operations_exception_page_size(request.get_ref().page_size)?;
            let registry = SqlxTenantStoreRegistry::new(self.state.store.clone());
            let registration = registry
                .get(tenant_id)
                .await
                .map_err(|_| Status::unavailable("tenant registry is unavailable"))?
                .ok_or_else(|| Status::not_found("tenant is not registered"))?;
            if registration.state != "active"
                || erpx_infrastructure::tenant_holding_scope_id(&registration.holding_ref)
                    != holding_id
            {
                return Err(Status::failed_precondition(
                    "operations tenant route is unavailable",
                ));
            }
            let router = self
                .state
                .tenant_router
                .as_ref()
                .ok_or_else(|| Status::unavailable("tenant routing is not configured"))?;
            let write = router
                .route(
                    tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| Status::unavailable("tenant write route is unavailable"))?;
            let read = match router
                .route(
                    tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Read,
                )
                .await
            {
                Ok(read) => read,
                Err(_) => {
                    write.close().await;
                    return Err(Status::unavailable("tenant read route is unavailable"));
                }
            };
            let store = SqlxOperationsStore::from_pools(
                write.database_pool(),
                read.database_pool(),
                registration.holding_ref.clone(),
            )
            .map_err(|error| operations_persistence_status(error, context.correlation_id))?;
            let result = store
                .read_active_exception_page(OPERATIONS_READ_PROJECTION, cursor, page_size)
                .await;
            write.close().await;
            read.close().await;
            let page = result
                .map_err(|error| operations_persistence_status(error, context.correlation_id))?;
            Ok(Response::new(ListOperationsExceptionsResponse {
                exceptions: page
                    .cases
                    .into_iter()
                    .map(|row| OperationsException {
                        exception_id: row.aggregate_id.to_string(),
                        category: row.value.category,
                        severity: row.value.severity,
                        status: operations_exception_status_name(row.value.status).into(),
                        assigned_to: row.value.assigned_to.unwrap_or_default(),
                        manager_ref: row.value.manager_ref.unwrap_or_default(),
                        due_at: row
                            .value
                            .due_at
                            .map(|value| value.to_rfc3339())
                            .unwrap_or_default(),
                        aggregate_version: row.aggregate_version,
                        updated_at: row.updated_at.to_rfc3339(),
                    })
                    .collect(),
                has_more: page.has_more,
                next_cursor: page
                    .next_cursor
                    .as_ref()
                    .map(operations_exception_cursor)
                    .unwrap_or_default(),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn create_operations_exception_from_source(
        &self,
        request: Request<CreateOperationsExceptionFromSourceRequest>,
    ) -> Result<Response<CreateOperationsExceptionFromSourceResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let caller_scope = access_scope(&context.tenant_scope);
            let input = request.into_inner();
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let holding_scope = Scope::Holding { holding_id };
            if matches!(caller_scope, Scope::Platform) || !caller_scope.contains(&holding_scope) {
                return Err(Status::permission_denied(
                    "source exception scope is outside the authorized holding",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "operations.exception.manage",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let exception_id = parse_uuid(&input.exception_id, "exception_id")?;
            let source_id = parse_uuid(&input.source_id, "source_id")?;
            let source_event_id = parse_uuid(&input.source_event_id, "source_event_id")?;
            if input.source_version == 0 || input.event_version == 0 {
                return Err(Status::invalid_argument(
                    "source and event versions must be positive",
                ));
            }
            if input.payload_json.is_empty()
                || input.payload_json.len() > MAX_OPERATIONS_SOURCE_EVENT_PAYLOAD_BYTES
            {
                return Err(Status::invalid_argument(
                    "source payload is empty or exceeds the allowed bound",
                ));
            }
            let source_domain = OperationsSourceDomain::parse(&input.source_domain)
                .map_err(|_| Status::invalid_argument("source domain is unsupported"))?;
            let occurred_at = DateTime::parse_from_rfc3339(input.occurred_at.trim())
                .map_err(|_| Status::invalid_argument("occurred_at must be RFC3339"))?
                .with_timezone(&Utc);
            let payload: Value = serde_json::from_slice(&input.payload_json)
                .map_err(|_| Status::invalid_argument("source payload must be valid JSON"))?;
            let source_event = OperationsSourceEvent {
                source_domain,
                source_type: validate_transport_reference(&input.source_type, "source_type")?
                    .into(),
                source_id,
                source_event_id,
                source_version: input.source_version,
                event_type: validate_transport_reference(&input.event_type, "event_type")?.into(),
                event_version: u16::try_from(input.event_version)
                    .map_err(|_| Status::invalid_argument("event version is out of range"))?,
                holding_ref: String::new(),
                scope: holding_scope,
                actor: context.actor.clone(),
                correlation_id: context.correlation_id,
                causation_id: context.causation_id,
                occurred_at,
                payload,
            };
            let category = validate_transport_reference(&input.category, "category")?;
            let severity = validate_transport_reference(&input.severity, "severity")?;
            let (store, write, read) =
                routed_operations_store(&self.state, tenant_id, holding_id).await?;
            let registration = SqlxTenantStoreRegistry::new(self.state.store.clone())
                .get(tenant_id)
                .await
                .map_err(|_| Status::unavailable("tenant registry is unavailable"))?
                .ok_or_else(|| Status::not_found("tenant is not registered"))?;
            let mut source_event = source_event;
            source_event.holding_ref = registration.holding_ref.clone();
            let result = store
                .create_exception_from_source_event(&source_event, exception_id, category, severity)
                .await
                .map_err(|error| operations_persistence_status(error, context.correlation_id));
            write.close().await;
            read.close().await;
            let result = result?;
            Ok(Response::new(CreateOperationsExceptionFromSourceResponse {
                exception_id: exception_id.to_string(),
                source_event_id: result.source_event_id.to_string(),
                aggregate_version: result.aggregate_version,
                event_id: result.event_id.to_string(),
                applied: result.applied,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn transition_operations_exception(
        &self,
        request: Request<TransitionOperationsExceptionRequest>,
    ) -> Result<Response<TransitionOperationsExceptionResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let caller_scope = access_scope(&context.tenant_scope);
            if matches!(caller_scope, Scope::Platform) {
                return Err(Status::permission_denied(
                    "Operations exception commands require tenant scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "operations.exception.manage",
                )
                .await?;
            let input = request.into_inner();
            if matches!(input.action.trim(), "assign" | "reassign" | "escalate") {
                self.state
                    .auth
                    .authorize_mfa(&claims, &input.mfa_code)
                    .await?;
            }
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let exception_id = parse_uuid(&input.exception_id, "exception_id")?;
            let expected_version = context.expected_aggregate_version.ok_or_else(|| {
                Status::invalid_argument("expected aggregate version is required")
            })?;
            if expected_version == 0 {
                return Err(Status::invalid_argument(
                    "expected aggregate version must be positive",
                ));
            }
            let registry = SqlxTenantStoreRegistry::new(self.state.store.clone());
            let registration = registry
                .get(tenant_id)
                .await
                .map_err(|_| Status::unavailable("tenant registry is unavailable"))?
                .ok_or_else(|| Status::not_found("tenant is not registered"))?;
            if registration.state != "active"
                || erpx_infrastructure::tenant_holding_scope_id(&registration.holding_ref)
                    != holding_id
            {
                return Err(Status::failed_precondition(
                    "operations tenant route is unavailable",
                ));
            }
            let router = self
                .state
                .tenant_router
                .as_ref()
                .ok_or_else(|| Status::unavailable("tenant routing is not configured"))?;
            let write = router
                .route(
                    tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| Status::unavailable("tenant write route is unavailable"))?;
            let read = match router
                .route(
                    tenant_id,
                    &registration.holding_ref,
                    RoutedDatabaseSide::Read,
                )
                .await
            {
                Ok(read) => read,
                Err(_) => {
                    write.close().await;
                    return Err(Status::unavailable("tenant read route is unavailable"));
                }
            };
            let store = SqlxOperationsStore::from_pools(
                write.database_pool(),
                read.database_pool(),
                registration.holding_ref.clone(),
            )
            .map_err(|error| operations_persistence_status(error, context.correlation_id))?;
            let exception = store
                .get_aggregate::<erpx_domain::operations::ExceptionCase>(exception_id)
                .await
                .map_err(|error| operations_persistence_status(error, context.correlation_id))?;
            let Some(exception) = exception else {
                write.close().await;
                read.close().await;
                return Err(Status::not_found("operations exception not found"));
            };
            if !caller_scope.contains(&exception.meta.scope) {
                write.close().await;
                read.close().await;
                return Err(Status::permission_denied(
                    "operations exception scope is outside the authorized scope",
                ));
            }
            let at = Utc::now();
            let actor = self
                .active_exception_principal(
                    &claims.principal_ref,
                    &exception.meta.scope,
                    "operations.exception.manage",
                    at,
                )
                .await?;
            let action = input.action.trim();
            let action = match action {
                "assign" => {
                    let assignee = self
                        .active_exception_principal(
                            &input.assignee,
                            &exception.meta.scope,
                            "operations.read",
                            at,
                        )
                        .await?;
                    let (manager, _) = self
                        .active_manager_chain(
                            &assignee.principal_ref,
                            &input.manager,
                            &exception.meta.scope,
                            at,
                        )
                        .await?;
                    let due_at = DateTime::parse_from_rfc3339(input.sla_due_at.trim())
                        .map_err(|_| Status::invalid_argument("sla_due_at must be RFC3339"))?
                        .with_timezone(&Utc);
                    if input.sla_escalation_after_minutes == 0
                        || input.sla_escalation_after_minutes > MAX_EXCEPTION_SLA_MINUTES
                    {
                        return Err(Status::invalid_argument(
                            "sla escalation window is outside the allowed bound",
                        ));
                    }
                    ExceptionLifecycleAction::Assign {
                        assignee,
                        manager,
                        sla: ExceptionSlaMetadata {
                            policy_ref: validate_transport_reference(
                                &input.sla_policy_ref,
                                "sla_policy_ref",
                            )?
                            .into(),
                            due_at,
                            escalation_after_minutes: input.sla_escalation_after_minutes,
                        },
                    }
                }
                "reassign" => ExceptionLifecycleAction::Reassign {
                    assignee: self
                        .active_exception_principal(
                            &input.assignee,
                            &exception.meta.scope,
                            "operations.read",
                            at,
                        )
                        .await?,
                    manager: {
                        let (manager, _) = self
                            .active_manager_chain(
                                &input.assignee,
                                &input.manager,
                                &exception.meta.scope,
                                at,
                            )
                            .await?;
                        manager
                    },
                },
                "acknowledge" => ExceptionLifecycleAction::Acknowledge,
                "escalate" => ExceptionLifecycleAction::Escalate {
                    manager: {
                        let subject = exception.assigned_to.as_deref().ok_or_else(|| {
                            Status::failed_precondition(
                                "escalation requires an active exception assignee",
                            )
                        })?;
                        let (manager, _) = self
                            .active_manager_chain(
                                subject,
                                &input.manager,
                                &exception.meta.scope,
                                at,
                            )
                            .await?;
                        manager
                    },
                },
                "resolve" => ExceptionLifecycleAction::Resolve,
                "close" => ExceptionLifecycleAction::Close,
                _ => {
                    return Err(Status::invalid_argument(
                        "operations exception action is unsupported",
                    ));
                }
            };
            let command = ExceptionLifecycleCommand {
                exception_id,
                expected_version,
                at,
                actor,
                actor_attribution: context.actor,
                idempotency_key: context.idempotency_key,
                correlation_id: context.correlation_id,
                causation_id: context.causation_id,
                action,
            };
            let result = store
                .apply_exception_lifecycle_command(&command)
                .await
                .map_err(|error| operations_persistence_status(error, context.correlation_id));
            write.close().await;
            read.close().await;
            let result = result?;
            Ok(Response::new(TransitionOperationsExceptionResponse {
                exception_id: result.exception_id.to_string(),
                from_status: operations_exception_status_name(result.from).into(),
                to_status: operations_exception_status_name(result.to).into(),
                aggregate_version: result.aggregate_version,
                event_id: result.event_id.to_string(),
                applied: result.applied,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn publish_operations_work_queue_policy(
        &self,
        request: Request<PublishOperationsWorkQueuePolicyRequest>,
    ) -> Result<Response<PublishOperationsWorkQueuePolicyResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let caller_scope = access_scope(&context.tenant_scope);
            let holding_id = parse_uuid(&request.get_ref().holding_id, "holding_id")?;
            let holding_scope = Scope::Holding { holding_id };
            if matches!(caller_scope, Scope::Platform) || !caller_scope.contains(&holding_scope) {
                return Err(Status::permission_denied(
                    "work queue policy scope is outside the authorized scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "operations.manage",
                )
                .await?;
            let input = request.into_inner();
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let policy_id = parse_uuid(&input.policy_id, "policy_id")?;
            let name = validate_transport_reference(&input.name, "name")?;
            if input.allowed_domains.is_empty() || input.allowed_domains.len() > 16 {
                return Err(Status::invalid_argument(
                    "allowed_domains must contain between one and sixteen entries",
                ));
            }
            let allowed_domains = input
                .allowed_domains
                .iter()
                .map(|domain| {
                    validate_transport_reference(domain, "allowed_domain").map(str::to_owned)
                })
                .collect::<Result<Vec<_>, _>>()?;
            if input.escalation_after_minutes == 0
                || input.escalation_after_minutes > MAX_EXCEPTION_SLA_MINUTES
            {
                return Err(Status::invalid_argument(
                    "escalation_after_minutes is outside the allowed bound",
                ));
            }
            let expected_version = context.expected_aggregate_version.unwrap_or(0);
            let at = Utc::now();
            let (store, write, read) =
                routed_operations_store(&self.state, tenant_id, holding_id).await?;
            let result = store
                .publish_work_queue_policy(
                    policy_id,
                    holding_scope,
                    name,
                    allowed_domains,
                    input.escalation_after_minutes,
                    expected_version,
                    context.actor,
                    context.idempotency_key,
                    context.correlation_id,
                    context.causation_id,
                    at,
                )
                .await
                .map_err(|error| operations_persistence_status(error, context.correlation_id));
            write.close().await;
            read.close().await;
            let result = result?;
            Ok(Response::new(PublishOperationsWorkQueuePolicyResponse {
                policy_id: result.policy_id.to_string(),
                status: result.status,
                aggregate_version: result.aggregate_version,
                event_id: result.event_id.to_string(),
                applied: result.applied,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn list_operations_work_queue_policies(
        &self,
        request: Request<ListOperationsWorkQueuePoliciesRequest>,
    ) -> Result<Response<ListOperationsWorkQueuePoliciesResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let holding_id = parse_uuid(&request.get_ref().holding_id, "holding_id")?;
            let (_, scope) = operations_scope_and_holding(&proto_context, holding_id)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "operations.read",
                )
                .await?;
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let page_size = match request.get_ref().page_size {
                0 => 25,
                value if value <= 100 => value,
                _ => {
                    return Err(Status::invalid_argument(
                        "policy page size exceeds the allowed bound",
                    ));
                }
            };
            let cursor = parse_organization_cursor(&request.get_ref().after_cursor)?;
            let (store, write, read) =
                routed_operations_store(&self.state, tenant_id, holding_id).await?;
            let result = store
                .read_json_page("operations.work_queue_policy", cursor, page_size)
                .await
                .map_err(|error| operations_persistence_status(error, context.correlation_id));
            write.close().await;
            read.close().await;
            let page = result?;
            let mut policies = Vec::with_capacity(page.rows.len());
            for row in page.rows {
                let state = row.state;
                let name = state
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Status::failed_precondition("policy read model is malformed"))?;
                let allowed_domains = state
                    .get("allowed_domains")
                    .and_then(Value::as_array)
                    .ok_or_else(|| Status::failed_precondition("policy read model is malformed"))?
                    .iter()
                    .map(|value| {
                        value.as_str().map(str::to_owned).ok_or_else(|| {
                            Status::failed_precondition("policy read model is malformed")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let escalation_after_minutes = state
                    .get("escalation_after_minutes")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| Status::failed_precondition("policy read model is malformed"))?;
                policies.push(OperationsWorkQueuePolicyMessage {
                    policy_id: row.aggregate_id.to_string(),
                    name: name.to_owned(),
                    allowed_domains,
                    escalation_after_minutes,
                    status: row.lifecycle_status,
                    aggregate_version: row.aggregate_version,
                    updated_at: row.updated_at.to_rfc3339(),
                });
            }
            Ok(Response::new(ListOperationsWorkQueuePoliciesResponse {
                policies,
                has_more: page.has_more,
                next_cursor: page
                    .next_cursor
                    .as_ref()
                    .map(organization_cursor)
                    .unwrap_or_default(),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }
}

#[derive(Clone)]
struct SupportRpcImpl {
    state: AppState,
}

#[tonic::async_trait]
impl SupportRpc for SupportRpcImpl {
    async fn create_ticket(
        &self,
        request: Request<CreateTicketRequest>,
    ) -> Result<Response<CreateTicketResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &context.actor.principal_ref,
                    &access_scope(&context.tenant_scope),
                    "support.ticket.create",
                )
                .await?;
            let input = request.into_inner();
            let customer_id = parse_uuid(&input.customer_id, "customer_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "support",
                    "support.ticket.create",
                )
                .await?;
            let ticket = self
                .state
                .authoritative_support
                .open_ticket(&context, customer_id, input.severity, input.description)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            Ok(Response::new(CreateTicketResponse {
                ticket: Some(ticket_message(&ticket)),
            }))
        })
    }

    async fn update_ticket(
        &self,
        request: Request<UpdateTicketRequest>,
    ) -> Result<Response<UpdateTicketResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &context.actor.principal_ref,
                    &access_scope(&context.tenant_scope),
                    "support.ticket.update",
                )
                .await?;
            let input = request.into_inner();
            let ticket_id = parse_uuid(&input.ticket_id, "ticket_id")?;
            let next_status = parse_ticket_status(&input.next_status)?;
            let ticket = self
                .state
                .store
                .get_ticket(ticket_id)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?
                .ok_or_else(|| Status::not_found("support ticket not found"))?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    ticket.customer_account_id,
                    "support",
                    "support.ticket.update",
                )
                .await?;
            let ticket = self
                .state
                .authoritative_support
                .transition_ticket(&context, ticket_id, next_status)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            if matches!(ticket.status, TicketStatus::Closed) {
                self.state
                    .store
                    .revoke_support_capabilities_for_ticket(ticket.id, Utc::now())
                    .await
                    .map_err(|_| Status::internal("support capability revocation failed"))?;
            }
            Ok(Response::new(UpdateTicketResponse {
                ticket: Some(ticket_message(&ticket)),
            }))
        })
    }

    async fn get_ticket(
        &self,
        request: Request<GetTicketRequest>,
    ) -> Result<Response<GetTicketResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "support.ticket.read",
                )
                .await?;
            let ticket_id = parse_uuid(&request.get_ref().ticket_id, "ticket_id")?;
            let ticket = self
                .state
                .store
                .get_ticket(ticket_id)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?
                .ok_or_else(|| Status::not_found("support ticket not found"))?;
            if !self
                .state
                .store
                .customer_scope_contains(ticket.customer_account_id, &scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let purpose = validate_read_purpose(
                proto_context
                    .audit
                    .as_ref()
                    .map(|audit| audit.purpose.as_str())
                    .unwrap_or_default(),
            )?;
            self.state
                .store
                .record(erpx_application::AuditEvent {
                    actor_ref: context.actor,
                    customer_account_id: Some(ticket.customer_account_id),
                    action: "support.ticket.read".into(),
                    purpose: purpose.into(),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("support ticket audit unavailable"))?;
            Ok(Response::new(GetTicketResponse {
                ticket: Some(ticket_message(&ticket)),
                correlation_id: context.correlation_id.to_string(),
                present: true,
            }))
        })
    }

    async fn list_tickets(
        &self,
        request: Request<ListTicketsRequest>,
    ) -> Result<Response<ListTicketsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .clone()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "support.ticket.read",
                )
                .await?;
            let input = request.into_inner();
            let status = (!input.status.trim().is_empty())
                .then(|| parse_ticket_status(&input.status))
                .transpose()?;
            let customer_id = if input.customer_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.customer_id, "customer_id")?)
            };
            let after_cursor = if input.after_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_support_ticket_cursor(&input.after_cursor)?)
            };
            let page = self
                .state
                .support_persistence
                .list_tickets_after(status, customer_id, input.page_size, after_cursor.as_ref())
                .await
                .map_err(incident_persistence_status)?;

            let mut tickets = Vec::with_capacity(page.tickets.len());
            for ticket in page.tickets {
                if self
                    .state
                    .store
                    .customer_scope_contains(ticket.value.customer_account_id, &scope)
                    .await
                    .map_err(|_| Status::internal("customer scope lookup failed"))?
                {
                    tickets.push(ticket_message(&ticket.value));
                }
            }
            let purpose = validate_read_purpose(
                proto_context
                    .audit
                    .as_ref()
                    .map(|audit| audit.purpose.as_str())
                    .unwrap_or_default(),
            )?;
            self.state
                .store
                .record(erpx_application::AuditEvent {
                    actor_ref: context.actor,
                    customer_account_id: customer_id,
                    action: "support.ticket.list".into(),
                    purpose: purpose.into(),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("support ticket audit unavailable"))?;

            Ok(Response::new(ListTicketsResponse {
                tickets,
                has_more: page.has_more,
                correlation_id: context.correlation_id.to_string(),
                next_cursor: page
                    .next_cursor
                    .as_ref()
                    .map(encode_support_ticket_cursor)
                    .unwrap_or_default(),
            }))
        })
    }

    async fn create_ticket_recurrence(
        &self,
        request: Request<CreateTicketRecurrenceRequest>,
    ) -> Result<Response<CreateTicketRecurrenceResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &access_scope(&context.tenant_scope),
                    "support.ticket.create",
                )
                .await?;
            let input = request.into_inner();
            let closed_ticket_id = parse_uuid(&input.closed_ticket_id, "closed_ticket_id")?;
            let closed_ticket = self
                .state
                .store
                .get_ticket(closed_ticket_id)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?
                .ok_or_else(|| Status::not_found("closed support ticket not found"))?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    closed_ticket.customer_account_id,
                    "support",
                    "support.ticket.create",
                )
                .await?;
            let ticket = self
                .state
                .authoritative_support
                .open_ticket_recurrence(
                    &context,
                    closed_ticket_id,
                    input.severity,
                    input.description,
                )
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            Ok(Response::new(CreateTicketRecurrenceResponse {
                ticket: Some(ticket_message(&ticket)),
            }))
        })
    }

    async fn add_ticket_reply(
        &self,
        request: Request<AddTicketReplyRequest>,
    ) -> Result<Response<AddTicketReplyResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &context.actor.principal_ref,
                    &access_scope(&context.tenant_scope),
                    SUPPORT_TICKET_REPLY_PERMISSION,
                )
                .await?;
            let input = request.into_inner();
            let ticket_id = parse_uuid(&input.ticket_id, "ticket_id")?;
            let ticket = self
                .state
                .store
                .get_ticket(ticket_id)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?
                .ok_or_else(|| Status::not_found("support ticket not found"))?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    ticket.customer_account_id,
                    "support",
                    SUPPORT_TICKET_REPLY_PERMISSION,
                )
                .await?;
            let ticket = self
                .state
                .authoritative_support
                .add_customer_reply(&context, ticket_id, input.body)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            Ok(Response::new(AddTicketReplyResponse {
                ticket: Some(ticket_message(&ticket)),
            }))
        })
    }

    async fn add_ticket_internal_note(
        &self,
        request: Request<AddTicketInternalNoteRequest>,
    ) -> Result<Response<AddTicketInternalNoteResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &context.actor.principal_ref,
                    &access_scope(&context.tenant_scope),
                    SUPPORT_TICKET_INTERNAL_NOTE_PERMISSION,
                )
                .await?;
            let input = request.into_inner();
            let ticket_id = parse_uuid(&input.ticket_id, "ticket_id")?;
            let ticket = self
                .state
                .store
                .get_ticket(ticket_id)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?
                .ok_or_else(|| Status::not_found("support ticket not found"))?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    ticket.customer_account_id,
                    "support",
                    SUPPORT_TICKET_INTERNAL_NOTE_PERMISSION,
                )
                .await?;
            let ticket = self
                .state
                .authoritative_support
                .add_internal_note(&context, ticket_id, input.body)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            Ok(Response::new(AddTicketInternalNoteResponse {
                ticket: Some(ticket_message(&ticket)),
            }))
        })
    }

    async fn create_incident(
        &self,
        request: Request<CreateIncidentRequest>,
    ) -> Result<Response<CreateIncidentResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if !matches!(scope, Scope::Platform) {
                return Err(Status::permission_denied(
                    "incident commands require platform scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    INCIDENT_CREATE_PERMISSION,
                )
                .await?;

            let input = request.into_inner();
            let commander_ref = validate_incident_text(
                &input.commander_ref,
                "commander_ref",
                MAX_INCIDENT_REFERENCE_BYTES,
            )?;
            require_actor_match(
                &claims.principal_ref,
                &commander_ref,
                "incident commander must match the authenticated principal",
            )?;
            let leads = validate_incident_text_list(&input.leads, "leads", MAX_INCIDENT_LEADS)?;
            let affected_services = validate_incident_text_list(
                &input.affected_services,
                "affected_services",
                MAX_INCIDENT_SERVICES,
            )?;
            if input.affected_customer_accounts.len() > MAX_INCIDENT_CUSTOMERS {
                return Err(Status::invalid_argument(
                    "affected_customer_accounts has too many entries",
                ));
            }
            let affected_customer_accounts = input
                .affected_customer_accounts
                .iter()
                .map(|value| parse_uuid(value, "affected_customer_account"))
                .collect::<Result<Vec<_>, _>>()?;
            let request_hash = incident_request_hash(&serde_json::json!({
                "command_id": context.command_id,
                "commander_ref": commander_ref.clone(),
                "leads": leads.clone(),
                "affected_services": affected_services.clone(),
                "affected_customer_accounts": affected_customer_accounts.clone(),
            }))?;
            let mut incident = Incident::new(
                commander_ref,
                leads,
                affected_services,
                affected_customer_accounts,
                Utc::now(),
            )
            .map_err(|_| Status::invalid_argument("incident input is invalid"))?;
            // The command ID is the stable aggregate identity for create
            // retries. This lets SupportPersistence replay the same command
            // without generating a second incident.
            incident.id = context.command_id;
            let persisted = self
                .state
                .support_persistence
                .insert_incident(
                    &incident,
                    &SupportIdempotencyKey::new(context.idempotency_key.clone(), request_hash),
                )
                .await
                .map_err(incident_persistence_status)?;
            self.state
                .store
                .record(erpx_application::AuditEvent {
                    actor_ref: context.actor.principal_ref,
                    customer_account_id: None,
                    action: "incident_created".into(),
                    purpose: context.audit.purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("incident audit unavailable"))?;
            Ok(Response::new(CreateIncidentResponse {
                incident: Some(incident_message(persisted.value, persisted.version)),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn update_incident(
        &self,
        request: Request<UpdateIncidentRequest>,
    ) -> Result<Response<UpdateIncidentResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if !matches!(scope, Scope::Platform) {
                return Err(Status::permission_denied(
                    "incident commands require platform scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    INCIDENT_UPDATE_PERMISSION,
                )
                .await?;

            let input = request.into_inner();
            let incident_id = parse_uuid(&input.incident_id, "incident_id")?;
            let next_status = parse_incident_status(&input.next_status)?;
            let expected_version = context.expected_aggregate_version.ok_or_else(|| {
                Status::failed_precondition("incident update requires expected_aggregate_version")
            })?;
            let current = self
                .state
                .support_persistence
                .get_incident(incident_id)
                .await
                .map_err(incident_persistence_status)?
                .ok_or_else(|| Status::not_found("incident not found"))?;
            let request_hash = incident_request_hash(&serde_json::json!({
                "command_id": context.command_id,
                "incident_id": incident_id,
                "next_status": incident_status_name(next_status),
                "expected_version": expected_version,
                "actor_ref": context.actor.principal_ref.clone(),
            }))?;
            let mut incident = current.value;
            if incident.status == next_status {
                if current.version == expected_version {
                    return Err(Status::failed_precondition(
                        "invalid incident lifecycle transition",
                    ));
                }
                // A same-status request with a stale version is allowed to
                // reach persistence so an already-completed idempotent retry
                // can return its stored result; a fresh command still fails
                // the optimistic concurrency check.
            } else {
                incident
                    .transition(next_status, Utc::now(), &context.actor.principal_ref)
                    .map_err(|_| {
                        Status::failed_precondition("invalid incident lifecycle transition")
                    })?;
            }
            let persisted = self
                .state
                .support_persistence
                .update_incident(
                    &incident,
                    expected_version,
                    &SupportIdempotencyKey::new(context.idempotency_key.clone(), request_hash),
                )
                .await
                .map_err(incident_persistence_status)?;
            self.state
                .store
                .record(erpx_application::AuditEvent {
                    actor_ref: context.actor.principal_ref,
                    customer_account_id: None,
                    action: "incident_transitioned".into(),
                    purpose: context.audit.purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("incident audit unavailable"))?;
            Ok(Response::new(UpdateIncidentResponse {
                incident: Some(incident_message(persisted.value, persisted.version)),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn get_incident(
        &self,
        request: Request<GetIncidentRequest>,
    ) -> Result<Response<GetIncidentResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            let purpose = proto_context
                .audit
                .as_ref()
                .map(|audit| audit.purpose.clone())
                .unwrap_or_default();
            if !matches!(scope, Scope::Platform) {
                return Err(Status::permission_denied(
                    "incident reads require platform scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "support.incident.read",
                )
                .await?;
            let input = request.into_inner();
            let incident_id = parse_uuid(&input.incident_id, "incident_id")?;
            self.state
                .store
                .record(erpx_application::AuditEvent {
                    actor_ref: context.actor,
                    customer_account_id: None,
                    action: "incident_read".into(),
                    purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("incident audit unavailable"))?;
            let incident = self
                .state
                .support_persistence
                .get_incident(incident_id)
                .await
                .map_err(incident_persistence_status)?;
            let mut response = GetIncidentResponse {
                correlation_id: context.correlation_id.to_string(),
                ..Default::default()
            };
            if let Some(persisted) = incident {
                response.incident = Some(incident_message(persisted.value, persisted.version));
                response.present = true;
            }
            Ok(Response::new(response))
        })
    }

    async fn list_incidents(
        &self,
        request: Request<ListIncidentsRequest>,
    ) -> Result<Response<ListIncidentsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            let purpose = proto_context
                .audit
                .as_ref()
                .map(|audit| audit.purpose.clone())
                .unwrap_or_default();
            if !matches!(scope, Scope::Platform) {
                return Err(Status::permission_denied(
                    "incident reads require platform scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "support.incident.read",
                )
                .await?;
            let input = request.into_inner();
            if input.page_size == 0 || input.page_size > MAX_INCIDENT_LIST_PAGE_SIZE {
                return Err(Status::invalid_argument(
                    "incident list page_size must be between 1 and 100",
                ));
            }
            let status = (!input.status.trim().is_empty())
                .then(|| parse_incident_status(input.status.trim()))
                .transpose()?;

            // The shared AuditEvent contract has no separate scope column;
            // the fixed platform scope is explicit in the action while actor,
            // purpose, and correlation remain independently queryable.
            self.state
                .store
                .record(erpx_application::AuditEvent {
                    actor_ref: context.actor,
                    customer_account_id: None,
                    action: "incident_list_read:platform".into(),
                    purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("incident audit unavailable"))?;

            let page = self
                .state
                .support_persistence
                .list_incidents(status, input.page_size)
                .await
                .map_err(incident_persistence_status)?;
            let has_more = page.has_more;
            Ok(Response::new(ListIncidentsResponse {
                incidents: page
                    .incidents
                    .into_iter()
                    .map(|incident| incident_message(incident.value, incident.version))
                    .collect(),
                has_more,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn request_support_session(
        &self,
        request: Request<RequestSupportSessionRequest>,
    ) -> Result<Response<RequestSupportSessionResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &context.actor.principal_ref,
                    &access_scope(&context.tenant_scope),
                    "support.session.request",
                )
                .await?;
            let input = request.into_inner();
            require_actor_match(
                &claims.principal_ref,
                &input.agent_ref,
                "support agent must match the authenticated principal",
            )?;
            let requested_scope = access_scope(&context.tenant_scope);
            let approved_scope = parse_support_scope(&input.scope)?;
            if input.scope != support_scope_reference(&approved_scope) {
                return Err(Status::invalid_argument(
                    "support scope must use canonical formatting",
                ));
            }
            if !approved_scope.contains(&requested_scope) {
                return Err(Status::permission_denied(
                    "support approval scope does not contain the command scope",
                ));
            }
            let mode = parse_mode(&input.mode)?;
            let ticket_id = parse_uuid(&input.ticket_id, "ticket_id")?;
            let customer_id = parse_uuid(&input.customer_id, "customer_id")?;
            let module = if input.module.trim().is_empty() {
                "support"
            } else {
                input.module.as_str()
            };
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "support",
                    "support.session.request",
                )
                .await?;
            let session = self
                .state
                .authoritative_support
                .request_session(
                    &context,
                    ticket_id,
                    customer_id,
                    &input.agent_ref,
                    mode,
                    input.permissions,
                    &input.scope,
                    module,
                    &input.reason,
                    Duration::minutes(i64::from(input.duration_minutes)),
                    (!input.represented_tenant_user.is_empty())
                        .then_some(input.represented_tenant_user),
                )
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            self.state
                .store
                .persist_support_access_request(&session)
                .await
                .map_err(|_| Status::internal("support approval request persistence failed"))?;
            Ok(Response::new(RequestSupportSessionResponse {
                session: Some(to_session_message(session)),
            }))
        })
    }

    async fn approve_support_session(
        &self,
        request: Request<ApproveSupportSessionRequest>,
    ) -> Result<Response<SupportSessionMessage>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            if context.actor.principal_ref != claims.principal_ref {
                return Err(Status::permission_denied(
                    "customer approver must match the authenticated principal",
                ));
            }
            self.state
                .auth
                .authorize_customer_support_approver(
                    &self.state.store,
                    &claims.principal_ref,
                    &access_scope(&context.tenant_scope),
                )
                .await?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &context.actor.principal_ref,
                    &access_scope(&context.tenant_scope),
                    "support.session.approve",
                )
                .await?;
            let token = request
                .metadata()
                .get("x-erpx-customer-approval")
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| Status::unauthenticated("customer approval metadata is required"))?
                .to_owned();
            let input = request.into_inner();
            require_actor_match(
                &claims.principal_ref,
                &input.customer_approver,
                "customer approver must match the authenticated principal",
            )?;
            let id = parse_uuid(&input.session_id, "session_id")?;
            let session = self
                .state
                .store
                .get_session(id)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?
                .ok_or_else(|| Status::not_found("support session not found"))?;
            let requested_scope = parse_support_scope(&session.scope)?;
            if session.scope != support_scope_reference(&requested_scope) {
                return Err(Status::failed_precondition(
                    "stored support session scope is not canonical",
                ));
            }
            let approver_scope = access_scope(&context.tenant_scope);
            if !approver_scope.contains(&requested_scope) {
                return Err(Status::permission_denied(
                    "customer approval scope does not contain the requested support scope",
                ));
            }
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    session.customer_account_id,
                    "support",
                    "support.session.approve",
                )
                .await?;
            verify_customer_approval(&token, id, session.customer_account_id)?;
            let session = self
                .state
                .authoritative_support
                .approve_session(&context, id, &input.customer_approver)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            let active_device_id = self
                .state
                .auth
                .identity
                .active_session_device_id(claims.session_id, Utc::now())
                .await
                .map_err(map_identity_auth_error)?;
            let device_ref = active_device_id.to_string();
            validate_authenticated_device_ref(active_device_id, &input.device_ref)?;
            let approved_at = session
                .approved_at
                .ok_or_else(|| Status::internal("approved support session has no timestamp"))?;
            let duration = session.expires_at.signed_duration_since(approved_at);
            let binding = SupportCapabilityBinding::new(
                session.customer_account_id,
                session.agent_ref.clone(),
                &device_ref,
                session.id,
                session.ticket_id,
                session.mode,
                session.permissions.clone(),
                session.scope.clone(),
                session.module.clone(),
            )
            .map_err(|_| Status::invalid_argument("support capability binding is invalid"))?;
            let (capability, secret) = SupportCapability::issue(binding, approved_at, duration)
                .map_err(|_| Status::failed_precondition("support capability is expired"))?;
            self.state
                .store
                .persist_approved_support_capability(
                    &session,
                    &capability,
                    &hex::encode(Sha256::digest(token.as_bytes())),
                )
                .await
                .map_err(|_| {
                    Status::internal("support approval and capability persistence failed")
                })?;
            let mut response = to_session_message(session);
            response.capability_id = capability.id.to_string();
            response.capability_secret = secret.as_hex();
            Ok(Response::new(response))
        })
    }

    async fn get_support_diagnostics(
        &self,
        request: Request<GetSupportDiagnosticsRequest>,
    ) -> Result<Response<GetSupportDiagnosticsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            let customer_account_id = match request.get_ref().access.as_ref() {
                None => {
                    // Preserve the original platform-only operational
                    // diagnostics contract for older clients. A missing
                    // access context can never select tenant data because
                    // platform scope and the platform role are still exact.
                    require_platform_scope(&scope)?;
                    self.state
                        .auth
                        .authorize_role(
                            &self.state.store,
                            &claims.principal_ref,
                            &scope,
                            SUPPORT_DIAGNOSTICS_PERMISSION,
                        )
                        .await?;
                    None
                }
                Some(access) => match access.mode {
                    1 => {
                        platform_diagnostics_access_is_empty(access)?;
                        require_platform_scope(&scope)?;
                        self.state
                            .auth
                            .authorize_role(
                                &self.state.store,
                                &claims.principal_ref,
                                &scope,
                                SUPPORT_DIAGNOSTICS_PERMISSION,
                            )
                            .await?;
                        None
                    }
                    2 => Some(
                        self.state
                            .authorize_support_diagnostics(&claims, &scope, access)
                            .await?,
                    ),
                    _ => {
                        return Err(Status::invalid_argument(
                            "diagnostics access mode must be platform or customer_support",
                        ));
                    }
                },
            };
            let purpose = validate_read_purpose(&request.get_ref().purpose)?.to_owned();
            let captured_at = Utc::now();
            let control_ready = self.state.store.check_ready().await.is_ok();
            let identity_ready = self.state.auth.identity.check_ready().await.is_ok();
            let mut input = DiagnosticsSnapshotInput::new(captured_at);
            input.add_version("server", env!("CARGO_PKG_VERSION"));
            input.set_worker_health(
                if control_ready && identity_ready {
                    ReadinessObservationState::NotProbed
                } else {
                    ReadinessObservationState::Unavailable
                },
                Some(captured_at),
                Some(0),
                Some(if control_ready && identity_ready {
                    "worker_observation_not_available"
                } else {
                    "control_plane_or_identity_unavailable"
                }),
            );
            let snapshot = input
                .build(DiagnosticsLimits::default())
                .map_err(|_| Status::internal("diagnostics snapshot exceeded its safety bound"))?;
            let payload_json = serde_json::to_vec(&snapshot)
                .map_err(|_| Status::internal("diagnostics serialization failed"))?;
            self.state
                .store
                .record(erpx_application::AuditEvent {
                    actor_ref: context.actor,
                    customer_account_id,
                    action: "support.diagnostics.read".into(),
                    purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("diagnostics audit persistence failed"))?;
            Ok(Response::new(GetSupportDiagnosticsResponse {
                payload_json,
                schema_version: snapshot.schema_version().into(),
                correlation_id: context.correlation_id.to_string(),
                fields: support_diagnostics_transport_fields(&snapshot),
                captured_at: snapshot.captured_at().to_rfc3339(),
            }))
        })
    }

    async fn revoke_support_session(
        &self,
        request: Request<RevokeSupportSessionRequest>,
    ) -> Result<Response<()>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &context.actor.principal_ref,
                    &access_scope(&context.tenant_scope),
                    "support.session.revoke",
                )
                .await?;
            require_actor_match(
                &claims.principal_ref,
                &request.get_ref().actor_ref,
                "revoking actor must match the authenticated principal",
            )?;
            let id = parse_uuid(&request.get_ref().session_id, "session_id")?;
            let session = self
                .state
                .store
                .get_session(id)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?
                .ok_or_else(|| Status::not_found("support session not found"))?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    session.customer_account_id,
                    "support",
                    "support.session.revoke",
                )
                .await?;
            self.state
                .authoritative_support
                .revoke_session(&context, id)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            self.state
                .store
                .revoke_support_capabilities_for_session(id, Utc::now())
                .await
                .map_err(|_| Status::internal("support capability revocation failed"))?;
            tracing::info!(session_id = %id, actor = %context.actor.principal_ref, "support session revoked");
            Ok(Response::new(()))
        })
    }
}

#[derive(Clone)]
struct RoleRpcImpl {
    state: AppState,
}

#[tonic::async_trait]
impl RoleLifecycleRpc for RoleRpcImpl {
    async fn list_expiring_roles(
        &self,
        request: Request<ListExpiringRolesRequest>,
    ) -> Result<Response<ListExpiringRolesResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "role.assignment.read",
                )
                .await?;
            let assignments = self
                .state
                .roles
                .list_expiring_in_scope(
                    Duration::days(i64::from(request.get_ref().within_days)),
                    &scope,
                )
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            Ok(Response::new(ListExpiringRolesResponse {
                assignments: assignments.into_iter().map(to_role_message).collect(),
            }))
        })
    }

    async fn renew_role(
        &self,
        request: Request<RenewRoleRequest>,
    ) -> Result<Response<RenewRoleResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let requested_scope = access_scope(&context.tenant_scope);
            let input = request.into_inner();
            require_actor_match(
                &claims.principal_ref,
                &input.grantor,
                "role grantor must match the authenticated principal",
            )?;
            let customer_id = (!input.customer_id.trim().is_empty())
                .then(|| parse_uuid(&input.customer_id, "customer_id"))
                .transpose()?;
            self.state
                .authorize_role_lifecycle_command(&claims, &context, customer_id, &input.module_key)
                .await?;
            let id = parse_uuid(&input.assignment_id, "assignment_id")?;
            let current = self
                .state
                .store
                .get(id)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?
                .ok_or_else(|| Status::not_found("role assignment not found"))?;
            // The command actor may renew only assignments fully contained by the
            // actor's command scope. Reversing this relation would let a narrow
            // branch operator renew a holding/company-wide assignment.
            if !requested_scope.contains(&current.scope) {
                return Err(Status::permission_denied(
                    "role assignment is outside the command scope",
                ));
            }
            if !input.approver.trim().is_empty() {
                self.state
                    .auth
                    .authorize_independent_role(
                        &self.state.store,
                        &input.approver,
                        &current.scope,
                        "role.assignment.approve",
                        &claims.principal_ref,
                    )
                    .await?;
            }
            let valid_until = DateTime::parse_from_rfc3339(&input.valid_until)
                .map_err(|_| Status::invalid_argument("valid_until must be RFC3339"))?
                .with_timezone(&Utc);
            let assignment = self
                .state
                .authoritative_roles
                .renew(
                    &context,
                    id,
                    valid_until,
                    &input.grantor,
                    (!input.approver.is_empty()).then_some(input.approver),
                )
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            Ok(Response::new(RenewRoleResponse {
                assignment: Some(to_role_message(assignment)),
            }))
        })
    }

    async fn list_role_review(
        &self,
        request: Request<ListRoleReviewRequest>,
    ) -> Result<Response<ListRoleReviewResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "role.assignment.review",
                )
                .await?;
            let within_days = request.get_ref().within_days;
            if within_days == 0 || within_days > MAX_ROLE_REVIEW_DAYS {
                return Err(Status::invalid_argument(format!(
                    "within_days must be between 1 and {MAX_ROLE_REVIEW_DAYS}"
                )));
            }
            let queue = self
                .state
                .authoritative_roles
                .review_queue(Duration::days(i64::from(within_days)))
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            let items = queue
                .items
                .into_iter()
                .filter(|item| scope.contains(&item.assignment.scope))
                .map(to_role_review_message)
                .collect();
            Ok(Response::new(ListRoleReviewResponse {
                evaluated_at: queue.evaluated_at.to_rfc3339(),
                expiry_horizon_until: queue.expiry_horizon_until.to_rfc3339(),
                items,
            }))
        })
    }

    async fn recover_final_role(
        &self,
        request: Request<RecoverFinalRoleRequest>,
    ) -> Result<Response<RecoverFinalRoleResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let requested_scope = access_scope(&context.tenant_scope);
            let input = request.into_inner();
            require_actor_match(
                &claims.principal_ref,
                &input.requester_ref,
                "recovery requester must match the authenticated principal",
            )?;
            let target = match input.target.as_str() {
                "platform_root" => {
                    if !input.holding_id.trim().is_empty() {
                        return Err(Status::invalid_argument(
                            "holding_id is not valid for platform_root recovery",
                        ));
                    }
                    RoleRecoveryTarget::PlatformRoot
                }
                "holding_owner" => RoleRecoveryTarget::HoldingOwner {
                    holding_id: parse_uuid(&input.holding_id, "holding_id")?,
                },
                _ => return Err(Status::invalid_argument("unsupported recovery target")),
            };
            if !requested_scope.contains(&target_scope(&target)) {
                return Err(Status::permission_denied(
                    "recovery target is outside the command scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &requested_scope,
                    "role.assignment.recover",
                )
                .await?;
            let valid_until = DateTime::parse_from_rfc3339(&input.valid_until)
                .map_err(|_| Status::invalid_argument("valid_until must be RFC3339"))?
                .with_timezone(&Utc);
            let authority_level = u8::try_from(input.authority_level)
                .map_err(|_| Status::invalid_argument("authority_level is out of range"))?;
            let recovery = RoleRecoveryRequest {
                request_id: parse_uuid(&input.request_id, "request_id")?,
                target,
                principal_ref: input.principal_ref,
                permissions: input.permissions,
                functional_domain: input.functional_domain,
                authority_level,
                valid_from: Utc::now(),
                valid_until,
                timezone: input.timezone,
                grant_reason: input.reason,
                ticket_reference: (!input.ticket_reference.trim().is_empty())
                    .then_some(input.ticket_reference),
                requester_ref: input.requester_ref,
                approver_ref: input.approver_ref,
                reviewer_ref: (!input.reviewer_ref.trim().is_empty()).then_some(input.reviewer_ref),
            };
            self.state
                .auth
                .authorize_independent_role(
                    &self.state.store,
                    &recovery.approver_ref,
                    &target_scope(&recovery.target),
                    "role.assignment.approve",
                    &claims.principal_ref,
                )
                .await?;
            if let Some(reviewer_ref) = recovery.reviewer_ref.as_deref() {
                self.state
                    .auth
                    .authorize_independent_role(
                        &self.state.store,
                        reviewer_ref,
                        &target_scope(&recovery.target),
                        "role.assignment.review",
                        &claims.principal_ref,
                    )
                    .await?;
            }
            let mut recovery_receipt_request = recovery.clone();
            let assignment = self
                .state
                .authoritative_roles
                .recover_final_role(&context, recovery)
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            // The authoritative role service replaces the transport-supplied
            // start instant with its write-side clock. Bind the durable
            // recovery receipt to the committed assignment, not to the
            // untrusted request timestamp captured before the command.
            recovery_receipt_request.valid_from = assignment.valid_from;
            RoleReviewPersistence::new(self.state.store.pools().write.clone())
                .record_recovery_receipt(
                    &recovery_receipt_request,
                    u64::from(assignment.assignment_version),
                    "approved",
                    &context.idempotency_key,
                    &context.actor.principal_ref,
                    context.correlation_id,
                )
                .await
                .map_err(|_| {
                    Status::unavailable("role recovery receipt persistence unavailable")
                })?;
            Ok(Response::new(RecoverFinalRoleResponse {
                assignment: Some(to_role_message(assignment)),
            }))
        })
    }
}

#[derive(Clone)]
struct FinanceRpcImpl {
    state: AppState,
}

#[derive(Clone)]
struct HrRpcImpl {
    state: AppState,
}

impl HrRpcImpl {
    async fn save_employee(
        &self,
        request: Request<SaveHrEmployeeRequest>,
    ) -> Result<Response<SaveHrEmployeeResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "HR writes require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            if input.employee_json.is_empty() || input.employee_json.len() > 256 * 1024 {
                return Err(Status::invalid_argument("employee_json is out of bounds"));
            }
            let employee: EmployeeRecord = serde_json::from_slice(&input.employee_json)
                .map_err(|_| Status::invalid_argument("employee_json is invalid"))?;
            if employee.last_command_id != context.command_id
                || employee.last_idempotency_key != context.idempotency_key
                || access_scope(&employee.scope.tenant_scope()) != scope
            {
                return Err(Status::permission_denied(
                    "HR employee identity or command scope does not match context",
                ));
            }
            let expected_version = if input.has_expected_version {
                if input.expected_version == 0 {
                    return Err(Status::invalid_argument(
                        "expected_version must be positive",
                    ));
                }
                Some(input.expected_version)
            } else {
                None
            };
            if expected_version != context.expected_aggregate_version {
                return Err(Status::invalid_argument(
                    "expected_version does not match command context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "hr", "hr.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let result = async {
                let store = SqlxHrPersistence::new(write.database_pool().clone(), holding_ref)
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                let receipt = store
                    .save_employee(&employee, expected_version)
                    .await
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "hr.employee.saved".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("HR audit persistence unavailable"))?;
                Ok::<SaveHrEmployeeResponse, Status>(SaveHrEmployeeResponse {
                    receipt: Some(erpx_proto::platform::HrWriteReceipt {
                        aggregate_type: receipt.aggregate_type.as_str().into(),
                        aggregate_id: receipt.aggregate_id.to_string(),
                        version: receipt.version,
                        idempotency_key: receipt.idempotency_key,
                        replayed: receipt.replayed,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn transition_employee(
        &self,
        request: Request<TransitionHrEmployeeRequest>,
    ) -> Result<Response<TransitionHrEmployeeResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "HR transitions require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let employee_id = parse_uuid(&input.employee_id, "employee_id")?;
            if input.transition.trim().is_empty() || input.transition.len() > 64 {
                return Err(Status::invalid_argument(
                    "employee transition is outside its bound",
                ));
            }
            if input.expected_version == 0
                || context.expected_aggregate_version != Some(input.expected_version)
            {
                return Err(Status::invalid_argument(
                    "expected_version must be positive and match command context",
                ));
            }
            let terminated_on = if input.terminated_on.trim().is_empty() {
                None
            } else {
                Some(
                    NaiveDate::parse_from_str(input.terminated_on.trim(), "%Y-%m-%d")
                        .map_err(|_| Status::invalid_argument("terminated_on is invalid"))?,
                )
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "hr", "hr.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let result = async {
                let store = SqlxHrPersistence::new(write.database_pool().clone(), holding_ref)
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                let receipt = store
                    .transition_employee(
                        employee_id,
                        &input.transition,
                        terminated_on,
                        input.expected_version,
                        &context,
                    )
                    .await
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "hr.employee.transitioned".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("HR audit persistence unavailable"))?;
                Ok::<TransitionHrEmployeeResponse, Status>(TransitionHrEmployeeResponse {
                    receipt: Some(erpx_proto::platform::HrWriteReceipt {
                        aggregate_type: receipt.aggregate_type,
                        aggregate_id: receipt.aggregate_id.to_string(),
                        version: receipt.version,
                        idempotency_key: receipt.idempotency_key,
                        replayed: receipt.replayed,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn save_leave(
        &self,
        request: Request<SaveHrLeaveRequest>,
    ) -> Result<Response<SaveHrLeaveResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "HR writes require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            if input.leave_json.is_empty() || input.leave_json.len() > 256 * 1024 {
                return Err(Status::invalid_argument("leave_json is out of bounds"));
            }
            let leave: LeaveRequest = serde_json::from_slice(&input.leave_json)
                .map_err(|_| Status::invalid_argument("leave_json is invalid"))?;
            if leave.last_command_id != context.command_id
                || access_scope(&leave.scope.tenant_scope()) != scope
            {
                return Err(Status::permission_denied(
                    "HR leave identity or command scope does not match context",
                ));
            }
            let expected_version = if input.has_expected_version {
                if input.expected_version == 0 {
                    return Err(Status::invalid_argument(
                        "expected_version must be positive",
                    ));
                }
                Some(input.expected_version)
            } else {
                None
            };
            if expected_version != context.expected_aggregate_version {
                return Err(Status::invalid_argument(
                    "expected_version does not match command context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "hr", "hr.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let result = async {
                let store = SqlxHrPersistence::new(write.database_pool().clone(), holding_ref)
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                let receipt = store
                    .save_leave(&leave, &context.idempotency_key, expected_version)
                    .await
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "hr.leave.saved".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("HR audit persistence unavailable"))?;
                Ok::<SaveHrLeaveResponse, Status>(SaveHrLeaveResponse {
                    receipt: Some(erpx_proto::platform::HrWriteReceipt {
                        aggregate_type: receipt.aggregate_type.as_str().into(),
                        aggregate_id: receipt.aggregate_id.to_string(),
                        version: receipt.version,
                        idempotency_key: receipt.idempotency_key,
                        replayed: receipt.replayed,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn save_expense(
        &self,
        request: Request<SaveHrExpenseRequest>,
    ) -> Result<Response<SaveHrExpenseResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "HR writes require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            if input.expense_json.is_empty() || input.expense_json.len() > 256 * 1024 {
                return Err(Status::invalid_argument("expense_json is out of bounds"));
            }
            let expense: ExpenseClaim = serde_json::from_slice(&input.expense_json)
                .map_err(|_| Status::invalid_argument("expense_json is invalid"))?;
            if access_scope(&expense.scope.tenant_scope()) != scope {
                return Err(Status::permission_denied(
                    "HR expense scope does not match context",
                ));
            }
            let expected_version = if input.has_expected_version {
                if input.expected_version == 0 {
                    return Err(Status::invalid_argument(
                        "expected_version must be positive",
                    ));
                }
                Some(input.expected_version)
            } else {
                None
            };
            if expected_version != context.expected_aggregate_version {
                return Err(Status::invalid_argument(
                    "expected_version does not match command context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "hr", "hr.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let result = async {
                let store = SqlxHrPersistence::new(write.database_pool().clone(), holding_ref)
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                let receipt = store
                    .save_expense(&expense, &context.idempotency_key, expected_version)
                    .await
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "hr.expense.saved".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("HR audit persistence unavailable"))?;
                Ok::<SaveHrExpenseResponse, Status>(SaveHrExpenseResponse {
                    receipt: Some(erpx_proto::platform::HrWriteReceipt {
                        aggregate_type: receipt.aggregate_type.as_str().into(),
                        aggregate_id: receipt.aggregate_id.to_string(),
                        version: receipt.version,
                        idempotency_key: receipt.idempotency_key,
                        replayed: receipt.replayed,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn save_payroll(
        &self,
        request: Request<SaveHrPayrollRequest>,
    ) -> Result<Response<SaveHrPayrollResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "HR writes require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            if input.payroll_json.is_empty() || input.payroll_json.len() > 512 * 1024 {
                return Err(Status::invalid_argument("payroll_json is out of bounds"));
            }
            let payroll: PayrollRun = serde_json::from_slice(&input.payroll_json)
                .map_err(|_| Status::invalid_argument("payroll_json is invalid"))?;
            if access_scope(&payroll.scope.tenant_scope()) != scope {
                return Err(Status::permission_denied(
                    "HR payroll scope does not match context",
                ));
            }
            let expected_version = if input.has_expected_version {
                if input.expected_version == 0 {
                    return Err(Status::invalid_argument(
                        "expected_version must be positive",
                    ));
                }
                Some(input.expected_version)
            } else {
                None
            };
            if expected_version != context.expected_aggregate_version {
                return Err(Status::invalid_argument(
                    "expected_version does not match command context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "hr", "hr.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let result = async {
                let store = SqlxHrPersistence::new(write.database_pool().clone(), holding_ref)
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                let receipt = store
                    .save_payroll(&payroll, &context.idempotency_key, expected_version)
                    .await
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "hr.payroll.saved".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("HR audit persistence unavailable"))?;
                Ok::<SaveHrPayrollResponse, Status>(SaveHrPayrollResponse {
                    receipt: Some(erpx_proto::platform::HrWriteReceipt {
                        aggregate_type: receipt.aggregate_type,
                        aggregate_id: receipt.aggregate_id.to_string(),
                        version: receipt.version,
                        idempotency_key: receipt.idempotency_key,
                        replayed: receipt.replayed,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn list_leave_requests(
        &self,
        request: Request<ListHrLeaveRequestsRequest>,
    ) -> Result<Response<ListHrLeaveRequestsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            if matches!(caller_scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "HR reads require an explicit holding, company, or branch scope",
                ));
            }
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let branch_id = if input.branch_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.branch_id, "branch_id")?)
            };
            let requested_scope = match branch_id {
                Some(branch_id) => Scope::Branch {
                    holding_id,
                    company_id,
                    branch_id,
                },
                None => Scope::Company {
                    holding_id,
                    company_id,
                },
            };
            if !caller_scope.contains(&requested_scope) {
                return Err(Status::permission_denied("HR read scope is not contained"));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "hr.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let page_size = input.page_size;
            if page_size == 0 || page_size > MAX_HR_READ_PAGE_SIZE {
                return Err(Status::invalid_argument(format!(
                    "page_size must be 1..={MAX_HR_READ_PAGE_SIZE}"
                )));
            }
            let before_cursor = if input.before_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_hr_leave_cursor(&input.before_cursor)?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &requested_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxHrReadStore::new(read.database_pool().clone(), holding_ref)
                    .map_err(|_| Status::internal("HR read store configuration is invalid"))?;
                let page = store
                    .list_leave_requests(
                        HrReadScope {
                            holding_id,
                            company_id,
                            branch_id,
                        },
                        page_size,
                        before_cursor.as_ref(),
                    )
                    .await
                    .map_err(|_| Status::internal("HR leave read failed"))?;
                let leave_requests = page
                    .rows
                    .into_iter()
                    .map(|row| erpx_proto::platform::HrLeaveReadRow {
                        leave_id: row.leave_id.to_string(),
                        holding_id: row.holding_id.to_string(),
                        company_id: row.company_id.to_string(),
                        branch_id: row
                            .branch_id
                            .map(|value| value.to_string())
                            .unwrap_or_default(),
                        employee_id: row.employee_id.to_string(),
                        starts_on: row.starts_on.to_string(),
                        ends_on: row.ends_on.to_string(),
                        reason: row.reason,
                        status: row.status,
                        requested_by: row.requested_by,
                        approved_by: row.approved_by.unwrap_or_default(),
                        source_version: row.source_version,
                        source_event_id: row.source_event_id.to_string(),
                        projected_at: row.projected_at.to_rfc3339(),
                    })
                    .collect();
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "hr.leave.read".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "HR leave read".into()),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("HR audit persistence unavailable"))?;
                Ok::<ListHrLeaveRequestsResponse, Status>(ListHrLeaveRequestsResponse {
                    leave_requests,
                    has_more: page.has_more,
                    next_cursor: page
                        .next_cursor
                        .as_ref()
                        .map(encode_hr_leave_cursor)
                        .unwrap_or_default(),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn list_expense_claims(
        &self,
        request: Request<ListHrExpenseClaimsRequest>,
    ) -> Result<Response<ListHrExpenseClaimsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            if matches!(caller_scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "HR reads require an explicit holding, company, or branch scope",
                ));
            }
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let branch_id = if input.branch_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.branch_id, "branch_id")?)
            };
            let requested_scope = match branch_id {
                Some(branch_id) => Scope::Branch {
                    holding_id,
                    company_id,
                    branch_id,
                },
                None => Scope::Company {
                    holding_id,
                    company_id,
                },
            };
            if !caller_scope.contains(&requested_scope) {
                return Err(Status::permission_denied("HR read scope is not contained"));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "hr.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let page_size = input.page_size;
            if page_size == 0 || page_size > MAX_HR_READ_PAGE_SIZE {
                return Err(Status::invalid_argument(format!(
                    "page_size must be 1..={MAX_HR_READ_PAGE_SIZE}"
                )));
            }
            let before_cursor = if input.before_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_hr_expense_cursor(&input.before_cursor)?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &requested_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxHrReadStore::new(read.database_pool().clone(), holding_ref)
                    .map_err(|_| Status::internal("HR read store configuration is invalid"))?;
                let page = store
                    .list_expense_claims(
                        HrReadScope {
                            holding_id,
                            company_id,
                            branch_id,
                        },
                        page_size,
                        before_cursor.as_ref(),
                    )
                    .await
                    .map_err(|_| Status::internal("HR expense read failed"))?;
                let expenses = page
                    .rows
                    .into_iter()
                    .map(|row| erpx_proto::platform::HrExpenseReadRow {
                        expense_id: row.expense_id.to_string(),
                        holding_id: row.holding_id.to_string(),
                        company_id: row.company_id.to_string(),
                        branch_id: row
                            .branch_id
                            .map(|value| value.to_string())
                            .unwrap_or_default(),
                        employee_id: row.employee_id.to_string(),
                        currency: row.currency,
                        amount_minor: row.amount_minor,
                        status: row.status,
                        submitted_by: row.submitted_by,
                        approved_by: row.approved_by.unwrap_or_default(),
                        source_version: row.source_version,
                        source_event_id: row.source_event_id.to_string(),
                        projected_at: row.projected_at.to_rfc3339(),
                    })
                    .collect();
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "hr.expense.read".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "HR expense read".into()),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("HR audit persistence unavailable"))?;
                Ok::<ListHrExpenseClaimsResponse, Status>(ListHrExpenseClaimsResponse {
                    expenses,
                    has_more: page.has_more,
                    next_cursor: page
                        .next_cursor
                        .as_ref()
                        .map(encode_hr_expense_cursor)
                        .unwrap_or_default(),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn list_payroll_runs(
        &self,
        request: Request<ListHrPayrollRunsRequest>,
    ) -> Result<Response<ListHrPayrollRunsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            if matches!(caller_scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "HR reads require an explicit holding, company, or branch scope",
                ));
            }
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let branch_id = if input.branch_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.branch_id, "branch_id")?)
            };
            let requested_scope = match branch_id {
                Some(branch_id) => Scope::Branch {
                    holding_id,
                    company_id,
                    branch_id,
                },
                None => Scope::Company {
                    holding_id,
                    company_id,
                },
            };
            if !caller_scope.contains(&requested_scope) {
                return Err(Status::permission_denied("HR read scope is not contained"));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "hr.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let page_size = input.page_size;
            if page_size == 0 || page_size > MAX_HR_READ_PAGE_SIZE {
                return Err(Status::invalid_argument(format!(
                    "page_size must be 1..={MAX_HR_READ_PAGE_SIZE}"
                )));
            }
            let line_page_size = input.line_page_size;
            if line_page_size == 0 || line_page_size > MAX_HR_READ_PAYROLL_LINES_PER_RUN {
                return Err(Status::invalid_argument(format!(
                    "line_page_size must be 1..={MAX_HR_READ_PAYROLL_LINES_PER_RUN}"
                )));
            }
            let before_cursor = if input.before_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_hr_payroll_cursor(&input.before_cursor)?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &requested_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxHrReadStore::new(read.database_pool().clone(), holding_ref)
                    .map_err(|_| Status::internal("HR read store configuration is invalid"))?;
                let page = store
                    .list_payroll_runs(
                        HrReadScope {
                            holding_id,
                            company_id,
                            branch_id,
                        },
                        page_size,
                        line_page_size,
                        before_cursor.as_ref(),
                    )
                    .await
                    .map_err(|_| Status::internal("HR payroll read failed"))?;
                let payroll_runs = page
                    .rows
                    .into_iter()
                    .map(|row| erpx_proto::platform::HrPayrollRunReadRow {
                        payroll_id: row.payroll_id.to_string(),
                        holding_id: row.holding_id.to_string(),
                        company_id: row.company_id.to_string(),
                        branch_id: row
                            .branch_id
                            .map(|value| value.to_string())
                            .unwrap_or_default(),
                        starts_on: row.starts_on.to_string(),
                        ends_on: row.ends_on.to_string(),
                        status: row.status,
                        prepared_by: row.prepared_by,
                        reviewed_by: row.reviewed_by.unwrap_or_default(),
                        approved_by: row.approved_by.unwrap_or_default(),
                        total_gross_minor: row.total_gross_minor,
                        total_deductions_minor: row.total_deductions_minor,
                        total_net_minor: row.total_net_minor,
                        source_version: row.source_version,
                        source_event_id: row.source_event_id.to_string(),
                        projected_at: row.projected_at.to_rfc3339(),
                        lines: row
                            .lines
                            .into_iter()
                            .map(|line| erpx_proto::platform::HrPayrollLineReadRow {
                                line_no: line.line_no,
                                holding_id: line.holding_id.to_string(),
                                company_id: line.company_id.to_string(),
                                branch_id: line
                                    .branch_id
                                    .map(|value| value.to_string())
                                    .unwrap_or_default(),
                                employee_id: line.employee_id.to_string(),
                                gross_minor: line.gross_minor,
                                deductions_minor: line.deductions_minor,
                                net_minor: line.net_minor,
                                source_version: line.source_version,
                                source_event_id: line.source_event_id.to_string(),
                                projected_at: line.projected_at.to_rfc3339(),
                            })
                            .collect(),
                        lines_truncated: row.lines_truncated,
                    })
                    .collect();
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "hr.payroll.read".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "HR payroll read".into()),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("HR audit persistence unavailable"))?;
                Ok::<ListHrPayrollRunsResponse, Status>(ListHrPayrollRunsResponse {
                    payroll_runs,
                    has_more: page.has_more,
                    next_cursor: page
                        .next_cursor
                        .as_ref()
                        .map(encode_hr_payroll_cursor)
                        .unwrap_or_default(),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn list_reassignments(
        &self,
        request: Request<ListHrReassignmentsRequest>,
    ) -> Result<Response<ListHrReassignmentsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            if matches!(caller_scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "HR reads require an explicit holding, company, or branch scope",
                ));
            }
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let branch_id = if input.branch_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.branch_id, "branch_id")?)
            };
            let requested_scope = match branch_id {
                Some(branch_id) => Scope::Branch {
                    holding_id,
                    company_id,
                    branch_id,
                },
                None => Scope::Company {
                    holding_id,
                    company_id,
                },
            };
            if !caller_scope.contains(&requested_scope) {
                return Err(Status::permission_denied("HR read scope is not contained"));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "hr.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let page_size = input.page_size;
            if page_size == 0 || page_size > MAX_HR_READ_PAGE_SIZE {
                return Err(Status::invalid_argument(format!(
                    "page_size must be 1..={MAX_HR_READ_PAGE_SIZE}"
                )));
            }
            let before_cursor = if input.before_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_hr_reassignment_cursor(&input.before_cursor)?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &requested_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxHrReadStore::new(read.database_pool().clone(), holding_ref)
                    .map_err(|_| Status::internal("HR read store configuration is invalid"))?;
                let page = store
                    .list_reassignments(
                        HrReadScope {
                            holding_id,
                            company_id,
                            branch_id,
                        },
                        page_size,
                        before_cursor.as_ref(),
                    )
                    .await
                    .map_err(|_| Status::internal("HR reassignment read failed"))?;
                let reassignments = page
                    .rows
                    .into_iter()
                    .map(|row| erpx_proto::platform::HrReassignmentReadRow {
                        workflow_ref: row.workflow_ref,
                        holding_id: row.holding_id.to_string(),
                        company_id: row.company_id.to_string(),
                        branch_id: row.branch_id.map(|id| id.to_string()).unwrap_or_default(),
                        expired_principal_ref: row.expired_principal_ref,
                        replacement_principal_ref: row.replacement_principal_ref,
                        reason: row.reason,
                        reassigned_at: row.reassigned_at.to_rfc3339(),
                        source_version: row.source_version,
                        source_event_id: row.source_event_id.to_string(),
                        projected_at: row.projected_at.to_rfc3339(),
                    })
                    .collect();
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "hr.reassignment.read".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "HR reassignment read".into()),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("HR audit persistence unavailable"))?;
                Ok::<ListHrReassignmentsResponse, Status>(ListHrReassignmentsResponse {
                    reassignments,
                    has_more: page.has_more,
                    next_cursor: page
                        .next_cursor
                        .as_ref()
                        .map(encode_hr_reassignment_cursor)
                        .unwrap_or_default(),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn list_employees(
        &self,
        request: Request<ListHrEmployeesRequest>,
    ) -> Result<Response<ListHrEmployeesResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            if matches!(caller_scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "HR reads require an explicit holding, company, or branch scope",
                ));
            }
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let branch_id = if input.branch_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.branch_id, "branch_id")?)
            };
            let requested_scope = match branch_id {
                Some(branch_id) => Scope::Branch {
                    holding_id,
                    company_id,
                    branch_id,
                },
                None => Scope::Company {
                    holding_id,
                    company_id,
                },
            };
            if !caller_scope.contains(&requested_scope) {
                return Err(Status::permission_denied("HR read scope is not contained"));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "hr.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let page_size = input.page_size;
            if page_size == 0 || page_size > MAX_HR_READ_PAGE_SIZE {
                return Err(Status::invalid_argument(format!(
                    "page_size must be 1..={MAX_HR_READ_PAGE_SIZE}"
                )));
            }
            let before_employee_id = if input.before_employee_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.before_employee_id, "before_employee_id")?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &requested_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxHrReadStore::new(read.database_pool().clone(), holding_ref)
                    .map_err(|_| Status::internal("HR read store configuration is invalid"))?;
                let page = store
                    .list_employees(
                        HrReadScope {
                            holding_id,
                            company_id,
                            branch_id,
                        },
                        page_size,
                        before_employee_id,
                    )
                    .await
                    .map_err(|_| Status::internal("HR employee read failed"))?;
                let employees = page
                    .rows
                    .into_iter()
                    .map(|row| erpx_proto::platform::HrEmployeeReadRow {
                        employee_id: row.employee_id.to_string(),
                        holding_id: row.holding_id.to_string(),
                        company_id: row.company_id.to_string(),
                        branch_id: row
                            .branch_id
                            .map(|value| value.to_string())
                            .unwrap_or_default(),
                        employee_ref: row.employee_ref,
                        display_name: row.display_name,
                        status: row.status,
                        hired_on: row.hired_on.to_string(),
                        terminated_on: row
                            .terminated_on
                            .map(|value| value.to_string())
                            .unwrap_or_default(),
                        source_version: row.source_version,
                        source_event_id: row.source_event_id.to_string(),
                        projected_at: row.projected_at.to_rfc3339(),
                    })
                    .collect();
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "hr.employee.read".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "HR employee read".into()),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("HR audit persistence unavailable"))?;
                Ok::<ListHrEmployeesResponse, Status>(ListHrEmployeesResponse {
                    employees,
                    has_more: page.has_more,
                    next_before_employee_id: page
                        .next_before_employee_id
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }
}

#[tonic::async_trait]
impl HrService for HrRpcImpl {
    async fn save_hr_employee(
        &self,
        request: Request<SaveHrEmployeeRequest>,
    ) -> Result<Response<SaveHrEmployeeResponse>, Status> {
        self.save_employee(request).await
    }

    async fn transition_hr_employee(
        &self,
        request: Request<TransitionHrEmployeeRequest>,
    ) -> Result<Response<TransitionHrEmployeeResponse>, Status> {
        self.transition_employee(request).await
    }

    async fn save_hr_leave(
        &self,
        request: Request<SaveHrLeaveRequest>,
    ) -> Result<Response<SaveHrLeaveResponse>, Status> {
        self.save_leave(request).await
    }

    async fn save_hr_expense(
        &self,
        request: Request<SaveHrExpenseRequest>,
    ) -> Result<Response<SaveHrExpenseResponse>, Status> {
        self.save_expense(request).await
    }

    async fn save_hr_payroll(
        &self,
        request: Request<SaveHrPayrollRequest>,
    ) -> Result<Response<SaveHrPayrollResponse>, Status> {
        self.save_payroll(request).await
    }

    async fn list_hr_employees(
        &self,
        request: Request<ListHrEmployeesRequest>,
    ) -> Result<Response<ListHrEmployeesResponse>, Status> {
        self.list_employees(request).await
    }

    async fn list_hr_leave_requests(
        &self,
        request: Request<ListHrLeaveRequestsRequest>,
    ) -> Result<Response<ListHrLeaveRequestsResponse>, Status> {
        self.list_leave_requests(request).await
    }

    async fn list_hr_expense_claims(
        &self,
        request: Request<ListHrExpenseClaimsRequest>,
    ) -> Result<Response<ListHrExpenseClaimsResponse>, Status> {
        self.list_expense_claims(request).await
    }

    async fn list_hr_payroll_runs(
        &self,
        request: Request<ListHrPayrollRunsRequest>,
    ) -> Result<Response<ListHrPayrollRunsResponse>, Status> {
        self.list_payroll_runs(request).await
    }

    async fn list_hr_reassignments(
        &self,
        request: Request<ListHrReassignmentsRequest>,
    ) -> Result<Response<ListHrReassignmentsResponse>, Status> {
        self.list_reassignments(request).await
    }
}

#[derive(Debug, Deserialize)]
struct FinanceAllocationBatchInput {
    payment: Payment,
    invoices: Vec<Invoice>,
    allocations: Vec<Allocation>,
}

fn finance_subledger_receipt(
    receipt: FinanceSubledgerWriteReceipt,
) -> erpx_proto::platform::FinanceSubledgerReceipt {
    erpx_proto::platform::FinanceSubledgerReceipt {
        aggregate_type: receipt.aggregate_type,
        aggregate_id: receipt.aggregate_id.to_string(),
        version: receipt.version,
        replayed: receipt.replayed,
    }
}

fn finance_allocation_response(
    receipt: FinanceAllocationBatchReceipt,
    correlation_id: Uuid,
) -> SaveFinanceAllocationBatchResponse {
    SaveFinanceAllocationBatchResponse {
        payment: Some(finance_subledger_receipt(receipt.payment)),
        invoices: receipt
            .invoices
            .into_iter()
            .map(finance_subledger_receipt)
            .collect(),
        allocations: receipt
            .allocations
            .into_iter()
            .map(finance_subledger_receipt)
            .collect(),
        replayed: receipt.replayed,
        correlation_id: correlation_id.to_string(),
    }
}

fn finance_chart_source_metadata_message(
    source: erpx_infrastructure::FinanceChartSourceMetadata,
) -> erpx_proto::platform::FinanceChartSourceMetadata {
    let causation_id = source.causation_id.map(|value| value.to_string());
    erpx_proto::platform::FinanceChartSourceMetadata {
        contract_version: u32::from(source.contract_version),
        event_version: u32::from(source.event_version),
        actor_ref: source.actor_ref,
        actor_kind: source.actor_kind,
        correlation_id: source.correlation_id.to_string(),
        causation_id: causation_id.clone().unwrap_or_default(),
        has_causation_id: causation_id.is_some(),
        idempotency_key: source.idempotency_key,
    }
}

fn finance_chart_account_read_row_message(
    row: erpx_infrastructure::FinanceAccountReadRow,
) -> erpx_proto::platform::FinanceChartAccountReadRow {
    let parent_account_id = row.parent_account_id.map(|value| value.to_string());
    erpx_proto::platform::FinanceChartAccountReadRow {
        account_id: row.account_id.to_string(),
        holding_id: row.holding_id.to_string(),
        company_id: row.company_id.to_string(),
        code: row.code,
        name: row.name,
        category: row.category,
        normal_balance: row.normal_balance,
        parent_account_id: parent_account_id.clone().unwrap_or_default(),
        has_parent_account_id: parent_account_id.is_some(),
        status: row.status,
        source_version: row.source_version,
        source_event_id: row.source_event_id.to_string(),
        source: Some(finance_chart_source_metadata_message(row.source)),
        projected_at: row.projected_at.to_rfc3339(),
    }
}

fn finance_chart_read_row_message(
    row: erpx_infrastructure::FinanceChartReadRow,
) -> Result<erpx_proto::platform::FinanceChartReadRow, Status> {
    Ok(erpx_proto::platform::FinanceChartReadRow {
        chart_id: row.chart_id.to_string(),
        holding_id: row.holding_id.to_string(),
        company_id: row.company_id.to_string(),
        name: row.name,
        source_version: row.source_version,
        source_event_id: row.source_event_id.to_string(),
        source: Some(finance_chart_source_metadata_message(row.source)),
        projected_at: row.projected_at.to_rfc3339(),
        accounts: row
            .accounts
            .into_iter()
            .map(finance_chart_account_read_row_message)
            .collect(),
    })
}

fn finance_close_status_text(status: erpx_domain::finance_close::CloseStatus) -> &'static str {
    match status {
        erpx_domain::finance_close::CloseStatus::Open => "open",
        erpx_domain::finance_close::CloseStatus::Review => "review",
        erpx_domain::finance_close::CloseStatus::Closed => "closed",
    }
}

fn finance_close_receipt(
    receipt: CloseReviewSaveReceipt,
) -> erpx_proto::platform::FinanceCloseReviewReceipt {
    erpx_proto::platform::FinanceCloseReviewReceipt {
        review_id: receipt.review_id.to_string(),
        version: receipt.version,
        status: finance_close_status_text(receipt.status).into(),
        idempotency_key: receipt.idempotency_key,
        replayed: receipt.replayed,
    }
}

fn finance_close_read_availability_text(value: FinanceCloseReadAvailability) -> &'static str {
    match value {
        FinanceCloseReadAvailability::Present => "present",
        FinanceCloseReadAvailability::Missing => "missing",
    }
}

fn finance_close_read_freshness_text(value: FinanceCloseReadFreshness) -> &'static str {
    match value {
        FinanceCloseReadFreshness::Fresh => "fresh",
        FinanceCloseReadFreshness::Stale => "stale",
        FinanceCloseReadFreshness::Unknown => "unknown",
    }
}

fn finance_close_read_metadata_message(
    metadata: FinanceCloseReadMetadata,
) -> erpx_proto::platform::FinanceCloseReadMetadata {
    erpx_proto::platform::FinanceCloseReadMetadata {
        period_id: metadata.period_id.to_string(),
        holding_id: metadata.holding_id.to_string(),
        company_id: metadata.company_id.to_string(),
        availability: finance_close_read_availability_text(metadata.availability).into(),
        freshness: finance_close_read_freshness_text(metadata.freshness).into(),
        read_only: metadata.read_only,
        review_updated_at: metadata
            .review_updated_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        projection_updated_at: metadata
            .projection_updated_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        source_version: metadata.source_version.unwrap_or_default(),
        has_source_version: metadata.source_version.is_some(),
        expected_fact_count: metadata.expected_fact_count,
        observed_fact_count: metadata.observed_fact_count,
        missing_fact_count: metadata.missing_fact_count,
    }
}

fn finance_close_read_view_message(
    view: FinanceCloseReadView,
) -> erpx_proto::platform::FinanceCloseReadView {
    let statement =
        view.report.statement.map(
            |statement| erpx_proto::platform::FinanceCloseStatementSummary {
                debit_minor: statement.debit_minor,
                credit_minor: statement.credit_minor,
                net_minor: statement.net_minor,
                balanced: statement.balanced,
            },
        );
    erpx_proto::platform::FinanceCloseReadView {
        review: Some(erpx_proto::platform::FinanceCloseReviewProjection {
            holding_id: view.review.holding_id.to_string(),
            company_id: view.review.company_id.to_string(),
            period_id: view.review.period_id.to_string(),
            status: match view.review.status {
                erpx_infrastructure::FinanceCloseReviewStatus::Open => "open",
                erpx_infrastructure::FinanceCloseReviewStatus::Review => "review",
                erpx_infrastructure::FinanceCloseReviewStatus::Closed => "closed",
            }
            .into(),
            version: view.review.version,
            idempotency_key: view.review.idempotency_key,
            opened_by: view.review.opened_by,
            reviewed_by: view.review.reviewed_by.unwrap_or_default(),
            closed_by: view.review.closed_by.unwrap_or_default(),
        }),
        report: Some(erpx_proto::platform::FinanceCloseReport {
            statement,
            controls: Some(erpx_proto::platform::FinanceCloseControlSummary {
                total: view.report.controls.total,
                resolved: view.report.controls.resolved,
                unresolved: view.report.controls.unresolved,
            }),
            accounts: view
                .report
                .accounts
                .into_iter()
                .map(|account| erpx_proto::platform::FinanceCloseAccountSummary {
                    account_id: account.account_id.to_string(),
                    debit_minor: account.debit_minor,
                    credit_minor: account.credit_minor,
                    net_minor: account.net_minor,
                })
                .collect(),
        }),
        metadata: Some(finance_close_read_metadata_message(view.metadata)),
    }
}

fn finance_close_read_row_message(
    row: FinanceCloseReadListRow,
) -> erpx_proto::platform::FinanceCloseReviewReadRow {
    erpx_proto::platform::FinanceCloseReviewReadRow {
        holding_id: row.holding_id.to_string(),
        company_id: row.company_id.to_string(),
        period_id: row.period_id.to_string(),
        status: match row.status {
            erpx_infrastructure::FinanceCloseReviewStatus::Open => "open",
            erpx_infrastructure::FinanceCloseReviewStatus::Review => "review",
            erpx_infrastructure::FinanceCloseReviewStatus::Closed => "closed",
        }
        .into(),
        source_version: row.source_version,
        projected_at: row.projected_at.to_rfc3339(),
        freshness: finance_close_read_freshness_text(row.freshness).into(),
        control_total: row.control_total,
        unresolved_controls: row.unresolved_controls,
        net_minor: row.net_minor.unwrap_or_default(),
        statement_present: row.statement_present,
        balanced: row.balanced,
    }
}

async fn with_banking_store<T, F, Fut>(
    router: &TenantDatabaseRouter,
    tenant_id: &str,
    holding_ref: &str,
    correlation_id: Uuid,
    operation: F,
) -> Result<T, Status>
where
    F: FnOnce(SqlxFinanceBankingStore) -> Fut,
    Fut: Future<Output = Result<T, Status>>,
{
    let write = router
        .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
        .await
        .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
    let read = match router
        .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
        .await
    {
        Ok(read) => read,
        Err(_) => {
            write.close().await;
            return Err(Status::unavailable("tenant read database is unavailable"));
        }
    };
    let result = async {
        let store = SqlxFinanceBankingStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            holding_ref,
        )
        .map_err(|error| finance_persistence_status(error, correlation_id))?;
        operation(store).await
    }
    .await;
    write.close().await;
    read.close().await;
    result
}

async fn with_close_store<T, F, Fut>(
    router: &TenantDatabaseRouter,
    tenant_id: &str,
    holding_ref: &str,
    correlation_id: Uuid,
    operation: F,
) -> Result<T, Status>
where
    F: FnOnce(SqlxFinanceCloseStore) -> Fut,
    Fut: Future<Output = Result<T, Status>>,
{
    let write = router
        .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
        .await
        .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
    let result = async {
        let store = SqlxFinanceCloseStore::new(write.database_pool().clone(), holding_ref)
            .map_err(|error| finance_persistence_status(error, correlation_id))?;
        operation(store).await
    }
    .await;
    write.close().await;
    result
}

async fn with_close_read_adapter<T, F, Fut>(
    router: &TenantDatabaseRouter,
    tenant_id: &str,
    holding_ref: &str,
    correlation_id: Uuid,
    operation: F,
) -> Result<T, Status>
where
    F: FnOnce(SqlxFinanceCloseReadAdapter) -> Fut,
    Fut: Future<Output = Result<T, Status>>,
{
    let read = match router
        .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
        .await
    {
        Ok(read) => read,
        Err(_) => return Err(Status::unavailable("tenant read database is unavailable")),
    };
    let result = async {
        let adapter = SqlxFinanceCloseReadAdapter::new(read.database_pool().clone(), holding_ref)
            .map_err(|_| {
            finance_persistence_status(
                PersistenceError::Configuration("close read adapter"),
                correlation_id,
            )
        })?;
        operation(adapter).await
    }
    .await;
    read.close().await;
    result
}

async fn with_banking_read_store<T, F, Fut>(
    router: &TenantDatabaseRouter,
    tenant_id: &str,
    holding_ref: &str,
    correlation_id: Uuid,
    operation: F,
) -> Result<T, Status>
where
    F: FnOnce(SqlxFinanceBankingStore) -> Fut,
    Fut: Future<Output = Result<T, Status>>,
{
    let read = router
        .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
        .await
        .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
    let result = async {
        let store = SqlxFinanceBankingStore::from_pools(
            read.database_pool(),
            read.database_pool(),
            holding_ref,
        )
        .map_err(|error| finance_persistence_status(error, correlation_id))?;
        operation(store).await
    }
    .await;
    read.close().await;
    result
}

async fn with_crm_read_store<T, F, Fut>(
    router: &TenantDatabaseRouter,
    tenant_id: &str,
    holding_ref: &str,
    correlation_id: Uuid,
    operation: F,
) -> Result<T, Status>
where
    F: FnOnce(SqlxMarketingStore) -> Fut,
    Fut: Future<Output = Result<T, Status>>,
{
    let read = router
        .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
        .await
        .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
    let result = async {
        let store =
            SqlxMarketingStore::from_pools(read.database_pool(), read.database_pool(), holding_ref)
                .map_err(|error| crm_persistence_status(error, correlation_id))?;
        operation(store).await
    }
    .await;
    read.close().await;
    result
}

async fn with_crm_write_store<T, F, Fut>(
    router: &TenantDatabaseRouter,
    tenant_id: &str,
    holding_ref: &str,
    correlation_id: Uuid,
    operation: F,
) -> Result<T, Status>
where
    F: FnOnce(SqlxMarketingCrmStore) -> Fut,
    Fut: Future<Output = Result<T, Status>>,
{
    let write = router
        .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
        .await
        .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
    let result = async {
        let store = SqlxMarketingCrmStore::from_pool(write.database_pool(), holding_ref)
            .map_err(|error| crm_persistence_status(error, correlation_id))?;
        operation(store).await
    }
    .await;
    write.close().await;
    result
}

#[derive(Clone)]
struct PosRpcImpl {
    state: AppState,
}

#[tonic::async_trait]
impl PosService for PosRpcImpl {
    async fn save_pos_sale(
        &self,
        request: Request<SavePosSaleRequest>,
    ) -> Result<Response<SavePosSaleResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "POS writes require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            if input.sale_json.is_empty() || input.sale_json.len() > 512 * 1024 {
                return Err(Status::invalid_argument("sale_json is out of bounds"));
            }
            let sale: Sale = serde_json::from_slice(&input.sale_json)
                .map_err(|_| Status::invalid_argument("sale_json is invalid"))?;
            if sale.command_id != context.command_id
                || sale.idempotency_key != format!("pos/sale/{}", context.idempotency_key)
                || !context.tenant_scope.contains(&sale.scope)
                || access_scope(&sale.scope) != scope
            {
                return Err(Status::permission_denied(
                    "POS sale identity or command scope does not match context",
                ));
            }
            let expected_version = if input.has_expected_version {
                if input.expected_version == 0 {
                    return Err(Status::invalid_argument(
                        "expected_version must be positive",
                    ));
                }
                Some(input.expected_version)
            } else {
                None
            };
            if expected_version != context.expected_aggregate_version {
                return Err(Status::invalid_argument(
                    "expected_version does not match command context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "pos", "pos.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let result = async {
                let store = erpx_infrastructure::SqlxPosPersistence::new(
                    write.database_pool().clone(),
                    holding_ref,
                )
                .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                let receipt = store
                    .save_sale(&sale, expected_version)
                    .await
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "pos.sale.saved".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("POS audit persistence unavailable"))?;
                Ok::<SavePosSaleResponse, Status>(SavePosSaleResponse {
                    aggregate_type: receipt.aggregate_type,
                    aggregate_id: receipt.aggregate_id.to_string(),
                    version: receipt.version,
                    idempotency_key: receipt.idempotency_key,
                    replayed: receipt.replayed,
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn list_pos_sales(
        &self,
        request: Request<ListPosSalesRequest>,
    ) -> Result<Response<ListPosSalesResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            if matches!(caller_scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "POS reads require an explicit holding, company, or branch scope",
                ));
            }
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let branch_id = if input.branch_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.branch_id, "branch_id")?)
            };
            let requested_scope = match branch_id {
                Some(branch_id) => Scope::Branch {
                    holding_id,
                    company_id,
                    branch_id,
                },
                None => Scope::Company {
                    holding_id,
                    company_id,
                },
            };
            if !caller_scope.contains(&requested_scope) {
                return Err(Status::permission_denied("POS read scope is not contained"));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "pos.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let page_size = input.page_size;
            if page_size == 0 || page_size > MAX_POS_READ_PAGE_SIZE {
                return Err(Status::invalid_argument(format!(
                    "page_size must be 1..={MAX_POS_READ_PAGE_SIZE}"
                )));
            }
            let before_cursor = if input.before_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_pos_sales_cursor(&input.before_cursor)?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &requested_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = erpx_infrastructure::SqlxPosProjectionStore::from_pools(
                    read.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|_| Status::internal("POS read store configuration is invalid"))?;
                let page = store
                    .list_sales(
                        PosReadScope {
                            holding_id,
                            company_id,
                            branch_id,
                        },
                        page_size,
                        before_cursor.as_ref(),
                    )
                    .await
                    .map_err(|_| Status::internal("POS sales read failed"))?;
                let sales = page
                    .rows
                    .into_iter()
                    .map(|row| erpx_proto::platform::PosSaleReadRow {
                        sale_id: row.sale_id.to_string(),
                        holding_id: row.holding_id.to_string(),
                        company_id: row.company_id.to_string(),
                        branch_id: row.branch_id.map(|id| id.to_string()).unwrap_or_default(),
                        reference: row.reference,
                        customer_id: row.customer_id.map(|id| id.to_string()).unwrap_or_default(),
                        register_id: row.register_id.map(|id| id.to_string()).unwrap_or_default(),
                        currency: row.currency,
                        subtotal_minor: row.subtotal_minor,
                        tax_minor: row.tax_minor,
                        total_minor: row.total_minor,
                        paid_minor: row.paid_minor,
                        status: row.status,
                        created_at: row.created_at.to_rfc3339(),
                        updated_at: row.updated_at.to_rfc3339(),
                        source_version: row.source_version,
                        source_event_id: row.source_event_id.to_string(),
                        projected_at: row.projected_at.to_rfc3339(),
                        lines: row
                            .lines
                            .into_iter()
                            .map(|line| erpx_proto::platform::PosSaleReadLine {
                                line_id: line.line_id.to_string(),
                                line_no: line.line_no,
                                item_reference: line.item_reference,
                                description: line.description,
                                quantity: line.quantity,
                                unit_price_minor: line.unit_price_minor,
                                tax_minor: line.tax_minor,
                                subtotal_minor: line.subtotal_minor,
                                total_minor: line.total_minor,
                                product_id: line
                                    .product_id
                                    .map(|id| id.to_string())
                                    .unwrap_or_default(),
                                source_version: line.source_version,
                                source_event_id: line.source_event_id.to_string(),
                                projected_at: line.projected_at.to_rfc3339(),
                            })
                            .collect(),
                        payments: row
                            .payments
                            .into_iter()
                            .map(|payment| erpx_proto::platform::PosSaleReadPayment {
                                payment_id: payment.payment_id.to_string(),
                                amount_minor: payment.amount_minor,
                                method: payment.method,
                                received_at: payment.received_at.to_rfc3339(),
                                source_version: payment.source_version,
                                source_event_id: payment.source_event_id.to_string(),
                                projected_at: payment.projected_at.to_rfc3339(),
                            })
                            .collect(),
                        lines_truncated: row.lines_truncated,
                        payments_truncated: row.payments_truncated,
                    })
                    .collect();
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "pos.sale.read".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "POS sales read".into()),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("POS audit persistence unavailable"))?;
                Ok::<ListPosSalesResponse, Status>(ListPosSalesResponse {
                    sales,
                    has_more: page.has_more,
                    next_cursor: page
                        .next_cursor
                        .as_ref()
                        .map(encode_pos_sales_cursor)
                        .unwrap_or_default(),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }
}

#[derive(Clone)]
struct SalesRpcImpl {
    state: AppState,
}

#[tonic::async_trait]
impl SalesService for SalesRpcImpl {
    async fn save_sales_aggregate(
        &self,
        request: Request<SaveSalesAggregateRequest>,
    ) -> Result<Response<SaveSalesAggregateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            explicit_product_scope(access_scope(&context.tenant_scope), "Sales")?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            if input.aggregate_json.is_empty() || input.aggregate_json.len() > 1024 * 1024 {
                return Err(Status::invalid_argument("aggregate_json is out of bounds"));
            }
            let aggregate: SalesAggregate = serde_json::from_slice(&input.aggregate_json)
                .map_err(|_| Status::invalid_argument("Sales aggregate_json is invalid"))?;
            aggregate.validate_for_command(&context).map_err(|error| {
                product_persistence_status(error, "Sales", context.correlation_id)
            })?;
            let expected_version = if input.has_expected_version {
                if input.expected_version == 0 {
                    return Err(Status::invalid_argument(
                        "expected_version must be positive",
                    ));
                }
                Some(input.expected_version)
            } else {
                None
            };
            if expected_version != context.expected_aggregate_version {
                return Err(Status::invalid_argument(
                    "expected_version does not match command context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "sales", "sales.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxSalesStore::from_pools(
                    write.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    product_persistence_status(error, "Sales", context.correlation_id)
                })?;
                let receipt = store
                    .save(&aggregate, expected_version)
                    .await
                    .map_err(|error| {
                        product_persistence_status(error, "Sales", context.correlation_id)
                    })?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "sales.aggregate.saved".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Sales audit persistence unavailable"))?;
                Ok::<SaveSalesAggregateResponse, Status>(SaveSalesAggregateResponse {
                    receipt: Some(erpx_proto::platform::SalesWriteReceipt {
                        aggregate_type: receipt.aggregate_type,
                        aggregate_id: receipt.aggregate_id.to_string(),
                        version: receipt.version,
                        replayed: receipt.replayed,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn transition_sales_aggregate(
        &self,
        request: Request<TransitionSalesAggregateRequest>,
    ) -> Result<Response<TransitionSalesAggregateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            explicit_product_scope(access_scope(&context.tenant_scope), "Sales")?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let aggregate_type =
                validate_transport_reference(&input.aggregate_type, "aggregate_type")?;
            let aggregate_id = parse_uuid(&input.aggregate_id, "aggregate_id")?;
            let transition = validate_transport_reference(&input.transition, "transition")?;
            if input.expected_version == 0 {
                return Err(Status::invalid_argument(
                    "expected_version must be positive",
                ));
            }
            if context.expected_aggregate_version != Some(input.expected_version) {
                return Err(Status::invalid_argument(
                    "expected_version does not match command context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "sales", "sales.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxSalesStore::from_pools(
                    write.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    product_persistence_status(error, "Sales", context.correlation_id)
                })?;
                let receipt = store
                    .transition(
                        aggregate_type,
                        aggregate_id,
                        transition,
                        input.expected_version,
                        &context,
                    )
                    .await
                    .map_err(|error| {
                        product_persistence_status(error, "Sales", context.correlation_id)
                    })?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "sales.aggregate.transitioned".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Sales audit persistence unavailable"))?;
                Ok::<TransitionSalesAggregateResponse, Status>(TransitionSalesAggregateResponse {
                    receipt: Some(erpx_proto::platform::SalesWriteReceipt {
                        aggregate_type: receipt.aggregate_type,
                        aggregate_id: receipt.aggregate_id.to_string(),
                        version: receipt.version,
                        replayed: receipt.replayed,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn get_sales_aggregate(
        &self,
        request: Request<GetSalesAggregateRequest>,
    ) -> Result<Response<GetSalesAggregateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = explicit_product_scope(query_access_scope(proto_context)?, "Sales")?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let aggregate_type =
                validate_transport_reference(&input.aggregate_type, "aggregate_type")?;
            let aggregate_id = parse_uuid(&input.aggregate_id, "aggregate_id")?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "sales.read",
                )
                .await?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &caller_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxSalesStore::from_pools(
                    read.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    product_persistence_status(error, "Sales", context.correlation_id)
                })?;
                let value = store
                    .get(aggregate_type, aggregate_id)
                    .await
                    .map_err(|error| {
                        product_persistence_status(error, "Sales", context.correlation_id)
                    })?;
                let (aggregate_json, source_version, present) = match value {
                    Some((aggregate, source_version)) => (
                        serde_json::to_vec(&aggregate)
                            .map_err(|_| Status::internal("Sales aggregate encoding failed"))?,
                        source_version,
                        true,
                    ),
                    None => (Vec::new(), 0, false),
                };
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "sales.aggregate.read".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "Sales aggregate read".into()),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Sales audit persistence unavailable"))?;
                Ok::<GetSalesAggregateResponse, Status>(GetSalesAggregateResponse {
                    aggregate_type: aggregate_type.into(),
                    aggregate_id: aggregate_id.to_string(),
                    source_version,
                    aggregate_json,
                    present,
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn list_sales_aggregates(
        &self,
        request: Request<ListSalesAggregatesRequest>,
    ) -> Result<Response<ListSalesAggregatesResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = explicit_product_scope(query_access_scope(proto_context)?, "Sales")?;
            if matches!(caller_scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Sales reads require an explicit holding, company, or branch scope",
                ));
            }
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let branch_id = if input.branch_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.branch_id, "branch_id")?)
            };
            let requested_scope = match branch_id {
                Some(branch_id) => Scope::Branch {
                    holding_id,
                    company_id,
                    branch_id,
                },
                None => Scope::Company {
                    holding_id,
                    company_id,
                },
            };
            if !caller_scope.contains(&requested_scope) {
                return Err(Status::permission_denied(
                    "Sales read scope is not contained",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "sales.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let page_size = input.page_size;
            if page_size == 0 || page_size > MAX_SALES_READ_PAGE_SIZE {
                return Err(Status::invalid_argument(format!(
                    "page_size must be 1..={MAX_SALES_READ_PAGE_SIZE}"
                )));
            }
            let before_cursor = if input.before_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_sales_read_cursor(&input.before_cursor)?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &requested_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxSalesStore::from_pools(
                    read.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    product_persistence_status(error, "Sales", context.correlation_id)
                })?;
                let page = store
                    .list_for_scope(
                        SalesReadScope {
                            holding_id,
                            company_id,
                            branch_id,
                        },
                        page_size,
                        before_cursor.as_ref(),
                    )
                    .await
                    .map_err(|error| {
                        product_persistence_status(error, "Sales", context.correlation_id)
                    })?;
                let rows = page
                    .rows
                    .into_iter()
                    .map(|row| erpx_proto::platform::SalesAggregateReadRow {
                        aggregate_type: row.aggregate_type,
                        aggregate_id: row.aggregate_id.to_string(),
                        holding_id: row.holding_id.to_string(),
                        company_id: row.company_id.to_string(),
                        branch_id: row.branch_id.map(|id| id.to_string()).unwrap_or_default(),
                        reference: row.reference,
                        source_version: row.source_version,
                        projected_at: row.projected_at.to_rfc3339(),
                    })
                    .collect();
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "sales.aggregate.listed".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "Sales aggregate list read".into()),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Sales audit persistence unavailable"))?;
                Ok::<ListSalesAggregatesResponse, Status>(ListSalesAggregatesResponse {
                    rows,
                    has_more: page.has_more,
                    next_cursor: page
                        .next_cursor
                        .as_ref()
                        .map(encode_sales_read_cursor)
                        .unwrap_or_default(),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }
}

#[derive(Clone)]
struct ScmRpcImpl {
    state: AppState,
}

#[tonic::async_trait]
impl ScmService for ScmRpcImpl {
    async fn save_scm_aggregate(
        &self,
        request: Request<SaveScmAggregateRequest>,
    ) -> Result<Response<SaveScmAggregateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            explicit_product_scope(access_scope(&context.tenant_scope), "SCM")?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            if input.aggregate_json.is_empty() || input.aggregate_json.len() > 1024 * 1024 {
                return Err(Status::invalid_argument("aggregate_json is out of bounds"));
            }
            let aggregate: ScmAggregate = serde_json::from_slice(&input.aggregate_json)
                .map_err(|_| Status::invalid_argument("SCM aggregate_json is invalid"))?;
            aggregate.validate_for_command(&context).map_err(|error| {
                product_persistence_status(error, "SCM", context.correlation_id)
            })?;
            let expected_version = if input.has_expected_version {
                if input.expected_version == 0 {
                    return Err(Status::invalid_argument(
                        "expected_version must be positive",
                    ));
                }
                Some(input.expected_version)
            } else {
                None
            };
            if expected_version != context.expected_aggregate_version {
                return Err(Status::invalid_argument(
                    "expected_version does not match command context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "scm", "scm.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxScmStore::from_pools(
                    write.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    product_persistence_status(error, "SCM", context.correlation_id)
                })?;
                let receipt = store
                    .save(&aggregate, expected_version)
                    .await
                    .map_err(|error| {
                        product_persistence_status(error, "SCM", context.correlation_id)
                    })?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "scm.aggregate.saved".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("SCM audit persistence unavailable"))?;
                Ok::<SaveScmAggregateResponse, Status>(SaveScmAggregateResponse {
                    receipt: Some(erpx_proto::platform::ScmWriteReceipt {
                        aggregate_type: receipt.aggregate_type,
                        aggregate_id: receipt.aggregate_id.to_string(),
                        version: receipt.version,
                        replayed: receipt.replayed,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn transition_scm_aggregate(
        &self,
        request: Request<TransitionScmAggregateRequest>,
    ) -> Result<Response<TransitionScmAggregateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            explicit_product_scope(access_scope(&context.tenant_scope), "SCM")?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let aggregate_type =
                validate_transport_reference(&input.aggregate_type, "aggregate_type")?;
            let aggregate_id = parse_uuid(&input.aggregate_id, "aggregate_id")?;
            let transition = validate_transport_reference(&input.transition, "transition")?;
            if input.expected_version == 0 {
                return Err(Status::invalid_argument(
                    "expected_version must be positive",
                ));
            }
            if context.expected_aggregate_version != Some(input.expected_version) {
                return Err(Status::invalid_argument(
                    "expected_version does not match command context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "scm", "scm.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxScmStore::from_pools(
                    write.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    product_persistence_status(error, "SCM", context.correlation_id)
                })?;
                let receipt = store
                    .transition(
                        aggregate_type,
                        aggregate_id,
                        transition,
                        input.expected_version,
                        &context,
                    )
                    .await
                    .map_err(|error| {
                        product_persistence_status(error, "SCM", context.correlation_id)
                    })?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "scm.aggregate.transitioned".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("SCM audit persistence unavailable"))?;
                Ok::<TransitionScmAggregateResponse, Status>(TransitionScmAggregateResponse {
                    receipt: Some(erpx_proto::platform::ScmWriteReceipt {
                        aggregate_type: receipt.aggregate_type,
                        aggregate_id: receipt.aggregate_id.to_string(),
                        version: receipt.version,
                        replayed: receipt.replayed,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn get_scm_aggregate(
        &self,
        request: Request<GetScmAggregateRequest>,
    ) -> Result<Response<GetScmAggregateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = explicit_product_scope(query_access_scope(proto_context)?, "SCM")?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let aggregate_type =
                validate_transport_reference(&input.aggregate_type, "aggregate_type")?;
            let aggregate_id = parse_uuid(&input.aggregate_id, "aggregate_id")?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "scm.read",
                )
                .await?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &caller_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxScmStore::from_pools(
                    read.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    product_persistence_status(error, "SCM", context.correlation_id)
                })?;
                let value = store
                    .get(aggregate_type, aggregate_id)
                    .await
                    .map_err(|error| {
                        product_persistence_status(error, "SCM", context.correlation_id)
                    })?;
                let (aggregate_json, source_version, present) = match value {
                    Some(row) => (
                        serde_json::to_vec(&row.aggregate)
                            .map_err(|_| Status::internal("SCM aggregate encoding failed"))?,
                        row.aggregate_version,
                        true,
                    ),
                    None => (Vec::new(), 0, false),
                };
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "scm.aggregate.read".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "SCM aggregate read".into()),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("SCM audit persistence unavailable"))?;
                Ok::<GetScmAggregateResponse, Status>(GetScmAggregateResponse {
                    aggregate_type: aggregate_type.into(),
                    aggregate_id: aggregate_id.to_string(),
                    source_version,
                    aggregate_json,
                    present,
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn list_scm_aggregates(
        &self,
        request: Request<ListScmAggregatesRequest>,
    ) -> Result<Response<ListScmAggregatesResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = explicit_product_scope(query_access_scope(proto_context)?, "SCM")?;
            if matches!(caller_scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "SCM reads require an explicit holding, company, or branch scope",
                ));
            }
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let branch_id = if input.branch_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.branch_id, "branch_id")?)
            };
            let requested_scope = match branch_id {
                Some(branch_id) => Scope::Branch {
                    holding_id,
                    company_id,
                    branch_id,
                },
                None => Scope::Company {
                    holding_id,
                    company_id,
                },
            };
            if !caller_scope.contains(&requested_scope) {
                return Err(Status::permission_denied("SCM read scope is not contained"));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "scm.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let page_size = input.page_size;
            if page_size == 0 || page_size > MAX_SCM_READ_PAGE_SIZE {
                return Err(Status::invalid_argument(format!(
                    "page_size must be 1..={MAX_SCM_READ_PAGE_SIZE}"
                )));
            }
            let before_cursor = if input.before_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_scm_read_cursor(&input.before_cursor)?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &requested_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxScmStore::from_pools(
                    read.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    product_persistence_status(error, "SCM", context.correlation_id)
                })?;
                let page = store
                    .list_for_scope(
                        ScmReadScope {
                            holding_id,
                            company_id,
                            branch_id,
                        },
                        page_size,
                        before_cursor.as_ref(),
                    )
                    .await
                    .map_err(|error| {
                        product_persistence_status(error, "SCM", context.correlation_id)
                    })?;
                let rows = page
                    .rows
                    .into_iter()
                    .map(|row| {
                        let aggregate = row.row.aggregate;
                        let aggregate_type = row.row.aggregate_type;
                        let aggregate_json = serde_json::to_vec(&aggregate)
                            .map_err(|_| Status::internal("SCM aggregate encoding failed"))?;
                        let (holding_id, company_id, branch_id) = match aggregate.scope() {
                            erpx_domain::contracts::TenantScope::Company {
                                holding_id,
                                company_id,
                            } => (*holding_id, *company_id, None),
                            erpx_domain::contracts::TenantScope::Branch {
                                holding_id,
                                company_id,
                                branch_id,
                            } => (*holding_id, *company_id, Some(*branch_id)),
                            _ => {
                                return Err(Status::internal(
                                    "SCM read aggregate scope was invalid",
                                ));
                            }
                        };
                        Ok::<_, Status>(erpx_proto::platform::ScmAggregateReadRow {
                            aggregate_type,
                            aggregate_id: row.row.aggregate_id.to_string(),
                            holding_id: holding_id.to_string(),
                            company_id: company_id.to_string(),
                            branch_id: branch_id.map(|id| id.to_string()).unwrap_or_default(),
                            reference: row.reference,
                            source_version: row.row.aggregate_version,
                            aggregate_json,
                            projected_at: row.projected_at.to_rfc3339(),
                        })
                    })
                    .collect::<Result<Vec<_>, Status>>()?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "scm.aggregate.listed".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "SCM aggregate list read".into()),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("SCM audit persistence unavailable"))?;
                Ok::<ListScmAggregatesResponse, Status>(ListScmAggregatesResponse {
                    rows,
                    has_more: page.has_more,
                    next_cursor: page
                        .next_cursor
                        .as_ref()
                        .map(encode_scm_read_cursor)
                        .unwrap_or_default(),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }
}

#[derive(Clone)]
struct AssetMaintenanceRpcImpl {
    state: AppState,
}

#[tonic::async_trait]
impl AssetMaintenanceService for AssetMaintenanceRpcImpl {
    async fn save_asset_maintenance_aggregate(
        &self,
        request: Request<SaveAssetMaintenanceAggregateRequest>,
    ) -> Result<Response<SaveAssetMaintenanceAggregateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            explicit_product_scope(access_scope(&context.tenant_scope), "Asset/Maintenance")?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let idempotency_key =
                validate_transport_reference(&input.idempotency_key, "idempotency_key")?;
            if idempotency_key != context.idempotency_key {
                return Err(Status::invalid_argument(
                    "idempotency_key does not match command context",
                ));
            }
            if input.aggregate_json.is_empty() || input.aggregate_json.len() > 1024 * 1024 {
                return Err(Status::invalid_argument("aggregate_json is out of bounds"));
            }
            let aggregate: AssetMaintenanceAggregate =
                serde_json::from_slice(&input.aggregate_json).map_err(|_| {
                    Status::invalid_argument("Asset/Maintenance aggregate_json is invalid")
                })?;
            let expected_version = if input.has_expected_version {
                if input.expected_version == 0 {
                    return Err(Status::invalid_argument(
                        "expected_version must be positive",
                    ));
                }
                Some(input.expected_version)
            } else {
                None
            };
            if expected_version != context.expected_aggregate_version {
                return Err(Status::invalid_argument(
                    "expected_version does not match command context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "asset_maintenance",
                    "asset_maintenance.write",
                )
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxAssetMaintenanceStore::from_pools(
                    write.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    product_persistence_status(error, "Asset/Maintenance", context.correlation_id)
                })?;
                let receipt = store
                    .save(&aggregate, idempotency_key, expected_version)
                    .await
                    .map_err(|error| {
                        product_persistence_status(
                            error,
                            "Asset/Maintenance",
                            context.correlation_id,
                        )
                    })?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "asset_maintenance.aggregate.saved".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| {
                        Status::unavailable("Asset/Maintenance audit persistence unavailable")
                    })?;
                Ok::<SaveAssetMaintenanceAggregateResponse, Status>(
                    SaveAssetMaintenanceAggregateResponse {
                        receipt: Some(erpx_proto::platform::AssetMaintenanceWriteReceipt {
                            aggregate_type: receipt.aggregate_type,
                            aggregate_id: receipt.aggregate_id.to_string(),
                            version: receipt.version,
                            replayed: receipt.replayed,
                        }),
                        correlation_id: context.correlation_id.to_string(),
                    },
                )
            }
            .await;
            read.close().await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn get_asset_maintenance_aggregate(
        &self,
        request: Request<GetAssetMaintenanceAggregateRequest>,
    ) -> Result<Response<GetAssetMaintenanceAggregateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope =
                explicit_product_scope(query_access_scope(proto_context)?, "Asset/Maintenance")?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let aggregate_type =
                validate_transport_reference(&input.aggregate_type, "aggregate_type")?;
            let aggregate_id = parse_uuid(&input.aggregate_id, "aggregate_id")?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "asset_maintenance.read",
                )
                .await?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &caller_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxAssetMaintenanceStore::from_pools(
                    read.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    product_persistence_status(error, "Asset/Maintenance", context.correlation_id)
                })?;
                let value = store
                    .get(aggregate_type, aggregate_id)
                    .await
                    .map_err(|error| {
                        product_persistence_status(
                            error,
                            "Asset/Maintenance",
                            context.correlation_id,
                        )
                    })?;
                let (aggregate_json, source_version, present) = match value {
                    Some((aggregate, source_version)) => (
                        serde_json::to_vec(&aggregate).map_err(|_| {
                            Status::internal("Asset/Maintenance aggregate encoding failed")
                        })?,
                        source_version,
                        true,
                    ),
                    None => (Vec::new(), 0, false),
                };
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "asset_maintenance.aggregate.read".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "Asset/Maintenance aggregate read".into()),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| {
                        Status::unavailable("Asset/Maintenance audit persistence unavailable")
                    })?;
                Ok::<GetAssetMaintenanceAggregateResponse, Status>(
                    GetAssetMaintenanceAggregateResponse {
                        aggregate_type: aggregate_type.into(),
                        aggregate_id: aggregate_id.to_string(),
                        source_version,
                        aggregate_json,
                        present,
                        correlation_id: context.correlation_id.to_string(),
                    },
                )
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn list_asset_maintenance_aggregates(
        &self,
        request: Request<ListAssetMaintenanceAggregatesRequest>,
    ) -> Result<Response<ListAssetMaintenanceAggregatesResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope =
                explicit_product_scope(query_access_scope(proto_context)?, "Asset/Maintenance")?;
            if matches!(caller_scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Asset/Maintenance reads require an explicit holding, company, or branch scope",
                ));
            }
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let branch_id = if input.branch_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.branch_id, "branch_id")?)
            };
            let requested_scope = match branch_id {
                Some(branch_id) => Scope::Branch {
                    holding_id,
                    company_id,
                    branch_id,
                },
                None => Scope::Company {
                    holding_id,
                    company_id,
                },
            };
            if !caller_scope.contains(&requested_scope) {
                return Err(Status::permission_denied(
                    "Asset/Maintenance read scope is not contained",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "asset_maintenance.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            if input.page_size == 0 || input.page_size > MAX_ASSET_MAINTENANCE_READ_PAGE_SIZE {
                return Err(Status::invalid_argument(format!(
                    "page_size must be 1..={MAX_ASSET_MAINTENANCE_READ_PAGE_SIZE}"
                )));
            }
            let before_cursor = if input.before_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_asset_maintenance_read_cursor(&input.before_cursor)?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &requested_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxAssetMaintenanceStore::from_pools(
                    read.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    product_persistence_status(error, "Asset/Maintenance", context.correlation_id)
                })?;
                let page = store
                    .list_for_scope(
                        AssetMaintenanceReadScope {
                            holding_id,
                            company_id,
                            branch_id,
                        },
                        input.page_size,
                        before_cursor.as_ref(),
                    )
                    .await
                    .map_err(|error| {
                        product_persistence_status(
                            error,
                            "Asset/Maintenance",
                            context.correlation_id,
                        )
                    })?;
                let rows = page
                    .rows
                    .into_iter()
                    .map(|row| {
                        let (holding_id, company_id, branch_id) = match &row.aggregate {
                            AssetMaintenanceAggregate::Asset(asset) => (
                                asset.scope.holding_id,
                                asset.scope.company_id,
                                asset.scope.branch_id,
                            ),
                            AssetMaintenanceAggregate::MaintenanceWorkOrder(work_order) => (
                                work_order.scope.holding_id,
                                work_order.scope.company_id,
                                work_order.scope.branch_id,
                            ),
                        };
                        let aggregate_json = serde_json::to_vec(&row.aggregate).map_err(|_| {
                            Status::internal("Asset/Maintenance aggregate encoding failed")
                        })?;
                        Ok::<_, Status>(erpx_proto::platform::AssetMaintenanceAggregateReadRow {
                            aggregate_type: row.aggregate.aggregate_type().into(),
                            aggregate_id: row.aggregate.aggregate_id().to_string(),
                            holding_id: holding_id.to_string(),
                            company_id: company_id.to_string(),
                            branch_id: branch_id.map(|id| id.to_string()).unwrap_or_default(),
                            reference: row.reference,
                            source_version: row.source_version,
                            aggregate_json,
                            projected_at: row.projected_at.to_rfc3339(),
                        })
                    })
                    .collect::<Result<Vec<_>, Status>>()?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "asset_maintenance.aggregate.listed".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "Asset/Maintenance aggregate list read".into()),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| {
                        Status::unavailable("Asset/Maintenance audit persistence unavailable")
                    })?;
                Ok::<ListAssetMaintenanceAggregatesResponse, Status>(
                    ListAssetMaintenanceAggregatesResponse {
                        rows,
                        has_more: page.has_more,
                        next_cursor: page
                            .next_cursor
                            .as_ref()
                            .map(encode_asset_maintenance_read_cursor)
                            .unwrap_or_default(),
                        correlation_id: context.correlation_id.to_string(),
                    },
                )
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }
}

#[derive(Clone)]
struct QualityRpcImpl {
    state: AppState,
}

#[tonic::async_trait]
impl QualityService for QualityRpcImpl {
    async fn create_quality_inspection(
        &self,
        request: Request<CreateQualityInspectionRequest>,
    ) -> Result<Response<CreateQualityInspectionResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            let _scope = explicit_product_scope(access_scope(&context.tenant_scope), "Quality")?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            if input.inspection_json.is_empty() || input.inspection_json.len() > 1024 * 1024 {
                return Err(Status::invalid_argument("inspection_json is out of bounds"));
            }
            let inspection: QualityInspection = serde_json::from_slice(&input.inspection_json)
                .map_err(|_| Status::invalid_argument("quality inspection_json is invalid"))?;
            if inspection.version != 1
                || inspection.status != erpx_domain::quality::QualityInspectionStatus::Draft
            {
                return Err(Status::invalid_argument(
                    "quality creation requires a draft version-one inspection",
                ));
            }
            let expected_version = if input.has_expected_version {
                if input.expected_version != 0 {
                    return Err(Status::invalid_argument(
                        "quality creation expected_version must be zero",
                    ));
                }
                Some(0)
            } else {
                None
            };
            if expected_version != context.expected_aggregate_version {
                return Err(Status::invalid_argument(
                    "expected_version does not match command context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "quality",
                    "quality.write",
                )
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxQualityStore::from_pools(
                    write.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    product_persistence_status(error, "Quality", context.correlation_id)
                })?;
                let receipt = store
                    .save(&inspection, expected_version, &context)
                    .await
                    .map_err(|error| {
                        product_persistence_status(error, "Quality", context.correlation_id)
                    })?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "quality.inspection.created".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Quality audit persistence unavailable"))?;
                Ok::<CreateQualityInspectionResponse, Status>(CreateQualityInspectionResponse {
                    receipt: Some(to_quality_receipt(&receipt)),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn transition_quality_inspection(
        &self,
        request: Request<TransitionQualityInspectionRequest>,
    ) -> Result<Response<TransitionQualityInspectionResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            let _scope = explicit_product_scope(access_scope(&context.tenant_scope), "Quality")?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let inspection_id = parse_uuid(&input.inspection_id, "inspection_id")?;
            let expected_version = if input.expected_version == 0 {
                return Err(Status::invalid_argument(
                    "expected_version must be positive",
                ));
            } else {
                Some(input.expected_version)
            };
            if expected_version != context.expected_aggregate_version {
                return Err(Status::invalid_argument(
                    "expected_version does not match command context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "quality",
                    "quality.write",
                )
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxQualityStore::from_pools(
                    write.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    product_persistence_status(error, "Quality", context.correlation_id)
                })?;
                let mut inspection = store
                    .get_authoritative(inspection_id)
                    .await
                    .map_err(|error| {
                        product_persistence_status(error, "Quality", context.correlation_id)
                    })?
                    .ok_or_else(|| Status::not_found("quality inspection not found"))?;
                let payload = serde_json::to_value(&inspection)
                    .map_err(|_| Status::internal("quality inspection encoding failed"))?;
                let request_hash = quality_request_hash(&payload, expected_version, &context)
                    .map_err(|_| Status::internal("quality idempotency hash failed"))?;
                let command_key = format!("quality/inspection/{}", context.idempotency_key);
                if let Some(receipt) = store
                    .replay_receipt(&command_key, &request_hash)
                    .await
                    .map_err(|error| {
                        product_persistence_status(error, "Quality", context.correlation_id)
                    })?
                {
                    return Ok::<TransitionQualityInspectionResponse, Status>(
                        TransitionQualityInspectionResponse {
                            receipt: Some(to_quality_receipt(&receipt)),
                            correlation_id: context.correlation_id.to_string(),
                        },
                    );
                }
                match input.action.as_str() {
                    "start" => inspection
                        .start(Utc::now(), &context)
                        .map_err(|error| Status::failed_precondition(error.to_string()))?,
                    "record_result" => inspection
                        .record_result(
                            input.passed_quantity,
                            input.failed_quantity,
                            (!input.disposition_note.trim().is_empty())
                                .then_some(input.disposition_note.clone()),
                            Utc::now(),
                            &context,
                        )
                        .map_err(|error| Status::failed_precondition(error.to_string()))?,
                    "release" => inspection
                        .release(Utc::now(), &context)
                        .map_err(|error| Status::failed_precondition(error.to_string()))?,
                    "cancel" => inspection
                        .cancel(Utc::now(), &context)
                        .map_err(|error| Status::failed_precondition(error.to_string()))?,
                    _ => return Err(Status::invalid_argument("unsupported quality action")),
                }
                let receipt = store
                    .save(&inspection, expected_version, &context)
                    .await
                    .map_err(|error| {
                        product_persistence_status(error, "Quality", context.correlation_id)
                    })?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: format!("quality.inspection.{}", input.action),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Quality audit persistence unavailable"))?;
                Ok::<TransitionQualityInspectionResponse, Status>(
                    TransitionQualityInspectionResponse {
                        receipt: Some(to_quality_receipt(&receipt)),
                        correlation_id: context.correlation_id.to_string(),
                    },
                )
            }
            .await;
            read.close().await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn get_quality_inspection(
        &self,
        request: Request<GetQualityInspectionRequest>,
    ) -> Result<Response<GetQualityInspectionResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope =
                explicit_product_scope(query_access_scope(proto_context)?, "Quality")?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let inspection_id = parse_uuid(&input.inspection_id, "inspection_id")?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "quality.read",
                )
                .await?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &caller_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxQualityStore::from_pools(
                    read.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    product_persistence_status(error, "Quality", context.correlation_id)
                })?;
                let value = store.get(inspection_id).await.map_err(|error| {
                    product_persistence_status(error, "Quality", context.correlation_id)
                })?;
                let (inspection_json, source_version, present) = match value {
                    Some((inspection, version)) => (
                        serde_json::to_vec(&inspection)
                            .map_err(|_| Status::internal("quality inspection encoding failed"))?,
                        version,
                        true,
                    ),
                    None => (Vec::new(), 0, false),
                };
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "quality.inspection.read".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "Quality inspection read".into()),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Quality audit persistence unavailable"))?;
                Ok::<GetQualityInspectionResponse, Status>(GetQualityInspectionResponse {
                    inspection_id: inspection_id.to_string(),
                    source_version,
                    inspection_json,
                    present,
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn list_quality_inspections(
        &self,
        request: Request<ListQualityInspectionsRequest>,
    ) -> Result<Response<ListQualityInspectionsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope =
                explicit_product_scope(query_access_scope(proto_context)?, "Quality")?;
            if matches!(caller_scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Quality reads require an explicit holding, company, or branch scope",
                ));
            }
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let branch_id = if input.branch_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.branch_id, "branch_id")?)
            };
            let requested_scope = match branch_id {
                Some(branch_id) => Scope::Branch {
                    holding_id,
                    company_id,
                    branch_id,
                },
                None => Scope::Company {
                    holding_id,
                    company_id,
                },
            };
            if !caller_scope.contains(&requested_scope) {
                return Err(Status::permission_denied(
                    "Quality read scope is not contained",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "quality.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let page_size = input.page_size;
            if page_size == 0 || page_size > MAX_QUALITY_READ_PAGE_SIZE {
                return Err(Status::invalid_argument(format!(
                    "page_size must be 1..={MAX_QUALITY_READ_PAGE_SIZE}"
                )));
            }
            let before_cursor = if input.before_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_quality_inspection_cursor(&input.before_cursor)?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &requested_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxQualityStore::from_pools(
                    read.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    product_persistence_status(error, "Quality", context.correlation_id)
                })?;
                let page = store
                    .list_read_rows(
                        QualityReadScope {
                            holding_id,
                            company_id,
                            branch_id,
                        },
                        page_size,
                        before_cursor.as_ref(),
                    )
                    .await
                    .map_err(|error| {
                        product_persistence_status(error, "Quality", context.correlation_id)
                    })?;
                let rows = page
                    .rows
                    .into_iter()
                    .map(|row| {
                        let inspection = row.inspection;
                        let inspection_json = serde_json::to_vec(&inspection)
                            .map_err(|_| Status::internal("quality inspection encoding failed"))?;
                        Ok::<_, Status>(erpx_proto::platform::QualityInspectionReadRow {
                            inspection_id: inspection.id.to_string(),
                            holding_id: inspection.holding_id.to_string(),
                            company_id: inspection.company_id.to_string(),
                            branch_id: inspection
                                .branch_id
                                .map(|id| id.to_string())
                                .unwrap_or_default(),
                            reference: inspection.reference,
                            source_version: row.source_version,
                            inspection_json,
                            projected_at: row.projected_at.to_rfc3339(),
                            kind: inspection.kind.as_str().into(),
                            status: inspection.status.as_str().into(),
                            quantity: inspection.quantity,
                        })
                    })
                    .collect::<Result<Vec<_>, Status>>()?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "quality.inspection.listed".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "Quality inspection list read".into()),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Quality audit persistence unavailable"))?;
                Ok::<ListQualityInspectionsResponse, Status>(ListQualityInspectionsResponse {
                    rows,
                    has_more: page.has_more,
                    next_cursor: page
                        .next_cursor
                        .as_ref()
                        .map(encode_quality_inspection_cursor)
                        .unwrap_or_default(),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }
}

fn to_quality_receipt(
    receipt: &erpx_infrastructure::QualityWriteReceipt,
) -> erpx_proto::platform::QualityWriteReceipt {
    erpx_proto::platform::QualityWriteReceipt {
        inspection_id: receipt.aggregate_id.to_string(),
        version: receipt.version,
        event_id: receipt.event_id.to_string(),
        replayed: receipt.replayed,
    }
}

fn legal_read_scope(
    caller_scope: &Scope,
    holding_id: &str,
    company_id: &str,
    branch_id: &str,
) -> Result<LegalReadScope, Status> {
    if matches!(caller_scope, Scope::Platform | Scope::Subtree { .. }) {
        return Err(Status::permission_denied(
            "Legal reads require an explicit holding, company, or branch scope",
        ));
    }
    let holding_id = parse_uuid(holding_id, "holding_id")?;
    let company_id = parse_uuid(company_id, "company_id")?;
    let branch_id = if branch_id.trim().is_empty() {
        None
    } else {
        Some(parse_uuid(branch_id, "branch_id")?)
    };
    let requested_scope = match branch_id {
        Some(branch_id) => Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        },
        None => Scope::Company {
            holding_id,
            company_id,
        },
    };
    if !caller_scope.contains(&requested_scope) {
        return Err(Status::permission_denied(
            "Legal read scope is not contained",
        ));
    }
    Ok(LegalReadScope {
        holding_id,
        company_id,
        branch_id,
    })
}

fn legal_access_scope(scope: LegalReadScope) -> Scope {
    match scope.branch_id {
        Some(branch_id) => Scope::Branch {
            holding_id: scope.holding_id,
            company_id: scope.company_id,
            branch_id,
        },
        None => Scope::Company {
            holding_id: scope.holding_id,
            company_id: scope.company_id,
        },
    }
}

#[derive(Clone)]
struct LegalRpcImpl {
    state: AppState,
}

impl LegalRpcImpl {
    async fn save_contract(
        &self,
        request: Request<SaveLegalContractRequest>,
    ) -> Result<Response<SaveLegalContractResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let context = command_domain(&request, input.context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Legal writes require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let contract: LegalContract = legal_json(&input.contract_json, "contract_json")?;
            let expected = legal_expected_version(
                input.has_expected_version,
                input.expected_version,
                &context,
            )?;
            if !context
                .tenant_scope
                .contains(&contract.scope.tenant_scope())
                || contract.last_command_id != context.command_id
            {
                return Err(Status::permission_denied(
                    "Legal contract identity or command scope does not match context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "legal", "legal.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let correlation_id = context.correlation_id;
            let result = async {
                let store = SqlxLegalPersistence::new(write.database_pool().clone(), holding_ref)
                    .map_err(|error| legal_persistence_status(error, correlation_id))?;
                let receipt = store
                    .save_contract(&contract, &context, expected)
                    .await
                    .map_err(|error| legal_persistence_status(error, correlation_id))?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "legal.contract.saved".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Legal audit persistence unavailable"))?;
                Ok::<SaveLegalContractResponse, Status>(SaveLegalContractResponse {
                    receipt: Some(erpx_proto::platform::LegalWriteReceipt {
                        aggregate_type: receipt.aggregate_type.as_str().into(),
                        aggregate_id: receipt.aggregate_id.to_string(),
                        version: receipt.version,
                        idempotency_key: receipt.idempotency_key,
                        event_id: receipt.event_id.to_string(),
                        replayed: receipt.replayed,
                    }),
                    correlation_id: correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn save_document(
        &self,
        request: Request<SaveLegalDocumentRequest>,
    ) -> Result<Response<SaveLegalDocumentResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let context = command_domain(&request, input.context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Legal writes require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let document: LegalDocument = legal_json(&input.document_json, "document_json")?;
            let expected = legal_expected_version(
                input.has_expected_version,
                input.expected_version,
                &context,
            )?;
            if !context
                .tenant_scope
                .contains(&document.scope.tenant_scope())
            {
                return Err(Status::permission_denied(
                    "Legal document scope does not match context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "legal", "legal.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let correlation_id = context.correlation_id;
            let result = async {
                let store = SqlxLegalPersistence::new(write.database_pool().clone(), holding_ref)
                    .map_err(|error| legal_persistence_status(error, correlation_id))?;
                let receipt = store
                    .save_document(&document, &context, expected)
                    .await
                    .map_err(|error| legal_persistence_status(error, correlation_id))?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "legal.document.saved".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Legal audit persistence unavailable"))?;
                Ok::<SaveLegalDocumentResponse, Status>(SaveLegalDocumentResponse {
                    receipt: Some(erpx_proto::platform::LegalWriteReceipt {
                        aggregate_type: receipt.aggregate_type.as_str().into(),
                        aggregate_id: receipt.aggregate_id.to_string(),
                        version: receipt.version,
                        idempotency_key: receipt.idempotency_key,
                        event_id: receipt.event_id.to_string(),
                        replayed: receipt.replayed,
                    }),
                    correlation_id: correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn record_access(
        &self,
        request: Request<RecordLegalAccessRequest>,
    ) -> Result<Response<RecordLegalAccessResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let context = command_domain(&request, input.context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Legal writes require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let entry: LegalAccessHistoryEntry = legal_json(&input.access_json, "access_json")?;
            if context.expected_aggregate_version.is_some()
                || !context.tenant_scope.contains(&entry.scope.tenant_scope())
                || entry.actor_ref != context.actor.principal_ref
            {
                return Err(Status::permission_denied(
                    "Legal access identity or command scope does not match context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "legal", "legal.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let correlation_id = context.correlation_id;
            let result = async {
                let store = SqlxLegalPersistence::new(write.database_pool().clone(), holding_ref)
                    .map_err(|error| legal_persistence_status(error, correlation_id))?;
                let receipt = store
                    .record_access_history(&entry, &context)
                    .await
                    .map_err(|error| legal_persistence_status(error, correlation_id))?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "legal.access.recorded".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Legal audit persistence unavailable"))?;
                Ok::<RecordLegalAccessResponse, Status>(RecordLegalAccessResponse {
                    receipt: Some(erpx_proto::platform::LegalWriteReceipt {
                        aggregate_type: receipt.aggregate_type.as_str().into(),
                        aggregate_id: receipt.aggregate_id.to_string(),
                        version: receipt.version,
                        idempotency_key: receipt.idempotency_key,
                        event_id: receipt.event_id.to_string(),
                        replayed: receipt.replayed,
                    }),
                    correlation_id: correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn list_contracts(
        &self,
        request: Request<ListLegalContractsRequest>,
    ) -> Result<Response<ListLegalContractsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            let read_scope = legal_read_scope(
                &caller_scope,
                &input.holding_id,
                &input.company_id,
                &input.branch_id,
            )?;
            let requested_scope = legal_access_scope(read_scope);
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "legal.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            if input.page_size == 0 || input.page_size > MAX_LEGAL_READ_PAGE_SIZE {
                return Err(Status::invalid_argument(format!(
                    "page_size must be 1..={MAX_LEGAL_READ_PAGE_SIZE}"
                )));
            }
            let before = if input.before_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_legal_contract_cursor(&input.before_cursor)?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &requested_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let correlation_id = context.correlation_id;
            let result = async {
                let store = SqlxLegalReadStore::new(read.database_pool().clone(), holding_ref)
                    .map_err(|error| legal_persistence_status(error, correlation_id))?;
                let page = store
                    .list_contracts(read_scope, input.page_size, before)
                    .await
                    .map_err(|error| legal_persistence_status(error, correlation_id))?;
                let contracts = page
                    .rows
                    .into_iter()
                    .map(|row| erpx_proto::platform::LegalContractReadRow {
                        contract_id: row.contract_id.to_string(),
                        holding_id: row.holding_id.to_string(),
                        company_id: row.company_id.to_string(),
                        branch_id: row.branch_id.map(|id| id.to_string()).unwrap_or_default(),
                        reference: row.reference,
                        title: row.title,
                        object_ref: row.object_ref,
                        content_sha256: row.content_sha256,
                        retention_until: row.retention_until.to_rfc3339(),
                        status: row.status,
                        created_by: row.created_by,
                        approved_by: row.approved_by.unwrap_or_default(),
                        source_version: row.source_version,
                        source_event_id: row.source_event_id.to_string(),
                        projected_at: row.projected_at.to_rfc3339(),
                        parties: row
                            .parties
                            .into_iter()
                            .map(|party| erpx_proto::platform::LegalContractPartyReadRow {
                                party_id: party.party_id.to_string(),
                                role: party.party_role,
                            })
                            .collect(),
                        retention_state: row.retention.retention_state,
                        source_correlation_id: row.source.correlation_id.to_string(),
                    })
                    .collect();
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "legal.contract.read".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "Legal contract read".into()),
                        correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Legal audit persistence unavailable"))?;
                Ok::<ListLegalContractsResponse, Status>(ListLegalContractsResponse {
                    contracts,
                    has_more: page.has_more,
                    next_cursor: page
                        .next_cursor
                        .as_ref()
                        .map(encode_legal_contract_cursor)
                        .unwrap_or_default(),
                    correlation_id: correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn list_documents(
        &self,
        request: Request<ListLegalDocumentsRequest>,
    ) -> Result<Response<ListLegalDocumentsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            let read_scope = legal_read_scope(
                &caller_scope,
                &input.holding_id,
                &input.company_id,
                &input.branch_id,
            )?;
            let requested_scope = legal_access_scope(read_scope);
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "legal.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            if input.page_size == 0 || input.page_size > MAX_LEGAL_READ_PAGE_SIZE {
                return Err(Status::invalid_argument(format!(
                    "page_size must be 1..={MAX_LEGAL_READ_PAGE_SIZE}"
                )));
            }
            let before = if input.before_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_legal_document_cursor(&input.before_cursor)?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &requested_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let correlation_id = context.correlation_id;
            let result = async {
                let store = SqlxLegalReadStore::new(read.database_pool().clone(), holding_ref)
                    .map_err(|error| legal_persistence_status(error, correlation_id))?;
                let page = store
                    .list_documents(read_scope, input.page_size, before)
                    .await
                    .map_err(|error| legal_persistence_status(error, correlation_id))?;
                let documents = page
                    .rows
                    .into_iter()
                    .map(|row| erpx_proto::platform::LegalDocumentReadRow {
                        document_id: row.document_id.to_string(),
                        holding_id: row.holding_id.to_string(),
                        company_id: row.company_id.to_string(),
                        branch_id: row.branch_id.map(|id| id.to_string()).unwrap_or_default(),
                        title: row.title,
                        current_version: row.current_version,
                        archived: row.archived,
                        source_version: row.source_version,
                        source_event_id: row.source_event_id.to_string(),
                        projected_at: row.projected_at.to_rfc3339(),
                        versions: row
                            .versions
                            .into_iter()
                            .map(
                                |version| erpx_proto::platform::LegalDocumentVersionReadRow {
                                    version: version.version,
                                    object_ref: version.object_ref,
                                    content_sha256: version.content_sha256,
                                    created_at: version.created_at.to_rfc3339(),
                                    created_by: version.created_by,
                                    source_version: version.source_version,
                                    source_event_id: version.source_event_id.to_string(),
                                },
                            )
                            .collect(),
                        source_correlation_id: row.source.correlation_id.to_string(),
                    })
                    .collect();
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "legal.document.read".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "Legal document read".into()),
                        correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Legal audit persistence unavailable"))?;
                Ok::<ListLegalDocumentsResponse, Status>(ListLegalDocumentsResponse {
                    documents,
                    has_more: page.has_more,
                    next_cursor: page
                        .next_cursor
                        .as_ref()
                        .map(encode_legal_document_cursor)
                        .unwrap_or_default(),
                    correlation_id: correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn list_access_history(
        &self,
        request: Request<ListLegalAccessHistoryRequest>,
    ) -> Result<Response<ListLegalAccessHistoryResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            let read_scope = legal_read_scope(
                &caller_scope,
                &input.holding_id,
                &input.company_id,
                &input.branch_id,
            )?;
            let requested_scope = legal_access_scope(read_scope);
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "legal.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let document_id = if input.document_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.document_id, "document_id")?)
            };
            if input.page_size == 0 || input.page_size > MAX_LEGAL_READ_PAGE_SIZE {
                return Err(Status::invalid_argument(format!(
                    "page_size must be 1..={MAX_LEGAL_READ_PAGE_SIZE}"
                )));
            }
            let before = if input.before_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_legal_access_cursor(&input.before_cursor)?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &requested_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let correlation_id = context.correlation_id;
            let result = async {
                let store = SqlxLegalReadStore::new(read.database_pool().clone(), holding_ref)
                    .map_err(|error| legal_persistence_status(error, correlation_id))?;
                let page = store
                    .list_access_history(read_scope, document_id, input.page_size, before)
                    .await
                    .map_err(|error| legal_persistence_status(error, correlation_id))?;
                let entries = page
                    .rows
                    .into_iter()
                    .map(|row| erpx_proto::platform::LegalAccessHistoryReadRow {
                        entry_id: row.entry_id.to_string(),
                        document_id: row.document_id.to_string(),
                        holding_id: row.holding_id.to_string(),
                        company_id: row.company_id.to_string(),
                        branch_id: row.branch_id.map(|id| id.to_string()).unwrap_or_default(),
                        document_version: row.document_version,
                        actor_ref: row.actor_ref,
                        action: row.action,
                        purpose: row.purpose,
                        observed_at: row.observed_at.to_rfc3339(),
                        source_version: row.source_version,
                        source_event_id: row.source_event_id.to_string(),
                        projected_at: row.projected_at.to_rfc3339(),
                        source_correlation_id: row.source.correlation_id.to_string(),
                    })
                    .collect();
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "legal.access.read".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "Legal access history read".into()),
                        correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Legal audit persistence unavailable"))?;
                Ok::<ListLegalAccessHistoryResponse, Status>(ListLegalAccessHistoryResponse {
                    entries,
                    has_more: page.has_more,
                    next_cursor: page
                        .next_cursor
                        .as_ref()
                        .map(encode_legal_access_cursor)
                        .unwrap_or_default(),
                    correlation_id: correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }
}

#[tonic::async_trait]
impl LegalService for LegalRpcImpl {
    async fn save_legal_contract(
        &self,
        request: Request<SaveLegalContractRequest>,
    ) -> Result<Response<SaveLegalContractResponse>, Status> {
        self.save_contract(request).await
    }

    async fn save_legal_document(
        &self,
        request: Request<SaveLegalDocumentRequest>,
    ) -> Result<Response<SaveLegalDocumentResponse>, Status> {
        self.save_document(request).await
    }

    async fn record_legal_access(
        &self,
        request: Request<RecordLegalAccessRequest>,
    ) -> Result<Response<RecordLegalAccessResponse>, Status> {
        self.record_access(request).await
    }

    async fn list_legal_contracts(
        &self,
        request: Request<ListLegalContractsRequest>,
    ) -> Result<Response<ListLegalContractsResponse>, Status> {
        self.list_contracts(request).await
    }

    async fn list_legal_documents(
        &self,
        request: Request<ListLegalDocumentsRequest>,
    ) -> Result<Response<ListLegalDocumentsResponse>, Status> {
        self.list_documents(request).await
    }

    async fn list_legal_access_history(
        &self,
        request: Request<ListLegalAccessHistoryRequest>,
    ) -> Result<Response<ListLegalAccessHistoryResponse>, Status> {
        self.list_access_history(request).await
    }
}

#[derive(Clone)]
struct CrmRpcImpl {
    state: AppState,
}

#[tonic::async_trait]
impl CrmService for CrmRpcImpl {
    async fn list_crm_read_rows(
        &self,
        request: Request<ListCrmReadRowsRequest>,
    ) -> Result<Response<ListCrmReadRowsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "CRM reads require an explicit holding, company, or branch scope",
                ));
            }
            self.state
                .auth
                .authorize_role(&self.state.store, &claims.principal_ref, &scope, "crm.read")
                .await?;
            let read_scope = finance_banking_read_tenant_scope(&scope)?;
            let input = request.get_ref();
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let aggregate_type =
                validate_transport_reference(&input.aggregate_type, "aggregate_type")?;
            if !matches!(
                aggregate_type,
                "marketing_crm_accepted_lead_handoff"
                    | "marketing_crm_opportunity"
                    | "marketing_crm_attribution"
            ) {
                return Err(Status::invalid_argument("aggregate_type is unsupported"));
            }
            let page_size = match input.page_size {
                0 => 25,
                value if value <= MAX_CRM_READ_PAGE_SIZE => value,
                _ => {
                    return Err(Status::invalid_argument(
                        "CRM read page size exceeds the allowed bound",
                    ));
                }
            };
            let after_id = if input.after_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.after_id, "after_id")?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let correlation_id = context.correlation_id;
            let page = with_crm_read_store(
                router,
                tenant_id,
                holding_ref,
                correlation_id,
                |store| async move {
                    store
                        .read_crm_page(aggregate_type, &read_scope, after_id, page_size)
                        .await
                        .map_err(|error| crm_persistence_status(error, correlation_id))
                },
            )
            .await?;
            let rows = page
                .rows
                .into_iter()
                .map(|row| {
                    Ok::<CrmReadRow, Status>(CrmReadRow {
                        aggregate_type: row.aggregate_type,
                        aggregate_id: row.aggregate_id.to_string(),
                        holding_id: row.holding_id.to_string(),
                        company_id: row.company_id.to_string(),
                        branch_id: row.branch_id.map(|id| id.to_string()).unwrap_or_default(),
                        source_version: row.source_version,
                        payload_json: serde_json::to_vec(&row.payload)
                            .map_err(|_| Status::internal("CRM read encoding failed"))?,
                    })
                })
                .collect::<Result<Vec<_>, Status>>()?;
            Ok(Response::new(ListCrmReadRowsResponse {
                rows,
                has_more: page.has_more,
                next_after_id: page
                    .next_after_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                correlation_id: correlation_id.to_string(),
            }))
        })
    }

    async fn save_crm_handoff(
        &self,
        request: Request<SaveCrmHandoffRequest>,
    ) -> Result<Response<SaveCrmHandoffResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "CRM mutations require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            let handoff: AcceptedLeadHandoff =
                decode_finance_banking_json(&request.get_ref().handoff_json, "handoff_json")?;
            handoff
                .validate()
                .map_err(|_| Status::failed_precondition("CRM handoff is invalid"))?;
            if !context.tenant_scope.contains(&handoff.scope)
                || handoff.command_id != context.command_id
            {
                return Err(Status::permission_denied("CRM handoff binding is invalid"));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "crm", "crm.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let correlation_id = context.correlation_id;
            let command_context = context.clone();
            let receipt = with_crm_write_store(
                router,
                tenant_id,
                holding_ref,
                correlation_id,
                |store| async move {
                    store
                        .save_handoff(&handoff, &command_context, None)
                        .await
                        .map_err(|error| crm_persistence_status(error, correlation_id))
                },
            )
            .await?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "crm.handoff.saved".into(),
                    purpose: "crm.write".into(),
                    correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("CRM audit persistence unavailable"))?;
            Ok(Response::new(SaveCrmHandoffResponse {
                receipt: Some(erpx_proto::platform::CrmWriteReceipt {
                    aggregate_type: receipt.aggregate_type,
                    aggregate_id: receipt.aggregate_id.to_string(),
                    reference: receipt.reference,
                    version: receipt.version,
                    replayed: receipt.replayed,
                }),
                correlation_id: correlation_id.to_string(),
            }))
        })
    }

    async fn save_crm_opportunity(
        &self,
        request: Request<SaveCrmOpportunityRequest>,
    ) -> Result<Response<SaveCrmOpportunityResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "CRM mutations require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            let opportunity: CrmOpportunity = decode_finance_banking_json(
                &request.get_ref().opportunity_json,
                "opportunity_json",
            )?;
            opportunity
                .validate()
                .map_err(|_| Status::failed_precondition("CRM opportunity is invalid"))?;
            if !context.tenant_scope.contains(&opportunity.scope)
                || opportunity.command_id != context.command_id
            {
                return Err(Status::permission_denied(
                    "CRM opportunity binding is invalid",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "crm", "crm.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let correlation_id = context.correlation_id;
            let command_context = context.clone();
            let expected = request
                .get_ref()
                .has_expected_version
                .then_some(request.get_ref().expected_version);
            let receipt = with_crm_write_store(
                router,
                tenant_id,
                holding_ref,
                correlation_id,
                |store| async move {
                    store
                        .save_opportunity(&opportunity, &command_context, expected)
                        .await
                        .map_err(|error| crm_persistence_status(error, correlation_id))
                },
            )
            .await?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "crm.opportunity.saved".into(),
                    purpose: "crm.write".into(),
                    correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("CRM audit persistence unavailable"))?;
            Ok(Response::new(SaveCrmOpportunityResponse {
                receipt: Some(erpx_proto::platform::CrmWriteReceipt {
                    aggregate_type: receipt.aggregate_type,
                    aggregate_id: receipt.aggregate_id.to_string(),
                    reference: receipt.reference,
                    version: receipt.version,
                    replayed: receipt.replayed,
                }),
                correlation_id: correlation_id.to_string(),
            }))
        })
    }

    async fn record_crm_attribution(
        &self,
        request: Request<RecordCrmAttributionRequest>,
    ) -> Result<Response<RecordCrmAttributionResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "CRM mutations require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            let opportunity_id = parse_uuid(&request.get_ref().opportunity_id, "opportunity_id")?;
            let model = match request.get_ref().model.as_str() {
                "first_touch" => AttributionModel::FirstTouch,
                "last_touch" => AttributionModel::LastTouch,
                "linear" => AttributionModel::Linear,
                "u_shaped" => AttributionModel::UShaped,
                "weighted" => AttributionModel::Weighted,
                _ => return Err(Status::invalid_argument("CRM attribution model is invalid")),
            };
            if request.get_ref().expected_opportunity_version == 0 {
                return Err(Status::invalid_argument(
                    "expected_opportunity_version must be positive",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(&claims, &context, customer_id, "crm", "crm.write")
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let correlation_id = context.correlation_id;
            let command_context = context.clone();
            let expected = request.get_ref().expected_opportunity_version;
            let receipt = with_crm_write_store(
                router,
                tenant_id,
                holding_ref,
                correlation_id,
                |store| async move {
                    store
                        .record_attribution(opportunity_id, model, &command_context, expected)
                        .await
                        .map_err(|error| crm_persistence_status(error, correlation_id))
                },
            )
            .await?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "crm.attribution.recorded".into(),
                    purpose: "crm.write".into(),
                    correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("CRM audit persistence unavailable"))?;
            Ok(Response::new(RecordCrmAttributionResponse {
                receipt: Some(erpx_proto::platform::CrmAttributionWriteReceipt {
                    record_id: receipt.record_id.to_string(),
                    reference: receipt.reference,
                    model: receipt.model,
                    source_opportunity_version: receipt.source_opportunity_version,
                    version: receipt.version,
                    replayed: receipt.replayed,
                }),
                correlation_id: correlation_id.to_string(),
            }))
        })
    }
}

fn marketing_opaque_secret() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn marketing_secret_hash(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

fn marketing_scope(
    context: &erpx_domain::contracts::CommandContext,
) -> Result<TenantScope, Status> {
    match context.tenant_scope {
        TenantScope::Platform | TenantScope::Subtree { .. } => Err(Status::permission_denied(
            "Marketing content writes require an explicit tenant scope",
        )),
        ref scope => Ok(scope.clone()),
    }
}

fn marketing_expiry(value: &str) -> Result<DateTime<Utc>, Status> {
    DateTime::parse_from_rfc3339(value.trim())
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| Status::invalid_argument("expires_at must be RFC3339"))
}

fn marketing_application_status(_: ApplicationError, _: Uuid) -> Status {
    Status::failed_precondition("Marketing content command failed")
}

fn provider_account_scope(
    context: &erpx_domain::contracts::CommandContext,
) -> Result<Uuid, Status> {
    match context.tenant_scope {
        TenantScope::Holding { holding_id } => Ok(holding_id),
        TenantScope::Platform
        | TenantScope::Company { .. }
        | TenantScope::Branch { .. }
        | TenantScope::Subtree { .. } => Err(Status::permission_denied(
            "Marketing provider account commands require holding scope",
        )),
    }
}

fn validate_marketing_secret_reference<'a>(
    value: &'a str,
    field: &'static str,
) -> Result<&'a str, Status> {
    let value = value.trim();
    MarketingCallbackSecretReference::new(value.to_owned())
        .map(|_| value)
        .map_err(|_| Status::invalid_argument(format!("{field} is invalid")))
}

fn marketing_provider_onboarding_status(error: PersistenceError, correlation_id: Uuid) -> Status {
    let status = match error {
        PersistenceError::OptimisticConcurrency { .. }
        | PersistenceError::IdempotencyConflict { .. } => {
            Status::aborted("Marketing provider account conflicts with current state")
        }
        PersistenceError::NotFound { .. } => {
            Status::not_found("Marketing provider account not found")
        }
        PersistenceError::InvalidData(_) => {
            Status::failed_precondition("Marketing provider account cannot be persisted")
        }
        PersistenceError::Configuration(_) => {
            Status::failed_precondition("Marketing provider persistence is not configured")
        }
        PersistenceError::Database(_) | PersistenceError::Migration(_) => {
            Status::unavailable("Marketing provider persistence is unavailable")
        }
        PersistenceError::Serialization(_) => {
            Status::internal("Marketing provider persistence failed")
        }
        PersistenceError::ClaimLost { .. } => {
            Status::aborted("Marketing provider command ownership changed")
        }
    };
    errors::enrich(status, Some(correlation_id))
}

fn marketing_provider_receipt(
    receipt: erpx_application::MarketingProviderAccountWriteReceipt,
) -> erpx_proto::platform::MarketingProviderAccountWriteReceipt {
    erpx_proto::platform::MarketingProviderAccountWriteReceipt {
        provider_account_id: receipt.provider_account_id.to_string(),
        provider_account_ref: receipt.provider_account_ref,
        tenant_id: receipt.tenant_id,
        holding_ref: receipt.holding_ref,
        provider_key: receipt.provider_key,
        provider_priority: receipt.provider_priority,
        channels: receipt.channels,
        state: receipt.state,
        onboarding_state: receipt.onboarding_state,
        version: receipt.version,
        changed_at: receipt.changed_at.to_rfc3339(),
        replayed: receipt.replayed,
    }
}

fn marketing_receipt(
    receipt: erpx_application::MarketingContentWriteReceipt,
) -> erpx_proto::platform::MarketingContentWriteReceipt {
    erpx_proto::platform::MarketingContentWriteReceipt {
        aggregate_type: receipt.aggregate_type,
        aggregate_id: receipt.aggregate_id.to_string(),
        version: receipt.version,
        idempotency_key: receipt.idempotency_key,
        event_id: receipt.event_id.to_string(),
        replayed: receipt.replayed,
    }
}

async fn route_marketing_content_store(
    state: &AppState,
    tenant_id: &str,
    holding_ref: &str,
    correlation_id: Uuid,
) -> Result<
    (
        RoutedTenantDatabase,
        RoutedTenantDatabase,
        SqlxMarketingContentWriteAdapter,
    ),
    Status,
> {
    let (write, read) =
        route_marketing_databases(state, tenant_id, holding_ref, correlation_id).await?;
    let store = match SqlxMarketingContentStore::new(
        write.database_pool().clone(),
        read.database_pool().clone(),
        holding_ref.to_owned(),
    ) {
        Ok(store) => store,
        Err(error) => {
            write.close().await;
            read.close().await;
            return Err(marketing_content_persistence_status(error, correlation_id));
        }
    };
    Ok((
        write,
        read,
        SqlxMarketingContentWriteAdapter::from_store(store),
    ))
}

async fn route_marketing_profile_store(
    state: &AppState,
    tenant_id: &str,
    holding_ref: &str,
    correlation_id: Uuid,
) -> Result<
    (
        RoutedTenantDatabase,
        RoutedTenantDatabase,
        SqlxMarketingStore,
    ),
    Status,
> {
    let (write, read) =
        route_marketing_databases(state, tenant_id, holding_ref, correlation_id).await?;
    let store = match SqlxMarketingStore::new(
        write.database_pool().clone(),
        read.database_pool().clone(),
        holding_ref.to_owned(),
    ) {
        Ok(store) => store,
        Err(error) => {
            write.close().await;
            read.close().await;
            return Err(marketing_content_persistence_status(error, correlation_id));
        }
    };
    Ok((write, read, store))
}

async fn route_marketing_databases(
    state: &AppState,
    tenant_id: &str,
    holding_ref: &str,
    correlation_id: Uuid,
) -> Result<(RoutedTenantDatabase, RoutedTenantDatabase), Status> {
    let router = state
        .tenant_router
        .as_ref()
        .ok_or_else(|| Status::failed_precondition("tenant database routing is not configured"))?;
    let write = router
        .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
        .await
        .map_err(|error| marketing_content_persistence_status(error, correlation_id))?;
    let read = match router
        .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
        .await
    {
        Ok(read) => read,
        Err(error) => {
            write.close().await;
            return Err(marketing_content_persistence_status(error, correlation_id));
        }
    };
    Ok((write, read))
}

async fn record_marketing_write_audit(
    state: &AppState,
    claims: &AccessTokenClaims,
    customer_id: Uuid,
    action: &str,
    context: &erpx_domain::contracts::CommandContext,
) -> Result<(), Status> {
    state
        .store
        .record(AuditEvent {
            actor_ref: claims.principal_ref.clone(),
            customer_account_id: Some(customer_id),
            action: action.into(),
            purpose: context.audit.purpose.clone(),
            correlation_id: context.correlation_id,
        })
        .await
        .map_err(|_| Status::unavailable("Marketing audit persistence unavailable"))
}

#[derive(Clone)]
struct MarketingContentRpcImpl {
    state: AppState,
}

#[tonic::async_trait]
impl MarketingContentService for MarketingContentRpcImpl {
    async fn submit_marketing_form(
        &self,
        request: Request<SubmitMarketingFormRequest>,
    ) -> Result<Response<SubmitMarketingFormResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            if matches!(
                context.tenant_scope,
                TenantScope::Platform | TenantScope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "Marketing form submission requires an explicit tenant scope",
                ));
            }
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let form_id = parse_uuid(&input.form_id, "form_id")?;
            let submission_id = parse_uuid(&input.submission_id, "submission_id")?;
            let values = marketing_form_values(&input.values_json)?;
            let form = ContentFormRef::new(form_id, context.tenant_scope.clone())
                .map_err(|_| Status::invalid_argument("form scope is invalid"))?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing.write",
                )
                .await?;
            let correlation_id = context.correlation_id;
            let (write, read, port) =
                route_marketing_content_store(&self.state, tenant_id, holding_ref, correlation_id)
                    .await?;
            let result = async {
                let command = SubmitPublishedFormCommand::new(
                    form.clone(),
                    submission_id,
                    values,
                    Utc::now(),
                    context.clone(),
                )
                .map_err(|error| marketing_application_status(error, correlation_id))?;
                port.submit_published_form(command)
                    .await
                    .map_err(|error| marketing_application_status(error, correlation_id))
            }
            .await;
            write.close().await;
            read.close().await;
            let receipt = result?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "marketing.form.submitted".into(),
                    purpose: context.audit.purpose,
                    correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Marketing audit persistence unavailable"))?;
            Ok(Response::new(SubmitMarketingFormResponse {
                receipt: Some(erpx_proto::platform::MarketingContentWriteReceipt {
                    aggregate_type: receipt.aggregate_type,
                    aggregate_id: receipt.aggregate_id.to_string(),
                    version: receipt.version,
                    idempotency_key: receipt.idempotency_key,
                    event_id: receipt.event_id.to_string(),
                    replayed: receipt.replayed,
                }),
                correlation_id: correlation_id.to_string(),
            }))
        })
    }

    async fn get_marketing_delivery_read(
        &self,
        request: Request<GetMarketingDeliveryReadRequest>,
    ) -> Result<Response<GetMarketingDeliveryReadResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let delivery_id = parse_uuid(&input.delivery_id, "delivery_id")?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "marketing.read",
                )
                .await?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &caller_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let (write, read) = route_marketing_databases(
                &self.state,
                tenant_id,
                holding_ref,
                context.correlation_id,
            )
            .await?;
            let result = async {
                let store = SqlxMarketingDeliveryStore::from_pools(
                    write.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| {
                    marketing_content_persistence_status(error, context.correlation_id)
                })?;
                let dispatch = store.get(delivery_id).await.map_err(|error| {
                    marketing_content_persistence_status(error, context.correlation_id)
                })?;
                let Some(dispatch) = dispatch else {
                    return Ok::<GetMarketingDeliveryReadResponse, Status>(
                        GetMarketingDeliveryReadResponse {
                            delivery: None,
                            correlation_id: context.correlation_id.to_string(),
                        },
                    );
                };
                let holding_id = match dispatch.scope {
                    TenantScope::Holding { holding_id }
                    | TenantScope::Company { holding_id, .. }
                    | TenantScope::Branch { holding_id, .. }
                    | TenantScope::Subtree { holding_id, .. } => holding_id,
                    TenantScope::Platform => {
                        return Err(Status::failed_precondition(
                            "Marketing delivery read cannot be platform scoped",
                        ));
                    }
                };
                if !caller_scope.contains(&Scope::Holding { holding_id }) {
                    return Err(Status::permission_denied(
                        "Marketing delivery read scope is outside the authorized scope",
                    ));
                }
                Ok(GetMarketingDeliveryReadResponse {
                    delivery: Some(MarketingDeliveryRead {
                        delivery_id: dispatch.id.to_string(),
                        holding_id: holding_id.to_string(),
                        state: marketing_delivery_state_text(dispatch.state).into(),
                        attempt_count: dispatch.attempt_count,
                        next_attempt_at: dispatch.next_attempt_at.to_rfc3339(),
                        expires_at: dispatch.expires_at.to_rfc3339(),
                        version: dispatch.version,
                        read_only: true,
                        present: true,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            read.close().await;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "marketing.delivery.read".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "Marketing delivery read".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Marketing audit persistence unavailable"))?;
            Ok(Response::new(result?))
        })
    }

    async fn list_marketing_content_read_rows(
        &self,
        request: Request<ListMarketingContentReadRowsRequest>,
    ) -> Result<Response<ListMarketingContentReadRowsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            if matches!(caller_scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Marketing content reads require an explicit holding, company, or branch scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "marketing.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let aggregate_type = if input.aggregate_type.trim().is_empty() {
                None
            } else {
                Some(validate_transport_reference(
                    &input.aggregate_type,
                    "aggregate_type",
                )?)
            };
            if aggregate_type
                .is_some_and(|value| !matches!(value, "asset" | "template" | "form" | "domain"))
            {
                return Err(Status::invalid_argument("aggregate_type is unsupported"));
            }
            let page_size = match input.page_size {
                0 => 25,
                value if value <= MAX_MARKETING_CONTENT_READ_PAGE_SIZE => value,
                _ => {
                    return Err(Status::invalid_argument(
                        "Marketing content read page size exceeds the allowed bound",
                    ));
                }
            };
            let after = if input.after_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(
                    &input.after_id,
                    "Marketing content read cursor",
                )?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &caller_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let correlation_id = context.correlation_id;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let result = async {
                let store = SqlxMarketingContentStore::new(
                    read.database_pool().clone(),
                    read.database_pool().clone(),
                    holding_ref,
                )
                .map_err(|error| marketing_content_persistence_status(error, correlation_id))?;
                let (rows, next_cursor) = store
                    .list_read_rows(aggregate_type, page_size, after)
                    .await
                    .map_err(|error| marketing_content_persistence_status(error, correlation_id))?;
                let mut mapped = Vec::with_capacity(rows.len());
                for row in rows {
                    let row_scope = access_scope(&row.scope);
                    if !caller_scope.contains(&row_scope) {
                        return Err(Status::permission_denied(
                            "Marketing content read scope is outside the authorized scope",
                        ));
                    }
                    let (holding_id, company_id, branch_id) = match row_scope {
                        Scope::Holding { holding_id } => {
                            (holding_id.to_string(), String::new(), String::new())
                        }
                        Scope::Company {
                            holding_id,
                            company_id,
                        } => (
                            holding_id.to_string(),
                            company_id.to_string(),
                            String::new(),
                        ),
                        Scope::Branch {
                            holding_id,
                            company_id,
                            branch_id,
                        } => (
                            holding_id.to_string(),
                            company_id.to_string(),
                            branch_id.to_string(),
                        ),
                        Scope::Platform | Scope::Subtree { .. } => {
                            return Err(Status::failed_precondition(
                                "Marketing content projection contains an unsupported scope",
                            ));
                        }
                    };
                    let label = row.label.unwrap_or_default();
                    if label.len() > 255 || label.chars().any(char::is_control) {
                        return Err(Status::failed_precondition(
                            "Marketing content projection contains an invalid label",
                        ));
                    }
                    mapped.push(MarketingContentReadRow {
                        aggregate_type: row.aggregate_type,
                        aggregate_id: row.aggregate_id.to_string(),
                        holding_id,
                        company_id,
                        branch_id,
                        label,
                        status: row.status,
                        source_version: row.source_version,
                        source_event_id: row.source_event_id.to_string(),
                        projected_at: row.projected_at.to_rfc3339(),
                        read_only: true,
                        present: true,
                    });
                }
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "marketing.content.read".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "Marketing content read".into()),
                        correlation_id,
                    })
                    .await
                    .map_err(|_| {
                        Status::unavailable("Marketing content audit persistence unavailable")
                    })?;
                Ok::<ListMarketingContentReadRowsResponse, Status>(
                    ListMarketingContentReadRowsResponse {
                        rows: mapped,
                        has_more: next_cursor.is_some(),
                        next_after_id: next_cursor.map(|id| id.to_string()).unwrap_or_default(),
                        correlation_id: correlation_id.to_string(),
                    },
                )
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn erase_marketing_profile(
        &self,
        request: Request<EraseMarketingProfileRequest>,
    ) -> Result<Response<EraseMarketingProfileResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            if matches!(
                context.tenant_scope,
                TenantScope::Platform | TenantScope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "Marketing profile erasure requires an explicit tenant scope",
                ));
            }
            if input.reason.trim().is_empty()
                || input.reason.len() > erpx_domain::marketing::MAX_MARKETING_PROFILE_ERASURE_REASON
                || input.reason.chars().any(char::is_control)
            {
                return Err(Status::invalid_argument("erasure reason is invalid"));
            }
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let profile_id = parse_uuid(&input.profile_id, "profile_id")?;
            let erased_at = if input.erased_at.trim().is_empty() {
                Utc::now()
            } else {
                marketing_expiry(&input.erased_at)?
            };
            let erasure = MarketingProfileErasure::new(
                profile_id,
                context.tenant_scope.clone(),
                erased_at,
                input.reason.trim(),
            )
            .map_err(|_| Status::invalid_argument("profile erasure is invalid"))?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing.profile.erase",
                )
                .await?;
            let correlation = MarketingProfileErasureCorrelation::new(
                context.correlation_id,
                context.causation_id,
            )
            .map_err(|_| Status::invalid_argument("erasure correlation is invalid"))?;
            let command = EraseMarketingProfileCommand::new(
                erasure,
                context.tenant_scope.clone(),
                context.idempotency_key.clone(),
                correlation,
            )
            .map_err(|error| errors::application(error, context.correlation_id))?;
            let correlation_id = context.correlation_id;
            let (write, read, store) =
                route_marketing_profile_store(&self.state, tenant_id, holding_ref, correlation_id)
                    .await?;
            let adapter = SqlxMarketingProfilePrivacyAdapter::from_store(store);
            let port = AuthorizedMarketingProfilePrivacyPort::new(adapter);
            let result = authorize_and_erase_marketing_profile(&port, command)
                .await
                .map_err(|error| errors::application(error, correlation_id));
            write.close().await;
            read.close().await;
            let receipt = result?;
            record_marketing_write_audit(
                &self.state,
                &claims,
                customer_id,
                "marketing.profile.erased",
                &context,
            )
            .await?;
            Ok(Response::new(EraseMarketingProfileResponse {
                receipt: Some(erpx_proto::platform::MarketingProfileErasureReceipt {
                    profile_id: receipt.profile_id.to_string(),
                    idempotency_key: receipt.idempotency_key,
                    replayed: receipt.replayed,
                }),
                correlation_id: correlation_id.to_string(),
            }))
        })
    }

    async fn issue_marketing_preview(
        &self,
        request: Request<IssueMarketingPreviewRequest>,
    ) -> Result<Response<IssueMarketingPreviewResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            let scope = marketing_scope(&context)?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let template_id = parse_uuid(&input.template_id, "template_id")?;
            let template =
                ContentTemplateRef::new(template_id, input.template_version, scope.clone())
                    .map_err(|_| Status::invalid_argument("template reference is invalid"))?;
            let expires_at = marketing_expiry(&input.expires_at)?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing.write",
                )
                .await?;
            let issued_at = Utc::now();
            let preview = PreviewGrant::new(Uuid::new_v4(), template, issued_at, expires_at)
                .map_err(|_| Status::invalid_argument("preview expiry is invalid"))?;
            let token = marketing_opaque_secret();
            let correlation_id = context.correlation_id;
            let (write, read, port) =
                route_marketing_content_store(&self.state, tenant_id, holding_ref, correlation_id)
                    .await?;
            let result = async {
                let command = IssuePreviewCommand::new(
                    preview.clone(),
                    marketing_secret_hash(&token),
                    context.clone(),
                )
                .map_err(|error| marketing_application_status(error, correlation_id))?;
                port.issue_preview(command)
                    .await
                    .map_err(|error| marketing_application_status(error, correlation_id))
            }
            .await;
            write.close().await;
            read.close().await;
            let receipt = result?;
            record_marketing_write_audit(
                &self.state,
                &claims,
                customer_id,
                "marketing.preview.issued",
                &context,
            )
            .await?;
            Ok(Response::new(IssueMarketingPreviewResponse {
                receipt: Some(marketing_receipt(receipt)),
                preview_id: preview.id.to_string(),
                token,
                expires_at: preview.expires_at.to_rfc3339(),
                correlation_id: correlation_id.to_string(),
            }))
        })
    }

    async fn revoke_marketing_preview(
        &self,
        request: Request<RevokeMarketingPreviewRequest>,
    ) -> Result<Response<RevokeMarketingPreviewResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            let scope = marketing_scope(&context)?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let preview_id = parse_uuid(&input.preview_id, "preview_id")?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing.write",
                )
                .await?;
            let correlation_id = context.correlation_id;
            let (write, read, port) =
                route_marketing_content_store(&self.state, tenant_id, holding_ref, correlation_id)
                    .await?;
            let result = async {
                let command =
                    RevokePreviewCommand::new(preview_id, scope, Utc::now(), context.clone())
                        .map_err(|error| marketing_application_status(error, correlation_id))?;
                port.revoke_preview(command)
                    .await
                    .map_err(|error| marketing_application_status(error, correlation_id))
            }
            .await;
            write.close().await;
            read.close().await;
            let receipt = result?;
            record_marketing_write_audit(
                &self.state,
                &claims,
                customer_id,
                "marketing.preview.revoked",
                &context,
            )
            .await?;
            Ok(Response::new(RevokeMarketingPreviewResponse {
                receipt: Some(marketing_receipt(receipt)),
                correlation_id: correlation_id.to_string(),
            }))
        })
    }

    async fn issue_marketing_domain_verification(
        &self,
        request: Request<IssueMarketingDomainVerificationRequest>,
    ) -> Result<Response<IssueMarketingDomainVerificationResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            let scope = marketing_scope(&context)?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let domain_id = parse_uuid(&input.domain_id, "domain_id")?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing.write",
                )
                .await?;
            let challenge = marketing_opaque_secret();
            let correlation_id = context.correlation_id;
            let (write, read, port) =
                route_marketing_content_store(&self.state, tenant_id, holding_ref, correlation_id)
                    .await?;
            let result = async {
                let command = IssueDomainVerificationCommand::new(
                    domain_id,
                    scope,
                    marketing_secret_hash(&challenge),
                    context.clone(),
                )
                .map_err(|error| marketing_application_status(error, correlation_id))?;
                port.issue_domain_verification(command)
                    .await
                    .map_err(|error| marketing_application_status(error, correlation_id))
            }
            .await;
            write.close().await;
            read.close().await;
            let receipt = result?;
            record_marketing_write_audit(
                &self.state,
                &claims,
                customer_id,
                "marketing.domain.verification_issued",
                &context,
            )
            .await?;
            Ok(Response::new(IssueMarketingDomainVerificationResponse {
                receipt: Some(marketing_receipt(receipt)),
                challenge,
                correlation_id: correlation_id.to_string(),
            }))
        })
    }

    async fn verify_marketing_domain(
        &self,
        request: Request<VerifyMarketingDomainRequest>,
    ) -> Result<Response<VerifyMarketingDomainResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            if input.proof.trim().is_empty() || input.proof.len() > 512 {
                return Err(Status::invalid_argument("domain proof is invalid"));
            }
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            let scope = marketing_scope(&context)?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let domain_id = parse_uuid(&input.domain_id, "domain_id")?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing.write",
                )
                .await?;
            let correlation_id = context.correlation_id;
            let (write, read, port) =
                route_marketing_content_store(&self.state, tenant_id, holding_ref, correlation_id)
                    .await?;
            let result = async {
                let command = VerifyDomainCommand::new(
                    domain_id,
                    scope,
                    marketing_secret_hash(input.proof.trim()),
                    context.clone(),
                )
                .map_err(|error| marketing_application_status(error, correlation_id))?;
                port.verify_domain(command)
                    .await
                    .map_err(|error| marketing_application_status(error, correlation_id))
            }
            .await;
            write.close().await;
            read.close().await;
            let receipt = result?;
            record_marketing_write_audit(
                &self.state,
                &claims,
                customer_id,
                "marketing.domain.verified",
                &context,
            )
            .await?;
            Ok(Response::new(VerifyMarketingDomainResponse {
                receipt: Some(marketing_receipt(receipt)),
                correlation_id: correlation_id.to_string(),
            }))
        })
    }

    async fn bind_marketing_domain(
        &self,
        request: Request<BindMarketingDomainRequest>,
    ) -> Result<Response<BindMarketingDomainResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            let scope = marketing_scope(&context)?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let domain_id = parse_uuid(&input.domain_id, "domain_id")?;
            let template = ContentTemplateRef::new(
                parse_uuid(&input.template_id, "template_id")?,
                input.template_version,
                scope.clone(),
            )
            .map_err(|_| Status::invalid_argument("template reference is invalid"))?;
            let binding = CustomDomainBinding {
                domain_id,
                domain_scope: scope,
                template,
                bound_at: Utc::now(),
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing.write",
                )
                .await?;
            let correlation_id = context.correlation_id;
            let (write, read, port) =
                route_marketing_content_store(&self.state, tenant_id, holding_ref, correlation_id)
                    .await?;
            let result = async {
                let command = BindDomainCommand::new(binding, context.clone())
                    .map_err(|error| marketing_application_status(error, correlation_id))?;
                port.bind_domain(command)
                    .await
                    .map_err(|error| marketing_application_status(error, correlation_id))
            }
            .await;
            write.close().await;
            read.close().await;
            let receipt = result?;
            record_marketing_write_audit(
                &self.state,
                &claims,
                customer_id,
                "marketing.domain.bound",
                &context,
            )
            .await?;
            Ok(Response::new(BindMarketingDomainResponse {
                receipt: Some(marketing_receipt(receipt)),
                correlation_id: correlation_id.to_string(),
            }))
        })
    }

    async fn configure_marketing_provider_account(
        &self,
        request: Request<ConfigureMarketingProviderAccountRequest>,
    ) -> Result<Response<ConfigureMarketingProviderAccountResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            let holding_id = provider_account_scope(&context)?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let account_id = parse_uuid(&input.provider_account_id, "provider_account_id")?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing.provider.configure",
                )
                .await?;
            let channel_routes = input
                .channel_routes
                .iter()
                .map(|route| {
                    MarketingProviderChannelRoute::new(
                        route.channel.clone(),
                        route.endpoint.clone(),
                        (!route.reconciliation_endpoint.trim().is_empty())
                            .then(|| route.reconciliation_endpoint.clone()),
                    )
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| {
                    Status::invalid_argument("Marketing provider channel route is invalid")
                })?;
            let command = ConfigureMarketingProviderAccountCommand::new(
                account_id,
                validate_transport_reference(&input.provider_account_ref, "provider_account_ref")?,
                tenant_id,
                holding_ref,
                holding_id,
                validate_transport_reference(&input.provider_key, "provider_key")?,
                if input.provider_priority == 0 {
                    DEFAULT_MARKETING_PROVIDER_PRIORITY
                } else {
                    input.provider_priority
                },
                validate_marketing_secret_reference(&input.verifier_key_ref, "verifier_key_ref")?,
                validate_marketing_secret_reference(
                    &input.outbound_credential_ref,
                    "outbound_credential_ref",
                )?,
                input.channels.clone(),
                channel_routes,
                context.clone(),
            )
            .map_err(|_| {
                Status::invalid_argument("Marketing provider account configuration is invalid")
            })?;
            let receipt = SqlxMarketingProviderOnboardingStore::new(self.state.store.clone())
                .configure(command)
                .await
                .map_err(|error| {
                    marketing_provider_onboarding_status(error, context.correlation_id)
                })?;
            Ok(Response::new(ConfigureMarketingProviderAccountResponse {
                receipt: Some(marketing_provider_receipt(receipt)),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn acknowledge_marketing_provider_verification(
        &self,
        request: Request<AcknowledgeMarketingProviderVerificationRequest>,
    ) -> Result<Response<AcknowledgeMarketingProviderVerificationResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            provider_account_scope(&context)?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let account_id = parse_uuid(&input.provider_account_id, "provider_account_id")?;
            let verification_reference = validate_transport_reference(
                &input.verification_reference,
                "verification_reference",
            )?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing.provider.verify",
                )
                .await?;
            let command = AcknowledgeMarketingProviderVerificationCommand::new(
                account_id,
                tenant_id,
                holding_ref,
                verification_reference,
                input.activate,
                context.clone(),
            )
            .map_err(|_| Status::invalid_argument("Marketing provider verification is invalid"))?;
            let receipt = SqlxMarketingProviderOnboardingStore::new(self.state.store.clone())
                .acknowledge_verification(command)
                .await
                .map_err(|error| {
                    marketing_provider_onboarding_status(error, context.correlation_id)
                })?;
            Ok(Response::new(
                AcknowledgeMarketingProviderVerificationResponse {
                    receipt: Some(marketing_provider_receipt(receipt)),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn set_marketing_provider_account_state(
        &self,
        request: Request<SetMarketingProviderAccountStateRequest>,
    ) -> Result<Response<SetMarketingProviderAccountStateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(proto_context))?;
            provider_account_scope(&context)?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let account_id = parse_uuid(&input.provider_account_id, "provider_account_id")?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing.provider.configure",
                )
                .await?;
            let command = SetMarketingProviderAccountStateCommand::new(
                account_id,
                tenant_id,
                holding_ref,
                input.active,
                context.clone(),
            )
            .map_err(|_| Status::invalid_argument("Marketing provider account state is invalid"))?;
            let receipt = SqlxMarketingProviderOnboardingStore::new(self.state.store.clone())
                .set_state(command)
                .await
                .map_err(|error| {
                    marketing_provider_onboarding_status(error, context.correlation_id)
                })?;
            Ok(Response::new(SetMarketingProviderAccountStateResponse {
                receipt: Some(marketing_provider_receipt(receipt)),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }
}

#[tonic::async_trait]
impl FinanceService for FinanceRpcImpl {
    async fn list_posted_journals(
        &self,
        request: Request<ListFinanceJournalsRequest>,
    ) -> Result<Response<ListFinanceJournalsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Finance journal reads require an explicit holding, company, or branch scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "finance.read",
                )
                .await?;
            let input = request.get_ref();
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let page_size = input.page_size;
            if page_size == 0 || page_size > MAX_FINANCE_READ_JOURNAL_PAGE_SIZE {
                return Err(Status::invalid_argument(format!(
                    "page_size must be 1..={MAX_FINANCE_READ_JOURNAL_PAGE_SIZE}"
                )));
            }
            let before_journal_id = if input.before_journal_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.before_journal_id, "before_journal_id")?)
            };
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(
                    tenant_id,
                    holding_ref,
                    erpx_infrastructure::RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let read = match router
                .route(
                    tenant_id,
                    holding_ref,
                    erpx_infrastructure::RoutedDatabaseSide::Read,
                )
                .await
            {
                Ok(read) => read,
                Err(_) => {
                    write.close().await;
                    return Err(Status::unavailable("tenant read database is unavailable"));
                }
            };
            let result = async {
                let store = SqlxFinanceStore::from_pools(
                    write.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|_| Status::internal("Finance store configuration is invalid"))?;
                let (journals, has_more) = store
                    .read_journal_page(page_size, before_journal_id)
                    .await
                    .map_err(|_| Status::internal("Finance journal read failed"))?;
                let next_before_journal_id = if has_more {
                    journals
                        .last()
                        .map(|journal| journal.journal_id.to_string())
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let journals = journals
                    .into_iter()
                    .map(|journal| -> Result<FinanceJournal, Status> {
                        Ok(FinanceJournal {
                            id: journal.journal_id.to_string(),
                            company_id: journal.company_id.to_string(),
                            period_id: journal.period_id.to_string(),
                            source_version: journal.source_version,
                            posted_at: journal.posted_at.to_rfc3339(),
                            payload_json: serde_json::to_vec(&journal.payload)
                                .map_err(|_| Status::internal("Finance journal encoding failed"))?,
                        })
                    })
                    .collect::<Result<Vec<_>, Status>>()?;
                Ok::<ListFinanceJournalsResponse, Status>(ListFinanceJournalsResponse {
                    journals,
                    has_more,
                    next_before_journal_id,
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn get_finance_trial_balance(
        &self,
        request: Request<GetFinanceTrialBalanceRequest>,
    ) -> Result<Response<GetFinanceTrialBalanceResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            let read_scope =
                finance_chart_read_scope(&caller_scope, &input.holding_id, &input.company_id)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "finance.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let chart_id = parse_uuid(&input.chart_id, "chart_id")?;
            if input.chart_json.is_empty() || input.chart_json.len() > 2 * 1024 * 1024 {
                return Err(Status::invalid_argument("chart_json is out of bounds"));
            }
            let chart: ChartOfAccounts = serde_json::from_slice(&input.chart_json)
                .map_err(|_| Status::invalid_argument("chart_json is invalid"))?;
            chart
                .validate()
                .map_err(|_| Status::invalid_argument("chart_json is invalid"))?;
            if chart.id != chart_id
                || chart.holding_id != read_scope.holding_id
                || chart.company_id != read_scope.company_id
            {
                return Err(Status::permission_denied(
                    "Finance chart scope does not match the requested read scope",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &caller_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let correlation_id = context.correlation_id;
            let result = async {
                let chart_store =
                    SqlxFinanceChartReadStore::new(read.database_pool().clone(), holding_ref)
                        .map_err(|_| {
                            Status::internal("Finance chart read configuration is invalid")
                        })?;
                let mut cursor = None;
                let mut projected_chart = None;
                for _ in 0..100 {
                    let page = chart_store
                        .list_charts(read_scope, 100, cursor)
                        .await
                        .map_err(|_| Status::unavailable("Finance chart read failed"))?;
                    projected_chart = page.rows.into_iter().find(|row| row.chart_id == chart_id);
                    if projected_chart.is_some() || !page.has_more {
                        break;
                    }
                    cursor = page.next_cursor;
                }
                let projected_chart = projected_chart.ok_or_else(|| {
                    Status::failed_precondition(
                        "Finance chart projection is unavailable within read bounds",
                    )
                })?;
                if projected_chart.holding_id != chart.holding_id
                    || projected_chart.company_id != chart.company_id
                    || projected_chart.name != chart.name
                {
                    return Err(Status::failed_precondition(
                        "Finance chart snapshot does not match the read projection",
                    ));
                }
                if projected_chart.accounts.len() > 4096
                    || projected_chart.accounts.len() != chart.accounts.len()
                {
                    return Err(Status::failed_precondition(
                        "Finance chart projection account set is incomplete",
                    ));
                }
                for projected_account in projected_chart.accounts {
                    let account = chart.account(projected_account.account_id).ok_or_else(|| {
                        Status::failed_precondition(
                            "Finance chart projection account set is inconsistent",
                        )
                    })?;
                    let projected_kind =
                        finance_trial_balance_account_kind(&projected_account.category)?;
                    let expected_normal_balance = match account.kind {
                        AccountKind::Asset | AccountKind::Expense => "debit",
                        AccountKind::Liability | AccountKind::Equity | AccountKind::Revenue => {
                            "credit"
                        }
                    };
                    let expected_status = if account.active { "active" } else { "inactive" };
                    if account.code != projected_account.code
                        || account.name != projected_account.name
                        || account.kind != projected_kind
                        || expected_normal_balance != projected_account.normal_balance
                        || account.parent_id != projected_account.parent_account_id
                        || expected_status != projected_account.status
                    {
                        return Err(Status::failed_precondition(
                            "Finance chart snapshot does not match the read projection",
                        ));
                    }
                }

                let store = SqlxFinanceStore::from_pools(
                    read.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|_| Status::internal("Finance store configuration is invalid"))?;
                let trial_balance = store
                    .trial_balance(&chart)
                    .await
                    .map_err(|error| finance_persistence_status(error, correlation_id))?;
                let source_version = projected_chart.source_version;
                let projection_updated_at = projected_chart.projected_at;
                let now = Utc::now();
                let lines = trial_balance
                    .lines
                    .into_iter()
                    .map(|line| FinanceTrialBalanceLine {
                        account_id: line.account_id.to_string(),
                        account_code: line.account_code,
                        account_name: line.account_name,
                        account_kind: finance_trial_balance_account_kind_text(line.account_kind)
                            .into(),
                        debit_minor: line.debit_minor,
                        credit_minor: line.credit_minor,
                        net_minor: line.net_minor,
                    })
                    .collect();
                Ok::<GetFinanceTrialBalanceResponse, Status>(GetFinanceTrialBalanceResponse {
                    holding_id: trial_balance.holding_id.to_string(),
                    company_id: trial_balance.company_id.to_string(),
                    chart_id: chart.id.to_string(),
                    currency: trial_balance.currency,
                    lines,
                    total_debit_minor: trial_balance.total_debit_minor,
                    total_credit_minor: trial_balance.total_credit_minor,
                    projection_updated_at: projection_updated_at.to_rfc3339(),
                    freshness: finance_trial_balance_freshness(Some(projection_updated_at), now)
                        .into(),
                    read_only: true,
                    source_version,
                    has_source_version: source_version > 0,
                    correlation_id: correlation_id.to_string(),
                })
            }
            .await;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "finance.trial_balance.read".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "Finance trial-balance read".into()),
                    correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn get_finance_consolidated_trial_balance(
        &self,
        request: Request<GetFinanceConsolidatedTrialBalanceRequest>,
    ) -> Result<Response<GetFinanceConsolidatedTrialBalanceResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let requested_scope = Scope::Holding { holding_id };
            if !caller_scope.contains(&requested_scope) {
                return Err(Status::permission_denied(
                    "Finance consolidation scope is not contained",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "finance.read",
                )
                .await?;
            if input.chart_json.is_empty()
                || input.chart_json.len() > erpx_infrastructure::MAX_FINANCE_CONSOLIDATED_CHARTS
            {
                return Err(Status::invalid_argument(
                    "at least one and at most 1000 company charts are required",
                ));
            }
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &caller_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let charts = input
                .chart_json
                .iter()
                .map(|chart_json| {
                    if chart_json.is_empty() || chart_json.len() > 2 * 1024 * 1024 {
                        return Err(Status::invalid_argument("chart_json is out of bounds"));
                    }
                    let chart: ChartOfAccounts = serde_json::from_slice(chart_json)
                        .map_err(|_| Status::invalid_argument("chart_json is invalid"))?;
                    chart
                        .validate()
                        .map_err(|_| Status::invalid_argument("chart_json is invalid"))?;
                    if chart.holding_id != holding_id {
                        return Err(Status::permission_denied(
                            "Finance chart holding does not match consolidation scope",
                        ));
                    }
                    Ok(chart)
                })
                .collect::<Result<Vec<_>, Status>>()?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let correlation_id = context.correlation_id;
            let result = async {
                let chart_store = SqlxFinanceChartReadStore::new(
                    read.database_pool().clone(),
                    holding_ref.to_owned(),
                )
                .map_err(|_| Status::internal("Finance chart read configuration is invalid"))?;
                for chart in &charts {
                    let scope = erpx_infrastructure::FinanceChartReadScope {
                        holding_id,
                        company_id: chart.company_id,
                    };
                    let mut cursor = None;
                    let projected = loop {
                        let page = chart_store
                            .list_charts(scope, 100, cursor)
                            .await
                            .map_err(|_| Status::unavailable("Finance chart read failed"))?;
                        if let Some(row) =
                            page.rows.into_iter().find(|row| row.chart_id == chart.id)
                        {
                            break Some(row);
                        }
                        if !page.has_more {
                            break None;
                        }
                        cursor = page.next_cursor;
                    }
                    .ok_or_else(|| {
                        Status::failed_precondition(
                            "Finance chart projection is unavailable within read bounds",
                        )
                    })?;
                    if projected.holding_id != chart.holding_id
                        || projected.company_id != chart.company_id
                        || projected.name != chart.name
                        || projected.accounts.len() != chart.accounts.len()
                    {
                        return Err(Status::failed_precondition(
                            "Finance chart projection does not match the supplied chart",
                        ));
                    }
                    for account in &chart.accounts {
                        let projected_account = projected
                            .accounts
                            .iter()
                            .find(|candidate| candidate.account_id == account.id)
                            .ok_or_else(|| {
                                Status::failed_precondition(
                                    "Finance chart projection account set is incomplete",
                                )
                            })?;
                        let kind = finance_trial_balance_account_kind(&projected_account.category)?;
                        let expected_normal_balance = match account.kind {
                            AccountKind::Asset | AccountKind::Expense => "debit",
                            AccountKind::Liability | AccountKind::Equity | AccountKind::Revenue => {
                                "credit"
                            }
                        };
                        let expected_status = if account.active { "active" } else { "inactive" };
                        if projected_account.code != account.code
                            || projected_account.name != account.name
                            || kind != account.kind
                            || projected_account.normal_balance != expected_normal_balance
                            || projected_account.parent_account_id != account.parent_id
                            || projected_account.status != expected_status
                        {
                            return Err(Status::failed_precondition(
                                "Finance chart projection account is inconsistent",
                            ));
                        }
                    }
                }
                let store = SqlxFinanceStore::from_pools(
                    read.database_pool(),
                    read.database_pool(),
                    holding_ref.to_owned(),
                )
                .map_err(|_| Status::internal("Finance store configuration is invalid"))?;
                let (report, source_version) = store
                    .consolidated_trial_balance(&charts)
                    .await
                    .map_err(|error| finance_persistence_status(error, correlation_id))?;
                let lines = report
                    .lines
                    .into_iter()
                    .map(|line| FinanceConsolidatedTrialBalanceLine {
                        account_code: line.account_code,
                        account_name: line.account_name,
                        account_kind: finance_trial_balance_account_kind_text(line.account_kind)
                            .into(),
                        debit_minor: line.debit_minor,
                        credit_minor: line.credit_minor,
                        net_minor: line.net_minor,
                    })
                    .collect();
                Ok::<GetFinanceConsolidatedTrialBalanceResponse, Status>(
                    GetFinanceConsolidatedTrialBalanceResponse {
                        holding_id: report.holding_id.to_string(),
                        currency: report.currency,
                        company_ids: report
                            .company_ids
                            .into_iter()
                            .map(|company_id| company_id.to_string())
                            .collect(),
                        lines,
                        total_debit_minor: report.total_debit_minor,
                        total_credit_minor: report.total_credit_minor,
                        source_version,
                        has_source_version: source_version > 0,
                        read_only: true,
                        correlation_id: correlation_id.to_string(),
                    },
                )
            }
            .await;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "finance.consolidated_trial_balance.read".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "Finance consolidated trial-balance read".into()),
                    correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn save_finance_period(
        &self,
        request: Request<SaveFinancePeriodRequest>,
    ) -> Result<Response<SaveFinancePeriodResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Finance period mutations require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let period_json = &request.get_ref().period_json;
            if period_json.is_empty() || period_json.len() > 256 * 1024 {
                return Err(Status::invalid_argument("period_json is out of bounds"));
            }
            let period: AccountingPeriod = serde_json::from_slice(period_json)
                .map_err(|_| Status::invalid_argument("period_json is invalid"))?;
            period.validate().map_err(|error| {
                finance_persistence_status(
                    PersistenceError::InvalidData(error.to_string()),
                    context.correlation_id,
                )
            })?;
            if !finance_company_scope_contains(&scope, period.holding_id, period.company_id) {
                return Err(Status::permission_denied(
                    "Finance period company is outside the command scope",
                ));
            }
            let expected_version = if request.get_ref().has_expected_version {
                if request.get_ref().expected_version == 0 {
                    return Err(Status::invalid_argument(
                        "expected_version must be positive",
                    ));
                }
                Some(request.get_ref().expected_version)
            } else {
                None
            };
            if expected_version != context.expected_aggregate_version {
                return Err(Status::invalid_argument(
                    "expected_version does not match command context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "finance",
                    "finance.write",
                )
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(
                    tenant_id,
                    holding_ref,
                    erpx_infrastructure::RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let result = async {
                let store = SqlxFinanceStore::from_pools(
                    write.database_pool(),
                    write.database_pool(),
                    holding_ref,
                )
                .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                let version = store
                    .save_period(&period, expected_version)
                    .await
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "finance.period.saved".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
                Ok::<SaveFinancePeriodResponse, Status>(SaveFinancePeriodResponse {
                    receipt: Some(erpx_proto::platform::FinancePeriodWriteReceipt {
                        aggregate_type: "finance_period".into(),
                        period_id: period.id.to_string(),
                        version,
                        replayed: false,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn post_finance_journal(
        &self,
        request: Request<PostFinanceJournalRequest>,
    ) -> Result<Response<PostFinanceJournalResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Finance journal mutations require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let input = request.get_ref();
            if input.chart_json.is_empty()
                || input.chart_json.len() > 2 * 1024 * 1024
                || input.period_json.is_empty()
                || input.period_json.len() > 256 * 1024
                || input.journal_json.is_empty()
                || input.journal_json.len() > 2 * 1024 * 1024
            {
                return Err(Status::invalid_argument(
                    "Finance journal payload is out of bounds",
                ));
            }
            let chart: ChartOfAccounts = serde_json::from_slice(&input.chart_json)
                .map_err(|_| Status::invalid_argument("chart_json is invalid"))?;
            let period: AccountingPeriod = serde_json::from_slice(&input.period_json)
                .map_err(|_| Status::invalid_argument("period_json is invalid"))?;
            let journal: JournalEntry = serde_json::from_slice(&input.journal_json)
                .map_err(|_| Status::invalid_argument("journal_json is invalid"))?;
            chart.validate().map_err(|error| {
                finance_persistence_status(
                    PersistenceError::InvalidData(error.to_string()),
                    context.correlation_id,
                )
            })?;
            period.validate().map_err(|error| {
                finance_persistence_status(
                    PersistenceError::InvalidData(error.to_string()),
                    context.correlation_id,
                )
            })?;
            journal.validate().map_err(|error| {
                finance_persistence_status(
                    PersistenceError::InvalidData(error.to_string()),
                    context.correlation_id,
                )
            })?;
            if !finance_company_scope_contains(&scope, journal.holding_id(), journal.company_id())
                || chart.holding_id != journal.holding_id()
                || chart.company_id != journal.company_id()
                || period.holding_id != journal.holding_id()
                || period.company_id != journal.company_id()
                || period.id != journal.period_id()
            {
                return Err(Status::permission_denied(
                    "Finance journal scope or period binding is invalid",
                ));
            }
            let journal_context = journal.context();
            if journal_context.command_id != context.command_id
                || journal_context.idempotency_key != context.idempotency_key
                || journal_context.actor.principal_ref != context.actor.principal_ref
                || access_scope(&journal_context.tenant_scope) != scope
            {
                return Err(Status::permission_denied(
                    "Finance journal command context does not match transport context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "finance",
                    "finance.write",
                )
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(
                    tenant_id,
                    holding_ref,
                    erpx_infrastructure::RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let result = async {
                let store = SqlxFinanceStore::from_pools(
                    write.database_pool(),
                    write.database_pool(),
                    holding_ref,
                )
                .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                let receipt = store
                    .post_journal(journal, &chart, &period, Utc::now())
                    .await
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "finance.journal.posted".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
                Ok::<PostFinanceJournalResponse, Status>(PostFinanceJournalResponse {
                    receipt: Some(erpx_proto::platform::FinanceJournalPostReceipt {
                        journal_id: receipt.journal.id().to_string(),
                        company_id: receipt.journal.company_id().to_string(),
                        period_id: receipt.journal.period_id().to_string(),
                        posted_at: receipt.journal.posted_at().to_rfc3339(),
                        replayed: receipt.replayed,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn save_subledger(
        &self,
        request: Request<SaveFinanceSubledgerRequest>,
    ) -> Result<Response<SaveFinanceSubledgerResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Finance mutations require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "finance.write",
                )
                .await?;
            if request.get_ref().aggregate_json.len() > 1_048_576 {
                return Err(Status::invalid_argument(
                    "aggregate_json exceeds its size bound",
                ));
            }
            let payload =
                serde_json::from_slice::<serde_json::Value>(&request.get_ref().aggregate_json)
                    .map_err(|_| Status::invalid_argument("aggregate_json is invalid"))?;
            let requested_type = request.get_ref().aggregate_type.trim();
            if requested_type.is_empty() || requested_type.len() > 64 {
                return Err(Status::invalid_argument("aggregate_type is invalid"));
            }
            let aggregate: FinanceSubledgerAggregate = serde_json::from_value(serde_json::json!({
                "aggregate_type": requested_type,
                "aggregate": payload,
            }))
            .map_err(|_| Status::invalid_argument("Finance aggregate payload is invalid"))?;
            aggregate
                .validate()
                .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
            if aggregate.aggregate_type() != requested_type
                || access_scope(aggregate.scope()) != scope
            {
                return Err(Status::permission_denied(
                    "Finance aggregate type or scope does not match the command",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "finance",
                    "finance.write",
                )
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(
                    tenant_id,
                    holding_ref,
                    erpx_infrastructure::RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let result = async {
                let store = SqlxFinanceSubledgerStore::from_pools(
                    write.database_pool(),
                    write.database_pool(),
                    holding_ref,
                )
                .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                let receipt = store
                    .save_with_request_key(
                        &aggregate,
                        context.expected_aggregate_version,
                        &context.idempotency_key,
                    )
                    .await
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                self.state
                    .store
                    .record(erpx_application::AuditEvent {
                        actor_ref: context.actor.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "finance.subledger.saved".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
                Ok::<SaveFinanceSubledgerResponse, Status>(SaveFinanceSubledgerResponse {
                    receipt: Some(erpx_proto::platform::FinanceSubledgerReceipt {
                        aggregate_type: receipt.aggregate_type,
                        aggregate_id: receipt.aggregate_id.to_string(),
                        version: receipt.version,
                        replayed: receipt.replayed,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn save_allocation_batch(
        &self,
        request: Request<SaveFinanceAllocationBatchRequest>,
    ) -> Result<Response<SaveFinanceAllocationBatchResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Finance allocation commands require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            if request.get_ref().batch_json.len() > 2_097_152 {
                return Err(Status::invalid_argument(
                    "batch_json exceeds its size bound",
                ));
            }
            let input = serde_json::from_slice::<FinanceAllocationBatchInput>(
                &request.get_ref().batch_json,
            )
            .map_err(|_| Status::invalid_argument("batch_json is invalid"))?;
            let batch = AllocationBatch::new(input.payment.metadata.id, input.allocations)
                .map_err(|_| Status::failed_precondition("Finance allocation batch is invalid"))?;
            if access_scope(&input.payment.metadata.scope) != scope
                || input
                    .invoices
                    .iter()
                    .any(|invoice| access_scope(&invoice.metadata.scope) != scope)
                || batch
                    .allocations
                    .iter()
                    .any(|allocation| access_scope(&allocation.metadata.scope) != scope)
            {
                return Err(Status::permission_denied(
                    "Finance allocation records do not match the command scope",
                ));
            }
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "finance.write",
                )
                .await?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "finance",
                    "finance.write",
                )
                .await?;
            let expected_payment_version = context.expected_aggregate_version.ok_or_else(|| {
                Status::failed_precondition(
                    "Finance allocation commands require expected_aggregate_version",
                )
            })?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(
                    tenant_id,
                    holding_ref,
                    erpx_infrastructure::RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let result = async {
                let store = SqlxFinanceSubledgerStore::from_pools(
                    write.database_pool(),
                    write.database_pool(),
                    holding_ref,
                )
                .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                let receipt = store
                    .save_allocation_batch(
                        &batch,
                        &input.payment,
                        &input.invoices,
                        expected_payment_version,
                    )
                    .await
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                self.state
                    .store
                    .record(erpx_application::AuditEvent {
                        actor_ref: context.actor.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "finance.allocation_batch.saved".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
                Ok::<SaveFinanceAllocationBatchResponse, Status>(finance_allocation_response(
                    receipt,
                    context.correlation_id,
                ))
            }
            .await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn save_finance_chart(
        &self,
        request: Request<SaveFinanceChartRequest>,
    ) -> Result<Response<SaveFinanceChartResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Finance chart mutations require an explicit holding, company, or branch scope",
                ));
            }
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let chart_json = &request.get_ref().chart_json;
            if chart_json.is_empty() || chart_json.len() > 2 * 1024 * 1024 {
                return Err(Status::invalid_argument("chart_json is out of bounds"));
            }
            let chart: FinanceChart = serde_json::from_slice(chart_json)
                .map_err(|_| Status::invalid_argument("chart_json is invalid"))?;
            chart.validate().map_err(|error| {
                finance_persistence_status(
                    PersistenceError::InvalidData(error.to_string()),
                    context.correlation_id,
                )
            })?;
            if chart.last_command.command_id != context.command_id
                || chart.last_command.idempotency_key != context.idempotency_key
                || access_scope(&chart.tenant_scope()) != scope
            {
                return Err(Status::permission_denied(
                    "Finance chart identity or command scope does not match context",
                ));
            }
            let expected_version = if request.get_ref().has_expected_version {
                if request.get_ref().expected_version == 0 {
                    return Err(Status::invalid_argument(
                        "expected_version must be positive",
                    ));
                }
                Some(request.get_ref().expected_version)
            } else {
                None
            };
            if expected_version != context.expected_aggregate_version {
                return Err(Status::invalid_argument(
                    "expected_version does not match command context",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "finance.write",
                )
                .await?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "finance",
                    "finance.write",
                )
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(
                    tenant_id,
                    holding_ref,
                    erpx_infrastructure::RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let result = async {
                let store = SqlxFinanceChartStore::new(write.database_pool().clone(), holding_ref)
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                let receipt = store
                    .save_chart(&chart, &context, expected_version)
                    .await
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "finance.chart.saved".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
                Ok::<SaveFinanceChartResponse, Status>(SaveFinanceChartResponse {
                    receipt: Some(erpx_proto::platform::FinanceChartWriteReceipt {
                        aggregate_type: erpx_infrastructure::FINANCE_CHART_AGGREGATE.into(),
                        chart_id: receipt.chart_id.to_string(),
                        version: receipt.version,
                        idempotency_key: receipt.idempotency_key,
                        event_id: receipt.event_id.to_string(),
                        replayed: receipt.replayed,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn list_finance_charts(
        &self,
        request: Request<ListFinanceChartsRequest>,
    ) -> Result<Response<ListFinanceChartsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            let read_scope =
                finance_chart_read_scope(&caller_scope, &input.holding_id, &input.company_id)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "finance.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let page_size = if input.page_size == 0 {
                25
            } else if input.page_size <= MAX_FINANCE_CHART_READ_PAGE_SIZE {
                input.page_size
            } else {
                return Err(Status::invalid_argument(format!(
                    "page_size must be 1..={MAX_FINANCE_CHART_READ_PAGE_SIZE}"
                )));
            };
            let cursor = if input.after_chart_id.trim().is_empty() {
                None
            } else {
                Some(FinanceChartReadCursor {
                    chart_id: parse_uuid(&input.after_chart_id, "after_chart_id")?,
                })
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &caller_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let correlation_id = context.correlation_id;
            let result = async {
                let store =
                    SqlxFinanceChartReadStore::new(read.database_pool().clone(), holding_ref)
                        .map_err(|error| finance_persistence_status(error, correlation_id))?;
                let page = store
                    .list_charts(read_scope, page_size, cursor)
                    .await
                    .map_err(|error| finance_persistence_status(error, correlation_id))?;
                let charts = page
                    .rows
                    .into_iter()
                    .map(finance_chart_read_row_message)
                    .collect::<Result<Vec<_>, Status>>()?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "finance.chart.read.list".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "Finance chart read".into()),
                        correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
                Ok::<ListFinanceChartsResponse, Status>(ListFinanceChartsResponse {
                    charts,
                    has_more: page.has_more,
                    next_after_chart_id: page
                        .next_cursor
                        .map(|cursor| cursor.chart_id.to_string())
                        .unwrap_or_default(),
                    correlation_id: correlation_id.to_string(),
                })
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn list_finance_subledger_read_rows(
        &self,
        request: Request<ListFinanceSubledgerReadRowsRequest>,
    ) -> Result<Response<ListFinanceSubledgerReadRowsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            let read_scope = finance_subledger_read_tenant_scope(&caller_scope)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "finance.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let page_size = finance_subledger_read_page_size(input.page_size)?;
            let aggregate_type = if input.aggregate_type.trim().is_empty() {
                None
            } else {
                Some(
                    validate_transport_reference(&input.aggregate_type, "aggregate_type")?
                        .to_owned(),
                )
            };
            let company_id = if input.company_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.company_id, "company_id")?)
            };
            let after = if input.after_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_finance_subledger_read_cursor(&input.after_cursor)?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &caller_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let read = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
                .map_err(|_| Status::unavailable("tenant read database is unavailable"))?;
            let correlation_id = context.correlation_id;
            let result = async {
                let store = SqlxFinanceSubledgerStore::from_pools(
                    read.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| finance_persistence_status(error, correlation_id))?;
                let page = store
                    .list_read_rows(
                        &read_scope,
                        aggregate_type.as_deref(),
                        company_id,
                        after.as_ref(),
                        page_size,
                    )
                    .await
                    .map_err(|error| finance_persistence_status(error, correlation_id))?;
                let mut rows = Vec::with_capacity(page.rows.len());
                for row in page.rows {
                    if !caller_scope.contains(&finance_subledger_row_scope(&row)) {
                        return Err(Status::internal("Finance subledger read scope mismatch"));
                    }
                    rows.push(finance_subledger_read_row_message(row)?);
                }
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "finance.subledger.read.list".into(),
                        purpose: proto_context
                            .audit
                            .as_ref()
                            .map(|audit| audit.purpose.clone())
                            .unwrap_or_else(|| "Finance subledger read list".into()),
                        correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
                Ok::<ListFinanceSubledgerReadRowsResponse, Status>(
                    ListFinanceSubledgerReadRowsResponse {
                        rows,
                        has_more: page.has_more,
                        next_cursor: page
                            .next_cursor
                            .as_ref()
                            .map(encode_finance_subledger_read_cursor)
                            .unwrap_or_default(),
                        correlation_id: correlation_id.to_string(),
                    },
                )
            }
            .await;
            read.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn save_finance_close_review(
        &self,
        request: Request<SaveFinanceCloseReviewRequest>,
    ) -> Result<Response<SaveFinanceCloseReviewResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Finance close mutations require an explicit scope",
                ));
            }
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            let period: AccountingPeriod =
                decode_finance_banking_json(&request.get_ref().period_json, "period_json")?;
            let review: FinanceCloseReview =
                decode_finance_banking_json(&request.get_ref().review_json, "review_json")?;
            period
                .validate()
                .map_err(|_| Status::failed_precondition("Finance accounting period is invalid"))?;
            if review.holding_id() != period.holding_id
                || review.company_id() != period.company_id
                || review.period_id() != period.id
                || !context.tenant_scope.contains(&TenantScope::Company {
                    holding_id: review.holding_id(),
                    company_id: review.company_id(),
                })
            {
                return Err(Status::permission_denied(
                    "Finance close scope does not match the accounting period",
                ));
            }
            let embedded_context = match review.status() {
                erpx_domain::finance_close::CloseStatus::Open => review.opened_context(),
                erpx_domain::finance_close::CloseStatus::Review => {
                    review.reviewed_context().ok_or_else(|| {
                        Status::failed_precondition("Finance close reviewer is missing")
                    })?
                }
                erpx_domain::finance_close::CloseStatus::Closed => {
                    review.closed_context().ok_or_else(|| {
                        Status::failed_precondition("Finance close closer is missing")
                    })?
                }
            };
            if embedded_context.command_id != context.command_id
                || embedded_context.idempotency_key != context.idempotency_key
                || embedded_context.actor != context.actor
                || embedded_context.correlation_id != context.correlation_id
            {
                return Err(Status::failed_precondition(
                    "Finance close command context is not bound to the review",
                ));
            }
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "finance",
                    "finance.write",
                )
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let expected_version = request
                .get_ref()
                .has_expected_version
                .then_some(request.get_ref().expected_version);
            let receipt = with_close_store(
                router,
                tenant_id,
                holding_ref,
                context.correlation_id,
                |store| async move {
                    store
                        .save_review(&review, &period, expected_version)
                        .await
                        .map_err(|error| finance_persistence_status(error, context.correlation_id))
                },
            )
            .await?;
            self.state
                .store
                .record(erpx_application::AuditEvent {
                    actor_ref: context.actor.principal_ref.clone(),
                    customer_account_id: Some(customer_id),
                    action: "finance.close_review.saved".into(),
                    purpose: context.audit.purpose.clone(),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
            Ok(Response::new(SaveFinanceCloseReviewResponse {
                receipt: Some(finance_close_receipt(receipt)),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn register_bank_account(
        &self,
        request: Request<RegisterBankAccountRequest>,
    ) -> Result<Response<RegisterBankAccountResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Finance banking mutations require an explicit scope",
                ));
            }
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            let account: BankAccount =
                decode_finance_banking_json(&request.get_ref().account_json, "account_json")?;
            account
                .validate_for_command(&context)
                .map_err(|_| Status::failed_precondition("Finance bank account is invalid"))?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "finance",
                    "finance.write",
                )
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let receipt = with_banking_store(
                router,
                tenant_id,
                holding_ref,
                context.correlation_id,
                |store| async move {
                    store
                        .register_account(&account, &context.idempotency_key)
                        .await
                        .map_err(|error| finance_persistence_status(error, context.correlation_id))
                },
            )
            .await?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: context.actor.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "finance.bank_account.registered".into(),
                    purpose: context.audit.purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
            Ok(Response::new(RegisterBankAccountResponse {
                receipt: Some(erpx_proto::platform::BankAccountReceipt {
                    account_id: receipt.account_id.to_string(),
                    version: receipt.version,
                    replayed: receipt.replayed,
                }),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn import_bank_statement(
        &self,
        request: Request<ImportBankStatementRequest>,
    ) -> Result<Response<ImportBankStatementResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Finance banking mutations require an explicit scope",
                ));
            }
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            let account: BankAccount =
                decode_finance_banking_json(&request.get_ref().account_json, "account_json")?;
            let import: BankStatementImport =
                decode_finance_banking_json(&request.get_ref().import_json, "import_json")?;
            account
                .validate_for_command(&context)
                .map_err(|_| Status::failed_precondition("Finance bank account is invalid"))?;
            import
                .validate_for_command(&context)
                .map_err(|_| Status::failed_precondition("Finance bank statement is invalid"))?;
            if import.bank_account_id() != account.id() {
                return Err(Status::failed_precondition(
                    "Finance bank statement binding is invalid",
                ));
            }
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            require_actor_match(
                &claims.principal_ref,
                &import.context().actor.principal_ref,
                "bank statement actor must match the authenticated principal",
            )?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "finance",
                    "finance.write",
                )
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let receipt = with_banking_store(
                router,
                tenant_id,
                holding_ref,
                context.correlation_id,
                |store| async move {
                    store
                        .import_statement(&account, &import)
                        .await
                        .map_err(|error| finance_persistence_status(error, context.correlation_id))
                },
            )
            .await?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: context.actor.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "finance.bank_statement.imported".into(),
                    purpose: context.audit.purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
            Ok(Response::new(ImportBankStatementResponse {
                receipt: Some(erpx_proto::platform::BankImportReceipt {
                    import_id: receipt.import_id.to_string(),
                    transaction_count: receipt.transaction_count as u64,
                    replayed: receipt.replayed,
                }),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn enqueue_finance_bank_provider_import(
        &self,
        request: Request<EnqueueFinanceBankProviderImportRequest>,
    ) -> Result<Response<EnqueueFinanceBankProviderImportResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let context = command_domain(&request, input.context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Finance bank-provider imports require an explicit scope",
                ));
            }
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let provider_key = validate_transport_reference(&input.provider_key, "provider_key")?;
            let provider_account_ref =
                validate_transport_reference(&input.provider_account_ref, "provider_account_ref")?;
            let credential_key_ref =
                validate_transport_reference(&input.credential_key_ref, "credential_key_ref")?;
            let account: BankAccount =
                decode_finance_banking_json(&input.account_json, "account_json")?;
            account
                .validate_for_command(&context)
                .map_err(|_| Status::failed_precondition("Finance bank account is invalid"))?;
            if !account.is_active() {
                return Err(Status::failed_precondition(
                    "Finance bank account is inactive",
                ));
            }
            let expected_provider_prefix = format!("{provider_key}:");
            let local_provider_account_ref = provider_account_ref
                .strip_prefix(&expected_provider_prefix)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    Status::invalid_argument(
                        "provider_account_ref must be provider_key:opaque_account_reference",
                    )
                })?;
            let provider_account =
                BankProviderAccountRef::try_new(provider_key, local_provider_account_ref).map_err(
                    |_| Status::invalid_argument("provider account reference is invalid"),
                )?;
            if provider_account.storage_reference() != provider_account_ref {
                return Err(Status::invalid_argument(
                    "provider account reference is not canonical",
                ));
            }
            let credential_key = BankProviderCredentialKey::try_new(credential_key_ref)
                .map_err(|_| Status::invalid_argument("credential key reference is invalid"))?;
            let binding = BankProviderAccountBinding::try_new(
                account.id(),
                provider_account,
                account.scope().clone(),
                account.holding_id(),
                account.company_id(),
                account.currency(),
                account.display_name(),
            )
            .map_err(|_| Status::failed_precondition("bank provider account binding is invalid"))?;
            let starts_on = NaiveDate::parse_from_str(&input.starts_on, "%Y-%m-%d")
                .map_err(|_| Status::invalid_argument("starts_on must be YYYY-MM-DD"))?;
            let ends_on = NaiveDate::parse_from_str(&input.ends_on, "%Y-%m-%d")
                .map_err(|_| Status::invalid_argument("ends_on must be YYYY-MM-DD"))?;
            if starts_on > ends_on {
                return Err(Status::invalid_argument("statement date window is invalid"));
            }
            let page_size = if input.page_size == 0 {
                100
            } else {
                input.page_size
            };
            if page_size > 500 {
                return Err(Status::invalid_argument(
                    "page_size exceeds the provider bound",
                ));
            }
            let import_id = parse_uuid(&input.import_id, "import_id")?;
            let max_attempts = if input.max_attempts == 0 {
                3
            } else {
                input.max_attempts
            };
            if !(1..=8).contains(&max_attempts) {
                return Err(Status::invalid_argument(
                    "max_attempts must be between 1 and 8",
                ));
            }

            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "finance",
                    "finance.write",
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(&claims, &input.mfa_code)
                .await?;
            let decision = FinanceAuthorizationRequest {
                context: context.clone(),
                operation: FinanceAuthorizationOperation::ImportBankStatement,
                mfa_verified: true,
                approver_principal_ref: None,
                access_path: FinanceAccessPath::Normal,
            }
            .evaluate()
            .map_err(|_| {
                Status::permission_denied("Finance bank import authorization was denied")
            })?;
            let authorization =
                FinanceBankProviderImportAuthorization::from_decision(&decision, &context)
                    .map_err(|_| {
                        Status::permission_denied("Finance bank import authorization was denied")
                    })?;

            let route = self
                .state
                .store
                .resolve_finance_bank_provider_account(
                    provider_account_ref,
                    provider_key,
                    tenant_id,
                    holding_ref,
                    credential_key.as_str(),
                )
                .await
                .map_err(|_| Status::unavailable("Finance bank-provider route is unavailable"))?
                .ok_or_else(|| {
                    Status::failed_precondition("Finance bank-provider route is not active")
                })?;
            if route.holding_id != account.holding_id()
                || route.provider_account_ref != provider_account_ref
                || route.provider_key != provider_key
                || route.tenant_id != tenant_id
                || route.holding_ref != holding_ref
                || route.credential_key_ref != credential_key.as_str()
            {
                return Err(Status::failed_precondition(
                    "Finance bank-provider route does not match the account",
                ));
            }
            let import_request = FinanceBankProviderImportRequest {
                provider_account_ref: provider_account_ref.to_owned(),
                provider_key: provider_key.to_owned(),
                tenant_id: tenant_id.to_owned(),
                holding_ref: holding_ref.to_owned(),
                credential_key,
                account: binding,
                starts_on,
                ends_on,
                page_size,
                import_id,
                context: context.clone(),
            };
            let source = SqlxFinanceBankProviderImportJobSource::new(&self.state.store);
            let job_id = source
                .enqueue(
                    &import_request,
                    &authorization,
                    route.version,
                    Utc::now(),
                    u16::try_from(max_attempts).map_err(|_| {
                        Status::invalid_argument("max_attempts is outside the allowed bound")
                    })?,
                )
                .await
                .map_err(|_| {
                    Status::unavailable("Finance bank-provider import queue is unavailable")
                })?;
            Ok(Response::new(EnqueueFinanceBankProviderImportResponse {
                job_id: job_id.to_string(),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn get_finance_bank_provider_import_job_status(
        &self,
        request: Request<GetFinanceBankProviderImportJobStatusRequest>,
    ) -> Result<Response<GetFinanceBankProviderImportJobStatusResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Finance bank import status requires an explicit scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "finance.read",
                )
                .await?;
            let input = request.get_ref();
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let job_id = parse_uuid(&input.job_id, "job_id")?;
            let source = SqlxFinanceBankProviderImportJobSource::new(&self.state.store);
            let status = source
                .status(job_id)
                .await
                .map_err(|_| Status::unavailable("Finance bank import status unavailable"))?
                .ok_or_else(|| Status::not_found("Finance bank import job was not found"))?;
            if status.tenant_id != tenant_id || status.holding_ref != holding_ref {
                return Err(Status::not_found("Finance bank import job was not found"));
            }
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "finance.bank_provider_import.status_read".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "finance bank-provider import status read".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
            Ok(Response::new(
                GetFinanceBankProviderImportJobStatusResponse {
                    status: Some(finance_bank_provider_import_status_message(status)?),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn save_bank_reconciliation(
        &self,
        request: Request<SaveBankReconciliationRequest>,
    ) -> Result<Response<SaveBankReconciliationResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Finance banking mutations require an explicit scope",
                ));
            }
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            let account: BankAccount =
                decode_finance_banking_json(&request.get_ref().account_json, "account_json")?;
            let import: BankStatementImport =
                decode_finance_banking_json(&request.get_ref().import_json, "import_json")?;
            let reconciliation: BankReconciliation = decode_finance_banking_json(
                &request.get_ref().reconciliation_json,
                "reconciliation_json",
            )?;
            account
                .validate_for_command(&context)
                .map_err(|_| Status::failed_precondition("Finance bank account is invalid"))?;
            import
                .validate_for_command(&context)
                .map_err(|_| Status::failed_precondition("Finance bank statement is invalid"))?;
            reconciliation
                .validate()
                .map_err(|_| Status::failed_precondition("Finance reconciliation is invalid"))?;
            if !context.tenant_scope.contains(reconciliation.scope())
                || reconciliation.bank_account_id() != account.id()
                || reconciliation.statement_import_id() != import.id()
            {
                return Err(Status::permission_denied(
                    "Finance reconciliation binding is invalid",
                ));
            }
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            require_actor_match(
                &claims.principal_ref,
                &reconciliation.context().actor.principal_ref,
                "reconciliation actor must match the authenticated principal",
            )?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "finance",
                    "finance.write",
                )
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let expected = request
                .get_ref()
                .has_expected_version
                .then_some(request.get_ref().expected_version);
            let receipt = with_banking_store(
                router,
                tenant_id,
                holding_ref,
                context.correlation_id,
                |store| async move {
                    store
                        .save_reconciliation(&account, &import, &reconciliation, expected)
                        .await
                        .map_err(|error| finance_persistence_status(error, context.correlation_id))
                },
            )
            .await?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: context.actor.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "finance.bank_reconciliation.saved".into(),
                    purpose: context.audit.purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
            Ok(Response::new(SaveBankReconciliationResponse {
                receipt: Some(erpx_proto::platform::BankReconciliationReceipt {
                    reconciliation_id: receipt.reconciliation_id.to_string(),
                    version: receipt.version,
                    replayed: receipt.replayed,
                }),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn save_bank_match_proposal(
        &self,
        request: Request<SaveBankMatchProposalRequest>,
    ) -> Result<Response<SaveBankMatchProposalResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Finance bank matching requires an explicit scope",
                ));
            }
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            let proposal: BankMatchProposal =
                decode_finance_banking_json(&request.get_ref().proposal_json, "proposal_json")?;
            proposal.validate().map_err(|_| {
                Status::failed_precondition("Finance bank match proposal is invalid")
            })?;
            if !context.tenant_scope.contains(proposal.scope()) {
                return Err(Status::permission_denied(
                    "Finance bank match proposal scope is outside the command",
                ));
            }
            require_actor_match(
                &claims.principal_ref,
                &proposal.context().actor.principal_ref,
                "bank match proposal actor must match the authenticated principal",
            )?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "finance",
                    "finance.write",
                )
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let correlation_id = context.correlation_id;
            let receipt = with_banking_store(
                router,
                tenant_id,
                holding_ref,
                correlation_id,
                |store| async move {
                    store
                        .save_match_proposal(&proposal)
                        .await
                        .map_err(|error| finance_persistence_status(error, correlation_id))
                },
            )
            .await?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: context.actor.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "finance.bank_match.proposal_saved".into(),
                    purpose: context.audit.purpose,
                    correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
            Ok(Response::new(SaveBankMatchProposalResponse {
                receipt: Some(erpx_proto::platform::BankMatchProposalReceipt {
                    proposal_id: receipt.proposal_id.to_string(),
                    idempotency_key: receipt.idempotency_key,
                    replayed: receipt.replayed,
                }),
                correlation_id: correlation_id.to_string(),
            }))
        })
    }

    async fn save_bank_match_decision(
        &self,
        request: Request<SaveBankMatchDecisionRequest>,
    ) -> Result<Response<SaveBankMatchDecisionResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Finance bank matching requires an explicit scope",
                ));
            }
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            let decision: BankMatchDecision =
                decode_finance_banking_json(&request.get_ref().decision_json, "decision_json")?;
            decision.validate().map_err(|_| {
                Status::failed_precondition("Finance bank match decision is invalid")
            })?;
            if !context.tenant_scope.contains(decision.scope()) {
                return Err(Status::permission_denied(
                    "Finance bank match decision scope is outside the command",
                ));
            }
            require_actor_match(
                &claims.principal_ref,
                &decision.context().actor.principal_ref,
                "bank match decision actor must match the authenticated principal",
            )?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "finance",
                    "finance.write",
                )
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let correlation_id = context.correlation_id;
            let receipt = with_banking_store(
                router,
                tenant_id,
                holding_ref,
                correlation_id,
                |store| async move {
                    store
                        .save_match_decision(&decision)
                        .await
                        .map_err(|error| finance_persistence_status(error, correlation_id))
                },
            )
            .await?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: context.actor.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "finance.bank_match.decision_saved".into(),
                    purpose: context.audit.purpose,
                    correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
            Ok(Response::new(SaveBankMatchDecisionResponse {
                receipt: Some(erpx_proto::platform::BankMatchDecisionReceipt {
                    decision_id: receipt.decision_id.to_string(),
                    proposal_id: receipt.proposal_id.to_string(),
                    idempotency_key: receipt.idempotency_key,
                    replayed: receipt.replayed,
                }),
                correlation_id: correlation_id.to_string(),
            }))
        })
    }

    async fn get_finance_close_review(
        &self,
        request: Request<GetFinanceCloseReviewRequest>,
    ) -> Result<Response<GetFinanceCloseReviewResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            let holding_id = parse_uuid(&request.get_ref().holding_id, "holding_id")?;
            let company_id = parse_uuid(&request.get_ref().company_id, "company_id")?;
            let read_scope = finance_close_read_scope(&scope, holding_id, company_id)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "finance.read",
                )
                .await?;
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            let period_id = parse_uuid(&request.get_ref().period_id, "period_id")?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let result = with_close_read_adapter(
                router,
                tenant_id,
                holding_ref,
                context.correlation_id,
                |adapter| async move {
                    adapter
                        .read_review(period_id, read_scope, Utc::now())
                        .await
                        .map_err(|error| finance_close_read_status(error, context.correlation_id))
                },
            )
            .await?;
            let purpose = proto_context
                .audit
                .as_ref()
                .map(|audit| audit.purpose.clone())
                .unwrap_or_else(|| "finance close review read".into());
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "finance.close_review.read".into(),
                    purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
            let mut response = GetFinanceCloseReviewResponse {
                view: None,
                missing: false,
                missing_metadata: None,
                correlation_id: context.correlation_id.to_string(),
            };
            match result {
                FinanceCloseReadResult::Present(view) => {
                    response.view = Some(finance_close_read_view_message(*view));
                }
                FinanceCloseReadResult::Missing(metadata) => {
                    response.missing = true;
                    response.missing_metadata = Some(finance_close_read_metadata_message(metadata));
                }
            }
            Ok(Response::new(response))
        })
    }

    async fn list_finance_close_reviews(
        &self,
        request: Request<ListFinanceCloseReviewsRequest>,
    ) -> Result<Response<ListFinanceCloseReviewsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let read_scope = finance_close_read_scope(&caller_scope, holding_id, company_id)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &caller_scope,
                    "finance.read",
                )
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let page_size = match input.page_size {
                0 => 25,
                value if value <= MAX_FINANCE_CLOSE_READ_PAGE_SIZE => value,
                _ => {
                    return Err(Status::invalid_argument(format!(
                        "Finance close read page size must be 1..={MAX_FINANCE_CLOSE_READ_PAGE_SIZE}"
                    )));
                }
            };
            let before = if input.before_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_finance_close_read_cursor(&input.before_cursor)?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &caller_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let correlation_id = context.correlation_id;
            let page = with_close_read_adapter(
                self.state.tenant_router.as_ref().ok_or_else(|| {
                    Status::failed_precondition("tenant database routing is not configured")
                })?,
                tenant_id,
                holding_ref,
                correlation_id,
                |adapter| async move {
                    adapter
                        .list_reviews(read_scope, page_size, before, Utc::now())
                        .await
                        .map_err(|error| finance_close_read_status(error, correlation_id))
                },
            )
            .await?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "finance.close_review.list".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "finance close review list".into()),
                    correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
            Ok(Response::new(ListFinanceCloseReviewsResponse {
                rows: page
                    .rows
                    .into_iter()
                    .map(finance_close_read_row_message)
                    .collect(),
                has_more: page.has_more,
                next_cursor: page
                    .next_cursor
                    .as_ref()
                    .map(encode_finance_close_read_cursor)
                    .unwrap_or_default(),
                correlation_id: correlation_id.to_string(),
            }))
        })
    }

    async fn get_finance_banking_read_row(
        &self,
        request: Request<GetFinanceBankingReadRowRequest>,
    ) -> Result<Response<GetFinanceBankingReadRowResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Finance banking reads require an explicit scope",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "finance.read",
                )
                .await?;
            let tenant_id =
                validate_transport_reference(&request.get_ref().tenant_id, "tenant_id")?;
            let holding_ref =
                validate_transport_reference(&request.get_ref().holding_ref, "holding_ref")?;
            let aggregate_type =
                validate_transport_reference(&request.get_ref().aggregate_type, "aggregate_type")?;
            let aggregate_id = parse_uuid(&request.get_ref().aggregate_id, "aggregate_id")?;
            let read_scope = finance_banking_read_tenant_scope(&scope)?;
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let row = with_banking_read_store(
                router,
                tenant_id,
                holding_ref,
                context.correlation_id,
                |store| async move {
                    store
                        .get_read_row(aggregate_type, aggregate_id, &read_scope)
                        .await
                        .map_err(|error| finance_persistence_status(error, context.correlation_id))
                },
            )
            .await?;
            let row = match row {
                Some(row) if !scope.contains(&finance_banking_row_scope(&row)) => {
                    return Err(Status::internal("Finance banking read scope mismatch"));
                }
                Some(row) => Some(banking_read_row_message(row)?),
                None => None,
            };
            let purpose = proto_context
                .audit
                .as_ref()
                .map(|audit| audit.purpose.clone())
                .unwrap_or_else(|| "finance banking read".into());
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "finance.banking.read".into(),
                    purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
            Ok(Response::new(GetFinanceBankingReadRowResponse {
                row,
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn list_finance_banking_read_rows(
        &self,
        request: Request<ListFinanceBankingReadRowsRequest>,
    ) -> Result<Response<ListFinanceBankingReadRowsResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            let read_scope = finance_banking_read_tenant_scope(&scope)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "finance.read",
                )
                .await?;
            let input = request.get_ref();
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let page_size = finance_banking_read_page_size(input.page_size)?;
            let aggregate_type = if input.aggregate_type.trim().is_empty() {
                None
            } else {
                Some(
                    validate_transport_reference(&input.aggregate_type, "aggregate_type")?
                        .to_owned(),
                )
            };
            let company_id = if input.company_id.trim().is_empty() {
                None
            } else {
                Some(parse_uuid(&input.company_id, "company_id")?)
            };
            let after = if input.after_cursor.trim().is_empty() {
                None
            } else {
                Some(decode_finance_banking_read_cursor(&input.after_cursor)?)
            };
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let page = with_banking_read_store(
                router,
                tenant_id,
                holding_ref,
                context.correlation_id,
                |store| async move {
                    store
                        .list_read_rows(
                            &read_scope,
                            aggregate_type.as_deref(),
                            company_id,
                            after.as_ref(),
                            page_size,
                        )
                        .await
                        .map_err(|error| finance_persistence_status(error, context.correlation_id))
                },
            )
            .await?;
            let mut rows = Vec::with_capacity(page.rows.len());
            for row in page.rows {
                if !scope.contains(&finance_banking_row_scope(&row)) {
                    return Err(Status::internal("Finance banking read scope mismatch"));
                }
                rows.push(banking_read_row_message(row)?);
            }
            let purpose = proto_context
                .audit
                .as_ref()
                .map(|audit| audit.purpose.clone())
                .unwrap_or_else(|| "finance banking read list".into());
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "finance.banking.read.list".into(),
                    purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
            Ok(Response::new(ListFinanceBankingReadRowsResponse {
                rows,
                has_more: page.has_more,
                next_cursor: page
                    .next_cursor
                    .as_ref()
                    .map(encode_finance_banking_read_cursor)
                    .unwrap_or_default(),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn record_thai_tax_fact(
        &self,
        request: Request<RecordThaiTaxFactRequest>,
    ) -> Result<Response<RecordThaiTaxFactResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.tenant_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "Thai tax facts require an explicit company scope",
                ));
            }
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "finance.write",
                )
                .await?;
            let input = request.get_ref();
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            if input.fact_json.is_empty() || input.fact_json.len() > 1_048_576 {
                return Err(Status::invalid_argument("fact_json is out of bounds"));
            }
            let mut fact: ThaiTaxFact = serde_json::from_slice(&input.fact_json)
                .map_err(|_| Status::invalid_argument("fact_json is invalid"))?;
            fact.context = context.clone();
            fact.validate()
                .map_err(|_| Status::failed_precondition("Thai tax fact is invalid"))?;
            let fact_scope = Scope::Company {
                holding_id: fact.holding_id,
                company_id: fact.company_id,
            };
            if !context.tenant_scope.contains(&TenantScope::Company {
                holding_id: fact.holding_id,
                company_id: fact.company_id,
            }) || !scope.contains(&fact_scope)
            {
                return Err(Status::permission_denied(
                    "Thai tax fact scope does not match the command scope",
                ));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            self.state
                .authorize_entitled_command(
                    &claims,
                    &context,
                    customer_id,
                    "finance",
                    "finance.write",
                )
                .await?;
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let result = async {
                let store = SqlxFinanceThaiTaxStore::from_pools(
                    write.database_pool(),
                    write.database_pool(),
                    holding_ref,
                )
                .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                let receipt = store
                    .record_fact(&fact)
                    .await
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: context.actor.principal_ref.clone(),
                        customer_account_id: Some(customer_id),
                        action: "finance.thai_tax_fact.recorded".into(),
                        purpose: context.audit.purpose.clone(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
                Ok::<RecordThaiTaxFactResponse, Status>(RecordThaiTaxFactResponse {
                    receipt: Some(erpx_proto::platform::ThaiTaxFactReceipt {
                        fact_id: receipt.fact_id.to_string(),
                        version: receipt.version,
                        replayed: receipt.replayed,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            Ok(Response::new(result?))
        })
    }

    async fn get_thai_tax_report(
        &self,
        request: Request<GetThaiTaxReportRequest>,
    ) -> Result<Response<GetThaiTaxReportResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            let holding_id = parse_uuid(&input.holding_id, "holding_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let requested_scope = Scope::Company {
                holding_id,
                company_id,
            };
            if !scope.contains(&requested_scope) {
                return Err(Status::permission_denied(
                    "Thai tax report scope is not granted",
                ));
            }
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    "finance.read",
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(&claims, &input.mfa_code)
                .await?;
            let tenant_id = validate_transport_reference(&input.tenant_id, "tenant_id")?;
            let holding_ref = validate_transport_reference(&input.holding_ref, "holding_ref")?;
            let tax_period = validate_transport_reference(&input.tax_period, "tax_period")?;
            let currency = validate_transport_reference(&input.currency, "currency")?;
            if input.page_size == 0 || input.page_size > 1_000_000 {
                return Err(Status::invalid_argument("page_size must be 1..=1000000"));
            }
            let customer_id = parse_uuid(tenant_id, "tenant_id")?;
            if !self
                .state
                .store
                .customer_scope_contains(customer_id, &scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.tenant_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("tenant database routing is not configured")
            })?;
            let write = router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
                .await
                .map_err(|_| Status::unavailable("tenant write database is unavailable"))?;
            let read = match router
                .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
                .await
            {
                Ok(read) => read,
                Err(_) => {
                    write.close().await;
                    return Err(Status::unavailable("tenant read database is unavailable"));
                }
            };
            let result = async {
                let store = SqlxFinanceThaiTaxStore::from_pools(
                    write.database_pool(),
                    read.database_pool(),
                    holding_ref,
                )
                .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                let reconciliation = store
                    .reconcile_projection()
                    .await
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                require_finance_thai_tax_reconciliation(reconciliation)?;
                let read = store
                    .list_report(
                        holding_id,
                        company_id,
                        tax_period,
                        currency,
                        input.page_size,
                    )
                    .await
                    .map_err(|error| finance_persistence_status(error, context.correlation_id))?;
                let facts = read
                    .report
                    .facts
                    .iter()
                    .map(|fact| erpx_proto::platform::ThaiTaxFactReadRow {
                        fact_id: fact.fact_id.to_string(),
                        journal_id: fact.journal_id.to_string(),
                        line_id: fact.line_id.to_string(),
                        tax_date: fact.tax_date.to_string(),
                        kind: fact.kind.as_str().into(),
                        currency: fact.currency.clone(),
                        base_minor: fact.base_minor,
                        tax_minor: fact.tax_minor,
                        tax_rate_basis_points: fact.tax_rate_basis_points,
                        tax_identifier: fact.tax_identifier.clone(),
                        source_version: read.source_version,
                    })
                    .collect();
                Ok::<GetThaiTaxReportResponse, Status>(GetThaiTaxReportResponse {
                    holding_id: read.report.holding_id.to_string(),
                    company_id: read.report.company_id.to_string(),
                    tax_period: read.report.tax_period,
                    currency: read.report.currency,
                    fact_count: read.report.fact_count,
                    output_vat_base_minor: read.report.output_vat_base_minor,
                    output_vat_minor: read.report.output_vat_minor,
                    input_vat_base_minor: read.report.input_vat_base_minor,
                    input_vat_minor: read.report.input_vat_minor,
                    withholding_base_minor: read.report.withholding_base_minor,
                    withholding_minor: read.report.withholding_minor,
                    facts,
                    source_version: read.source_version,
                    projected_at: read.projected_at.to_rfc3339(),
                    correlation_id: context.correlation_id.to_string(),
                })
            }
            .await;
            write.close().await;
            read.close().await;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: Some(customer_id),
                    action: "finance.thai_tax_report.read".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "finance statutory tax report read".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("Finance audit persistence unavailable"))?;
            Ok(Response::new(result?))
        })
    }
}

#[derive(Clone)]
struct EntitlementRpcImpl {
    auth: Arc<AuthContext>,
    store: Arc<SqlxControlPlane>,
    commercial: Arc<SqlxCommercialStore>,
}

impl EntitlementRpcImpl {
    async fn authorize_entitlement_command(
        &self,
        claims: &AccessTokenClaims,
        context: &erpx_domain::contracts::CommandContext,
        customer_id: Uuid,
        module_key: &str,
        permission: &str,
        target_scope: &Scope,
    ) -> Result<(), Status> {
        require_actor_match(
            &claims.principal_ref,
            &context.actor.principal_ref,
            "command actor must match the authenticated principal",
        )?;
        let scope = access_scope(&context.tenant_scope);
        if !scope.contains(target_scope) {
            return Err(Status::permission_denied(
                "entitlement target is outside the command scope",
            ));
        }
        if !self
            .store
            .customer_scope_contains(customer_id, &scope)
            .await
            .map_err(|_| Status::internal("customer scope lookup failed"))?
        {
            return Err(Status::permission_denied("customer_scope_not_granted"));
        }
        self.auth
            .authorize_role(&self.store, &claims.principal_ref, &scope, permission)
            .await?;
        let subscription_id = context
            .subscription_id
            .ok_or_else(|| Status::invalid_argument("subscription_id is required"))?;
        let at = Utc::now();
        let Some((lifecycle, _)) = self
            .commercial
            .get_billing_lifecycle(customer_id, subscription_id)
            .await
            .map_err(|_| Status::internal("billing state lookup failed"))?
        else {
            return Err(Status::permission_denied("billing_state_not_found"));
        };
        let Some((book, _)) = self
            .commercial
            .get_entitlement_book(customer_id, module_key)
            .await
            .map_err(|_| Status::internal("entitlement lookup failed"))?
        else {
            return Err(Status::permission_denied("module_not_entitled"));
        };
        if !matches!(
            authorize_module(
                lifecycle.state,
                Some(&book.module_entitlement()),
                true,
                at,
                true,
            ),
            EntitlementDecision::Allowed
        ) {
            return Err(Status::permission_denied(
                "commercial_module_mutation_denied",
            ));
        }
        let Some((assignment, parents)) = self
            .store
            .effective_role_assignment_chain(&claims.principal_ref, &scope, permission, at)
            .await
            .map_err(|_| Status::internal("role authorization lookup failed"))?
        else {
            return Err(Status::permission_denied("effective_role_missing"));
        };
        evaluate_effective_access_chain(
            &assignment,
            &parents,
            &EffectiveAccessRequest {
                principal_ref: claims.principal_ref.clone(),
                at,
                requested_scope: scope.clone(),
                module_key: module_key.into(),
                required_permission: permission.into(),
                requested_approval_minor: 0,
                approval_limit_minor: assignment.approval_limit_minor,
                identity_active: true,
                session_active: true,
                billing_state: lifecycle.state,
                entitlement: Some(book.module_entitlement()),
                entitlement_book: None,
                is_mutation: true,
                segregation_of_duties_passed: true,
                support_capability_required: false,
                support_capability_active: false,
            },
        )
        .map_err(|error| Status::permission_denied(error.to_string()))?;
        if !self
            .store
            .sod_decision_active(&claims.principal_ref, module_key, &scope, at)
            .await
            .map_err(|_| Status::internal("segregation-of-duties lookup failed"))?
        {
            return Err(Status::permission_denied("segregation_of_duties_failed"));
        }
        Ok(())
    }
}

#[tonic::async_trait]
impl EntitlementService for EntitlementRpcImpl {
    async fn check_entitlement(
        &self,
        request: Request<CheckEntitlementRequest>,
    ) -> Result<Response<CheckEntitlementResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            query_context(&request, Some(proto_context))?;
            let scope = query_access_scope(proto_context)?;
            let support_capability_secret = request
                .metadata()
                .get("x-erpx-support-capability-secret")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            let input = request.into_inner();
            if input.customer_id.trim().is_empty()
                || input.subscription_id.trim().is_empty()
                || input.module_key.trim().is_empty()
                || input.required_permission.trim().is_empty()
            {
                return Err(Status::invalid_argument(
                    "customer_id, subscription_id, module_key, and required_permission are required",
                ));
            }
            let customer_id = parse_uuid(&input.customer_id, "customer_id")?;
            let subscription_id = parse_uuid(&input.subscription_id, "subscription_id")?;
            if !self
                .store
                .customer_scope_contains(customer_id, &scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Ok(Response::new(CheckEntitlementResponse {
                    allowed: false,
                    reason: "customer_scope_not_granted".into(),
                }));
            }
            let at = Utc::now();
            let support_capability_required = !input.support_capability_id.trim().is_empty();
            let support_capability_active = if support_capability_required {
                let capability_id =
                    parse_uuid(&input.support_capability_id, "support_capability_id")?;
                let approved_scope = parse_support_scope(&input.support_scope)?;
                if !approved_scope.contains(&scope) {
                    return Ok(Response::new(CheckEntitlementResponse {
                        allowed: false,
                        reason: "support_scope_not_granted".into(),
                    }));
                }
                let Some(device_ref) = (!input.support_device_ref.trim().is_empty())
                    .then_some(input.support_device_ref.clone())
                else {
                    return Ok(Response::new(CheckEntitlementResponse {
                        allowed: false,
                        reason: "support_capability_inactive".into(),
                    }));
                };
                let (Some(session_id), Some(ticket_id)) = (
                    (!input.support_session_id.trim().is_empty())
                        .then(|| parse_uuid(&input.support_session_id, "support_session_id"))
                        .transpose()?,
                    (!input.support_ticket_id.trim().is_empty())
                        .then(|| parse_uuid(&input.support_ticket_id, "support_ticket_id"))
                        .transpose()?,
                ) else {
                    return Ok(Response::new(CheckEntitlementResponse {
                        allowed: false,
                        reason: "support_capability_inactive".into(),
                    }));
                };
                self.store
                    .authorize_support_capability(&SupportCapabilityAuthorizationRequest {
                        capability_id,
                        customer_account_id: customer_id,
                        agent_ref: claims.principal_ref.clone(),
                        device_ref,
                        session_id,
                        ticket_id,
                        mode: input.support_mode.clone(),
                        permission: input.required_permission.clone(),
                        scope: input.support_scope.clone(),
                        module: input.module_key.clone(),
                        secret_hex: support_capability_secret,
                        at,
                    })
                    .await
                    .map_err(|_| Status::internal("support capability lookup failed"))?
            } else {
                false
            };
            let Some((lifecycle, _billing_version)) = self
                .commercial
                .get_billing_lifecycle(customer_id, subscription_id)
                .await
                .map_err(|_| Status::internal("billing state lookup failed"))?
            else {
                return Ok(Response::new(CheckEntitlementResponse {
                    allowed: false,
                    reason: "billing_state_not_found".into(),
                }));
            };
            let Some((book, _entitlement_version)) = self
                .commercial
                .get_entitlement_book(customer_id, &input.module_key)
                .await
                .map_err(|_| Status::internal("entitlement lookup failed"))?
            else {
                return Ok(Response::new(CheckEntitlementResponse {
                    allowed: false,
                    reason: "module_not_entitled".into(),
                }));
            };
            let Some((assignment, parents)) = self
                .store
                .effective_role_assignment_chain(
                    &claims.principal_ref,
                    &scope,
                    &input.required_permission,
                    at,
                )
                .await
                .map_err(|_| Status::internal("role authorization lookup failed"))?
            else {
                return Ok(Response::new(CheckEntitlementResponse {
                    allowed: false,
                    reason: "effective_role_missing".into(),
                }));
            };
            if !self
                .store
                .sod_decision_active(&claims.principal_ref, &input.module_key, &scope, at)
                .await
                .map_err(|_| Status::internal("segregation-of-duties lookup failed"))?
            {
                return Ok(Response::new(CheckEntitlementResponse {
                    allowed: false,
                    reason: "segregation_of_duties_failed".into(),
                }));
            }
            let entitlement = book.module_entitlement();
            let mut decision = evaluate_effective_access_chain(
                &assignment,
                &parents,
                &EffectiveAccessRequest {
                    principal_ref: claims.principal_ref.clone(),
                    at,
                    requested_scope: scope.clone(),
                    module_key: input.module_key.clone(),
                    required_permission: input.required_permission,
                    requested_approval_minor: input.approval_amount_minor,
                    approval_limit_minor: assignment.approval_limit_minor,
                    identity_active: true,
                    session_active: true,
                    billing_state: lifecycle.state,
                    entitlement: Some(entitlement),
                    entitlement_book: Some(book.clone()),
                    is_mutation: input.mutation,
                    segregation_of_duties_passed: true,
                    support_capability_required,
                    support_capability_active,
                },
            )
            .map(|_| ())
            .map_err(|error| Status::permission_denied(error.to_string()));
            if decision.is_ok() {
                decision = match scope {
                    Scope::Company { company_id, .. } => match book.authorize(
                        lifecycle.state,
                        company_id,
                        None,
                        true,
                        at,
                        input.mutation,
                    ) {
                        EntitlementDecision::Allowed => Ok(()),
                        EntitlementDecision::Denied(reason) => {
                            Err(Status::permission_denied(reason))
                        }
                    },
                    Scope::Branch {
                        company_id,
                        branch_id,
                        ..
                    } => match book.authorize(
                        lifecycle.state,
                        company_id,
                        Some(branch_id),
                        true,
                        at,
                        input.mutation,
                    ) {
                        EntitlementDecision::Allowed => Ok(()),
                        EntitlementDecision::Denied(reason) => {
                            Err(Status::permission_denied(reason))
                        }
                    },
                    _ => Err(Status::permission_denied(
                        "company_or_branch_scope_required",
                    )),
                };
            }
            Ok(Response::new(CheckEntitlementResponse {
                allowed: decision.is_ok(),
                reason: decision
                    .err()
                    .map_or_else(|| "allowed".into(), |error| error.to_string()),
            }))
        })
    }

    async fn allocate_company_entitlement(
        &self,
        request: Request<AllocateCompanyEntitlementRequest>,
    ) -> Result<Response<AllocateCompanyEntitlementResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let input = request.into_inner();
            self.auth.authorize_mfa(&claims, &input.mfa_code).await?;
            let customer_id = parse_uuid(&input.customer_id, "customer_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let module_key =
                validate_transport_reference(&input.module_key, "module_key")?.to_owned();
            let scope = access_scope(&context.tenant_scope);
            let target_scope = Scope::Company {
                holding_id: entitlement_holding_id(&scope)?,
                company_id,
            };
            self.authorize_entitlement_command(
                &claims,
                &context,
                customer_id,
                &module_key,
                ENTITLEMENT_ALLOCATE_PERMISSION,
                &target_scope,
            )
            .await?;
            let commercial_context = commercial_command_context(&context)?;
            let receipt = self
                .commercial
                .allocate_entitlement_company_with_expected_version(
                    AllocateCompanyCommand {
                        customer_id,
                        module_key,
                        company_id,
                        seats: input.seats,
                        branch_limit: input.branch_limit,
                        context: commercial_context,
                    },
                    context.expected_aggregate_version,
                )
                .await
                .map_err(|error| commercial_persistence_status(error, context.correlation_id))?;
            Ok(Response::new(AllocateCompanyEntitlementResponse {
                receipt: Some(entitlement_command_receipt(&receipt)),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn activate_company_entitlement(
        &self,
        request: Request<ActivateCompanyEntitlementRequest>,
    ) -> Result<Response<ActivateCompanyEntitlementResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let input = request.into_inner();
            self.auth.authorize_mfa(&claims, &input.mfa_code).await?;
            let customer_id = parse_uuid(&input.customer_id, "customer_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let module_key =
                validate_transport_reference(&input.module_key, "module_key")?.to_owned();
            let activated_at = entitlement_activation_time(&input.activated_at)?;
            let scope = access_scope(&context.tenant_scope);
            let target_scope = Scope::Company {
                holding_id: entitlement_holding_id(&scope)?,
                company_id,
            };
            self.authorize_entitlement_command(
                &claims,
                &context,
                customer_id,
                &module_key,
                ENTITLEMENT_ACTIVATE_PERMISSION,
                &target_scope,
            )
            .await?;
            let commercial_context = commercial_command_context(&context)?;
            let receipt = self
                .commercial
                .activate_entitlement_company_with_expected_version(
                    ActivateCompanyModuleCommand {
                        customer_id,
                        module_key,
                        company_id,
                        at: activated_at,
                        context: commercial_context,
                    },
                    context.expected_aggregate_version,
                )
                .await
                .map_err(|error| commercial_persistence_status(error, context.correlation_id))?;
            Ok(Response::new(ActivateCompanyEntitlementResponse {
                receipt: Some(entitlement_command_receipt(&receipt)),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }

    async fn activate_branch_entitlement(
        &self,
        request: Request<ActivateBranchEntitlementRequest>,
    ) -> Result<Response<ActivateBranchEntitlementResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let input = request.into_inner();
            self.auth.authorize_mfa(&claims, &input.mfa_code).await?;
            let customer_id = parse_uuid(&input.customer_id, "customer_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let branch_id = parse_uuid(&input.branch_id, "branch_id")?;
            let module_key =
                validate_transport_reference(&input.module_key, "module_key")?.to_owned();
            let activated_at = entitlement_activation_time(&input.activated_at)?;
            let scope = access_scope(&context.tenant_scope);
            let target_scope = Scope::Branch {
                holding_id: entitlement_holding_id(&scope)?,
                company_id,
                branch_id,
            };
            self.authorize_entitlement_command(
                &claims,
                &context,
                customer_id,
                &module_key,
                ENTITLEMENT_ACTIVATE_PERMISSION,
                &target_scope,
            )
            .await?;
            let commercial_context = commercial_command_context(&context)?;
            let receipt = self
                .commercial
                .activate_entitlement_branch_with_expected_version(
                    ActivateBranchModuleCommand {
                        customer_id,
                        module_key,
                        company_id,
                        branch_id,
                        at: activated_at,
                        context: commercial_context,
                    },
                    context.expected_aggregate_version,
                )
                .await
                .map_err(|error| commercial_persistence_status(error, context.correlation_id))?;
            Ok(Response::new(ActivateBranchEntitlementResponse {
                receipt: Some(entitlement_command_receipt(&receipt)),
                correlation_id: context.correlation_id.to_string(),
            }))
        })
    }
}

fn to_session_message(session: erpx_domain::support::SupportSession) -> SupportSessionMessage {
    SupportSessionMessage {
        id: session.id.to_string(),
        ticket_id: session.ticket_id.to_string(),
        customer_id: session.customer_account_id.to_string(),
        mode: support_mode_name(session.mode).into(),
        status: if session.approved_at.is_some() {
            "approved".into()
        } else {
            "requested".into()
        },
        expires_at: session.expires_at.to_rfc3339(),
        approval_state: session
            .approved_by
            .unwrap_or_else(|| "pending_customer".into()),
        capability_id: String::new(),
        capability_secret: String::new(),
        module: session.module,
    }
}

fn to_role_message(assignment: RoleAssignment) -> RoleAssignmentMessage {
    let effective = assignment.is_active_at(Utc::now());
    RoleAssignmentMessage {
        id: assignment.id.to_string(),
        principal_ref: assignment.principal_ref,
        role_key: assignment.role_key,
        scope: format!("{:?}", assignment.scope),
        valid_from: assignment.valid_from.to_rfc3339(),
        valid_until: assignment.valid_until.to_rfc3339(),
        risk_tier: risk_tier_name(assignment.risk_tier).into(),
        assignment_version: assignment.assignment_version,
        effective,
    }
}

fn target_scope(target: &RoleRecoveryTarget) -> Scope {
    match target {
        RoleRecoveryTarget::PlatformRoot => Scope::Platform,
        RoleRecoveryTarget::HoldingOwner { holding_id } => Scope::Holding {
            holding_id: *holding_id,
        },
    }
}

fn to_role_review_message(item: RoleReviewItem) -> RoleReviewItemMessage {
    let (notification_kind, notification_key) = item
        .notification
        .as_ref()
        .map(|notification| {
            (
                match notification.kind {
                    RoleExpiryNotificationKind::ExpiringSoon => "expiring_soon",
                    RoleExpiryNotificationKind::Expired => "expired",
                }
                .to_owned(),
                notification.stable_key(),
            )
        })
        .unwrap_or_default();
    RoleReviewItemMessage {
        assignment_id: item.assignment_id.to_string(),
        principal_ref: item.principal_ref,
        valid_from: item.valid_from.to_rfc3339(),
        valid_until: item.valid_until.to_rfc3339(),
        classifications: item
            .classifications
            .into_iter()
            .map(|classification| classification.code().to_owned())
            .collect(),
        notification_kind,
        notification_key,
        assignment: Some(to_role_message(item.assignment)),
    }
}

fn transport_interceptor(
    request: Request<()>,
    signer: &AccessTokenSigner,
) -> Result<Request<()>, Status> {
    if request.metadata().get("origin").is_some()
        && request
            .metadata()
            .get("access-control-request-method")
            .is_some()
    {
        return Ok(request);
    }
    let correlation_id = boundary::request_correlation(&request);
    let result = (|| {
        let request = transport(request)?;
        let token = request
            .metadata()
            .get("x-erpx-access-token")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Status::unauthenticated("x-erpx-access-token is required"))?;
        let claims = signer
            .verify(token, Utc::now())
            .map_err(|_| Status::unauthenticated("access token is invalid or expired"))?;
        let actor = request
            .metadata()
            .get("x-erpx-actor")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("x-erpx-actor is required"))?;
        let session = request
            .metadata()
            .get("x-erpx-session")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("x-erpx-session is required"))?;
        if claims.principal_ref != actor || claims.session_id.to_string() != session {
            return Err(Status::permission_denied(
                "access token attribution does not match transport identity",
            ));
        }
        Ok(request)
    })();
    result.map_err(|status| errors::enrich(status, Some(correlation_id)))
}

#[derive(Debug, Deserialize)]
struct LoginBody {
    principal_ref: String,
    password: String,
    device_label: String,
}

#[derive(Debug, Deserialize)]
struct RefreshBody {
    session_id: String,
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct RevokeBody {
    session_id: String,
}

#[derive(Debug, Serialize)]
struct SessionBody {
    session_id: String,
    access_token: String,
    refresh_token: String,
    access_expires_at: String,
    refresh_expires_at: String,
}

fn issued_session_body(session: &erpx_identity::IssuedSession) -> SessionBody {
    SessionBody {
        session_id: session.record().id().to_string(),
        access_token: session.access_token().token().to_owned(),
        refresh_token: session.refresh_token().as_str().to_owned(),
        access_expires_at: session.access_token().claims().expires_at.to_rfc3339(),
        refresh_expires_at: session.record().refresh_expires_at().to_rfc3339(),
    }
}

fn rotated_session_body(session: &erpx_identity::RotatedSession) -> SessionBody {
    SessionBody {
        session_id: session.record().id().to_string(),
        access_token: session.access_token().token().to_owned(),
        refresh_token: session.refresh_token().as_str().to_owned(),
        access_expires_at: session.access_token().claims().expires_at.to_rfc3339(),
        refresh_expires_at: session.record().refresh_expires_at().to_rfc3339(),
    }
}

fn identity_http_error(error: IdentityStoreError) -> (&'static str, &'static str) {
    match error {
        IdentityStoreError::InvalidCredentials
        | IdentityStoreError::PrincipalInactive
        | IdentityStoreError::DeviceRevoked
        | IdentityStoreError::PasskeyAlreadyRegistered
        | IdentityStoreError::PasskeyNotFound
        | IdentityStoreError::PasskeyRevoked
        | IdentityStoreError::SessionInactive
        | IdentityStoreError::Primitive(_) => ("401 Unauthorized", "authentication_failed"),
        IdentityStoreError::Database(_) => ("503 Service Unavailable", "identity unavailable"),
        IdentityStoreError::InvalidRecord(_) => {
            ("500 Internal Server Error", "identity_record_invalid")
        }
    }
}

#[derive(Clone)]
struct HttpServiceContext {
    store: Arc<SqlxControlPlane>,
    registry: Arc<SqlxTenantStoreRegistry>,
    auth: Arc<AuthContext>,
    commercial: Arc<SqlxCommercialStore>,
    tenant_router: Option<Arc<TenantDatabaseRouter>>,
    storage: Arc<StorageComposition>,
    readiness: Arc<ServerReadinessAdapter>,
    marketing_callback_secrets: Arc<dyn MarketingCallbackSecretResolver>,
}

#[derive(Debug, Deserialize)]
struct PublicMarketingFormSubmissionBody {
    submission_id: Uuid,
    values: BTreeMap<String, String>,
}

struct PublicMarketingHttpContext<'a> {
    control: &'a SqlxControlPlane,
    registry: &'a SqlxTenantStoreRegistry,
    router: Option<&'a TenantDatabaseRouter>,
}

fn public_marketing_path(path: &str) -> bool {
    let path = path.split('?').next().unwrap_or(path);
    path.starts_with("/public/marketing/forms/") || path.starts_with("/public/marketing/previews/")
}

fn public_marketing_route(path: &str) -> Option<(&str, &str)> {
    let (path, _) = path.split_once('?').unwrap_or((path, ""));
    let form_prefix = "/public/marketing/forms/";
    if let Some(value) = path.strip_prefix(form_prefix) {
        return value
            .strip_suffix("/submissions")
            .filter(|id| !id.is_empty())
            .map(|id| ("form", id));
    }
    path.strip_prefix("/public/marketing/previews/")
        .filter(|id| !id.is_empty())
        .map(|id| ("preview", id))
}

fn public_marketing_hostname(headers: &HashMap<String, String>) -> Result<&str, &'static str> {
    let host = headers
        .get("host")
        .map(String::as_str)
        .ok_or("host header is required")?;
    let hostname = host.split_once(':').map_or(host, |(value, _)| value).trim();
    if hostname.is_empty()
        || hostname.len() > 253
        || hostname == "localhost"
        || hostname.parse::<std::net::IpAddr>().is_ok()
        || hostname
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return Err("host header is invalid");
    }
    Ok(hostname)
}

fn marketing_abuse_binding_id(namespace: &str, value: &str) -> Uuid {
    let digest = Sha256::digest(format!("erpx:marketing-abuse:{namespace}:{value}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn marketing_abuse_fingerprint(
    headers: &HashMap<String, String>,
    tenant_ref: &str,
    hostname: &str,
) -> Result<String, &'static str> {
    let key = std::env::var("ERPX_MARKETING_ABUSE_FINGERPRINT_KEY")
        .map_err(|_| "marketing abuse fingerprinting is not configured")?;
    if key.trim().is_empty() {
        return Err("marketing abuse fingerprinting is not configured");
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes())
        .map_err(|_| "marketing abuse fingerprinting is not configured")?;
    mac.update(b"erpx-marketing-public-abuse-v1\0");
    mac.update(tenant_ref.as_bytes());
    mac.update(b"\0");
    mac.update(hostname.as_bytes());
    for header in [
        "x-real-ip",
        "x-forwarded-for",
        "user-agent",
        "accept-language",
    ] {
        mac.update(b"\0");
        mac.update(header.as_bytes());
        mac.update(b"=");
        mac.update(headers.get(header).map_or(b"", String::as_bytes));
    }
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn public_marketing_abuse_signal(
    kind: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
) -> Option<MarketingAbuseSignal> {
    if let Some(value) = headers
        .get("x-erpx-marketing-challenge")
        .map(String::as_str)
    {
        match value.trim().to_ascii_lowercase().as_str() {
            "required" => return Some(MarketingAbuseSignal::ChallengeRequired),
            "failed" => return Some(MarketingAbuseSignal::ChallengeFailed),
            "passed" => return Some(MarketingAbuseSignal::ChallengePassed),
            _ => {}
        }
    }
    if kind == "form"
        && serde_json::from_slice::<PublicMarketingFormSubmissionBody>(body)
            .ok()
            .is_some_and(|input| {
                ["_honeypot", "honeypot", "website", "homepage"]
                    .into_iter()
                    .any(|field| {
                        input
                            .values
                            .get(field)
                            .is_some_and(|value| !value.trim().is_empty())
                    })
            })
    {
        return Some(MarketingAbuseSignal::HoneypotTriggered);
    }
    None
}

fn public_marketing_abuse_decision_response(
    decision: MarketingAbuseDecision,
) -> Option<(&'static str, &'static str, String)> {
    match decision {
        MarketingAbuseDecision::Allow => None,
        MarketingAbuseDecision::Reject { .. } => Some((
            "400 Bad Request",
            "application/json",
            "{\"error\":\"marketing_request_rejected\"}".into(),
        )),
        MarketingAbuseDecision::RateLimited { .. } => Some((
            "429 Too Many Requests",
            "application/json",
            "{\"error\":\"marketing_rate_limited\"}".into(),
        )),
        MarketingAbuseDecision::Challenge { .. } => Some((
            "403 Forbidden",
            "application/json",
            "{\"error\":\"marketing_challenge_required\"}".into(),
        )),
    }
}

fn public_marketing_error(error: PersistenceError) -> (&'static str, &'static str) {
    match error {
        PersistenceError::InvalidData(_) | PersistenceError::Configuration(_) => {
            ("400 Bad Request", "marketing request is invalid")
        }
        _ => ("503 Service Unavailable", "marketing service unavailable"),
    }
}

async fn handle_public_marketing_http(
    method: &str,
    path: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
    correlation_id: Uuid,
    public_context: PublicMarketingHttpContext<'_>,
) -> Option<(&'static str, &'static str, String)> {
    let PublicMarketingHttpContext {
        control,
        registry,
        router,
    } = public_context;
    if !public_marketing_path(path) {
        return None;
    }
    let Some((kind, identifier)) = public_marketing_route(path) else {
        return Some((
            "404 Not Found",
            "application/json",
            "{\"error\":\"not_found\"}".into(),
        ));
    };
    let expected_method = if kind == "form" { "POST" } else { "GET" };
    if method != expected_method {
        return Some((
            "405 Method Not Allowed",
            "application/json",
            "{\"error\":\"method_not_allowed\"}".into(),
        ));
    }
    let tenant_id = match headers
        .get("x-erpx-tenant-id")
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        Some(value) => value,
        None => {
            return Some((
                "400 Bad Request",
                "application/json",
                "{\"error\":\"tenant_id_required\"}".into(),
            ));
        }
    };
    let registration = match registry.get(tenant_id).await {
        Ok(Some(registration)) if registration.state == "active" => registration,
        Ok(_) => {
            return Some((
                "404 Not Found",
                "application/json",
                "{\"error\":\"marketing_route_not_found\"}".into(),
            ));
        }
        Err(_) => {
            return Some((
                "503 Service Unavailable",
                "application/json",
                "{\"error\":\"marketing_service_unavailable\"}".into(),
            ));
        }
    };
    let Some(router) = router else {
        return Some((
            "503 Service Unavailable",
            "application/json",
            "{\"error\":\"tenant_routing_unavailable\"}".into(),
        ));
    };
    let hostname = match public_marketing_hostname(headers) {
        Ok(hostname) => hostname,
        Err(error) => {
            return Some((
                "400 Bad Request",
                "application/json",
                serde_json::json!({"error": error}).to_string(),
            ));
        }
    };
    let resource_id = match Uuid::parse_str(identifier) {
        Ok(id) if !id.is_nil() => id,
        _ => {
            let code = if kind == "form" {
                "form_id_invalid"
            } else {
                "preview_id_invalid"
            };
            return Some((
                "400 Bad Request",
                "application/json",
                serde_json::json!({"error": code}).to_string(),
            ));
        }
    };
    let (resource_kind, request_limit) = if kind == "form" {
        ("form", PUBLIC_MARKETING_FORM_REQUESTS_PER_MINUTE)
    } else {
        ("preview", PUBLIC_MARKETING_PREVIEW_REQUESTS_PER_MINUTE)
    };
    let fingerprint = match marketing_abuse_fingerprint(headers, tenant_id, hostname) {
        Ok(fingerprint) => fingerprint,
        Err(_) => {
            return Some((
                "503 Service Unavailable",
                "application/json",
                "{\"error\":\"marketing_abuse_service_unavailable\"}".into(),
            ));
        }
    };
    let tenant_binding_id = marketing_abuse_binding_id("tenant", tenant_id);
    let domain_binding_id = marketing_abuse_binding_id("hostname", &hostname.to_ascii_lowercase());
    let policy = match AnonymousMarketingAbusePolicy::new(
        tenant_binding_id,
        resource_id,
        domain_binding_id,
        Duration::seconds(erpx_domain::marketing_abuse::DEFAULT_MARKETING_ABUSE_WINDOW_SECONDS),
        Duration::seconds(
            erpx_domain::marketing_abuse::DEFAULT_MARKETING_ABUSE_BURST_WINDOW_SECONDS,
        ),
        request_limit,
        request_limit.min(erpx_domain::marketing_abuse::DEFAULT_MARKETING_ABUSE_REQUESTS_PER_BURST),
    ) {
        Ok(policy) => policy,
        Err(_) => {
            return Some((
                "503 Service Unavailable",
                "application/json",
                "{\"error\":\"marketing_abuse_service_unavailable\"}".into(),
            ));
        }
    };
    let signal = public_marketing_abuse_signal(kind, headers, body);
    let policy_input = match AnonymousMarketingAbuseInput::new(
        tenant_binding_id,
        resource_id,
        domain_binding_id,
        fingerprint,
        Utc::now(),
        0,
        0,
        signal,
    ) {
        Ok(input) => input,
        Err(_) => {
            return Some((
                "503 Service Unavailable",
                "application/json",
                "{\"error\":\"marketing_abuse_service_unavailable\"}".into(),
            ));
        }
    };
    match policy.evaluate(&policy_input) {
        Ok(decision) => {
            if let Some(response) = public_marketing_abuse_decision_response(decision) {
                return Some(response);
            }
        }
        Err(_) => {
            return Some((
                "503 Service Unavailable",
                "application/json",
                "{\"error\":\"marketing_abuse_service_unavailable\"}".into(),
            ));
        }
    }
    let abuse_allowed = control
        .consume_marketing_public_abuse_window(
            &MarketingPublicAbuseWindowRequest {
                tenant_id: tenant_id.to_owned(),
                holding_ref: registration.holding_ref.clone(),
                hostname: hostname.to_ascii_lowercase(),
                resource_kind: resource_kind.to_owned(),
                resource_id,
            },
            Utc::now(),
            request_limit,
        )
        .await;
    let abuse_allowed = match abuse_allowed {
        Ok(allowed) => allowed,
        Err(_) => {
            return Some((
                "503 Service Unavailable",
                "application/json",
                "{\"error\":\"marketing_abuse_service_unavailable\"}".into(),
            ));
        }
    };
    let mut observed_input = policy_input;
    if !abuse_allowed {
        observed_input.requests_in_window = request_limit;
    }
    match policy.evaluate(&observed_input) {
        Ok(decision) => {
            if let Some(response) = public_marketing_abuse_decision_response(decision) {
                return Some(response);
            }
        }
        Err(_) => {
            return Some((
                "503 Service Unavailable",
                "application/json",
                "{\"error\":\"marketing_abuse_service_unavailable\"}".into(),
            ));
        }
    }
    let write = match router
        .route(
            tenant_id,
            &registration.holding_ref,
            RoutedDatabaseSide::Write,
        )
        .await
    {
        Ok(database) => database,
        Err(_) => {
            return Some((
                "503 Service Unavailable",
                "application/json",
                "{\"error\":\"tenant_write_unavailable\"}".into(),
            ));
        }
    };
    let read = match router
        .route(
            tenant_id,
            &registration.holding_ref,
            RoutedDatabaseSide::Read,
        )
        .await
    {
        Ok(database) => database,
        Err(_) => {
            write.close().await;
            return Some((
                "503 Service Unavailable",
                "application/json",
                "{\"error\":\"tenant_read_unavailable\"}".into(),
            ));
        }
    };
    let store = match SqlxMarketingContentStore::new(
        write.database_pool().clone(),
        read.database_pool().clone(),
        registration.holding_ref.clone(),
    ) {
        Ok(store) => store,
        Err(error) => {
            write.close().await;
            read.close().await;
            let (status, code) = public_marketing_error(error);
            return Some((
                status,
                "application/json",
                serde_json::json!({"error": code}).to_string(),
            ));
        }
    };
    let result = if kind == "form" {
        let form_id = match Uuid::parse_str(identifier) {
            Ok(id) if !id.is_nil() => id,
            _ => {
                write.close().await;
                read.close().await;
                return Some((
                    "400 Bad Request",
                    "application/json",
                    "{\"error\":\"form_id_invalid\"}".into(),
                ));
            }
        };
        let input = match serde_json::from_slice::<PublicMarketingFormSubmissionBody>(body) {
            Ok(input) if body.len() <= 2 * 1024 * 1024 => input,
            _ => {
                write.close().await;
                read.close().await;
                return Some((
                    "400 Bad Request",
                    "application/json",
                    "{\"error\":\"invalid_submission\"}".into(),
                ));
            }
        };
        let idempotency_key = match headers
            .get("idempotency-key")
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty() && value.len() <= 255)
        {
            Some(value) => value.to_owned(),
            None => {
                write.close().await;
                read.close().await;
                return Some((
                    "400 Bad Request",
                    "application/json",
                    "{\"error\":\"idempotency_key_required\"}".into(),
                ));
            }
        };
        let binding = match store.load_active_domain_binding(hostname).await {
            Ok(Some(binding)) => binding,
            Ok(None) => {
                write.close().await;
                read.close().await;
                return Some((
                    "404 Not Found",
                    "application/json",
                    "{\"error\":\"marketing_domain_not_bound\"}".into(),
                ));
            }
            Err(error) => {
                write.close().await;
                read.close().await;
                let (status, code) = public_marketing_error(error);
                return Some((
                    status,
                    "application/json",
                    serde_json::json!({"error": code}).to_string(),
                ));
            }
        };
        let form = match store.load_published_form(form_id).await {
            Ok(Some(form)) => form,
            Ok(None) => {
                write.close().await;
                read.close().await;
                return Some((
                    "404 Not Found",
                    "application/json",
                    "{\"error\":\"published_form_not_found\"}".into(),
                ));
            }
            Err(error) => {
                write.close().await;
                read.close().await;
                let (status, code) = public_marketing_error(error);
                return Some((
                    status,
                    "application/json",
                    serde_json::json!({"error": code}).to_string(),
                ));
            }
        };
        if !binding.domain_scope.contains(&form.scope) {
            write.close().await;
            read.close().await;
            return Some((
                "403 Forbidden",
                "application/json",
                "{\"error\":\"marketing_scope_denied\"}".into(),
            ));
        }
        let form_ref = match ContentFormRef::new(form.id, form.scope.clone()) {
            Ok(form_ref) => form_ref,
            Err(_) => {
                write.close().await;
                read.close().await;
                return Some((
                    "503 Service Unavailable",
                    "application/json",
                    "{\"error\":\"published_form_invalid\"}".into(),
                ));
            }
        };
        let mut context = match CommandContext::try_new(
            idempotency_key,
            ActorAttribution {
                principal_ref: format!("public-form:{tenant_id}"),
                kind: ActorKind::Service,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            form.scope.clone(),
            "public marketing form submission",
        ) {
            Ok(context) => context,
            Err(_) => {
                write.close().await;
                read.close().await;
                return Some((
                    "400 Bad Request",
                    "application/json",
                    "{\"error\":\"command_context_invalid\"}".into(),
                ));
            }
        };
        context.audit.customer_visible = true;
        context.audit.module = Some("marketing".into());
        match store
            .submit_form(
                &form_ref,
                input.submission_id,
                &input.values,
                Utc::now(),
                &context,
            )
            .await
        {
            Ok(receipt) => serde_json::json!({
                "accepted": true,
                "submission_id": input.submission_id,
                "replayed": receipt.replayed,
                "correlation_id": correlation_id,
            })
            .to_string(),
            Err(error) => {
                let (status, code) = public_marketing_error(error);
                write.close().await;
                read.close().await;
                return Some((
                    status,
                    "application/json",
                    serde_json::json!({"error": code}).to_string(),
                ));
            }
        }
    } else {
        let preview_id = match Uuid::parse_str(identifier) {
            Ok(id) if !id.is_nil() => id,
            _ => {
                write.close().await;
                read.close().await;
                return Some((
                    "400 Bad Request",
                    "application/json",
                    "{\"error\":\"preview_id_invalid\"}".into(),
                ));
            }
        };
        let token = path
            .split_once('?')
            .and_then(|(_, query)| {
                query
                    .split('&')
                    .find_map(|pair| pair.strip_prefix("token="))
            })
            .filter(|token| !token.is_empty());
        let Some(token) = token else {
            write.close().await;
            read.close().await;
            return Some((
                "401 Unauthorized",
                "application/json",
                "{\"error\":\"preview_token_required\"}".into(),
            ));
        };
        let preview = match store.authorize_preview(preview_id, token, Utc::now()).await {
            Ok(Some(preview)) => preview,
            Ok(None) => {
                write.close().await;
                read.close().await;
                return Some((
                    "404 Not Found",
                    "application/json",
                    "{\"error\":\"preview_not_found\"}".into(),
                ));
            }
            Err(error) => {
                write.close().await;
                read.close().await;
                let (status, code) = public_marketing_error(error);
                return Some((
                    status,
                    "application/json",
                    serde_json::json!({"error": code}).to_string(),
                ));
            }
        };
        let binding = match store.load_active_domain_binding(hostname).await {
            Ok(Some(binding)) if binding.template == preview => binding,
            Ok(_) => {
                write.close().await;
                read.close().await;
                return Some((
                    "404 Not Found",
                    "application/json",
                    "{\"error\":\"preview_route_not_found\"}".into(),
                ));
            }
            Err(error) => {
                write.close().await;
                read.close().await;
                let (status, code) = public_marketing_error(error);
                return Some((
                    status,
                    "application/json",
                    serde_json::json!({"error": code}).to_string(),
                ));
            }
        };
        let template = match store
            .load_published_template_read(preview.template_id)
            .await
        {
            Ok(Some(template)) => template,
            Ok(None) => {
                write.close().await;
                read.close().await;
                return Some((
                    "404 Not Found",
                    "application/json",
                    "{\"error\":\"preview_content_not_found\"}".into(),
                ));
            }
            Err(error) => {
                write.close().await;
                read.close().await;
                let (status, code) = public_marketing_error(error);
                return Some((
                    status,
                    "application/json",
                    serde_json::json!({"error": code}).to_string(),
                ));
            }
        };
        if template.published_version != Some(binding.template.version) {
            write.close().await;
            read.close().await;
            return Some((
                "404 Not Found",
                "application/json",
                "{\"error\":\"preview_content_not_found\"}".into(),
            ));
        }
        match serde_json::to_string(&template) {
            Ok(body) => body,
            Err(_) => {
                write.close().await;
                read.close().await;
                return Some((
                    "503 Service Unavailable",
                    "application/json",
                    "{\"error\":\"preview_encoding_failed\"}".into(),
                ));
            }
        }
    };
    write.close().await;
    read.close().await;
    Some(("200 OK", "application/json", result))
}

async fn serve_http_health(listener: TcpListener, context: HttpServiceContext) {
    loop {
        let accepted = listener.accept().await;
        let (stream, _) = match accepted {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(%error, "HTTP health listener accept failed");
                continue;
            }
        };
        let context = context.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_http_health(stream, context).await {
                tracing::debug!(%error, "HTTP health request failed");
            }
        });
    }
}

async fn handle_http_health(
    mut stream: TcpStream,
    context: HttpServiceContext,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let HttpServiceContext {
        store,
        registry,
        auth,
        commercial,
        tenant_router,
        storage,
        readiness,
        marketing_callback_secrets,
    } = context;
    let (method, path, headers, body) = read_http_request(&mut stream).await?;
    let cors_origin = http_cors_origin(&headers);
    if method == "OPTIONS" {
        let status = if headers.contains_key("origin") && cors_origin.is_none() {
            "403 Forbidden"
        } else {
            "204 No Content"
        };
        return write_http_response(
            &mut stream,
            status,
            "application/json",
            "",
            Uuid::new_v4(),
            cors_origin.as_deref(),
        )
        .await;
    }
    let _request_digest = boundary::raw_body_digest(&body);
    let correlation_id = match boundary::correlation_values([
        headers.get("x-erpx-correlation-id").map(String::as_bytes),
        headers.get("x-erpx-correlation").map(String::as_bytes),
        headers.get("x-correlation-id").map(String::as_bytes),
    ]) {
        Ok(Some(correlation_id)) => correlation_id,
        Ok(None) => Uuid::new_v4(),
        Err(_) => {
            let correlation_id = Uuid::new_v4();
            return write_http_response(
                &mut stream,
                "400 Bad Request",
                "application/json",
                &errors::http_body(
                    "400 Bad Request",
                    r#"{"error":"invalid_correlation"}"#,
                    correlation_id,
                ),
                correlation_id,
                cors_origin.as_deref(),
            )
            .await;
        }
    };
    let _request_trace = errors::HttpRequestTrace::start(&method, &path, correlation_id);
    if path.starts_with("/auth/passkeys/") && auth.passkeys.is_none() {
        return write_http_response(
            &mut stream,
            "503 Service Unavailable",
            "application/json",
            r#"{"error":"webauthn_unavailable"}"#,
            correlation_id,
            cors_origin.as_deref(),
        )
        .await;
    }
    if let Some(passkey_service) = auth.passkeys.clone()
        && let Some(passkey_response) = identity_http::handle_passkey(
            &method,
            &path,
            &headers,
            &body,
            auth.clone(),
            passkey_service,
        )
        .await
    {
        return write_http_response(
            &mut stream,
            passkey_response.status,
            "application/json",
            &passkey_response.body,
            correlation_id,
            cors_origin.as_deref(),
        )
        .await;
    }
    if let Some(identity_response) =
        identity_http::handle(&method, &path, &headers, &body, auth.clone()).await?
    {
        return write_http_response(
            &mut stream,
            identity_response.status,
            "application/json",
            &identity_response.body,
            correlation_id,
            cors_origin.as_deref(),
        )
        .await;
    }
    if public_marketing_path(&path)
        && let Some((status, content_type, response_body)) = handle_public_marketing_http(
            &method,
            &path,
            &headers,
            &body,
            correlation_id,
            PublicMarketingHttpContext {
                control: &store,
                registry: &registry,
                router: tenant_router.as_deref(),
            },
        )
        .await
    {
        return write_http_response(
            &mut stream,
            status,
            content_type,
            &response_body,
            correlation_id,
            cors_origin.as_deref(),
        )
        .await;
    }
    if path == "/objects/upload" || path == "/objects/download" {
        let (status, body) = if method != "POST" {
            (
                "405 Method Not Allowed",
                serde_json::json!({"error":"method_not_allowed"}).to_string(),
            )
        } else {
            handle_object_http(
                &path,
                &headers,
                &body,
                correlation_id,
                ObjectHttpContext {
                    registry: &registry,
                    auth: &auth,
                    store: &store,
                    commercial: &commercial,
                    storage: &storage,
                },
            )
            .await
        };
        return write_http_response(
            &mut stream,
            status,
            "application/json",
            &body,
            correlation_id,
            cors_origin.as_deref(),
        )
        .await;
    }
    if path == "/webhooks/marketing" {
        let (status, body) = if method != "POST" {
            (
                "405 Method Not Allowed",
                serde_json::json!({"error":"method_not_allowed"}).to_string(),
            )
        } else {
            match accept_marketing_provider_callback(
                &body,
                &headers,
                &store,
                tenant_router.as_deref(),
                marketing_callback_secrets.as_ref(),
                correlation_id,
            )
            .await
            {
                Ok(receipt) => (
                    "200 OK",
                    serde_json::json!({
                        "accepted": true,
                        "duplicate": receipt.duplicate,
                        "order": receipt.order,
                        "delivery_version": receipt.delivery.map(|value| value.version),
                    })
                    .to_string(),
                ),
                Err(error) => {
                    let status = match error {
                        MarketingCallbackHttpError::Configuration
                        | MarketingCallbackHttpError::Persistence => "503 Service Unavailable",
                        MarketingCallbackHttpError::Invalid(_) => "400 Bad Request",
                    };
                    (
                        status,
                        serde_json::json!({"error": error.message()}).to_string(),
                    )
                }
            }
        };
        return write_http_response(
            &mut stream,
            status,
            "application/json",
            &body,
            correlation_id,
            cors_origin.as_deref(),
        )
        .await;
    }
    let (status, content_type, body) = match path.as_str() {
        "/healthz" if method == "GET" => (
            "200 OK",
            "application/json",
            serde_json::json!({
                "status": "ok",
                "storage": storage.health_signal(),
            })
            .to_string(),
        ),
        "/readyz" if method == "GET" => {
            let system_readiness = readiness.observe().await;
            let control_plane_ready = store.check_ready().await.is_ok();
            let identity_ready = auth.identity.check_ready().await.is_ok();
            storage.refresh_readiness().await;
            let storage_ready = storage.configuration_ready();
            let ready = control_plane_ready
                && identity_ready
                && storage_ready
                && system_readiness.is_ready();
            if ready {
                (
                    "200 OK",
                    "application/json",
                    serde_json::json!({
                        "status": "ready",
                        "control_plane": "ready",
                        "identity": "ready",
                        "storage": storage.health_signal(),
                        "system_readiness": {
                            "status": system_readiness.overall().as_str(),
                            "required": true,
                            "worker_heartbeat": system_readiness.worker_heartbeat.as_str(),
                            "tenant_routing": system_readiness.tenant_routing.as_str(),
                            "projection_freshness": system_readiness.projection_freshness.as_str(),
                            "backup_pitr": system_readiness.backup_pitr.as_str(),
                        },
                    })
                    .to_string(),
                )
            } else {
                (
                    "503 Service Unavailable",
                    "application/json",
                    serde_json::json!({
                        "status": "not_ready",
                        "control_plane": if control_plane_ready { "ready" } else { "not_ready" },
                        "identity": if identity_ready { "ready" } else { "not_ready" },
                        "storage": storage.health_signal(),
                        "system_readiness": {
                            "status": system_readiness.overall().as_str(),
                            "required": true,
                            "worker_heartbeat": system_readiness.worker_heartbeat.as_str(),
                            "tenant_routing": system_readiness.tenant_routing.as_str(),
                            "projection_freshness": system_readiness.projection_freshness.as_str(),
                            "backup_pitr": system_readiness.backup_pitr.as_str(),
                        },
                    })
                    .to_string(),
                )
            }
        }
        "/metrics" if method == "GET" => (
            "200 OK",
            "text/plain; version=0.0.4",
            "erpx_server_up 1\n".to_owned(),
        ),
        "/webhooks/stripe" if method == "POST" => {
            match accept_stripe_webhook(&body, &headers, commercial.as_ref(), correlation_id).await
            {
                Ok(inserted) => (
                    "200 OK",
                    "application/json",
                    serde_json::json!({
                        "accepted": true,
                        "duplicate": matches!(inserted, StripeInboxInsert::Duplicate)
                    })
                    .to_string(),
                ),
                Err(error) => {
                    let status = match error {
                        StripeWebhookError::Configuration | StripeWebhookError::Persistence => {
                            "503 Service Unavailable"
                        }
                        StripeWebhookError::Invalid(_) => "400 Bad Request",
                    };
                    (
                        status,
                        "application/json",
                        serde_json::json!({ "error": error.message() }).to_string(),
                    )
                }
            }
        }
        "/healthz" | "/readyz" | "/metrics" => (
            "405 Method Not Allowed",
            "application/json",
            "{\"error\":\"method_not_allowed\"}".to_owned(),
        ),
        "/auth/login" if method == "POST" => match serde_json::from_slice::<LoginBody>(&body) {
            Err(_) => (
                "400 Bad Request",
                "application/json",
                "{\"error\":\"invalid_json\"}".to_owned(),
            ),
            Ok(input) => match auth
                .identity
                .login(
                    &auth.signer,
                    &input.principal_ref,
                    &input.password,
                    &input.device_label,
                    Utc::now(),
                )
                .await
            {
                Ok(session) => (
                    "200 OK",
                    "application/json",
                    serde_json::to_string(&issued_session_body(&session))?,
                ),
                Err(error) => {
                    let (status, message) = identity_http_error(error);
                    (
                        status,
                        "application/json",
                        serde_json::json!({ "error": message }).to_string(),
                    )
                }
            },
        },
        "/auth/refresh" if method == "POST" => match serde_json::from_slice::<RefreshBody>(&body) {
            Err(_) => (
                "400 Bad Request",
                "application/json",
                "{\"error\":\"invalid_json\"}".to_owned(),
            ),
            Ok(input) => match Uuid::parse_str(&input.session_id) {
                Err(_) => (
                    "400 Bad Request",
                    "application/json",
                    "{\"error\":\"session_id_must_be_uuid\"}".to_owned(),
                ),
                Ok(session_id) => match auth
                    .identity
                    .refresh(&auth.signer, session_id, &input.refresh_token, Utc::now())
                    .await
                {
                    Ok(session) => (
                        "200 OK",
                        "application/json",
                        serde_json::to_string(&rotated_session_body(&session))?,
                    ),
                    Err(error) => {
                        let (status, message) = identity_http_error(error);
                        (
                            status,
                            "application/json",
                            serde_json::json!({ "error": message }).to_string(),
                        )
                    }
                },
            },
        },
        "/auth/revoke" if method == "POST" => {
            let Some(token) = headers
                .get("x-erpx-access-token")
                .filter(|value| !value.is_empty())
            else {
                return write_http_response(
                    &mut stream,
                    "401 Unauthorized",
                    "application/json",
                    "{\"error\":\"access_token_required\"}",
                    correlation_id,
                    cors_origin.as_deref(),
                )
                .await;
            };
            let claims = match auth
                .identity
                .authorize_access_token(&auth.signer, token, Utc::now())
                .await
            {
                Ok(claims) => claims,
                Err(_) => {
                    return write_http_response(
                        &mut stream,
                        "401 Unauthorized",
                        "application/json",
                        "{\"error\":\"authentication_failed\"}",
                        correlation_id,
                        cors_origin.as_deref(),
                    )
                    .await;
                }
            };
            match serde_json::from_slice::<RevokeBody>(&body) {
                Err(_) => (
                    "400 Bad Request",
                    "application/json",
                    "{\"error\":\"invalid_json\"}".to_owned(),
                ),
                Ok(input) => match Uuid::parse_str(&input.session_id) {
                    Err(_) => (
                        "400 Bad Request",
                        "application/json",
                        "{\"error\":\"session_id_must_be_uuid\"}".to_owned(),
                    ),
                    Ok(session_id) if claims.session_id != session_id => (
                        "403 Forbidden",
                        "application/json",
                        "{\"error\":\"session_mismatch\"}".to_owned(),
                    ),
                    Ok(session_id) => {
                        match auth.identity.revoke_session(session_id, Utc::now()).await {
                            Ok(_) => (
                                "200 OK",
                                "application/json",
                                "{\"revoked\":true}".to_owned(),
                            ),
                            Err(_) => (
                                "503 Service Unavailable",
                                "application/json",
                                "{\"error\":\"identity_unavailable\"}".to_owned(),
                            ),
                        }
                    }
                },
            }
        }
        "/auth/login" | "/auth/refresh" | "/auth/revoke" => (
            "405 Method Not Allowed",
            "application/json",
            "{\"error\":\"method_not_allowed\"}".to_owned(),
        ),
        "/webhooks/stripe" => (
            "405 Method Not Allowed",
            "application/json",
            "{\"error\":\"method_not_allowed\"}".to_owned(),
        ),
        _ => (
            "404 Not Found",
            "application/json",
            "{\"status\":\"not_found\"}".to_owned(),
        ),
    };
    write_http_response(
        &mut stream,
        status,
        content_type,
        &body,
        correlation_id,
        cors_origin.as_deref(),
    )
    .await
}

#[derive(Debug, Deserialize)]
struct ObjectUploadBody {
    tenant_id: String,
    storage_namespace: String,
    customer_id: String,
    subscription_id: String,
    company_id: Option<String>,
    branch_id: Option<String>,
    object_key: String,
    expires_seconds: u32,
    content_type: String,
    content_length: u64,
}

#[derive(Debug, Deserialize)]
struct ObjectDownloadBody {
    tenant_id: String,
    storage_namespace: String,
    customer_id: String,
    subscription_id: String,
    company_id: Option<String>,
    branch_id: Option<String>,
    object_key: String,
    expires_seconds: u32,
    content_type: Option<String>,
    max_bytes: Option<u64>,
}

struct ObjectHttpContext<'a> {
    registry: &'a SqlxTenantStoreRegistry,
    auth: &'a AuthContext,
    store: &'a SqlxControlPlane,
    commercial: &'a SqlxCommercialStore,
    storage: &'a StorageComposition,
}

struct ObjectPolicyRequest<'a> {
    tenant_id: &'a str,
    storage_namespace: &'a str,
    customer_id_ref: &'a str,
    subscription_id_ref: &'a str,
    company_id_ref: Option<&'a str>,
    branch_id_ref: Option<&'a str>,
    object_key: &'a str,
    permission: &'a str,
    correlation_id: Uuid,
    audit_read: bool,
}

async fn handle_object_http(
    path: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
    correlation_id: Uuid,
    context: ObjectHttpContext<'_>,
) -> (&'static str, String) {
    if path == "/objects/upload" {
        let Ok(input) = serde_json::from_slice::<ObjectUploadBody>(body) else {
            return (
                "400 Bad Request",
                serde_json::json!({"error":"invalid_json"}).to_string(),
            );
        };
        let customer_id = input.customer_id.clone();
        let subscription_id = input.subscription_id.clone();
        let company_id = input.company_id.clone();
        let branch_id = input.branch_id.clone();
        let request = TenantObjectUploadRequest {
            tenant_id: input.tenant_id,
            storage_namespace: input.storage_namespace,
            object_key: input.object_key,
            expires_seconds: input.expires_seconds,
            content_type: input.content_type,
            content_length: input.content_length,
        };
        let authorization = authorize_object_request(
            headers,
            context.registry,
            context.auth,
            context.store,
            context.commercial,
            ObjectPolicyRequest {
                tenant_id: &request.tenant_id,
                storage_namespace: &request.storage_namespace,
                customer_id_ref: &customer_id,
                subscription_id_ref: &subscription_id,
                company_id_ref: company_id.as_deref(),
                branch_id_ref: branch_id.as_deref(),
                object_key: &request.object_key,
                permission: "documents.object.write",
                correlation_id,
                audit_read: false,
            },
        )
        .await;
        let signed = match context
            .storage
            .presign_upload_request(request, move |_| async move { authorization })
            .await
        {
            Ok(signed) => signed,
            Err(error) => return object_error_response(error),
        };
        return (
            "200 OK",
            serde_json::json!({
                "url": signed.url,
                "method": signed.method,
                "expires_at": signed.expires_at.to_rfc3339(),
            })
            .to_string(),
        );
    }
    let Ok(input) = serde_json::from_slice::<ObjectDownloadBody>(body) else {
        return (
            "400 Bad Request",
            serde_json::json!({"error":"invalid_json"}).to_string(),
        );
    };
    let customer_id = input.customer_id.clone();
    let subscription_id = input.subscription_id.clone();
    let company_id = input.company_id.clone();
    let branch_id = input.branch_id.clone();
    let request = TenantObjectDownloadRequest {
        tenant_id: input.tenant_id,
        storage_namespace: input.storage_namespace,
        object_key: input.object_key,
        expires_seconds: input.expires_seconds,
        content_type: input.content_type,
        max_bytes: input.max_bytes,
    };
    let authorization = authorize_object_request(
        headers,
        context.registry,
        context.auth,
        context.store,
        context.commercial,
        ObjectPolicyRequest {
            tenant_id: &request.tenant_id,
            storage_namespace: &request.storage_namespace,
            customer_id_ref: &customer_id,
            subscription_id_ref: &subscription_id,
            company_id_ref: company_id.as_deref(),
            branch_id_ref: branch_id.as_deref(),
            object_key: &request.object_key,
            permission: "documents.object.read",
            correlation_id,
            audit_read: true,
        },
    )
    .await;
    let signed = match context
        .storage
        .presign_download_request(request, move |_| async move { authorization })
        .await
    {
        Ok(signed) => signed,
        Err(error) => return object_error_response(error),
    };
    (
        "200 OK",
        serde_json::json!({
            "url": signed.url,
            "method": signed.method,
            "expires_at": signed.expires_at.to_rfc3339(),
        })
        .to_string(),
    )
}

async fn authorize_object_request(
    headers: &HashMap<String, String>,
    registry: &SqlxTenantStoreRegistry,
    auth: &AuthContext,
    store: &SqlxControlPlane,
    commercial: &SqlxCommercialStore,
    policy: ObjectPolicyRequest<'_>,
) -> Result<(), ObjectAuthorizationError> {
    let token = headers
        .get("x-erpx-access-token")
        .filter(|value| !value.is_empty())
        .ok_or(ObjectAuthorizationError::Denied)?;
    let actor = headers
        .get("x-erpx-actor")
        .filter(|value| !value.is_empty())
        .ok_or(ObjectAuthorizationError::Denied)?;
    let session = headers
        .get("x-erpx-session")
        .filter(|value| !value.is_empty())
        .ok_or(ObjectAuthorizationError::Denied)?;
    let mut request = Request::new(());
    request.metadata_mut().insert(
        "x-erpx-access-token",
        tonic::metadata::MetadataValue::try_from(token.as_str())
            .map_err(|_| ObjectAuthorizationError::Denied)?,
    );
    request.metadata_mut().insert(
        "x-erpx-actor",
        tonic::metadata::MetadataValue::try_from(actor.as_str())
            .map_err(|_| ObjectAuthorizationError::Denied)?,
    );
    request.metadata_mut().insert(
        "x-erpx-session",
        tonic::metadata::MetadataValue::try_from(session.as_str())
            .map_err(|_| ObjectAuthorizationError::Denied)?,
    );
    let claims = auth
        .authorize(&request)
        .await
        .map_err(|_| ObjectAuthorizationError::Denied)?;
    let Some(registration) = registry
        .get(policy.tenant_id)
        .await
        .map_err(|_| ObjectAuthorizationError::Denied)?
    else {
        return Err(ObjectAuthorizationError::Denied);
    };
    if registration.state != "active" || registration.storage_namespace != policy.storage_namespace
    {
        return Err(ObjectAuthorizationError::Denied);
    }
    let holding_id =
        Uuid::parse_str(&registration.holding_ref).map_err(|_| ObjectAuthorizationError::Denied)?;
    let customer_id =
        Uuid::parse_str(policy.customer_id_ref).map_err(|_| ObjectAuthorizationError::Denied)?;
    let subscription_id = Uuid::parse_str(policy.subscription_id_ref)
        .map_err(|_| ObjectAuthorizationError::Denied)?;
    let company_id = policy
        .company_id_ref
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| ObjectAuthorizationError::Denied)?;
    let branch_id = policy
        .branch_id_ref
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| ObjectAuthorizationError::Denied)?;
    if branch_id.is_some() && company_id.is_none() {
        return Err(ObjectAuthorizationError::Denied);
    }
    let scope = match (company_id, branch_id) {
        (None, None) => Scope::Holding { holding_id },
        (Some(company_id), None) => Scope::Company {
            holding_id,
            company_id,
        },
        (Some(company_id), Some(branch_id)) => Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        },
        (None, Some(_)) => return Err(ObjectAuthorizationError::Denied),
    };
    if !store
        .customer_scope_contains(customer_id, &scope)
        .await
        .map_err(|_| ObjectAuthorizationError::Denied)?
    {
        return Err(ObjectAuthorizationError::Denied);
    }
    let at = Utc::now();
    let direct_role = auth
        .authorize_role(store, &claims.principal_ref, &scope, policy.permission)
        .await
        .is_ok();
    let support_capability_id = headers
        .get("x-erpx-support-capability-id")
        .filter(|value| !value.is_empty())
        .map(|value| Uuid::parse_str(value))
        .transpose()
        .map_err(|_| ObjectAuthorizationError::Denied)?;
    let support_material = if let Some(capability_id) = support_capability_id {
        auth.authorize_role(
            store,
            &claims.principal_ref,
            &Scope::Platform,
            "support.session.request",
        )
        .await
        .map_err(|_| ObjectAuthorizationError::Denied)?;
        let session_id = headers
            .get("x-erpx-support-session-id")
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or(ObjectAuthorizationError::Denied)?;
        let ticket_id = headers
            .get("x-erpx-support-ticket-id")
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or(ObjectAuthorizationError::Denied)?;
        let device_ref = headers
            .get("x-erpx-support-device-ref")
            .filter(|value| !value.is_empty())
            .ok_or(ObjectAuthorizationError::Denied)?;
        let active_device_id = auth
            .identity
            .active_session_device_id(claims.session_id, at)
            .await
            .map_err(|_| ObjectAuthorizationError::Denied)?;
        if device_ref.as_str() != active_device_id.to_string() {
            return Err(ObjectAuthorizationError::Denied);
        }
        let mode = headers
            .get("x-erpx-support-mode")
            .filter(|value| !value.is_empty())
            .ok_or(ObjectAuthorizationError::Denied)?;
        let capability_scope = headers
            .get("x-erpx-support-scope")
            .filter(|value| !value.is_empty())
            .ok_or(ObjectAuthorizationError::Denied)?;
        let approved_scope =
            parse_support_scope(capability_scope).map_err(|_| ObjectAuthorizationError::Denied)?;
        if !approved_scope.contains(&scope) {
            return Err(ObjectAuthorizationError::Denied);
        }
        let secret_hex = headers
            .get("x-erpx-support-capability-secret")
            .cloned()
            .ok_or(ObjectAuthorizationError::Denied)?;
        let capability_secret = SupportCapabilitySecret::from_hex(&secret_hex)
            .map_err(|_| ObjectAuthorizationError::Denied)?;
        let session = store
            .get_session(session_id)
            .await
            .map_err(|_| ObjectAuthorizationError::Denied)?
            .ok_or(ObjectAuthorizationError::Denied)?;
        let capability = store
            .load_support_capability(capability_id)
            .await
            .map_err(|_| ObjectAuthorizationError::Denied)?
            .ok_or(ObjectAuthorizationError::Denied)?;
        let persisted_authorization = store
            .authorize_support_capability(&SupportCapabilityAuthorizationRequest {
                capability_id,
                customer_account_id: customer_id,
                agent_ref: claims.principal_ref.clone(),
                device_ref: device_ref.to_owned(),
                session_id,
                ticket_id,
                mode: mode.to_owned(),
                permission: policy.permission.to_owned(),
                scope: capability_scope.to_owned(),
                module: "documents".into(),
                secret_hex,
                at,
            })
            .await
            .map_err(|_| ObjectAuthorizationError::Denied)?;
        if !persisted_authorization {
            return Err(ObjectAuthorizationError::Denied);
        }
        let approved_scope =
            parse_support_scope(&session.scope).map_err(|_| ObjectAuthorizationError::Denied)?;
        let canonical_scope = approved_scope.canonical_key();
        let mut evaluation_session = session;
        evaluation_session.scope = canonical_scope.clone();
        let mut evaluation_capability = capability;
        evaluation_capability.binding.scope = canonical_scope;
        Some((
            evaluation_session,
            evaluation_capability,
            capability_secret,
            approved_scope,
        ))
    } else {
        None
    };
    let support_active = support_material.is_some();
    if !direct_role && !support_active {
        return Err(ObjectAuthorizationError::Denied);
    }
    let Some((lifecycle, _)) = commercial
        .get_billing_lifecycle(customer_id, subscription_id)
        .await
        .map_err(|_| ObjectAuthorizationError::Denied)?
    else {
        return Err(ObjectAuthorizationError::Denied);
    };
    let Some((book, _)) = commercial
        .get_entitlement_book(customer_id, "documents")
        .await
        .map_err(|_| ObjectAuthorizationError::Denied)?
    else {
        return Err(ObjectAuthorizationError::Denied);
    };
    let role_permission = if support_active {
        "support.session.request"
    } else {
        policy.permission
    };
    let role_scope = if support_active {
        Scope::Platform
    } else {
        scope.clone()
    };
    let Some((assignment, parents)) = store
        .effective_role_assignment_chain(&claims.principal_ref, &role_scope, role_permission, at)
        .await
        .map_err(|_| ObjectAuthorizationError::Denied)?
    else {
        return Err(ObjectAuthorizationError::Denied);
    };
    if !store
        .sod_decision_active(&claims.principal_ref, "documents", &role_scope, at)
        .await
        .map_err(|_| ObjectAuthorizationError::Denied)?
    {
        return Err(ObjectAuthorizationError::Denied);
    }
    let is_mutation = policy.permission.ends_with(".write");
    evaluate_effective_access_chain(
        &assignment,
        &parents,
        &EffectiveAccessRequest {
            principal_ref: claims.principal_ref.clone(),
            at,
            requested_scope: scope.clone(),
            module_key: "documents".into(),
            required_permission: role_permission.into(),
            requested_approval_minor: 0,
            approval_limit_minor: assignment.approval_limit_minor,
            identity_active: true,
            session_active: true,
            billing_state: lifecycle.state,
            entitlement: Some(book.module_entitlement()),
            entitlement_book: Some(book.clone()),
            is_mutation,
            segregation_of_duties_passed: true,
            support_capability_required: support_active,
            support_capability_active: support_active,
        },
    )
    .map_err(|_| ObjectAuthorizationError::Denied)?;
    if let Some((session, capability, capability_secret, approved_scope)) = &support_material {
        let agent_role = store
            .effective_role_assignment(
                &claims.principal_ref,
                &Scope::Platform,
                "support.session.request",
                at,
            )
            .await
            .map_err(|_| ObjectAuthorizationError::Denied)?
            .ok_or(ObjectAuthorizationError::Denied)?;
        let represented_user_role =
            session
                .represented_tenant_user
                .as_deref()
                .map(|principal_ref| async {
                    store
                        .effective_role_assignment(
                            principal_ref,
                            approved_scope,
                            policy.permission,
                            at,
                        )
                        .await
                        .map_err(|_| ObjectAuthorizationError::Denied)
                });
        let represented_user_role = match represented_user_role {
            Some(future) => future.await?.map(|(assignment, _)| assignment),
            None => None,
        };
        let domain_policy = SupportDomainPolicy::new(
            "support.session.request",
            BTreeSet::from([String::from("documents")]),
            matches!(session.mode, SupportMode::DiagnosticRead),
            matches!(session.mode, SupportMode::TenantMutation),
            matches!(session.mode, SupportMode::Impersonation),
            store
                .sod_decision_active(&claims.principal_ref, "documents", &Scope::Platform, at)
                .await
                .map_err(|_| ObjectAuthorizationError::Denied)?,
        );
        let scope_key = approved_scope.canonical_key();
        let intersection = SupportIntersectionRequest {
            at,
            agent_identity_ref: &claims.principal_ref,
            agent_identity_active: true,
            agent_device_ref: &capability.binding.device_ref,
            agent_device_active: true,
            session,
            capability,
            capability_secret,
            agent_role: &agent_role.0,
            agent_parent_role: agent_role.1.as_ref(),
            represented_user_identity_active: represented_user_role.is_some(),
            represented_user_role: represented_user_role.as_ref(),
            represented_user_parent_role: None,
            requested_scope: approved_scope,
            scope_key: &scope_key,
            module_key: "documents",
            required_permission: policy.permission,
            billing_state: lifecycle.state,
            entitlement: Some(&book.module_entitlement()),
            domain_policy: &domain_policy,
            is_mutation,
        };
        SupportIntersectionService
            .authorize(&intersection)
            .map_err(|_| ObjectAuthorizationError::Denied)?;
    }
    let entitlement_decision = match scope {
        Scope::Holding { .. } => authorize_module(
            lifecycle.state,
            Some(&book.module_entitlement()),
            true,
            at,
            is_mutation,
        ),
        Scope::Company { company_id, .. } => {
            book.authorize(lifecycle.state, company_id, None, true, at, is_mutation)
        }
        Scope::Branch {
            company_id,
            branch_id,
            ..
        } => book.authorize(
            lifecycle.state,
            company_id,
            Some(branch_id),
            true,
            at,
            is_mutation,
        ),
        Scope::Platform | Scope::Subtree { .. } => {
            EntitlementDecision::Denied("invalid_object_scope")
        }
    };
    if !matches!(entitlement_decision, EntitlementDecision::Allowed) {
        return Err(ObjectAuthorizationError::Denied);
    }
    if !store
        .authorize_object_binding(&ObjectAccessBindingRequest {
            tenant_id: policy.tenant_id,
            storage_namespace: policy.storage_namespace,
            object_key: policy.object_key,
            customer_id,
            scope: &scope,
            actor_ref: &claims.principal_ref,
            for_write: is_mutation,
        })
        .await
        .map_err(|_| ObjectAuthorizationError::Denied)?
    {
        return Err(ObjectAuthorizationError::Denied);
    }
    if policy.audit_read {
        let purpose = headers
            .get("x-erpx-read-purpose")
            .filter(|value| !value.trim().is_empty())
            .ok_or(ObjectAuthorizationError::Denied)?;
        if purpose.len() > 128 || purpose.chars().any(char::is_control) {
            return Err(ObjectAuthorizationError::Denied);
        }
        let audit = GlobalReadAuditRecord::new(
            GlobalReadActor {
                principal_ref: claims.principal_ref,
                session_ref: Some(session.to_owned()),
            },
            customer_id,
            "documents.object",
            GlobalReadScope {
                label: support_scope_reference(&scope),
                holding_id: Some(holding_id),
                company_id,
                branch_id,
            },
            purpose,
            at,
            policy.correlation_id,
        )
        .map_err(|_| ObjectAuthorizationError::Denied)?;
        store
            .record_global_read(&audit)
            .await
            .map_err(|_| ObjectAuthorizationError::Denied)?;
    }
    Ok(())
}

fn object_error_response(error: ObjectAccessError) -> (&'static str, String) {
    let (status, code) = match error {
        ObjectAccessError::InvalidRequest(_) => ("400 Bad Request", "invalid_object_request"),
        ObjectAccessError::Unauthorized => ("403 Forbidden", "object_access_denied"),
        ObjectAccessError::StorageUnavailable => {
            ("503 Service Unavailable", "object_storage_unavailable")
        }
        ObjectAccessError::PresignFailed => {
            ("503 Service Unavailable", "object_storage_unavailable")
        }
    };
    (status, serde_json::json!({"error":code}).to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StripeWebhookError {
    Configuration,
    Invalid(&'static str),
    Persistence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarketingCallbackHttpError {
    Configuration,
    Invalid(&'static str),
    Persistence,
}

impl MarketingCallbackHttpError {
    const fn message(self) -> &'static str {
        match self {
            Self::Configuration => "Marketing callback handling is not configured",
            Self::Invalid(message) => message,
            Self::Persistence => "Marketing callback persistence is unavailable",
        }
    }
}

#[derive(Debug, Deserialize)]
struct MarketingProviderCallbackBody {
    delivery_id: String,
    provider_message_ref: String,
    provider_event_id: String,
    provider_event_at: String,
    kind: String,
}

async fn accept_marketing_provider_callback(
    body: &[u8],
    headers: &HashMap<String, String>,
    control: &SqlxControlPlane,
    router: Option<&TenantDatabaseRouter>,
    secret_resolver: &dyn MarketingCallbackSecretResolver,
    correlation_id: Uuid,
) -> Result<erpx_infrastructure::MarketingProviderCallbackReceipt, MarketingCallbackHttpError> {
    let router = router.ok_or(MarketingCallbackHttpError::Configuration)?;
    let provider_account_ref = headers
        .get("x-erpx-marketing-provider-account")
        .filter(|value| !value.trim().is_empty())
        .ok_or(MarketingCallbackHttpError::Invalid(
            "marketing provider account is required",
        ))?;
    let binding = control
        .resolve_marketing_provider_account_for_callback(provider_account_ref)
        .await
        .map_err(|_| MarketingCallbackHttpError::Persistence)?
        .ok_or(MarketingCallbackHttpError::Invalid(
            "marketing provider account binding is not active",
        ))?;
    let signature =
        headers
            .get("x-erpx-marketing-signature")
            .ok_or(MarketingCallbackHttpError::Invalid(
                "marketing callback signature is required",
            ))?;
    let secret_reference = MarketingCallbackSecretReference::new(&binding.verifier_key_ref)
        .map_err(|_| MarketingCallbackHttpError::Configuration)?;
    {
        let secret = secret_resolver
            .resolve(&secret_reference)
            .await
            .map_err(|_| MarketingCallbackHttpError::Configuration)?;
        let signature_result = secret.with_material(|material| {
            verify_marketing_callback_signature(body, signature, material, Utc::now())
        });
        signature_result.map_err(MarketingCallbackHttpError::Invalid)?;
    }
    let input: MarketingProviderCallbackBody = serde_json::from_slice(body)
        .map_err(|_| MarketingCallbackHttpError::Invalid("marketing callback JSON is invalid"))?;
    let delivery_id = Uuid::parse_str(&input.delivery_id)
        .map_err(|_| MarketingCallbackHttpError::Invalid("marketing delivery ID is invalid"))?;
    let write = router
        .route(
            &binding.tenant_id,
            &binding.holding_ref,
            RoutedDatabaseSide::Write,
        )
        .await
        .map_err(|_| MarketingCallbackHttpError::Persistence)?;
    let read = match router
        .route(
            &binding.tenant_id,
            &binding.holding_ref,
            RoutedDatabaseSide::Read,
        )
        .await
    {
        Ok(read) => read,
        Err(_) => {
            write.close().await;
            return Err(MarketingCallbackHttpError::Persistence);
        }
    };
    let store = match SqlxMarketingDeliveryStore::from_pools(
        write.database_pool(),
        read.database_pool(),
        binding.holding_ref.clone(),
    ) {
        Ok(store) => store,
        Err(_) => {
            write.close().await;
            read.close().await;
            return Err(MarketingCallbackHttpError::Persistence);
        }
    };
    let dispatch = match store.load_dispatch(delivery_id).await {
        Ok(dispatch) => dispatch,
        Err(_) => {
            write.close().await;
            read.close().await;
            return Err(MarketingCallbackHttpError::Invalid(
                "marketing delivery is not routable",
            ));
        }
    };
    let dispatch_holding_id = match &dispatch.scope {
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => Some(*holding_id),
        TenantScope::Platform => None,
    };
    if dispatch_holding_id != Some(binding.holding_id) {
        write.close().await;
        read.close().await;
        return Err(MarketingCallbackHttpError::Invalid(
            "marketing delivery scope is not bound to the provider account",
        ));
    }
    let scope = dispatch.scope.clone();
    let callback_binding = MarketingProviderCallbackBinding::new(
        provider_account_ref.clone(),
        binding.tenant_id.clone(),
        binding.holding_id,
        scope,
    )
    .map_err(|_| MarketingCallbackHttpError::Invalid("marketing callback binding is invalid"))?;
    let provider_event_at = DateTime::parse_from_rfc3339(&input.provider_event_at)
        .map_err(|_| {
            MarketingCallbackHttpError::Invalid("marketing callback timestamp is invalid")
        })?
        .with_timezone(&Utc);
    let kind = match input.kind.as_str() {
        "delivered" => MarketingProviderCallbackKind::Delivered,
        "failed" => MarketingProviderCallbackKind::Failed,
        "bounced" => MarketingProviderCallbackKind::Bounced,
        _ => {
            return Err(MarketingCallbackHttpError::Invalid(
                "marketing callback kind is invalid",
            ));
        }
    };
    let authentication = VerifiedProviderAuthentication::new(
        "hmac_sha256",
        binding.verifier_key_ref.clone(),
        boundary::raw_body_digest(body),
    )
    .map_err(|_| {
        MarketingCallbackHttpError::Invalid("marketing callback authentication is invalid")
    })?;
    let request = MarketingProviderCallbackRequest::new(
        callback_binding.clone(),
        delivery_id,
        input.provider_message_ref,
        input.provider_event_id,
        provider_event_at,
        kind,
        ProviderAuthenticationDecision::Verified(authentication),
    )
    .map_err(|_| MarketingCallbackHttpError::Invalid("marketing callback fields are invalid"))?;
    let callback = request.verify(&callback_binding, Utc::now()).map_err(|_| {
        MarketingCallbackHttpError::Invalid("marketing callback verification failed")
    })?;
    let result = store
        .accept_provider_callback(&callback, Utc::now(), correlation_id)
        .await
        .map_err(|_| MarketingCallbackHttpError::Persistence);
    write.close().await;
    read.close().await;
    result
}

fn verify_marketing_callback_signature(
    raw_body: &[u8],
    header: &str,
    secret: &[u8],
    now: DateTime<Utc>,
) -> Result<(), &'static str> {
    let mut timestamp = None;
    let mut signatures = Vec::new();
    for item in header.split(',') {
        let (key, value) = item
            .split_once('=')
            .ok_or("marketing callback signature format is invalid")?;
        match key.trim() {
            "t" => {
                timestamp = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| "marketing callback timestamp is invalid")?,
                );
            }
            "v1" => signatures.push(
                hex::decode(value.trim())
                    .map_err(|_| "marketing callback signature digest is invalid")?,
            ),
            _ => {}
        }
    }
    let timestamp = timestamp.ok_or("marketing callback signature timestamp is required")?;
    if timestamp < (now - Duration::minutes(5)).timestamp()
        || timestamp >= (now + Duration::minutes(5)).timestamp()
    {
        return Err("marketing callback signature is outside its time window");
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(secret)
        .map_err(|_| "marketing callback secret is invalid")?;
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(raw_body);
    if signatures
        .iter()
        .any(|signature| mac.clone().verify_slice(signature).is_ok())
    {
        Ok(())
    } else {
        Err("marketing callback signature verification failed")
    }
}

impl StripeWebhookError {
    const fn message(self) -> &'static str {
        match self {
            Self::Configuration => "Stripe webhook handling is not configured",
            Self::Invalid(message) => message,
            Self::Persistence => "Stripe webhook persistence is unavailable",
        }
    }
}

async fn accept_stripe_webhook(
    body: &[u8],
    headers: &HashMap<String, String>,
    commercial: &SqlxCommercialStore,
    correlation_id: Uuid,
) -> Result<StripeInboxInsert, StripeWebhookError> {
    let secret = std::env::var("ERPX_STRIPE_WEBHOOK_SECRET")
        .map_err(|_| StripeWebhookError::Configuration)?;
    let signature = headers
        .get("stripe-signature")
        .ok_or(StripeWebhookError::Invalid(
            "Stripe-Signature header is required",
        ))?;
    let webhook =
        parse_verified_stripe_webhook(body, signature, secret.as_bytes(), correlation_id)?;
    commercial
        .insert_verified_stripe_webhook(webhook)
        .await
        .map_err(|_| StripeWebhookError::Persistence)
}

fn parse_verified_stripe_webhook(
    body: &[u8],
    signature: &str,
    secret: &[u8],
    correlation_id: Uuid,
) -> Result<StripeWebhookEvent, StripeWebhookError> {
    verify_stripe_signature(body, signature, secret, Utc::now())
        .map_err(StripeWebhookError::Invalid)?;
    let persistence_context = StripeWebhookPersistenceContext::from_raw_body(body, correlation_id)
        .map_err(|_| StripeWebhookError::Persistence)?;
    StripeWebhookEvent::from_verified_raw_body(body, persistence_context)
        .map_err(|_| StripeWebhookError::Invalid("Stripe webhook JSON is invalid or incomplete"))
}

fn verify_stripe_signature(
    raw_body: &[u8],
    header: &str,
    secret: &[u8],
    now: DateTime<Utc>,
) -> Result<(), &'static str> {
    let mut timestamp = None;
    let mut signatures = Vec::new();
    for item in header.split(',') {
        let (key, value) = item
            .split_once('=')
            .ok_or("Stripe signature format is invalid")?;
        match key.trim() {
            "t" => {
                timestamp = Some(
                    value
                        .trim()
                        .parse::<i64>()
                        .map_err(|_| "Stripe signature timestamp is invalid")?,
                );
            }
            "v1" => signatures
                .push(hex::decode(value.trim()).map_err(|_| "Stripe signature digest is invalid")?),
            _ => {}
        }
    }
    let timestamp = timestamp.ok_or("Stripe signature timestamp is missing")?;
    if signatures.is_empty() {
        return Err("Stripe signature digest is missing");
    }
    let age = now.timestamp().abs_diff(timestamp);
    if age > 300 {
        return Err("Stripe signature timestamp is outside the five-minute tolerance");
    }
    let mut mac = <ApprovalMac as Mac>::new_from_slice(secret)
        .map_err(|_| "Stripe webhook secret is invalid")?;
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(raw_body);
    if signatures
        .iter()
        .any(|signature| mac.clone().verify_slice(signature).is_ok())
    {
        Ok(())
    } else {
        Err("Stripe signature verification failed")
    }
}

async fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
    correlation_id: Uuid,
    cors_origin: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let response_body = errors::http_body(status, body, correlation_id);
    let cors_headers = cors_origin.map_or_else(String::new, |origin| {
        format!(
            "Access-Control-Allow-Origin: {origin}\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Cache-Control, Content-Type, X-Erpx-Access-Token, X-Erpx-Correlation-Id\r\nAccess-Control-Expose-Headers: X-Correlation-Id\r\nAccess-Control-Max-Age: 600\r\nVary: Origin\r\n"
        )
    });
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nX-Correlation-Id: {correlation_id}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n{cors_headers}Connection: close\r\n\r\n{response_body}",
        response_body.len(),
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}

const DEFAULT_HTTP_CORS_ORIGINS: &[&str] = &[
    "http://127.0.0.1:8080",
    "http://localhost:8080",
    "http://127.0.0.1:8081",
    "http://localhost:8081",
    "http://127.0.0.1:3005",
    "http://localhost:3005",
];

fn http_cors_origin(headers: &HashMap<String, String>) -> Option<String> {
    let origin = headers.get("origin")?.trim();
    if origin.is_empty()
        || origin.len() > 256
        || origin.chars().any(|character| character.is_control())
    {
        return None;
    }
    let configured = std::env::var("ERPX_HTTP_ALLOWED_ORIGINS").ok();
    let allowed = configured.as_deref().map_or_else(
        || DEFAULT_HTTP_CORS_ORIGINS.contains(&origin),
        |origins| origins.split(',').any(|value| value.trim() == origin),
    );
    allowed.then(|| origin.to_owned())
}

fn grpc_web_cors_layer() -> CorsLayer {
    let configured = std::env::var("ERPX_HTTP_ALLOWED_ORIGINS")
        .ok()
        .map(|origins| {
            origins
                .split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        });
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _| {
            let Ok(origin) = origin.to_str() else {
                return false;
            };
            configured.as_ref().map_or_else(
                || DEFAULT_HTTP_CORS_ORIGINS.contains(&origin),
                |origins| origins.iter().any(|allowed| allowed == origin),
            )
        }))
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any)
}

async fn read_http_request(
    stream: &mut TcpStream,
) -> Result<
    (String, String, HashMap<String, String>, Vec<u8>),
    Box<dyn std::error::Error + Send + Sync>,
> {
    const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
    const MAX_REQUEST_BYTES: usize = MAX_BODY_BYTES + (32 * 1024);
    let mut raw = Vec::with_capacity(4096);
    let mut header_end = None;
    let mut content_length = 0_usize;
    loop {
        let mut chunk = [0_u8; 2048];
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..read]);
        if raw.len() > MAX_REQUEST_BYTES {
            return Err("HTTP request too large".into());
        }
        if header_end.is_none()
            && let Some(position) = raw.windows(4).position(|window| window == b"\r\n\r\n")
        {
            let end = position + 4;
            let header = std::str::from_utf8(&raw[..position])?;
            for line in header.lines().skip(1) {
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    content_length = value.trim().parse()?;
                    if content_length > MAX_BODY_BYTES {
                        return Err("HTTP body too large".into());
                    }
                }
            }
            header_end = Some(end);
        }
        if let Some(end) = header_end
            && raw.len() >= end + content_length
        {
            break;
        }
    }
    let header_end = header_end.ok_or("HTTP headers are incomplete")?;
    if raw.len() < header_end + content_length {
        return Err("HTTP body is incomplete".into());
    }
    let header = std::str::from_utf8(&raw[..header_end - 4])?;
    let mut lines = header.lines();
    let request_line = lines.next().ok_or("HTTP request line is missing")?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or("HTTP method is missing")?
        .to_owned();
    let path = request_parts
        .next()
        .ok_or("HTTP path is missing")?
        .to_owned();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    Ok((
        method,
        path,
        headers,
        raw[header_end..header_end + content_length].to_vec(),
    ))
}

type PasskeyHttpService = PasskeyCeremonyService<Arc<dyn WebAuthnProvider>>;

fn validate_passkey_configuration(
    provider: Option<&str>,
    required: Option<&str>,
) -> Result<(), &'static str> {
    if provider.is_some_and(|value| !matches!(value, "disabled" | "webauthn-rs")) {
        return Err("ERPX_WEBAUTHN_PROVIDER must be disabled or webauthn-rs");
    }
    match required {
        None | Some("false") => Ok(()),
        Some("true") if provider == Some("webauthn-rs") => Ok(()),
        Some("true") => Err("ERPX_WEBAUTHN_REQUIRED needs a live WebAuthn provider"),
        Some(_) => Err("ERPX_WEBAUTHN_REQUIRED must be true or false"),
    }
}

fn configure_passkey_service(
    identity: SqlxIdentityStore,
) -> Result<Option<Arc<PasskeyHttpService>>, Box<dyn std::error::Error>> {
    let provider = match std::env::var("ERPX_WEBAUTHN_PROVIDER") {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(_) => return Err("ERPX_WEBAUTHN_PROVIDER could not be read".into()),
    };
    let required = match std::env::var("ERPX_WEBAUTHN_REQUIRED") {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(_) => return Err("ERPX_WEBAUTHN_REQUIRED could not be read".into()),
    };
    if let Err(message) = validate_passkey_configuration(provider.as_deref(), required.as_deref()) {
        return Err(message.into());
    }
    if provider.as_deref() != Some("webauthn-rs") {
        tracing::warn!("WebAuthn passkey routes are disabled by configuration");
        return Ok(None);
    }
    let rp_id = std::env::var("ERPX_WEBAUTHN_RP_ID")
        .map_err(|_| "ERPX_WEBAUTHN_RP_ID is required for webauthn-rs")?;
    let origins = std::env::var("ERPX_WEBAUTHN_ORIGINS")
        .map_err(|_| "ERPX_WEBAUTHN_ORIGINS is required for webauthn-rs")?
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let config = WebAuthnProviderConfig::try_new(rp_id, origins)?;
    let provider = WebAuthnRsProvider::try_new(config)?;
    Ok(Some(Arc::new(PasskeyCeremonyService::new(
        Arc::new(provider),
        identity,
    ))))
}

fn configure_totp_key_resolver() -> Result<Option<Arc<TotpKeyResolver>>, Box<dyn std::error::Error>>
{
    match std::env::var("ERPX_IDENTITY_TOTP_KEY") {
        Ok(_) => Ok(Some(Arc::new(TotpKeyResolver::from_env()?))),
        Err(std::env::VarError::NotPresent) => {
            tracing::warn!(
                "TOTP HTTP routes are fail-closed because ERPX_IDENTITY_TOTP_KEY is not configured"
            );
            Ok(None)
        }
        Err(_) => Err("ERPX_IDENTITY_TOTP_KEY could not be read".into()),
    }
}

fn configure_tenant_router(
    store: Arc<SqlxControlPlane>,
) -> Result<Option<Arc<TenantDatabaseRouter>>, Box<dyn std::error::Error>> {
    let admin = std::env::var("ERPX_TENANT_ADMIN_DATABASE_URL").ok();
    let template = std::env::var("ERPX_TENANT_DATABASE_URL_TEMPLATE").ok();
    match (admin, template) {
        (None, None) => Ok(None),
        (Some(admin_database_url), Some(database_url_template))
            if !admin_database_url.trim().is_empty()
                && !database_url_template.trim().is_empty() =>
        {
            Ok(Some(Arc::new(TenantDatabaseRouter::new(
                store,
                TenantDatabaseProvisioningConfig {
                    admin_database_url,
                    database_url_template,
                },
            )?)))
        }
        _ => Err("tenant database routing configuration is incomplete".into()),
    }
}

fn configure_marketing_callback_secret_resolver()
-> Result<Arc<dyn MarketingCallbackSecretResolver>, Box<dyn std::error::Error>> {
    match std::env::var("ERPX_MARKETING_CALLBACK_SECRET_RESOLVER") {
        Ok(value) if value == "env" => Ok(Arc::new(EnvMarketingCallbackSecretResolver)),
        Ok(value) if value == "http" || value == "secret-manager" => Ok(Arc::new(
            HttpMarketingCallbackSecretManager::from_env()?,
        )),
        Ok(value) if value == "unconfigured" => {
            Ok(Arc::new(UnconfiguredMarketingCallbackSecretResolver))
        }
        Ok(_) => Err(
            "ERPX_MARKETING_CALLBACK_SECRET_RESOLVER must be env, http, secret-manager, or unconfigured"
                .into(),
        ),
        Err(std::env::VarError::NotPresent) => Ok(Arc::new(EnvMarketingCallbackSecretResolver)),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err("ERPX_MARKETING_CALLBACK_SECRET_RESOLVER is invalid".into())
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let storage = Arc::new(StorageComposition::from_env()?);
    let database_config = ControlPlaneDatabaseConfig::from_env()?;
    let pools = ControlPlanePools::connect(&database_config).await?;
    erpx_migration::migrate_control_plane(&pools.write).await?;
    erpx_migration::migrate_control_read(&pools.read).await?;
    apply_identity_migration(&pools.write).await?;
    let access_token_key = std::env::var("ERPX_ACCESS_TOKEN_SIGNING_KEY")
        .map_err(|_| "ERPX_ACCESS_TOKEN_SIGNING_KEY is required")?;
    let access_token_signer = Arc::new(AccessTokenSigner::new(access_token_key.as_bytes())?);
    let store = Arc::new(SqlxControlPlane::new(pools));
    let commercial = Arc::new(SqlxCommercialStore::new(&store));
    let tenant_router = configure_tenant_router(store.clone())?;
    let marketing_callback_secrets = configure_marketing_callback_secret_resolver()?;
    let support_persistence = Arc::new(SupportPersistence::new(store.pools().write.clone()));
    let identity = Arc::new(SqlxIdentityStore::new(store.pools().write.clone()));
    let passkeys = configure_passkey_service((*identity).clone())?;
    let totp_keys = configure_totp_key_resolver()?;
    let auth = Arc::new(AuthContext {
        identity,
        directory: Arc::new(SqlxDirectoryStore::new(store.pools().write.clone())),
        signer: access_token_signer.clone(),
        passkeys,
        totp_keys,
    });
    let audit: Arc<dyn erpx_application::AuditSink> = store.clone();
    let clock = Arc::new(SystemClock);
    let state = AppState {
        auth: auth.clone(),
        store: store.clone(),
        commercial: commercial.clone(),
        tenant_router,
        storage: storage.clone(),
        support_persistence,
        customers: Arc::new(PlatformCustomerService::new(store.clone(), audit.clone())),
        authoritative_support: Arc::new(AuthoritativeSupportService::new(
            store.clone(),
            clock.clone(),
            store.clone(),
        )),
        roles: Arc::new(RoleLifecycleUseCase::new(store.clone(), clock, audit)),
        authoritative_roles: Arc::new(AuthoritativeRoleLifecycleService::new(
            store.clone(),
            Arc::new(SystemClock),
            store.clone(),
        )),
        platform_completion: Arc::new(PlatformCompletionApi::new(Arc::new(
            PlatformControlStore::new(store.clone()),
        ))),
        migration: Arc::new(SqlxG13MigrationStore::new(store.clone())),
    };
    let http_addr: SocketAddr = std::env::var("ERPX_HTTP_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50052".into())
        .parse()?;
    let tenant_registry = Arc::new(SqlxTenantStoreRegistry::new(state.store.clone()));
    let readiness = Arc::new(ServerReadinessAdapter::from_env(
        state.store.clone(),
        tenant_registry.clone(),
    ));
    let http_listener = TcpListener::bind(http_addr).await?;
    tokio::spawn(serve_http_health(
        http_listener,
        HttpServiceContext {
            store: state.store.clone(),
            registry: tenant_registry,
            auth: state.auth.clone(),
            commercial: state.commercial.clone(),
            tenant_router: state.tenant_router.clone(),
            storage: state.storage.clone(),
            readiness,
            marketing_callback_secrets,
        },
    ));
    tracing::info!(%http_addr, "ERPX HTTP health and metrics listening");
    let address = std::env::var("ERPX_GRPC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50051".into())
        .parse()?;
    tracing::info!(%address, "ERPX Tonic server listening");
    tonic::transport::Server::builder()
        .accept_http1(true)
        .layer(tonic::service::InterceptorLayer::new(move |request| {
            transport_interceptor(request, access_token_signer.as_ref())
        }))
        .layer(grpc_web_cors_layer())
        .layer(tonic_web::GrpcWebLayer::new())
        .add_service(PlatformCustomerServiceServer::new(CustomerRpcImpl {
            state: state.clone(),
        }))
        .add_service(PlatformOperationsServiceServer::new(
            PlatformOperationsRpcImpl {
                state: state.clone(),
            },
        ))
        .add_service(MigrationServiceServer::new(migration::MigrationRpcImpl {
            state: state.clone(),
        }))
        .add_service(DirectoryServiceServer::new(DirectoryRpcImpl {
            state: state.clone(),
        }))
        .add_service(OrganizationServiceServer::new(OrganizationRpcImpl {
            state: state.clone(),
        }))
        .add_service(OperationsServiceServer::new(OperationsRpcImpl {
            state: state.clone(),
        }))
        .add_service(SupportServiceServer::new(SupportRpcImpl {
            state: state.clone(),
        }))
        .add_service(RoleLifecycleServiceServer::new(RoleRpcImpl {
            state: state.clone(),
        }))
        .add_service(EntitlementServiceServer::new(EntitlementRpcImpl {
            auth: auth.clone(),
            store: store.clone(),
            commercial: commercial.clone(),
        }))
        .add_service(FinanceServiceServer::new(FinanceRpcImpl {
            state: state.clone(),
        }))
        .add_service(HrServiceServer::new(HrRpcImpl {
            state: state.clone(),
        }))
        .add_service(PosServiceServer::new(PosRpcImpl {
            state: state.clone(),
        }))
        .add_service(SalesServiceServer::new(SalesRpcImpl {
            state: state.clone(),
        }))
        .add_service(ScmServiceServer::new(ScmRpcImpl {
            state: state.clone(),
        }))
        .add_service(AssetMaintenanceServiceServer::new(
            AssetMaintenanceRpcImpl {
                state: state.clone(),
            },
        ))
        .add_service(QualityServiceServer::new(QualityRpcImpl {
            state: state.clone(),
        }))
        .add_service(LegalServiceServer::new(LegalRpcImpl {
            state: state.clone(),
        }))
        .add_service(CrmServiceServer::new(CrmRpcImpl {
            state: state.clone(),
        }))
        .add_service(MarketingContentServiceServer::new(
            MarketingContentRpcImpl {
                state: state.clone(),
            },
        ))
        .serve(address)
        .await?;
    Ok(())
}

#[cfg(test)]
mod stripe_webhook_tests {
    use super::*;

    fn signed_header(body: &[u8], secret: &[u8], now: DateTime<Utc>) -> String {
        let timestamp = now.timestamp();
        let mut mac = <ApprovalMac as Mac>::new_from_slice(secret).expect("HMAC key");
        mac.update(timestamp.to_string().as_bytes());
        mac.update(b".");
        mac.update(body);
        format!(
            "t={timestamp},v1={}",
            hex::encode(mac.finalize().into_bytes())
        )
    }

    #[test]
    fn stripe_signature_covers_raw_body_and_has_a_five_minute_window() {
        let now = Utc::now();
        let timestamp = now.timestamp();
        let body = br#"{"id":"evt_test","type":"invoice.paid"}"#;
        let secret = b"whsec_test_secret_that_is_long_enough";
        let mut mac = <ApprovalMac as Mac>::new_from_slice(secret).expect("HMAC key");
        mac.update(timestamp.to_string().as_bytes());
        mac.update(b".");
        mac.update(body);
        let signature = hex::encode(mac.finalize().into_bytes());
        let header = format!("t={timestamp},v1={signature}");

        assert!(verify_stripe_signature(body, &header, secret, now).is_ok());
        assert!(
            verify_stripe_signature(body, &header, secret, now + Duration::seconds(301)).is_err()
        );
        assert!(verify_stripe_signature(b"tampered", &header, secret, now).is_err());
    }

    #[test]
    fn stripe_signature_requires_a_v1_digest() {
        assert!(
            verify_stripe_signature(
                b"{}",
                &format!("t={},v0=deadbeef", Utc::now().timestamp()),
                b"secret",
                Utc::now(),
            )
            .is_err()
        );
    }

    #[test]
    fn verified_ingress_uses_the_typed_event_contract() {
        let now = Utc::now();
        let body = format!(
            r#"{{"id":"evt_server_ingress","type":"invoice.paid","created":{},"data":{{"object":{{"id":"in_test"}}}}}}"#,
            now.timestamp()
        );
        let secret = b"whsec_server_ingress_test_secret";
        let webhook = parse_verified_stripe_webhook(
            body.as_bytes(),
            &signed_header(body.as_bytes(), secret, now),
            secret,
            Uuid::new_v4(),
        )
        .expect("typed verified webhook");

        assert_eq!(webhook.event_id.as_str(), "evt_server_ingress");
        assert_eq!(webhook.event_type, "invoice.paid");
        assert_eq!(webhook.stripe_created_at.timestamp(), now.timestamp());
    }

    #[test]
    fn verified_ingress_rejects_an_incomplete_event_after_signature_verification() {
        let now = Utc::now();
        let body = format!(
            r#"{{"id":"evt_server_incomplete","type":"invoice.paid","created":{}}}"#,
            now.timestamp()
        );
        let secret = b"whsec_server_ingress_test_secret";
        let error = parse_verified_stripe_webhook(
            body.as_bytes(),
            &signed_header(body.as_bytes(), secret, now),
            secret,
            Uuid::new_v4(),
        )
        .expect_err("missing data.object must fail closed");

        assert_eq!(
            error,
            StripeWebhookError::Invalid("Stripe webhook JSON is invalid or incomplete")
        );
    }
}

#[cfg(test)]
mod marketing_callback_http_tests {
    use super::*;

    fn signed_header(body: &[u8], secret: &[u8], now: DateTime<Utc>) -> String {
        let timestamp = now.timestamp();
        let mut mac = <ApprovalMac as Mac>::new_from_slice(secret).expect("HMAC key");
        mac.update(timestamp.to_string().as_bytes());
        mac.update(b".");
        mac.update(body);
        format!(
            "t={timestamp},v1={}",
            hex::encode(mac.finalize().into_bytes())
        )
    }

    #[test]
    fn marketing_callback_signature_covers_the_exact_raw_body() {
        let now = Utc::now();
        let body = br#"{"delivery_id":"delivery-1","kind":"delivered"}"#;
        let secret = b"marketing_callback_test_secret";
        let header = signed_header(body, secret, now);

        assert!(verify_marketing_callback_signature(body, &header, secret, now).is_ok());
        assert!(verify_marketing_callback_signature(b"tampered", &header, secret, now).is_err());
    }

    #[test]
    fn marketing_callback_signature_requires_v1_and_a_bounded_timestamp() {
        let now = Utc::now();
        let body = br#"{}"#;
        let secret = b"marketing_callback_test_secret";
        let timestamp = now.timestamp();
        let mut mac = <ApprovalMac as Mac>::new_from_slice(secret).expect("HMAC key");
        mac.update(timestamp.to_string().as_bytes());
        mac.update(b".");
        mac.update(body);
        let digest = hex::encode(mac.finalize().into_bytes());

        assert!(
            verify_marketing_callback_signature(
                body,
                &format!("t={timestamp},v0={digest}"),
                secret,
                now,
            )
            .is_err()
        );
        assert!(
            verify_marketing_callback_signature(
                body,
                &format!("t={},v1={digest}", timestamp - 301),
                secret,
                now,
            )
            .is_err()
        );
        assert!(
            verify_marketing_callback_signature(
                body,
                &format!("t={},v1={digest}", timestamp + 301),
                secret,
                now,
            )
            .is_err()
        );
    }

    #[test]
    fn marketing_callback_signature_accepts_multiple_v1_values_when_one_matches() {
        let now = Utc::now();
        let body = br#"{}"#;
        let secret = b"marketing_callback_test_secret";
        let header = signed_header(body, secret, now);
        let digest = header.split_once("v1=").expect("v1 digest").1;

        assert!(
            verify_marketing_callback_signature(
                body,
                &format!("t={},v1=00{},v1={digest}", now.timestamp(), digest),
                secret,
                now,
            )
            .is_ok()
        );
    }
}

#[cfg(test)]
mod server_security_tests {
    use super::*;

    #[test]
    fn readiness_aggregation_is_fail_closed_across_the_active_registry() {
        assert_eq!(
            aggregate_readiness_states(std::iter::empty()),
            ReadinessState::NotProbed
        );
        assert_eq!(
            aggregate_readiness_states([ReadinessState::Ready, ReadinessState::Ready]),
            ReadinessState::Ready
        );
        assert_eq!(
            aggregate_readiness_states([ReadinessState::Ready, ReadinessState::NotProbed]),
            ReadinessState::NotProbed
        );
        assert_eq!(
            aggregate_readiness_states([
                ReadinessState::Unavailable,
                ReadinessState::Ready,
                ReadinessState::NotProbed,
            ]),
            ReadinessState::Unavailable
        );
    }

    #[test]
    fn infrastructure_readiness_states_preserve_the_fail_closed_mapping() {
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
    fn platform_tenant_page_size_is_defaulted_and_bounded() {
        assert_eq!(platform_tenant_page_size(0).expect("default"), 25);
        assert_eq!(
            platform_tenant_page_size(MAX_TENANT_REGISTRY_PAGE_SIZE).expect("maximum"),
            MAX_TENANT_REGISTRY_PAGE_SIZE
        );
        assert!(platform_tenant_page_size(MAX_TENANT_REGISTRY_PAGE_SIZE + 1).is_err());
    }

    #[test]
    fn projection_status_transport_bounds_and_opaque_cursor() {
        assert_eq!(projection_status_page_size(0).expect("default"), 25);
        assert_eq!(
            projection_status_page_size(MAX_PROJECTION_STATUS_PAGE_SIZE).expect("maximum"),
            MAX_PROJECTION_STATUS_PAGE_SIZE
        );
        assert!(projection_status_page_size(MAX_PROJECTION_STATUS_PAGE_SIZE + 1).is_err());

        let cursor = ProjectionStatusCursor::new(
            "tenant-opaque",
            "holding-opaque",
            "operations",
            "job-1",
            Uuid::from_u128(41),
        )
        .expect("cursor");
        let encoded = encode_projection_status_cursor(&cursor);
        assert!(!encoded.contains("tenant-opaque"));
        assert_eq!(
            decode_projection_status_cursor(&encoded).expect("round trip"),
            cursor
        );
        assert!(decode_projection_status_cursor("").is_err());
        assert!(decode_projection_status_cursor("zz").is_err());
        assert!(decode_projection_status_cursor(&"a".repeat(513)).is_err());
    }

    #[test]
    fn projection_status_freshness_has_an_exact_bounded_clock_window() {
        let now = Utc::now();
        assert_eq!(projection_status_freshness(now, now), "fresh");
        assert_eq!(
            projection_status_freshness(
                now - Duration::seconds(MAX_PROJECTION_STATUS_FRESHNESS_SECONDS),
                now,
            ),
            "fresh"
        );
        assert_eq!(
            projection_status_freshness(
                now - Duration::seconds(MAX_PROJECTION_STATUS_FRESHNESS_SECONDS + 1),
                now,
            ),
            "stale"
        );
        assert_eq!(
            projection_status_freshness(now + Duration::seconds(1), now),
            "unknown"
        );
    }

    #[test]
    fn projection_rebuild_transport_is_allowlisted_and_requires_a_source_version() {
        assert_eq!(
            projection_rebuild_name(OPERATIONS_READ_PROJECTION).expect("supported projection"),
            OPERATIONS_READ_PROJECTION
        );
        assert!(
            projection_rebuild_name(erpx_infrastructure::TENANT_PROJECTION_STATUS_READ_PROJECTION)
                .is_err()
        );
        assert!(projection_rebuild_source_version(0).is_err());
        assert_eq!(projection_rebuild_source_version(42).expect("version"), 42);
        assert!(projection_rebuild_generation(0).is_err());
        assert_eq!(projection_rebuild_generation(7).expect("generation"), 7);
        assert!(validate_transport_reference("candidate/read", "candidate").is_err());
    }

    #[test]
    fn passkey_provider_configuration_fails_closed_without_a_live_adapter() {
        assert!(validate_passkey_configuration(None, None).is_ok());
        assert!(validate_passkey_configuration(Some("disabled"), Some("false")).is_ok());
        assert!(validate_passkey_configuration(Some("live"), None).is_err());
        assert!(validate_passkey_configuration(Some("webauthn-rs"), Some("true")).is_ok());
        assert!(validate_passkey_configuration(None, Some("true")).is_err());
        assert!(validate_passkey_configuration(None, Some("maybe")).is_err());
    }

    #[test]
    fn support_actor_fields_must_match_the_authenticated_principal() {
        assert!(require_actor_match("agent-1", "agent-1", "mismatch").is_ok());
        assert_eq!(
            require_actor_match("agent-1", "agent-2", "mismatch")
                .expect_err("mismatched actor must fail")
                .code(),
            tonic::Code::PermissionDenied
        );
    }

    #[test]
    fn support_ticket_transition_parser_is_closed_and_terminal_status_is_explicit() {
        assert_eq!(
            parse_ticket_status("triaged").expect("triaged"),
            TicketStatus::Triaged
        );
        assert_eq!(
            parse_ticket_status("closed").expect("closed"),
            TicketStatus::Closed
        );
        assert!(parse_ticket_status("customer_confirmed ").is_err());
        assert!(parse_ticket_status("operations").is_err());
    }

    #[test]
    fn support_ticket_cursor_round_trip_is_opaque_and_bounded() {
        let timestamp = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).expect("timestamp");
        let cursor =
            SupportTicketListCursor::new(timestamp, Uuid::from_u128(0x9012)).expect("cursor");
        let encoded = encode_support_ticket_cursor(&cursor);
        assert!(!encoded.contains("1700000000"));
        assert_eq!(
            decode_support_ticket_cursor(&encoded).expect("decode"),
            cursor
        );
        assert!(decode_support_ticket_cursor("").is_err());
        assert!(decode_support_ticket_cursor(&"a".repeat(257)).is_err());
        assert!(decode_support_ticket_cursor("00").is_err());
    }

    #[test]
    fn ticket_transport_excludes_internal_notes_from_customer_replies() {
        let now = Utc::now();
        let mut ticket = erpx_domain::support::SupportTicket {
            id: Uuid::new_v4(),
            customer_account_id: Uuid::new_v4(),
            status: TicketStatus::InProgress,
            severity: "high".into(),
            description: "transport-safe ticket".into(),
            recurrence_of: None,
            messages: Vec::new(),
            assignment: None,
            escalation: None,
            sla_deadline: None,
        };
        ticket.messages.push(
            erpx_domain::support::SupportTicketMessage::customer_reply(
                Uuid::new_v4(),
                "customer-1",
                "visible reply",
                now,
            )
            .expect("reply"),
        );
        ticket.messages.push(
            erpx_domain::support::SupportTicketMessage::internal_note(
                Uuid::new_v4(),
                "agent-1",
                "secret internal note",
                now,
            )
            .expect("note"),
        );

        let response = ticket_message(&ticket);
        assert_eq!(response.customer_replies.len(), 1);
        assert_eq!(response.customer_replies[0].body, "visible reply");
        assert!(
            !response
                .customer_replies
                .iter()
                .any(|reply| reply.body.contains("secret"))
        );
    }

    #[test]
    fn incident_status_parser_preserves_postmortem_terminal_name() {
        assert_eq!(
            parse_incident_status("postmortem_complete").expect("postmortem status"),
            IncidentStatus::PostmortemComplete
        );
        assert_eq!(
            incident_status_name(IncidentStatus::PostmortemComplete),
            "postmortem_complete"
        );
        assert!(parse_incident_status("postmortemcomplete").is_err());
        assert!(parse_incident_status("resolved ").is_err());
    }

    #[test]
    fn incident_transport_retains_domain_terminal_transition_rules() {
        let now = Utc::now();
        let mut incident = Incident::new("commander-1", vec![], vec!["api".into()], vec![], now)
            .expect("incident");
        for status in [
            IncidentStatus::Identified,
            IncidentStatus::Monitoring,
            IncidentStatus::Resolved,
            IncidentStatus::PostmortemComplete,
        ] {
            incident
                .transition(status, now, "commander-1")
                .expect("valid lifecycle transition");
        }
        assert_eq!(incident.status, IncidentStatus::PostmortemComplete);
        assert!(
            incident
                .transition(IncidentStatus::Investigating, now, "commander-1")
                .is_err()
        );
    }

    #[test]
    fn incident_transport_bounds_inputs_and_redacts_persistence_details() {
        let too_many = vec!["service".to_owned(); MAX_INCIDENT_SERVICES + 1];
        assert!(validate_incident_text_list(&too_many, "services", MAX_INCIDENT_SERVICES).is_err());
        assert!(validate_incident_text("bad\nvalue", "service", 128).is_err());
        let status = incident_persistence_status(SupportPersistenceError::IdempotencyConflict {
            key: "opaque-idempotency-value".into(),
        });
        assert_eq!(status.code(), tonic::Code::Aborted);
        assert!(!status.message().contains("opaque-idempotency-value"));
    }

    #[test]
    fn support_scope_is_canonical_and_contains_only_authorized_descendants() {
        let holding_id = Uuid::from_u128(1);
        let company_id = Uuid::from_u128(2);
        let branch_id = Uuid::from_u128(3);
        let sibling_branch_id = Uuid::from_u128(4);
        let approved = Scope::Company {
            holding_id,
            company_id,
        };
        let approved_reference = support_scope_reference(&approved);
        assert_eq!(
            parse_support_scope(&approved_reference).expect("canonical scope"),
            approved
        );
        assert!(approved.contains(&Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        }));
        assert!(approved.contains(&Scope::Branch {
            holding_id,
            company_id,
            branch_id: sibling_branch_id,
        }));
        let branch_approved = Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        };
        assert!(!branch_approved.contains(&Scope::Branch {
            holding_id,
            company_id,
            branch_id: sibling_branch_id,
        }));
    }

    #[test]
    fn customer_approval_scope_must_contain_the_requested_support_scope() {
        let holding_id = Uuid::from_u128(101);
        let company_id = Uuid::from_u128(102);
        let branch_id = Uuid::from_u128(103);
        let holding = Scope::Holding { holding_id };
        let company = Scope::Company {
            holding_id,
            company_id,
        };
        let branch = Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        };
        assert!(holding.contains(&branch));
        assert!(company.contains(&branch));
        assert!(!branch.contains(&company));
        assert!(!branch.contains(&Scope::Holding { holding_id }));
    }

    #[test]
    fn support_capability_device_binding_uses_the_authenticated_session_device() {
        let device_id = Uuid::from_u128(104);
        assert!(validate_authenticated_device_ref(device_id, &device_id.to_string()).is_ok());
        assert!(validate_authenticated_device_ref(device_id, "client-chosen-device").is_err());
    }

    fn valid_support_diagnostics_access() -> (SupportDiagnosticsAccess, Scope) {
        let holding_id = Uuid::from_u128(201);
        let company_id = Uuid::from_u128(202);
        let requested_scope = Scope::Branch {
            holding_id,
            company_id,
            branch_id: Uuid::from_u128(203),
        };
        let approved_scope = Scope::Company {
            holding_id,
            company_id,
        };
        (
            SupportDiagnosticsAccess {
                mode: 2,
                customer_id: Uuid::from_u128(204).to_string(),
                capability_id: Uuid::from_u128(205).to_string(),
                capability_secret: "00".repeat(32),
                support_session_id: Uuid::from_u128(206).to_string(),
                support_ticket_id: Uuid::from_u128(207).to_string(),
                support_mode: "diagnostic_read".into(),
                support_scope: support_scope_reference(&approved_scope),
                module: "support".into(),
                subscription_id: Uuid::from_u128(208).to_string(),
                device_ref: Uuid::from_u128(209).to_string(),
            },
            requested_scope,
        )
    }

    #[test]
    fn support_diagnostics_transport_is_typed_and_fail_closed() {
        let (valid, requested_scope) = valid_support_diagnostics_access();
        assert!(parse_support_diagnostics_access(&valid, &requested_scope).is_ok());

        let missing = SupportDiagnosticsAccess {
            mode: 2,
            ..SupportDiagnosticsAccess::default()
        };
        assert!(parse_support_diagnostics_access(&missing, &requested_scope).is_err());

        let mut wrong_mode = valid.clone();
        wrong_mode.support_mode = "tenant_mutation".into();
        assert!(parse_support_diagnostics_access(&wrong_mode, &requested_scope).is_err());

        let mut wrong_scope = valid.clone();
        wrong_scope.support_scope = support_scope_reference(&Scope::Branch {
            holding_id: Uuid::from_u128(201),
            company_id: Uuid::from_u128(202),
            branch_id: Uuid::from_u128(999),
        });
        assert!(parse_support_diagnostics_access(&wrong_scope, &requested_scope).is_err());

        let mut malformed_device = valid.clone();
        malformed_device.device_ref = "client-chosen-device".into();
        assert!(parse_support_diagnostics_access(&malformed_device, &requested_scope).is_err());

        let mut malformed_session = valid;
        malformed_session.support_session_id = "different-session".into();
        assert!(parse_support_diagnostics_access(&malformed_session, &requested_scope).is_err());
    }

    #[test]
    fn platform_diagnostics_cannot_carry_customer_capability_material() {
        let mut access = SupportDiagnosticsAccess {
            mode: 1,
            ..SupportDiagnosticsAccess::default()
        };
        assert!(platform_diagnostics_access_is_empty(&access).is_ok());
        access.capability_secret = "00".repeat(32);
        assert_eq!(
            platform_diagnostics_access_is_empty(&access)
                .expect_err("platform mode must not carry a bearer")
                .code(),
            tonic::Code::InvalidArgument
        );
    }

    #[test]
    fn diagnostics_snapshot_redaction_survives_the_transport_boundary() {
        let mut input = DiagnosticsSnapshotInput::new(Utc::now());
        input.add_version("server", "test");
        input.add_configuration("DATABASE_PASSWORD", "must-not-appear");
        input.add_configuration("environment", "test");
        let snapshot = input
            .build(DiagnosticsLimits::default())
            .expect("bounded diagnostics snapshot");
        let payload = serde_json::to_string(&snapshot).expect("serialize diagnostics");
        assert!(!payload.contains("must-not-appear"));
        assert!(payload.contains("environment"));
        assert_eq!(snapshot.redacted_configuration_count(), 1);
    }

    #[test]
    fn support_diagnostics_capability_rechecks_expiry_revocation_and_binding() {
        let now = Utc::now();
        let customer_id = Uuid::from_u128(301);
        let session_id = Uuid::from_u128(302);
        let ticket_id = Uuid::from_u128(303);
        let binding = SupportCapabilityBinding::new(
            customer_id,
            "agent-1",
            "device-1",
            session_id,
            ticket_id,
            SupportMode::DiagnosticRead,
            vec![SUPPORT_DIAGNOSTICS_PERMISSION.into()],
            "holding/301",
            "support",
        )
        .expect("diagnostic binding");
        let (mut capability, secret) =
            SupportCapability::issue(binding.clone(), now, Duration::hours(1))
                .expect("diagnostic capability");
        capability
            .authenticate(&secret, &binding, now + Duration::minutes(1))
            .expect("active capability");
        assert!(
            capability
                .authenticate(&secret, &binding, capability.expires_at)
                .is_err()
        );

        for (device_ref, session_id, mode, scope, module) in [
            (
                "other-device",
                session_id,
                SupportMode::DiagnosticRead,
                "holding/301",
                "support",
            ),
            (
                "device-1",
                Uuid::from_u128(304),
                SupportMode::DiagnosticRead,
                "holding/301",
                "support",
            ),
            (
                "device-1",
                session_id,
                SupportMode::TenantMutation,
                "holding/301",
                "support",
            ),
            (
                "device-1",
                session_id,
                SupportMode::DiagnosticRead,
                "holding/302",
                "support",
            ),
            (
                "device-1",
                session_id,
                SupportMode::DiagnosticRead,
                "holding/301",
                "finance",
            ),
        ] {
            let mismatched = SupportCapabilityBinding::new(
                customer_id,
                "agent-1",
                device_ref,
                session_id,
                ticket_id,
                mode,
                vec![SUPPORT_DIAGNOSTICS_PERMISSION.into()],
                scope,
                module,
            )
            .expect("mismatched binding shape");
            assert!(
                capability
                    .authenticate(&secret, &mismatched, now + Duration::minutes(1))
                    .is_err()
            );
        }

        capability
            .revoke(
                erpx_domain::support::SupportCapabilityRevocationReason::CustomerRequested,
                now + Duration::minutes(2),
            )
            .expect("revoke capability");
        assert!(
            capability
                .authenticate(&secret, &binding, now + Duration::minutes(3))
                .is_err()
        );
    }

    #[test]
    fn role_renewal_scope_direction_cannot_expand_authority() {
        let holding_id = Uuid::from_u128(11);
        let company_id = Uuid::from_u128(12);
        let branch_id = Uuid::from_u128(13);
        let caller_scope = Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        };
        let target_scope = Scope::Holding { holding_id };
        assert!(!caller_scope.contains(&target_scope));
        assert!(Scope::Holding { holding_id }.contains(&Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        }));
    }

    #[test]
    fn customer_access_audit_requires_explicit_customer_scope_and_redacts_no_scope_data() {
        let customer_id = Uuid::from_u128(21);
        let holding_id = Uuid::from_u128(22);
        let company_id = Uuid::from_u128(23);
        let branch_id = Uuid::from_u128(24);
        let scope = Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        };
        let admin_scope =
            customer_security_admin_scope(customer_id, &scope).expect("branch scope is explicit");
        assert_eq!(admin_scope.customer_id, customer_id);
        assert_eq!(admin_scope.branch_id, Some(branch_id));
        let read_scope = global_read_scope_for_customer_scope(&scope).expect("read scope");
        assert_eq!(
            read_scope.label,
            format!("branch/{holding_id}/{company_id}/{branch_id}")
        );
        assert_eq!(read_scope.branch_id, Some(branch_id));
        let platform_scope = customer_security_admin_scope(customer_id, &Scope::Platform)
            .expect("platform audit scope");
        assert_eq!(platform_scope.holding_id, None);
        assert_eq!(
            global_read_scope_for_customer_scope(&Scope::Platform)
                .expect("platform read scope")
                .label,
            "platform"
        );
        assert!(
            customer_security_admin_scope(
                customer_id,
                &Scope::Subtree {
                    holding_id,
                    company_ids: vec![company_id],
                    branch_ids: vec![branch_id],
                }
            )
            .is_err()
        );
    }

    #[test]
    fn finance_close_read_scope_rejects_broad_and_cross_company_requests() {
        let holding_id = Uuid::from_u128(301);
        let company_id = Uuid::from_u128(302);
        let branch_id = Uuid::from_u128(303);
        assert!(
            finance_close_read_scope(&Scope::Holding { holding_id }, holding_id, company_id,)
                .is_ok()
        );
        assert!(
            finance_close_read_scope(
                &Scope::Branch {
                    holding_id,
                    company_id,
                    branch_id,
                },
                holding_id,
                company_id,
            )
            .is_ok()
        );
        assert!(
            finance_close_read_scope(
                &Scope::Company {
                    holding_id,
                    company_id,
                },
                holding_id,
                Uuid::from_u128(304),
            )
            .is_err()
        );
        assert!(
            finance_close_read_scope(
                &Scope::Subtree {
                    holding_id,
                    company_ids: vec![company_id],
                    branch_ids: vec![branch_id],
                },
                holding_id,
                company_id,
            )
            .is_err()
        );
        assert!(finance_close_read_scope(&Scope::Platform, holding_id, company_id).is_err());
    }

    #[test]
    fn finance_chart_read_scope_is_explicit_and_contained() {
        let holding_id = Uuid::from_u128(401);
        let company_id = Uuid::from_u128(402);
        let branch_id = Uuid::from_u128(403);
        let holding_scope = finance_chart_read_scope(
            &Scope::Holding { holding_id },
            &holding_id.to_string(),
            &company_id.to_string(),
        )
        .expect("holding contains company chart read");
        assert_eq!(holding_scope.holding_id, holding_id);
        assert_eq!(holding_scope.company_id, company_id);
        assert!(
            finance_chart_read_scope(
                &Scope::Branch {
                    holding_id,
                    company_id,
                    branch_id,
                },
                &holding_id.to_string(),
                &company_id.to_string(),
            )
            .is_err()
        );
        assert!(
            finance_chart_read_scope(
                &Scope::Company {
                    holding_id,
                    company_id,
                },
                &holding_id.to_string(),
                &Uuid::from_u128(404).to_string(),
            )
            .is_err()
        );
        assert!(
            finance_chart_read_scope(
                &Scope::Subtree {
                    holding_id,
                    company_ids: vec![company_id],
                    branch_ids: vec![branch_id],
                },
                &holding_id.to_string(),
                &company_id.to_string(),
            )
            .is_err()
        );
        assert!(
            finance_chart_read_scope(
                &Scope::Platform,
                &holding_id.to_string(),
                &company_id.to_string(),
            )
            .is_err()
        );
    }
}

#[cfg(test)]
mod finance_banking_transport_tests {
    use super::*;

    #[test]
    fn banking_json_decoder_is_bounded_and_typed() {
        let value: serde_json::Value =
            decode_finance_banking_json(br#"{"safe":true}"#, "payload").expect("JSON");
        assert_eq!(value["safe"], true);
        assert!(decode_finance_banking_json::<serde_json::Value>(b"", "payload").is_err());
        assert!(
            decode_finance_banking_json::<serde_json::Value>(
                &vec![b'x'; MAX_FINANCE_BANKING_JSON_BYTES + 1],
                "payload",
            )
            .is_err()
        );
        assert!(decode_finance_banking_json::<BankAccount>(br#"{"id":"bad"}"#, "account").is_err());
    }

    #[test]
    fn banking_read_mapping_exposes_only_typed_projection_fields() {
        assert_eq!(
            finance_persistence_status(
                PersistenceError::InvalidData("secret=do-not-leak".into()),
                Uuid::new_v4(),
            )
            .message(),
            "Finance aggregate cannot be persisted"
        );
    }

    #[test]
    fn banking_read_list_bounds_and_cursor_are_opaque_and_deterministic() {
        assert_eq!(finance_banking_read_page_size(0).expect("default"), 25);
        assert_eq!(
            finance_banking_read_page_size(MAX_FINANCE_BANKING_READ_PAGE_SIZE).expect("maximum"),
            MAX_FINANCE_BANKING_READ_PAGE_SIZE
        );
        assert!(finance_banking_read_page_size(MAX_FINANCE_BANKING_READ_PAGE_SIZE + 1).is_err());

        let cursor = FinanceBankingReadCursor::new("finance_bank_account", Uuid::from_u128(22))
            .expect("cursor");
        let encoded = encode_finance_banking_read_cursor(&cursor);
        assert_eq!(
            decode_finance_banking_read_cursor(&encoded)
                .expect("round trip")
                .aggregate_id(),
            cursor.aggregate_id()
        );
        assert_eq!(
            decode_finance_banking_read_cursor(&encoded)
                .expect("round trip")
                .aggregate_type(),
            cursor.aggregate_type()
        );
        assert!(decode_finance_banking_read_cursor("").is_err());
        assert!(decode_finance_banking_read_cursor("zz").is_err());
        assert!(decode_finance_banking_read_cursor(&"a".repeat(257)).is_err());
    }

    #[test]
    fn banking_read_scope_conversion_rejects_broad_or_composite_scope() {
        let holding_id = Uuid::from_u128(31);
        let company_id = Uuid::from_u128(32);
        let branch_id = Uuid::from_u128(33);
        assert!(finance_banking_read_tenant_scope(&Scope::Platform).is_err());
        assert!(
            finance_banking_read_tenant_scope(&Scope::Subtree {
                holding_id,
                company_ids: vec![company_id],
                branch_ids: vec![branch_id],
            })
            .is_err()
        );
        assert_eq!(
            finance_banking_read_tenant_scope(&Scope::Company {
                holding_id,
                company_id,
            })
            .expect("company scope"),
            TenantScope::Company {
                holding_id,
                company_id,
            }
        );
    }

    #[test]
    fn bank_match_transport_payloads_are_bounded_and_errors_are_redacted() {
        let proposal: serde_json::Value =
            decode_finance_banking_json(br#"{"candidates":[]}"#, "proposal_json")
                .expect("bounded proposal JSON");
        assert_eq!(proposal["candidates"], serde_json::json!([]));
        assert!(
            decode_finance_banking_json::<serde_json::Value>(
                &vec![b'x'; MAX_FINANCE_BANKING_JSON_BYTES + 1],
                "decision_json",
            )
            .is_err()
        );
        assert_eq!(
            finance_persistence_status(
                PersistenceError::InvalidData("token=must-not-leak".into()),
                Uuid::new_v4(),
            )
            .message(),
            "Finance aggregate cannot be persisted"
        );
    }
}

#[cfg(test)]
mod finance_subledger_transport_tests {
    use super::*;

    #[test]
    fn subledger_read_list_bounds_and_cursor_are_opaque_and_deterministic() {
        assert_eq!(finance_subledger_read_page_size(0).expect("default"), 25);
        assert_eq!(
            finance_subledger_read_page_size(MAX_FINANCE_SUBLEDGER_READ_PAGE_SIZE)
                .expect("maximum"),
            MAX_FINANCE_SUBLEDGER_READ_PAGE_SIZE
        );
        assert!(
            finance_subledger_read_page_size(MAX_FINANCE_SUBLEDGER_READ_PAGE_SIZE + 1).is_err()
        );

        let cursor = FinanceSubledgerReadCursor::new("receivable_invoice", Uuid::from_u128(52))
            .expect("cursor");
        let encoded = encode_finance_subledger_read_cursor(&cursor);
        let decoded = decode_finance_subledger_read_cursor(&encoded).expect("round trip");
        assert_eq!(decoded.aggregate_id(), cursor.aggregate_id());
        assert_eq!(decoded.aggregate_type(), cursor.aggregate_type());
        assert!(decode_finance_subledger_read_cursor("").is_err());
        assert!(decode_finance_subledger_read_cursor("zz").is_err());
        assert!(decode_finance_subledger_read_cursor(&"a".repeat(257)).is_err());
    }

    #[test]
    fn subledger_read_scope_conversion_rejects_broad_or_composite_scope() {
        let holding_id = Uuid::from_u128(61);
        let company_id = Uuid::from_u128(62);
        let branch_id = Uuid::from_u128(63);
        assert!(finance_subledger_read_tenant_scope(&Scope::Platform).is_err());
        assert!(
            finance_subledger_read_tenant_scope(&Scope::Subtree {
                holding_id,
                company_ids: vec![company_id],
                branch_ids: vec![branch_id],
            })
            .is_err()
        );
        assert_eq!(
            finance_subledger_read_tenant_scope(&Scope::Branch {
                holding_id,
                company_id,
                branch_id,
            })
            .expect("branch scope"),
            TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            }
        );
    }
}

#[cfg(test)]
mod finance_thai_tax_transport_tests {
    use super::*;

    #[test]
    fn thai_tax_report_rejects_source_read_drift_before_serving_totals() {
        let drifted = FinanceThaiTaxProjectionReconciliation {
            source_events: 4,
            projected_rows: 3,
            mismatched_rows: 1,
        };
        let status =
            require_finance_thai_tax_reconciliation(drifted).expect_err("drift must fail closed");
        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
        assert_eq!(
            status.message(),
            "Thai tax source and read projection are not reconciled"
        );
    }

    #[test]
    fn thai_tax_report_accepts_only_exact_reconciliation() {
        let reconciled = FinanceThaiTaxProjectionReconciliation {
            source_events: 4,
            projected_rows: 4,
            mismatched_rows: 0,
        };
        assert!(require_finance_thai_tax_reconciliation(reconciled).is_ok());
    }
}

#[cfg(test)]
mod hr_transport_tests {
    use super::*;

    #[test]
    fn leave_cursor_is_opaque_bounded_and_round_trips() {
        let cursor = HrLeaveReadCursor {
            starts_on: NaiveDate::from_ymd_opt(2026, 7, 25).expect("date"),
            leave_id: Uuid::new_v4(),
        };
        let encoded = encode_hr_leave_cursor(&cursor);
        assert!(!encoded.contains('-'));
        assert_eq!(decode_hr_leave_cursor(&encoded).expect("cursor"), cursor);
        assert!(decode_hr_leave_cursor("not-hex").is_err());
        assert!(decode_hr_leave_cursor(&"a".repeat(257)).is_err());
    }

    #[test]
    fn leave_cursor_rejects_nil_and_malformed_values() {
        let date = NaiveDate::from_ymd_opt(2026, 7, 25).expect("date");
        let nil = hex::encode(format!("{date}|{}", Uuid::nil()).as_bytes());
        assert!(decode_hr_leave_cursor(&nil).is_err());
        let bad_date = hex::encode(format!("not-a-date|{}", Uuid::new_v4()).as_bytes());
        assert!(decode_hr_leave_cursor(&bad_date).is_err());
    }

    #[test]
    fn expense_cursor_is_opaque_and_rejects_nil_values() {
        let cursor = HrExpenseReadCursor {
            expense_id: Uuid::new_v4(),
        };
        let encoded = encode_hr_expense_cursor(&cursor);
        assert!(!encoded.contains('-'));
        assert_eq!(decode_hr_expense_cursor(&encoded).expect("cursor"), cursor);
        let nil = encode_hr_expense_cursor(&HrExpenseReadCursor {
            expense_id: Uuid::nil(),
        });
        assert!(decode_hr_expense_cursor(&nil).is_err());
    }

    #[test]
    fn payroll_cursor_is_opaque_and_round_trips() {
        let cursor = HrPayrollRunReadCursor {
            starts_on: NaiveDate::from_ymd_opt(2026, 7, 25).expect("date"),
            payroll_id: Uuid::new_v4(),
        };
        let encoded = encode_hr_payroll_cursor(&cursor);
        assert_eq!(decode_hr_payroll_cursor(&encoded).expect("cursor"), cursor);
        assert!(decode_hr_payroll_cursor("00").is_err());
    }

    #[test]
    fn reassignment_cursor_is_opaque_bounded_and_round_trips() {
        let cursor = HrReassignmentReadCursor {
            reassigned_at: DateTime::parse_from_rfc3339("2026-07-25T04:05:06Z")
                .expect("timestamp")
                .with_timezone(&Utc),
            workflow_ref: "workflow-001".into(),
        };
        let encoded = encode_hr_reassignment_cursor(&cursor);
        assert!(!encoded.contains('-'));
        assert_eq!(
            decode_hr_reassignment_cursor(&encoded).expect("cursor"),
            cursor
        );
        assert!(decode_hr_reassignment_cursor("00").is_err());
        assert!(decode_hr_reassignment_cursor(&"a".repeat(257)).is_err());
    }
}

#[cfg(test)]
mod pos_transport_tests {
    use super::*;

    #[test]
    fn sales_cursor_is_opaque_bounded_and_round_trips() {
        let cursor = PosSaleReadCursor {
            updated_at: DateTime::parse_from_rfc3339("2026-07-25T04:05:06Z")
                .expect("timestamp")
                .with_timezone(&Utc),
            sale_id: Uuid::new_v4(),
        };
        let encoded = encode_pos_sales_cursor(&cursor);
        assert!(!encoded.contains('-'));
        assert_eq!(decode_pos_sales_cursor(&encoded).expect("cursor"), cursor);
        assert!(decode_pos_sales_cursor("not-hex").is_err());
        assert!(decode_pos_sales_cursor(&"a".repeat(257)).is_err());
    }

    #[test]
    fn sales_cursor_rejects_nil_and_malformed_values() {
        let timestamp = "2026-07-25T04:05:06Z";
        let nil = hex::encode(format!("{timestamp}|{}", Uuid::nil()).as_bytes());
        assert!(decode_pos_sales_cursor(&nil).is_err());
        let bad_timestamp = hex::encode(format!("not-a-time|{}", Uuid::new_v4()).as_bytes());
        assert!(decode_pos_sales_cursor(&bad_timestamp).is_err());
    }
}

#[cfg(test)]
mod sales_transport_tests {
    use super::*;

    #[test]
    fn sales_read_cursor_is_opaque_bounded_and_allowlisted() {
        let cursor = SalesReadCursor {
            projected_at: DateTime::parse_from_rfc3339("2026-07-25T04:05:06Z")
                .expect("timestamp")
                .with_timezone(&Utc),
            aggregate_type: "sales_order".into(),
            aggregate_id: Uuid::new_v4(),
        };
        let encoded = encode_sales_read_cursor(&cursor);
        assert!(!encoded.contains('-'));
        assert_eq!(decode_sales_read_cursor(&encoded).expect("cursor"), cursor);
        assert!(decode_sales_read_cursor("not-hex").is_err());
        assert!(decode_sales_read_cursor(&"a".repeat(385)).is_err());
        let invalid_type = hex::encode(
            format!(
                "{}|invoice|{}",
                cursor.projected_at.to_rfc3339(),
                cursor.aggregate_id
            )
            .as_bytes(),
        );
        assert!(decode_sales_read_cursor(&invalid_type).is_err());
    }
}

#[cfg(test)]
mod quality_transport_tests {
    use super::*;

    #[test]
    fn quality_cursor_is_opaque_bounded_and_round_trips() {
        let cursor = QualityInspectionReadCursor {
            projected_at: DateTime::parse_from_rfc3339("2026-07-25T04:05:06Z")
                .expect("timestamp")
                .with_timezone(&Utc),
            inspection_id: Uuid::new_v4(),
        };
        let encoded = encode_quality_inspection_cursor(&cursor);
        assert!(!encoded.contains('-'));
        assert_eq!(
            decode_quality_inspection_cursor(&encoded).expect("cursor"),
            cursor
        );
        assert!(decode_quality_inspection_cursor("not-hex").is_err());
        assert!(decode_quality_inspection_cursor(&"a".repeat(257)).is_err());
    }

    #[test]
    fn quality_cursor_rejects_nil_and_malformed_values() {
        let timestamp = "2026-07-25T04:05:06Z";
        let nil = hex::encode(format!("{timestamp}|{}", Uuid::nil()).as_bytes());
        assert!(decode_quality_inspection_cursor(&nil).is_err());
        let bad_timestamp = hex::encode(format!("not-a-time|{}", Uuid::new_v4()).as_bytes());
        assert!(decode_quality_inspection_cursor(&bad_timestamp).is_err());
    }
}

#[cfg(test)]
mod marketing_content_transport_tests {
    use super::*;

    #[test]
    fn form_values_are_bounded_string_maps() {
        let values =
            marketing_form_values(br#"{"email":"customer@example.com"}"#).expect("valid values");
        assert_eq!(
            values.get("email").map(String::as_str),
            Some("customer@example.com")
        );
        assert!(marketing_form_values(br#"{"email":42}"#).is_err());
        assert!(marketing_form_values(b"").is_err());
        assert!(marketing_form_values(&vec![b'a'; MAX_MARKETING_FORM_VALUES_BYTES + 1]).is_err());
    }

    #[test]
    fn marketing_persistence_errors_are_redacted_and_correlated() {
        let correlation_id = Uuid::new_v4();
        let status = marketing_content_persistence_status(
            PersistenceError::InvalidData("secret database detail".into()),
            correlation_id,
        );
        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
        assert!(!status.message().contains("secret"));
        assert!(status.metadata().get("x-erpx-correlation-id").is_some());
    }

    #[test]
    fn public_marketing_routes_are_explicit_and_host_bound() {
        assert!(public_marketing_path(
            "/public/marketing/forms/8f9f3a8f-24f0-4c9a-a7e8-6bd0d9d4e4d2/submissions"
        ));
        assert_eq!(
            public_marketing_route(
                "/public/marketing/forms/8f9f3a8f-24f0-4c9a-a7e8-6bd0d9d4e4d2/submissions"
            ),
            Some(("form", "8f9f3a8f-24f0-4c9a-a7e8-6bd0d9d4e4d2"))
        );
        assert_eq!(
            public_marketing_route(
                "/public/marketing/previews/8f9f3a8f-24f0-4c9a-a7e8-6bd0d9d4e4d2?token=opaque"
            ),
            Some(("preview", "8f9f3a8f-24f0-4c9a-a7e8-6bd0d9d4e4d2"))
        );
        let headers = HashMap::from([(
            String::from("host"),
            String::from("campaign.example.com:443"),
        )]);
        assert_eq!(
            public_marketing_hostname(&headers).expect("hostname"),
            "campaign.example.com"
        );
        let invalid = HashMap::from([(String::from("host"), String::from("localhost"))]);
        assert!(public_marketing_hostname(&invalid).is_err());
    }

    #[test]
    fn public_marketing_abuse_signals_reject_honeypots_and_surface_challenges() {
        let headers = HashMap::new();
        let body = serde_json::json!({
            "submission_id": Uuid::from_u128(7),
            "values": {"website": "https://bot.invalid"}
        })
        .to_string();
        assert_eq!(
            public_marketing_abuse_signal("form", &headers, body.as_bytes()),
            Some(MarketingAbuseSignal::HoneypotTriggered)
        );

        let challenge_headers = HashMap::from([(
            String::from("x-erpx-marketing-challenge"),
            String::from("required"),
        )]);
        assert_eq!(
            public_marketing_abuse_signal("preview", &challenge_headers, &[]),
            Some(MarketingAbuseSignal::ChallengeRequired)
        );
        let response = public_marketing_abuse_decision_response(MarketingAbuseDecision::Reject {
            reason: erpx_domain::marketing_abuse::MarketingAbuseRejectReason::HoneypotTriggered,
        })
        .expect("rejected request response");
        assert_eq!(response.0, "400 Bad Request");
        assert!(!response.2.contains("bot.invalid"));
    }

    #[test]
    fn public_marketing_abuse_binding_ids_are_deterministic_and_opaque() {
        let tenant = marketing_abuse_binding_id("tenant", "tenant-ref-1");
        assert_eq!(tenant, marketing_abuse_binding_id("tenant", "tenant-ref-1"));
        assert_ne!(tenant, marketing_abuse_binding_id("tenant", "tenant-ref-2"));
        assert_ne!(tenant, Uuid::nil());
        assert!(!format!("{tenant:?}").contains("tenant-ref-1"));
    }

    #[test]
    fn marketing_issued_secrets_are_exactly_256_bits_and_only_hashed_for_storage() {
        let secret = marketing_opaque_secret();
        assert_eq!(secret.len(), 64);
        assert!(secret.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(marketing_secret_hash(&secret).len(), 64);
        assert_ne!(marketing_secret_hash(&secret), secret);
    }
}

#[cfg(test)]
mod tenant_owner_seed_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires two disposable PostgreSQL databases"]
    async fn owner_seed_transaction_creates_recovery_identity_and_finite_role() {
        let config = ControlPlaneDatabaseConfig {
            write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
                .expect("write database URL"),
            read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
                .expect("read database URL"),
        };
        let pools = ControlPlanePools::connect(&config)
            .await
            .expect("control pools");
        erpx_migration::migrate_control_plane(&pools.write)
            .await
            .expect("write migration");
        erpx_migration::migrate_control_read(&pools.read)
            .await
            .expect("read migration");
        erpx_identity::apply_identity_migration(&pools.write)
            .await
            .expect("identity migration");
        let store = Arc::new(SqlxControlPlane::new(pools));
        let provisioner = TenantProvisioner::new(store.clone());
        provisioner
            .begin(TenantStoreRegistration {
                tenant_id: "tenant-owner-transaction".into(),
                holding_ref: "holding-owner-transaction".into(),
                write_database_key: "tenant-owner-transaction-write".into(),
                read_database_key: "tenant-owner-transaction-read".into(),
                storage_namespace: "tenant-owner-transaction-storage".into(),
                credential_key_ref: "tenant-owner-transaction-key".into(),
                backup_chain_ref: "tenant-owner-transaction-chain".into(),
                state: "provisioning".into(),
                registry_version: 1,
            })
            .await
            .expect("begin provisioning");
        let seed = provisioner
            .register_owner_seed(
                "tenant-owner-transaction",
                "owner-transaction",
                "approver-transaction",
            )
            .await
            .expect("owner seed");
        let receipt = seed_tenant_owner(&store, &seed, "grantor-transaction", Utc::now())
            .await
            .expect("seed owner");
        assert!(receipt.seeded);
        assert!(receipt.recovery_code.is_some());
        let principal_status: String = sqlx::query_scalar(
            "SELECT status FROM erpx_identity.principals
             WHERE principal_ref = 'owner-transaction'",
        )
        .fetch_one(&store.pools().write)
        .await
        .expect("principal status");
        assert_eq!(principal_status, "recovery_required");
        let recovery_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM erpx_identity.recovery_codes rc
             JOIN erpx_identity.principals p ON p.id = rc.principal_id
             WHERE p.principal_ref = 'owner-transaction'",
        )
        .fetch_one(&store.pools().write)
        .await
        .expect("recovery code count");
        assert_eq!(recovery_count, 1);
        let payload: Value = sqlx::query_scalar(
            "SELECT payload FROM erpx_control.role_assignments
             WHERE principal_ref = 'owner-transaction'",
        )
        .fetch_one(&store.pools().write)
        .await
        .expect("owner role");
        let assignment: RoleAssignment = serde_json::from_value(payload).expect("role payload");
        assert_eq!(assignment.role_key, HOLDING_OWNER_ROLE);
        assert!(assignment.valid_until > assignment.valid_from);
        assert!(assignment.valid_until < DateTime::<Utc>::MAX_UTC);
        assert_eq!(assignment.approver.as_deref(), Some("approver-transaction"));
        let replay = seed_tenant_owner(&store, &seed, "grantor-transaction", Utc::now())
            .await
            .expect("replay owner seed");
        assert!(replay.seeded);
        assert!(replay.recovery_code.is_none());
        let read_table: Option<String> =
            sqlx::query_scalar("SELECT to_regclass('erpx_control.tenant_owner_seeds')::text")
                .fetch_one(&store.pools().read)
                .await
                .expect("read isolation query");
        assert_eq!(read_table, None);
    }
}

#[cfg(test)]
mod finance_bank_provider_import_status_tests {
    use super::*;

    #[test]
    fn finance_bank_import_status_mapping_is_bounded_and_payload_free() {
        let now = Utc::now();
        let message =
            finance_bank_provider_import_status_message(FinanceBankProviderImportJobStatus {
                job_id: Uuid::from_u128(1),
                tenant_id: "tenant-opaque".into(),
                holding_ref: "holding-opaque".into(),
                import_id: Uuid::from_u128(2),
                state: "retryable".into(),
                source_fence: 7,
                attempts: 2,
                max_attempts: 3,
                available_at: now,
                lease_until: None,
                last_failure_class: Some("retryable".into()),
                completed_at: None,
                updated_at: now,
            })
            .expect("bounded status");
        assert_eq!(message.state, "retryable");
        assert_eq!(message.attempts, 2);
        assert_eq!(message.max_attempts, 3);
        assert!(message.lease_until.is_empty());
        assert!(!format!("{message:?}").contains("credential"));
        let mut invalid = FinanceBankProviderImportJobStatus {
            job_id: Uuid::from_u128(3),
            tenant_id: "tenant".into(),
            holding_ref: "holding".into(),
            import_id: Uuid::from_u128(4),
            state: "pending".into(),
            source_fence: 1,
            attempts: 0,
            max_attempts: 1,
            available_at: now,
            lease_until: None,
            last_failure_class: None,
            completed_at: None,
            updated_at: now,
        };
        invalid.state = "unknown".into();
        assert!(finance_bank_provider_import_status_message(invalid).is_err());
    }
}

#[cfg(test)]
mod directory_scope_tests {
    use super::*;

    #[test]
    fn directory_scope_accepts_platform_and_matching_opaque_holding_only() {
        let holding_ref = "holding-ref-42";
        let holding_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, holding_ref.as_bytes());
        let holding_scope = DirectoryScope::Holding {
            holding_ref: holding_ref.into(),
        };

        assert!(directory_scope_access(&Scope::Platform, &holding_scope).is_ok());
        assert!(directory_scope_access(&Scope::Holding { holding_id }, &holding_scope).is_ok());
        assert!(
            directory_scope_access(
                &Scope::Holding {
                    holding_id: Uuid::new_v4(),
                },
                &holding_scope,
            )
            .is_err()
        );
        assert!(
            directory_scope_access(
                &Scope::Company {
                    holding_id,
                    company_id: Uuid::new_v4(),
                },
                &holding_scope,
            )
            .is_err()
        );
    }

    #[test]
    fn directory_scope_parser_accepts_canonical_and_legacy_holding_inputs() {
        assert_eq!(
            parse_directory_scope("platform", "").expect("platform"),
            DirectoryScope::Platform
        );
        assert_eq!(
            parse_directory_scope("holding:holding-ref-42", "").expect("canonical"),
            DirectoryScope::Holding {
                holding_ref: "holding-ref-42".into()
            }
        );
        assert_eq!(
            parse_directory_scope("holding-ref-42", "holding-ref-42").expect("legacy"),
            DirectoryScope::Holding {
                holding_ref: "holding-ref-42".into()
            }
        );
        assert!(parse_directory_scope("holding:other", "holding-ref-42").is_err());
    }
}
