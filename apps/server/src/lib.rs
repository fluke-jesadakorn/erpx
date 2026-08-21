mod auth_actor;
mod contexts {
    pub mod assets_cqrs;
    pub mod brand_cqrs;
    pub mod commercial_cqrs;
    pub mod crm_cqrs;
    pub mod customer_360_cqrs;
    pub mod customer_authority_cqrs;
    pub mod finance_cqrs;
    pub mod identity_access;
    pub mod legal_cqrs;
    pub mod maintenance_cqrs;
    pub mod marketing_cqrs;
    pub mod operations_cqrs;
    pub mod organization_cqrs;
    pub mod people_payroll_cqrs;
    pub mod platform_administration_cqrs;
    pub mod pos_cqrs;
    pub mod procurement_inventory_cqrs;
    pub mod quality;
    pub mod sales_cqrs;
    pub mod support_cqrs;
}
mod future_agent_projects;
mod future_auth;
mod future_benefits;
mod future_ecommerce;
mod future_field_service;
mod future_learning;
mod future_manufacturing_planning;
mod future_talent;
mod holding_workspace;
mod operational_metrics;
mod persistence;
mod target_transport;
mod wave4_composition;
mod wave5_composition;
mod wave6_composition;
mod wave7_composition;
mod wave8_composition;
mod wave9_composition;

use contexts::quality::{
    AuthenticatedQualityRequest, QualityAccessMode, QualityAuthenticationError,
    QualityAuthenticationRequest, QualityCommandApplication, QualityCommandRpc,
    QualityHoldingRoute, QualityQueryApplication, QualityQueryRpc, QualityRequestAuthenticator,
};
use contexts::{
    assets_cqrs::{AssetsCommandRpc, AssetsQueryRpc},
    brand_cqrs::{BrandCommandRpc, BrandQueryRpc},
    commercial_cqrs::{
        CommercialCommandApplication, CommercialCommandRpc, CommercialQueryApplication,
        CommercialQueryRpc, CommercialRequestAuthenticator,
    },
    crm_cqrs::{
        CrmCommandApplication, CrmCommandRpc, CrmQueryApplication, CrmQueryRpc,
        CrmRequestAuthenticator,
    },
    customer_360_cqrs::{
        Customer360QueryApplication, Customer360QueryRpc, Customer360RequestAuthenticator,
    },
    customer_authority_cqrs::{
        CustomerAuthorityCommandRpc, CustomerAuthorityQueryApplication, CustomerAuthorityQueryRpc,
        CustomerAuthorityRequestAuthenticator,
    },
    finance_cqrs::{FinanceCommandRpc, FinanceQueryRpc},
    identity_access::{
        IdentityAccessCommandApplication, IdentityAccessCommandRpc, IdentityAccessQueryApplication,
        IdentityAccessQueryRpc, IdentityAccessRequestAuthenticator,
    },
    legal_cqrs::{LegalCommandRpc, LegalQueryRpc},
    maintenance_cqrs::{MaintenanceCommandRpc, MaintenanceQueryRpc},
    marketing_cqrs::{
        MarketingCommandApplication, MarketingCommandRpc, MarketingQueryApplication,
        MarketingQueryRpc, MarketingRequestAuthenticator,
    },
    operations_cqrs::{OperationsCommandRpc, OperationsQueryRpc},
    organization_cqrs::{OrganizationCommandRpc, OrganizationQueryRpc},
    people_payroll_cqrs::{PeoplePayrollCommandRpc, PeoplePayrollQueryRpc},
    platform_administration_cqrs::{
        PlatformAdministrationCommandRpc, PlatformAdministrationQueryRpc,
    },
    pos_cqrs::{PosCommandRpc, PosQueryRpc},
    procurement_inventory_cqrs::{
        InventoryCommandRpc, InventoryQueryRpc, ProcurementCommandRpc, ProcurementQueryRpc,
    },
    sales_cqrs::{SalesCommandRpc, SalesQueryRpc},
    support_cqrs::{SupportCommandRpc, SupportQueryRpc},
};
use holding_workspace::{
    PresentedHoldingRoute, ResolvedHoldingWorkspace, resolve_holding_workspace,
};
use persistence::seed_holding_owner;
use target_transport::{
    CollaborationCommandRpc, CollaborationQueryRpc, CollaborationStreamRpc, DataPlaneStreamRpc,
    ReferenceQueryRpc,
};
use wave4_composition::{
    CommercialProviderKey, CommercialServerApplication, CommercialServerAuthenticator,
    IdentityServerApplication, IdentityServerAuthenticator, OrganizationServerApplication,
    OrganizationServerAuthenticator,
};
use wave5_composition::{
    BrandServerApplication, BrandServerAuthenticator, CustomerAuthorityServerApplication,
    CustomerAuthorityServerAuthenticator, PlatformAdministrationServerApplication,
    PlatformAdministrationServerAuthenticator,
};
use wave6_composition::{Wave6HoldingApplication, Wave6ServerAuthenticator};
use wave7_composition::{Wave7HoldingApplication, Wave7ServerAuthenticator};
use wave8_composition::{Wave8HoldingApplication, Wave8ServerAuthenticator};
use wave9_composition::{Wave9Application, Wave9ServerAuthenticator};

use std::future::Future;
use std::sync::Arc;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    net::SocketAddr,
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use commercial_target::{
    commercial_command_service_server::CommercialCommandService as TargetCommercialCommandService,
    commercial_query_service_server::CommercialQueryService as TargetCommercialQueryService,
};
use crm_target::{
    crm_command_service_server::CrmCommandService as TargetCrmCommandService,
    crm_query_service_server::CrmQueryService as TargetCrmQueryService,
};
use customer_authority_target::customer_authority_query_service_server::CustomerAuthorityQueryService as TargetCustomerAuthorityQueryService;
use customer360_target::customer360_query_service_server::Customer360QueryService as TargetCustomer360QueryService;
use erpx_application::{
    AcknowledgeMarketingProviderVerificationCommand, ApplicationError, AuditEvent, AuditSink,
    AuthorizedMarketingProfilePrivacyPort, BindDomainCommand,
    ConfigureMarketingProviderAccountCommand, CustomerGlobalReadAuditDetailCursor,
    CustomerGlobalReadAuditDetailQuery, CustomerGlobalReadAuditDetailService,
    CustomerGlobalReadAuditQueryContext, CustomerReadView, CustomerSecurityAdminScope,
    DEFAULT_MARKETING_PROVIDER_PRIORITY, EraseMarketingProfileCommand,
    IssueDomainVerificationCommand, IssuePreviewCommand, MarketingContentWritePort,
    MarketingProfileErasureCorrelation, MarketingProviderChannelRoute, RevokePreviewCommand,
    RoleLifecycleService as RoleLifecycleUseCase, SetMarketingProviderAccountStateCommand,
    SubmitPublishedFormCommand, SupportIntersectionRequest, SupportIntersectionService,
    SystemClock, VerifyDomainCommand, authorize_and_erase_marketing_profile,
};
use erpx_cqrs::{
    CommandEnvelope as CqrsCommandEnvelope, CommandHandler, CommandReceipt as CqrsCommandReceipt,
    CqrsError, IdempotencyKey, QueryConsistency, QueryEnvelope as CqrsQueryEnvelope, QueryHandler,
    QueryResult as CqrsQueryResult,
};
use erpx_cqrs_sqlx::{ReadPool as CqrsReadPool, WritePool as CqrsWritePool};
use erpx_customer_360::{
    Customer360OwnedRecord, Customer360RecordCommand, Customer360RecordKind,
    Customer360RecordUnitOfWork,
};
use erpx_customer_360_sqlx::Customer360RecordSqlxAdapter;
use erpx_customer_authority::{CustomerId as AuthorityCustomerId, WriteRepository as _};
use erpx_customer_authority_sqlx::CustomerAuthorityWriteRepository;
use erpx_domain::{
    access::{
        EffectiveAccessRequest, HOLDING_OWNER_ROLE, RiskTier, RoleAssignment,
        RoleExpiryNotificationKind, RoleReviewItem, evaluate_effective_access_chain,
    },
    commercial::{
        ACTIVE_CATALOG_CURRENCY, ActivateBranchModuleCommand, ActivateCompanyModuleCommand,
        AllocateCompanyCommand, BillingInterval, BillingState, CatalogPriceKind, ChargeKind,
        CommercialCommandContext, EntitlementDecision, PricingUnit,
        SubscriptionItemSelection as DomainSubscriptionItemSelection, active_subscription_currency,
        authorize_module,
    },
    contracts::{
        ActorAttribution, ActorKind, AuthorityScope, AuthorityScope as Scope, CONTRACT_VERSION,
        CommandContext,
    },
    customer::{
        Customer360DetailReadContext, CustomerGlobalReadContext, CustomerNoteCursor,
        CustomerStatus, GlobalReadActor, GlobalReadAuditRecord, GlobalReadScope, HealthState,
    },
    data_plane::{
        BusinessUnitId, BusinessUnitRoleTemplate, ConfigurationLayer,
        directory_role_template_is_allowed,
    },
    holding_administration::{HoldingAdminAuthorizationRequest, HoldingOrganizationScope},
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
    marketing_content::{
        ContentFormRef, ContentLocale, ContentTemplateRef, CustomDomainBinding, PreviewGrant,
    },
    marketing_crm::{AcceptedLeadHandoff, AttributionModel, CrmOpportunity},
    marketing_delivery::DeliveryState,
    platform_completion::{
        BackupControlCommand, HoldingBinding, HoldingLifecycleAction, HoldingLifecycleCommand,
        PitrRecoveryPoint, PlatformAuthorizationProof, PlatformBackupKind,
    },
    support::{SupportCapabilitySecret, SupportDomainPolicy, SupportMode},
};
use erpx_identity::{
    AccessTokenClaims, AccessTokenSigner, DIRECTORY_PREFLIGHT_MAX_AGE, DIRECTORY_REVOCATION_SLA,
    DirectoryConnector, DirectoryConnectorLifecycle, DirectoryFeedKind,
    DirectoryIdentityConflictSnapshot, DirectoryIdentityReplacementDecision,
    DirectoryIdentityReplacementDecisionAction, DirectoryProvider, DirectoryRoleAccess,
    DirectoryRoleMapping, DirectoryScope, DirectorySecretResolver, DirectoryTlsMode,
    DirectoryTlsPolicy, HttpsDirectorySecretResolver, IdentityStoreError, LiveDirectoryError,
    LocalDirectorySecretResolver, PasskeyCeremonyService, SqlxDirectoryStore, SqlxIdentityStore,
    TotpKeyResolver, UnconfiguredDirectorySecretResolver, WebAuthnProvider, WebAuthnProviderConfig,
    WebAuthnRsProvider, apply_identity_migration, directory_identity_replacement_decision_ref,
    preflight_directory_connector,
};
use erpx_infrastructure::{
    ClusterPitrRepository, ControlPlaneDatabaseConfig, ControlPlanePools, DeadLetterReplayCommand,
    EnvMarketingCallbackSecretResolver, HoldingAdminContextIssue,
    HoldingBillingGraceExtensionGrant, HoldingDatabaseProvisioner,
    HoldingDatabaseProvisioningConfig, HoldingDatabaseRouter, HoldingHoldingBinding,
    HoldingProjectionSchedulerRepository, HoldingProjectionStatusReadRepository,
    HoldingProvisioner, HoldingRegistryFilter, HoldingRegistrySummary, HoldingSourceProjection,
    HoldingStoreRegistration, HttpMarketingCallbackSecretManager, MAX_CLUSTER_PITR_PAGE_SIZE,
    MAX_CRM_READ_PAGE_SIZE, MAX_HOLDING_REGISTRY_PAGE_SIZE, MAX_MARKETING_CONTENT_READ_PAGE_SIZE,
    MAX_PLATFORM_AUDIT_PAGE_SIZE, MAX_PROJECTION_STATUS_FRESHNESS_SECONDS,
    MAX_PROJECTION_STATUS_PAGE_SIZE, MarketingCallbackSecretResolver,
    MarketingPublicAbuseWindowRequest, ObjectAccessBindingRequest, PersistenceError,
    PlatformControlStore, PlatformIntegrationObservation, ProjectionPolicy,
    ProjectionSchedulerAuthorization, ProjectionStatusCursor, ReadinessObservationState,
    ReadinessState, RoutedDatabaseSide, RoutedHoldingDatabase, SqlxBranchProvisioningStore,
    SqlxCommercialStore, SqlxControlPlane, SqlxCorporateIdentityStore, SqlxG13MigrationStore,
    SqlxHoldingAdministrationStore, SqlxHoldingStoreRegistry, SqlxMarketingContentStore,
    SqlxMarketingContentWriteAdapter, SqlxMarketingCrmStore, SqlxMarketingDeliveryStore,
    SqlxMarketingProfilePrivacyAdapter, SqlxMarketingProviderOnboardingStore, SqlxMarketingStore,
    StripeApiKey, StripeCheckoutLineItem, StripeCheckoutMode, StripeCheckoutSessionCreateRequest,
    StripeCustomerPortalSessionCreateRequest, StripeHttpClient, StripeInboxInsert,
    StripeProviderConfig, StripeWebhookEvent, StripeWebhookPersistenceContext,
    SupportCapabilityAuthorizationRequest, UnconfiguredMarketingCallbackSecretResolver,
    WorkerHeartbeatReadiness, WorkerLiveReadinessPolicy, WorkerReadinessComposition,
    holding_holding_scope_id,
};
use erpx_kernel::{
    ActorAttribution as KernelActorAttribution, AuditContext as KernelAuditContext,
    AuthorityScope as KernelHoldingScope, CommandId as KernelCommandId,
    CorrelationId as KernelCorrelationId, ErrorCategory as KernelErrorCategory,
    ErrorContext as KernelErrorContext, HumanDecisionProof, KernelError,
    RecordId as KernelRecordId, SessionId as KernelSessionId, SystemClock as KernelSystemClock,
};
use erpx_operations::SourceHighWatermarkPort;
use erpx_operations_sqlx::EXCEPTION_PROJECTION as OPERATIONS_READ_PROJECTION;
use erpx_operations_sqlx::OperationsSourceHighWatermarkReader;
use erpx_organization::application::MAX_ORGANIZATION_LIST_PAGE_SIZE;
use erpx_organization::{
    NodeListFilter as OrganizationNodeListFilter,
    OrganizationNodeKind as TypedOrganizationNodeKind,
    OrganizationPlacement as TypedOrganizationPlacement, ReadRepository as _,
};
use erpx_organization_sqlx::OrganizationReadRepository;
use erpx_proto::erpx::{
    commercial as commercial_target, crm as crm_target, customer_360 as customer360_target,
    customer_authority as customer_authority_target, identity_access as identity_target,
    marketing as marketing_target, platform_administration as platform_admin_target,
};
use erpx_quality::ports::AuthorizationRequest as QualityAuthorizationRequest;
use erpx_quality::{
    AuthorizationPort as QualityAuthorizationPort, EntitlementPort as QualityEntitlementPort,
    PortError as QualityPortError, QualityCommandHandler, QualityQueryHandler,
};
use erpx_quality_sqlx::{QualityCommandAdapter, QualityReadRepository, QualityWriteRepository};
use erpx_support_sqlx::SupportWriteRepository;
use hmac::{Hmac, Mac};
use identity_target::{
    identity_access_command_service_server::IdentityAccessCommandService as TargetIdentityAccessCommandService,
    identity_access_query_service_server::IdentityAccessQueryService as TargetIdentityAccessQueryService,
};
use marketing_target::{
    marketing_command_service_server::MarketingCommandService as TargetMarketingCommandService,
    marketing_query_service_server::MarketingQueryService as TargetMarketingQueryService,
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tonic::{Request, Response, Status};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

mod boundary;
mod data_plane_rpc;
mod errors;
mod identity_http;
mod migration;
mod platform_completion;
pub mod quote_pdf;
mod storage;
use boundary::{command_domain, query as query_context, transport};
use platform_completion::{
    PlatformBackupApiRequest, PlatformCompletionApi, PlatformLifecycleApiRequest,
};
use storage::{
    HoldingObjectDownloadRequest, HoldingObjectUploadRequest, ObjectAccessError,
    ObjectAuthorizationError, StorageComposition,
};

const PLATFORM_CUSTOMER_READ_PERMISSION: &str = "platform.customer.read";
const PLATFORM_CUSTOMER_ACCESS_AUDIT_PERMISSION: &str = "platform.customer.access_audit.read";
const PLATFORM_CUSTOMER_NOTE_WRITE_PERMISSION: &str = "platform.customer.notes.write";
const PLATFORM_HOLDING_READ_PERMISSION: &str = "platform.holding.read";
const PLATFORM_HOLDING_HISTORY_READ_PERMISSION: &str = "platform.holding.history.read";

const PLATFORM_HOLDING_ENTER_PERMISSION: &str = "platform.holding.enter";
const PLATFORM_HOLDING_PROVISION_PERMISSION: &str = "platform.holding.provision";
const PLATFORM_HOLDING_BACKUP_PERMISSION: &str = "platform.holding.backup";
const PLATFORM_HOLDING_SUSPEND_PERMISSION: &str = "platform.holding.suspend";
const PLATFORM_HOLDING_RECOVERY_PERMISSION: &str = "platform.holding.recovery";
const PLATFORM_PROJECTION_REBUILD_PERMISSION: &str = "platform.projection.rebuild";
const PLATFORM_DIRECTORY_READ_PERMISSION: &str = "platform.directory.read";
const PLATFORM_DIRECTORY_MANAGE_PERMISSION: &str = "platform.directory.manage";
const PLATFORM_DIRECTORY_ACTIVATE_PERMISSION: &str = "platform.directory.activate";
const HOLDING_DIRECTORY_READ_PERMISSION: &str = "holding.directory.read";
const HOLDING_DIRECTORY_MANAGE_PERMISSION: &str = "holding.directory.manage";
const HOLDING_DIRECTORY_ACTIVATE_PERMISSION: &str = "holding.directory.activate";

fn directory_permission(scope: &Scope, operation: &str) -> &'static str {
    match (scope, operation) {
        (Scope::Platform, "read") => PLATFORM_DIRECTORY_READ_PERMISSION,
        (Scope::Platform, "activate") => PLATFORM_DIRECTORY_ACTIVATE_PERMISSION,
        (Scope::Platform, _) => PLATFORM_DIRECTORY_MANAGE_PERMISSION,
        (_, "read") => HOLDING_DIRECTORY_READ_PERMISSION,
        (_, "activate") => HOLDING_DIRECTORY_ACTIVATE_PERMISSION,
        _ => HOLDING_DIRECTORY_MANAGE_PERMISSION,
    }
}
const MAX_DIRECTORY_IDENTITY_LINK_PAGE_SIZE: u32 = 500;
const PLATFORM_OUTBOX_REPLAY_PERMISSION: &str = "platform.outbox.replay";
const ENTITLEMENT_ALLOCATE_PERMISSION: &str = "commercial.entitlement.allocate";
const ENTITLEMENT_ACTIVATE_PERMISSION: &str = "commercial.entitlement.activate";
const COMMERCIAL_BILLING_READ_PERMISSION: &str = "commercial.billing.read";
const COMMERCIAL_BILLING_MANAGE_PERMISSION: &str = "commercial.billing.manage";
const COMMERCIAL_BILLING_GRACE_EXTEND_PERMISSION: &str = "commercial.billing.grace.extend";
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

fn platform_holding_page_size(value: u32) -> Result<u32, Status> {
    match value {
        0 => Ok(25),
        value if value <= MAX_HOLDING_REGISTRY_PAGE_SIZE => Ok(value),
        _ => Err(Status::invalid_argument(
            "platform holding page size exceeds the allowed bound",
        )),
    }
}

fn optional_registry_filter(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn platform_holding_summary(
    holding: HoldingRegistrySummary,
) -> platform_admin_target::PlatformHoldingSummary {
    let enterable = holding.enterable();
    platform_admin_target::PlatformHoldingSummary {
        holding_id: holding.holding_id,
        state: holding.holding_state,
        registry_version: 0,
        routing: "control-plane projection".into(),
        projection: holding.projection_freshness,
        backup_pitr: holding.backup_state,
        storage: holding.storage_state,
        customer_id: holding
            .customer_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        canonical_holding_id: holding
            .canonical_holding_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        customer_name: holding.customer_name,
        configuration_state: holding.configuration_state,
        subscription_state: holding.subscription_state,
        plan_key: holding.plan_key,
        enabled_modules: holding.enabled_modules,
        purchased_seats: holding.purchased_seats,
        used_seats: holding.used_seats,
        company_count: holding.company_count,
        branch_count: holding.branch_count,
        renewal_at: holding
            .renewal_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        overdue: holding.overdue,
        integration_health: holding.integration_health,
        health: holding.health,
        projected_at: holding.projected_at.to_rfc3339(),
        enterable,
    }
}

fn holding_admin_context_message(
    context: &erpx_domain::holding_administration::HoldingAdminContext,
    at: DateTime<Utc>,
) -> platform_admin_target::HoldingAdminContext {
    platform_admin_target::HoldingAdminContext {
        administrative_context_id: context.id.to_string(),
        holding_id: context.holding_id.clone(),
        customer_id: context.customer_id.to_string(),
        canonical_holding_id: context.canonical_holding_id.to_string(),
        issued_at: context.issued_at.to_rfc3339(),
        expires_at: context.expires_at.to_rfc3339(),
        active: context.revoked_at.is_none() && at >= context.issued_at && at < context.expires_at,
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
    router: &HoldingDatabaseRouter,
    holding_id: &str,
    data_profile_key: &str,
) -> Result<u64, PersistenceError> {
    let write = router
        .route(holding_id, data_profile_key, RoutedDatabaseSide::Write)
        .await?;
    let holding_uuid = Uuid::parse_str(holding_id).map_err(|_| {
        PersistenceError::InvalidData("current Operations source requires a UUID holding_id".into())
    })?;
    let pool = CqrsWritePool::for_holding(write.database_pool().clone(), holding_uuid)
        .map_err(|_| PersistenceError::Configuration("Operations holding binding is invalid"))?;
    let result = OperationsSourceHighWatermarkReader::new(pool)
        .source_high_watermark()
        .await
        .map_err(|_| PersistenceError::Configuration("Operations source fence is unavailable"));
    write.close().await;
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
) -> Vec<customer360_target::Customer360SectionState> {
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
    .map(
        |(section, state)| customer360_target::Customer360SectionState {
            section: section.into(),
            state: state.into(),
        },
    )
    .collect()
}

const CUSTOMER_DETAIL_FACT_MAX_BYTES: usize = 256;

fn customer_detail_fact(
    section: &'static str,
    key: &'static str,
    value: impl Into<String>,
) -> customer360_target::Customer360SectionFact {
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
    customer360_target::Customer360SectionFact {
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
) -> Vec<customer360_target::Customer360SectionFact> {
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
    let development_controller_attribution = supplied_principal
        == erpx_identity::DEVELOPMENT_CONTROLLER_PRINCIPAL
        && authenticated_principal.starts_with("dev:")
        && crate::identity_http::development_gates().is_ok();
    if authenticated_principal == supplied_principal || development_controller_attribution {
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

fn command_mfa_code(context: Option<&erpx_proto::common::CommandContext>) -> &str {
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

fn parse_directory_lifecycle(value: &str) -> Result<DirectoryConnectorLifecycle, Status> {
    match value.trim() {
        "draft" => Ok(DirectoryConnectorLifecycle::Draft),
        "verified" => Ok(DirectoryConnectorLifecycle::Verified),
        "importing" => Ok(DirectoryConnectorLifecycle::Importing),
        "ready" => Ok(DirectoryConnectorLifecycle::Ready),
        "active" => Ok(DirectoryConnectorLifecycle::Active),
        "suspended" => Ok(DirectoryConnectorLifecycle::Suspended),
        "failed" => Ok(DirectoryConnectorLifecycle::Failed),
        _ => Err(Status::invalid_argument(
            "directory connector lifecycle is unsupported",
        )),
    }
}

fn parse_directory_json_object(bytes: &[u8], field: &str) -> Result<Value, Status> {
    if bytes.len() > 64 * 1024 {
        return Err(Status::invalid_argument(format!(
            "{field} exceeds the allowed bound"
        )));
    }
    if bytes.is_empty() {
        return Ok(serde_json::json!({}));
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| Status::invalid_argument(format!("{field} is invalid JSON")))?;
    if !value.is_object() {
        return Err(Status::invalid_argument(format!(
            "{field} must be a JSON object"
        )));
    }
    Ok(value)
}

fn parse_directory_scope(
    scope_key: &str,
    data_profile_key: &str,
) -> Result<DirectoryScope, Status> {
    if let Some(branch) = scope_key.trim().strip_prefix("branch:") {
        let mut parts = branch.rsplitn(3, ':');
        let branch_id = parts
            .next()
            .ok_or_else(|| Status::invalid_argument("directory branch scope is invalid"))
            .and_then(|value| parse_uuid(value, "branch_id"))?;
        let company_id = parts
            .next()
            .ok_or_else(|| Status::invalid_argument("directory branch scope is invalid"))
            .and_then(|value| parse_uuid(value, "company_id"))?;
        let parsed_holding = parts
            .next()
            .ok_or_else(|| Status::invalid_argument("directory branch scope is invalid"))?;
        if !data_profile_key.trim().is_empty() && data_profile_key.trim() != parsed_holding {
            return Err(Status::invalid_argument(
                "directory branch scope and data_profile_key do not match",
            ));
        }
        return Ok(DirectoryScope::Branch {
            data_profile_key: parsed_holding.into(),
            company_id,
            branch_id,
        });
    }
    if scope_key.trim() == "platform" && data_profile_key.trim().is_empty() {
        return Ok(DirectoryScope::Platform);
    }
    let holding = if data_profile_key.trim().is_empty() {
        scope_key.trim()
    } else {
        data_profile_key.trim()
    }
    .strip_prefix("holding:")
    .unwrap_or_else(|| {
        if data_profile_key.trim().is_empty() {
            scope_key.trim()
        } else {
            data_profile_key.trim()
        }
    });
    if scope_key.trim().starts_with("holding:")
        && !data_profile_key.trim().is_empty()
        && scope_key.trim() != format!("holding:{holding}")
    {
        return Err(Status::invalid_argument(
            "directory holding scope and data_profile_key do not match",
        ));
    }
    let holding = holding.trim().to_owned();
    if holding.is_empty() || holding.len() > 255 || holding.chars().any(char::is_control) {
        return Err(Status::invalid_argument(
            "directory holding scope is invalid",
        ));
    }
    Ok(DirectoryScope::Holding {
        data_profile_key: holding,
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
) -> identity_target::DirectoryConnectorSummary {
    let at = Utc::now();
    let ready = snapshot.lifecycle == DirectoryConnectorLifecycle::Active
        && snapshot.unresolved_conflicts == 0
        && snapshot.last_preflight_at.is_some_and(|value| {
            value <= at && at.signed_duration_since(value) <= DIRECTORY_PREFLIGHT_MAX_AGE
        })
        && snapshot.last_heartbeat_at.is_some_and(|value| {
            value <= at && at.signed_duration_since(value) <= DIRECTORY_REVOCATION_SLA
        });
    identity_target::DirectoryConnectorSummary {
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
        lifecycle: snapshot.lifecycle.as_str().into(),
        readiness: if ready { "ready" } else { "not_ready" }.into(),
        last_preflight_at: snapshot
            .last_preflight_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        last_heartbeat_at: snapshot
            .last_heartbeat_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        credential_profile_key: snapshot.credential_profile_key,
        parent_connector_id: snapshot
            .parent_connector_id
            .map(|value| value.to_string())
            .unwrap_or_default(),
        directory_authority_fingerprint: snapshot
            .directory_authority_fingerprint
            .unwrap_or_default(),
        fallback_to_holding: snapshot.fallback_to_holding,
        data_profile_key: snapshot.data_profile_key.unwrap_or_default(),
        company_id: snapshot
            .company_id
            .map(|value| value.to_string())
            .unwrap_or_default(),
        branch_id: snapshot
            .branch_id
            .map(|value| value.to_string())
            .unwrap_or_default(),
    }
}

fn directory_conflict_summary(
    conflict: DirectoryIdentityConflictSnapshot,
) -> identity_target::DirectoryIdentityConflictSummary {
    identity_target::DirectoryIdentityConflictSummary {
        conflict_id: conflict.conflict_id.to_string(),
        connector_id: conflict.connector_id.to_string(),
        local_principal_ref: conflict.local_principal_ref,
        match_reasons: conflict.match_reasons,
        state: conflict.state,
        proposal_state: conflict.proposal_state.unwrap_or_default(),
        proposed_by: conflict.proposed_by.unwrap_or_default(),
        conflict_version: conflict.conflict_version,
        detected_at: conflict.detected_at.to_rfc3339(),
        proposed_at: conflict
            .proposed_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
    }
}

async fn authorize_directory_replacement_role(
    store: &SqlxControlPlane,
    principal_ref: &str,
    scope: &Scope,
    proposal: bool,
) -> Result<(), Status> {
    if matches!(scope, Scope::Platform) {
        return Ok(());
    }
    let allowed_roles: &[&str] = if proposal {
        &[HOLDING_OWNER_ROLE]
    } else {
        &[HOLDING_OWNER_ROLE, "customer_security_head"]
    };
    if !store
        .principal_has_role_key(principal_ref, scope, allowed_roles, Utc::now())
        .await
        .map_err(|_| Status::internal("directory replacement role lookup failed"))?
    {
        return Err(Status::permission_denied(if proposal {
            "directory replacement proposals require an active Holding Owner"
        } else {
            "directory replacement approval requires an active Holding Owner or Customer Security Head"
        }));
    }
    Ok(())
}

fn directory_audit_action(scope: &Scope, operation: &str) -> String {
    let prefix = if matches!(scope, Scope::Platform) {
        "platform.directory"
    } else {
        "holding.directory"
    };
    format!("{prefix}.{operation}")
}

async fn directory_audit_customer(
    store: &SqlxControlPlane,
    scope: &Scope,
) -> Result<Option<Uuid>, Status> {
    let Scope::Holding { holding_id } = scope else {
        return Ok(None);
    };
    persistence::directory_audit_customer(store, *holding_id)
        .await
        .map_err(|_| Status::internal("directory customer audit scope lookup failed"))
}

fn directory_import_status_message(
    status: erpx_identity::DirectoryImportStatus,
) -> identity_target::DirectoryImportStatus {
    identity_target::DirectoryImportStatus {
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
    holding_router: Option<Arc<HoldingDatabaseRouter>>,
    storage: Arc<StorageComposition>,
    roles: Arc<RoleLifecycleUseCase<SqlxControlPlane, SystemClock>>,
    platform_completion: Arc<PlatformCompletionApi>,
    migration: Arc<SqlxG13MigrationStore>,
}

#[derive(Clone)]
enum ReadinessTarget {
    NotConfigured,
    InvalidConfiguration,
    Configured {
        holding_id: Option<String>,
        router: Arc<HoldingDatabaseRouter>,
    },
}

const MAX_READINESS_HOLDINGS: usize = 1_000;

fn system_readiness_required(development_profile: Option<&str>) -> bool {
    !matches!(development_profile, Some("local" | "local-fresh-state"))
}

#[derive(Clone)]
struct ServerReadinessAdapter {
    store: Arc<SqlxControlPlane>,
    registry: Arc<SqlxHoldingStoreRegistry>,
    worker_id: Option<Uuid>,
    target: ReadinessTarget,
    policy: WorkerLiveReadinessPolicy,
    required: bool,
}

impl ServerReadinessAdapter {
    fn from_env(store: Arc<SqlxControlPlane>, registry: Arc<SqlxHoldingStoreRegistry>) -> Self {
        let worker_id = std::env::var("ERPX_READINESS_WORKER_ID")
            .ok()
            .and_then(|value| Uuid::parse_str(&value).ok())
            .filter(|worker_id| !worker_id.is_nil());
        let holding_id = std::env::var("ERPX_READINESS_HOLDING_ID")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let admin_database_url = std::env::var("ERPX_HOLDING_ADMIN_DATABASE_URL")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let database_url_template = std::env::var("ERPX_HOLDING_DATABASE_URL_TEMPLATE")
            .ok()
            .filter(|value| !value.trim().is_empty());

        let target = match (holding_id, admin_database_url, database_url_template) {
            (holding_id, Some(admin_database_url), Some(database_url_template)) => {
                let config = HoldingDatabaseProvisioningConfig {
                    admin_database_url,
                    database_url_template,
                };
                let router = HoldingDatabaseRouter::new(store.clone(), config);
                match router {
                    Ok(router) => ReadinessTarget::Configured {
                        holding_id,
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
            required: system_readiness_required(
                std::env::var("ERPX_DEVELOPMENT_PROFILE").ok().as_deref(),
            ),
        }
    }

    fn unavailable_composition() -> WorkerReadinessComposition {
        WorkerReadinessComposition {
            worker_heartbeat: ReadinessState::Unavailable,
            holding_routing: ReadinessState::Unavailable,
            projection_freshness: ReadinessState::Unavailable,
            backup_pitr: ReadinessState::Unavailable,
        }
    }

    fn not_probed_composition() -> WorkerReadinessComposition {
        WorkerReadinessComposition {
            worker_heartbeat: ReadinessState::NotProbed,
            holding_routing: ReadinessState::NotProbed,
            projection_freshness: ReadinessState::NotProbed,
            backup_pitr: ReadinessState::NotProbed,
        }
    }

    async fn observe(&self) -> WorkerReadinessComposition {
        if !self.required {
            return Self::not_probed_composition();
        }
        let ReadinessTarget::Configured { holding_id, router } = &self.target else {
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

        let holding_ids = match holding_id {
            Some(holding_id) => vec![holding_id.clone()],
            None => match self
                .registry
                .list_active_bounded(MAX_READINESS_HOLDINGS)
                .await
            {
                Ok(registrations) => registrations
                    .into_iter()
                    .map(|registration| registration.holding_id)
                    .collect(),
                Err(_) => {
                    // Do not expose database URLs, SQLx details, credentials, or
                    // registry references through the unauthenticated endpoint.
                    tracing::warn!("active holding readiness enumeration failed");
                    return WorkerReadinessComposition {
                        worker_heartbeat,
                        holding_routing: ReadinessState::Unavailable,
                        projection_freshness: ReadinessState::Unavailable,
                        backup_pitr: ReadinessState::Unavailable,
                    };
                }
            },
        };

        let mut observations = Vec::with_capacity(holding_ids.len());
        for holding_id in holding_ids {
            match router
                .observe_holding_readiness(&holding_id, now, self.policy.holding)
                .await
            {
                Ok(observation) => observations.push(observation),
                Err(_) => {
                    // Do not expose database URLs, SQLx details, credentials, or
                    // registry references through the unauthenticated endpoint.
                    tracing::warn!("holding readiness observation failed");
                    return WorkerReadinessComposition {
                        worker_heartbeat,
                        holding_routing: ReadinessState::Unavailable,
                        projection_freshness: ReadinessState::Unavailable,
                        backup_pitr: ReadinessState::Unavailable,
                    };
                }
            }
        }

        WorkerReadinessComposition {
            worker_heartbeat,
            holding_routing: aggregate_readiness_states(
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
    directory_secrets: Arc<dyn DirectorySecretResolver>,
    signer: Arc<AccessTokenSigner>,
    passkeys: Option<Arc<PasskeyCeremonyService<Arc<dyn WebAuthnProvider>>>>,
    totp_keys: Option<Arc<TotpKeyResolver>>,
    development_controller: Option<erpx_identity::DevelopmentControllerRoleConfig>,
}

struct DevelopmentMfaAuthorization<'a> {
    holding_id: &'a str,
    correlation_id: Uuid,
    validated_development_persona: bool,
    validated_role_permission: bool,
}

fn development_mfa_bypass_actor_is_authorized(
    config: Option<&erpx_identity::DevelopmentControllerRoleConfig>,
    principal_ref: &str,
    has_active_role: bool,
    holding_id: &str,
    validated_development_persona: bool,
    validated_role_permission: bool,
) -> bool {
    if !config.is_some_and(|config| {
        config.mfa_bypass && config.target_holding == holding_id && validated_role_permission
    }) {
        return false;
    }
    (principal_ref == erpx_identity::DEVELOPMENT_CONTROLLER_PRINCIPAL && has_active_role)
        || (validated_development_persona
            && principal_ref.starts_with(&format!("dev:{holding_id}:")))
}

#[cfg(test)]
mod development_mfa_bypass_tests {
    use super::*;

    fn config(holding_id: &str) -> erpx_identity::DevelopmentControllerRoleConfig {
        erpx_identity::DevelopmentControllerRoleConfig {
            target_holding: holding_id.into(),
            mfa_bypass: true,
            sod_bypass: true,
        }
    }

    #[test]
    fn bypass_intersects_local_gate_holding_and_authorized_business_role() {
        let holding_id = erpx_identity::DEVELOPMENT_CONTROLLER_TARGET_HOLDING;
        let config = config(holding_id);
        assert!(development_mfa_bypass_actor_is_authorized(
            Some(&config),
            erpx_identity::DEVELOPMENT_CONTROLLER_PRINCIPAL,
            true,
            holding_id,
            false,
            true,
        ));
        assert!(!development_mfa_bypass_actor_is_authorized(
            Some(&config),
            erpx_identity::DEVELOPMENT_CONTROLLER_PRINCIPAL,
            false,
            holding_id,
            false,
            true,
        ));
        assert!(!development_mfa_bypass_actor_is_authorized(
            Some(&config),
            erpx_identity::DEVELOPMENT_CONTROLLER_PRINCIPAL,
            true,
            holding_id,
            false,
            false,
        ));
        assert!(!development_mfa_bypass_actor_is_authorized(
            Some(&config),
            erpx_identity::DEVELOPMENT_CONTROLLER_PRINCIPAL,
            true,
            "another-holding",
            false,
            true,
        ));
    }

    #[test]
    fn bypass_accepts_only_a_server_validated_persona_for_the_exact_holding() {
        let holding_id = erpx_identity::DEVELOPMENT_CONTROLLER_TARGET_HOLDING;
        let config = config(holding_id);
        let persona = format!("dev:{holding_id}:customer_security_head");
        assert!(development_mfa_bypass_actor_is_authorized(
            Some(&config),
            &persona,
            false,
            holding_id,
            true,
            true,
        ));
        assert!(!development_mfa_bypass_actor_is_authorized(
            Some(&config),
            &persona,
            false,
            holding_id,
            false,
            true,
        ));
        assert!(!development_mfa_bypass_actor_is_authorized(
            None, &persona, false, holding_id, true, true,
        ));
    }
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
        let mut effective_claims = claims.clone();
        let context_token = request
            .metadata()
            .get(erpx_identity::DEVELOPMENT_CONTEXT_HEADER)
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty());
        let development_context = if let Some(context_token) = context_token {
            if crate::identity_http::development_gates().is_err() {
                return Err(Status::permission_denied(
                    "development persona mode is unavailable",
                ));
            }
            if claims.principal_ref != erpx_identity::DEVELOPMENT_CONTROLLER_PRINCIPAL
                || claims.session_id.to_string() != session
            {
                return Err(Status::permission_denied(
                    "development context is not bound to the controller session",
                ));
            }
            let context = self
                .identity
                .get_development_persona_context(context_token, Utc::now())
                .await
                .map_err(|_| Status::permission_denied("development context is inactive"))?;
            Some(context)
        } else {
            None
        };
        if let Some(context) = development_context.as_ref() {
            let device_id = self
                .identity
                .active_session_device_id(claims.session_id, Utc::now())
                .await
                .map_err(|_| Status::unauthenticated("controller session is inactive"))?;
            if context.controller_principal != claims.principal_ref
                || context.controller_session_id != claims.session_id
                || context.controller_device_id != device_id
                || !context.active_at(Utc::now())
                || !self
                    .identity
                    .principal_is_active(&context.persona.principal_ref)
                    .await
                    .map_err(|_| Status::internal("development persona lookup failed"))?
            {
                return Err(Status::permission_denied(
                    "development persona is inactive or no longer assigned",
                ));
            }
        }
        let has_development_context = development_context.is_some();
        if let Some(context) = development_context {
            effective_claims.principal_ref = context.persona.principal_ref;
            // Explicit QA persona contexts authorize through that persona's
            // own normal assignments. The controller's selected role must not
            // be inherited or unioned into the persona request.
            effective_claims.active_role = None;
            effective_claims.authorization_context_version = None;
        }
        if !has_development_context {
            let role = claims
                .active_role
                .as_ref()
                .ok_or_else(|| Status::permission_denied("role_selection_required"))?;
            let context_version = claims
                .authorization_context_version
                .ok_or_else(|| Status::permission_denied("role_selection_required"))?;
            let role_valid = self
                .identity
                .active_role_is_valid(&claims.principal_ref, role, Utc::now())
                .await
                .map_err(|_| Status::internal("active role validation failed"))?;
            if !role_valid {
                let _ = self
                    .identity
                    .invalidate_active_role(
                        &claims.principal_ref,
                        claims.session_id,
                        role,
                        context_version,
                        Utc::now(),
                    )
                    .await;
                return Err(Status::permission_denied("role_selection_required"));
            }
        }
        let actor_matches = effective_claims.principal_ref == actor
            || (has_development_context
                && claims.principal_ref == actor
                && claims.principal_ref == erpx_identity::DEVELOPMENT_CONTROLLER_PRINCIPAL);
        if !actor_matches || effective_claims.session_id.to_string() != session {
            return Err(Status::permission_denied(
                "access token attribution does not match transport identity",
            ));
        }
        Ok(effective_claims)
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

    async fn authorize_mfa_for_holding(
        &self,
        store: &SqlxControlPlane,
        claims: &AccessTokenClaims,
        holding_id: &str,
        presented_code: &str,
        correlation_id: Uuid,
    ) -> Result<(), Status> {
        if self.development_controller.as_ref().is_some_and(|config| {
            config.mfa_bypass
                && config.target_holding == holding_id
                && claims.principal_ref == erpx_identity::DEVELOPMENT_CONTROLLER_PRINCIPAL
        }) {
            let role = claims
                .active_role
                .as_ref()
                .ok_or_else(|| Status::permission_denied("role_selection_required"))?;
            if self
                .identity
                .selected_role_access_for_holding(
                    &claims.principal_ref,
                    role,
                    &Scope::Platform,
                    erpx_identity::DEVELOPMENT_MFA_BYPASS_PERMISSION,
                    Some(holding_id),
                    Utc::now(),
                )
                .await
                .map_err(|_| Status::internal("development MFA authorization lookup failed"))?
            {
                store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref.clone(),
                        customer_account_id: None,
                        action: "development.authorization.mfa_bypass".into(),
                        purpose: format!("local holding target {holding_id}"),
                        correlation_id,
                    })
                    .await
                    .map_err(|_| Status::unavailable("development authorization audit failed"))?;
                return Ok(());
            }
        }
        self.authorize_mfa(claims, presented_code).await
    }

    async fn authorize_mfa_for_holding_development_context(
        &self,
        store: &SqlxControlPlane,
        claims: &AccessTokenClaims,
        presented_code: &str,
        authorization: DevelopmentMfaAuthorization<'_>,
    ) -> Result<(), Status> {
        if development_mfa_bypass_actor_is_authorized(
            self.development_controller.as_ref(),
            &claims.principal_ref,
            claims.active_role.is_some(),
            authorization.holding_id,
            authorization.validated_development_persona,
            authorization.validated_role_permission,
        ) {
            store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref.clone(),
                    customer_account_id: None,
                    action: "development.authorization.mfa_bypass".into(),
                    purpose: format!(
                        "validated local command role and holding target {}",
                        authorization.holding_id
                    ),
                    correlation_id: authorization.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("development authorization audit failed"))?;
            return Ok(());
        }
        self.authorize_mfa_for_holding(
            store,
            claims,
            authorization.holding_id,
            presented_code,
            authorization.correlation_id,
        )
        .await
    }

    async fn authorize_local_sod_exception(
        &self,
        store: &SqlxControlPlane,
        claims: &AccessTokenClaims,
        holding_id: &str,
        correlation_id: Uuid,
    ) -> Result<bool, Status> {
        if !self.development_controller.as_ref().is_some_and(|config| {
            config.sod_bypass
                && config.target_holding == holding_id
                && claims.principal_ref == erpx_identity::DEVELOPMENT_CONTROLLER_PRINCIPAL
        }) {
            return Ok(false);
        }
        let Some(role) = claims.active_role.as_ref() else {
            return Ok(false);
        };
        if !self
            .identity
            .selected_role_access_for_holding(
                &claims.principal_ref,
                role,
                &Scope::Platform,
                erpx_identity::DEVELOPMENT_SOD_BYPASS_PERMISSION,
                Some(holding_id),
                Utc::now(),
            )
            .await
            .map_err(|_| Status::internal("development SoD authorization lookup failed"))?
        {
            return Ok(false);
        }
        store
            .record(AuditEvent {
                actor_ref: claims.principal_ref.clone(),
                customer_account_id: None,
                action: "development.authorization.sod_bypass".into(),
                purpose: format!("local holding target {holding_id}"),
                correlation_id,
            })
            .await
            .map_err(|_| Status::unavailable("development authorization audit failed"))?;
        Ok(true)
    }

    async fn authorize_role(
        &self,
        store: &SqlxControlPlane,
        claims: &AccessTokenClaims,
        requested_scope: &Scope,
        required_permission: &str,
    ) -> Result<(), Status> {
        self.authorize_role_for_holding(store, claims, requested_scope, required_permission, None)
            .await
    }

    async fn authorize_role_for_holding(
        &self,
        store: &SqlxControlPlane,
        claims: &AccessTokenClaims,
        requested_scope: &Scope,
        required_permission: &str,
        target_holding: Option<&str>,
    ) -> Result<(), Status> {
        self.authorize_role_for_holding_inner(
            store,
            claims,
            requested_scope,
            required_permission,
            target_holding,
            false,
        )
        .await
    }

    /// Explicit Phase-1 opt-in used only by Organization and the shared
    /// Collaboration/Item Directory pilot. Every legacy/custom ingress stays
    /// on `authorize_role_for_holding`, which rejects exact Subtree authority
    /// before role or storage lookup.
    async fn authorize_role_for_holding_with_exact_subtree(
        &self,
        store: &SqlxControlPlane,
        claims: &AccessTokenClaims,
        requested_scope: &Scope,
        required_permission: &str,
        target_holding: Option<&str>,
    ) -> Result<(), Status> {
        self.authorize_role_for_holding_inner(
            store,
            claims,
            requested_scope,
            required_permission,
            target_holding,
            true,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn authorize_role_for_holding_inner(
        &self,
        store: &SqlxControlPlane,
        claims: &AccessTokenClaims,
        requested_scope: &Scope,
        required_permission: &str,
        target_holding: Option<&str>,
        exact_subtree_enabled: bool,
    ) -> Result<(), Status> {
        if !exact_subtree_ingress_allowed(requested_scope, exact_subtree_enabled) {
            return Err(Status::permission_denied(
                "exact organization subtree authority is not enabled for this ingress",
            ));
        }
        if claims.principal_ref == erpx_identity::DEVELOPMENT_CONTROLLER_PRINCIPAL
            && self.development_controller.is_none()
        {
            return Err(Status::permission_denied(
                "development controller role is unavailable outside the gated local environment",
            ));
        }
        if let Some(role) = claims.active_role.as_ref() {
            if self
                .identity
                .selected_role_access_for_holding(
                    &claims.principal_ref,
                    role,
                    requested_scope,
                    required_permission,
                    target_holding,
                    Utc::now(),
                )
                .await
                .map_err(|_| Status::internal("selected role authorization lookup failed"))?
            {
                return Ok(());
            }
            return Err(Status::permission_denied("selected_role_permission_denied"));
        }
        // Development persona mode has its own controller-session binding and
        // server-owned persona context. Production requests cannot reach this
        // fallback because `authorize` rejects a missing selected role first.
        if crate::identity_http::development_gates().is_ok()
            && store
                .principal_has_role(
                    &claims.principal_ref,
                    requested_scope,
                    required_permission,
                    Utc::now(),
                )
                .await
                .map_err(|_| Status::internal("development authorization lookup failed"))?
        {
            return Ok(());
        }
        Err(Status::permission_denied("role_selection_required"))
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
    async fn authorize_customer_security_admin(
        &self,
        store: &SqlxControlPlane,
        claims: &AccessTokenClaims,
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
        self.authorize_role(
            store,
            claims,
            requested_scope,
            PLATFORM_CUSTOMER_ACCESS_AUDIT_PERMISSION,
        )
        .await?;
        let role = claims
            .active_role
            .as_ref()
            .ok_or_else(|| Status::permission_denied("role_selection_required"))?;
        if !self
            .identity
            .selected_local_role_has_any_key(
                &claims.principal_ref,
                role,
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

fn exact_subtree_ingress_allowed(scope: &Scope, exact_subtree_enabled: bool) -> bool {
    !matches!(scope, Scope::Subtree { .. }) || exact_subtree_enabled
}

async fn validate_holding_admin_context<T>(
    state: &AppState,
    request: &Request<T>,
    claims: &AccessTokenClaims,
    holding_id: &str,
    canonical_holding_id: Uuid,
) -> Result<(), Status> {
    let context_id = request
        .metadata()
        .get("x-erpx-holding-admin-context")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| Status::permission_denied("holding administration context is required"))
        .and_then(|value| parse_uuid(value, "administrative_context_id"))?;
    let at = Utc::now();
    let device_id = state
        .auth
        .identity
        .active_session_device_id(claims.session_id, at)
        .await
        .map_err(map_identity_auth_error)?;
    let administrative_context = SqlxHoldingAdministrationStore::new(state.store.clone())
        .get_context(context_id)
        .await
        .map_err(|_| Status::unavailable("holding administration context unavailable"))?
        .ok_or_else(|| Status::permission_denied("holding administration context not found"))?;
    if administrative_context.canonical_holding_id != canonical_holding_id {
        return Err(Status::permission_denied(
            "holding administration context holding mismatch",
        ));
    }
    administrative_context
        .authorize(&HoldingAdminAuthorizationRequest {
            principal_ref: &claims.principal_ref,
            device_id,
            session_id: claims.session_id,
            holding_id,
            requested_scope: &HoldingOrganizationScope::Holding {
                holding_id: canonical_holding_id,
            },
            module: None,
            entitlements: &[],
            identity_active: true,
            session_active: true,
            directory_allows: true,
            platform_role_active: true,
            at,
        })
        .map_err(|_| Status::permission_denied("holding administration context denied"))?;
    Ok(())
}

async fn validate_controller_holding_admin_context<T>(
    state: &AppState,
    request: &Request<T>,
    claims: &AccessTokenClaims,
    holding_id: &str,
    canonical_holding_id: Uuid,
) -> Result<(), Status> {
    if claims.principal_ref != erpx_identity::DEVELOPMENT_CONTROLLER_PRINCIPAL {
        return Ok(());
    }
    validate_holding_admin_context(state, request, claims, holding_id, canonical_holding_id).await
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
        self.authorize_entitled_command_with_holding(
            claims,
            context,
            customer_id,
            module_key,
            required_permission,
            None,
        )
        .await
    }

    async fn authorize_entitled_command_for_holding(
        &self,
        claims: &AccessTokenClaims,
        context: &erpx_domain::contracts::CommandContext,
        customer_id: Uuid,
        module_key: &str,
        required_permission: &str,
        authorization_holding: &str,
    ) -> Result<(), Status> {
        self.authorize_entitled_command_with_holding(
            claims,
            context,
            customer_id,
            module_key,
            required_permission,
            Some(authorization_holding),
        )
        .await
    }

    async fn authorize_entitled_command_with_holding(
        &self,
        claims: &AccessTokenClaims,
        context: &erpx_domain::contracts::CommandContext,
        customer_id: Uuid,
        module_key: &str,
        required_permission: &str,
        authorization_holding: Option<&str>,
    ) -> Result<(), Status> {
        require_actor_match(
            &claims.principal_ref,
            &context.actor.principal_ref,
            "command actor must match the authenticated principal",
        )?;
        let scope = access_scope(&context.active_scope);
        if !self
            .store
            .customer_scope_contains(customer_id, &scope)
            .await
            .map_err(|_| Status::internal("customer scope lookup failed"))?
        {
            return Err(Status::permission_denied("customer_scope_not_granted"));
        }
        self.auth
            .authorize_role_for_holding(
                &self.store,
                claims,
                &scope,
                required_permission,
                authorization_holding,
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
}

fn access_scope(scope: &AuthorityScope) -> Scope {
    match scope {
        AuthorityScope::Platform => Scope::Platform,
        AuthorityScope::Holding { holding_id } => Scope::Holding {
            holding_id: *holding_id,
        },
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Scope::Company {
            holding_id: *holding_id,
            company_id: *company_id,
        },
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Scope::Branch {
            holding_id: *holding_id,
            company_id: *company_id,
            branch_id: *branch_id,
        },
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => Scope::Subtree {
            holding_id: *holding_id,
            organization_node_id: *organization_node_id,
        },
    }
}

fn entitlement_holding_id(scope: &Scope) -> Result<Uuid, Status> {
    match scope {
        Scope::Platform => Err(Status::permission_denied(
            "commercial entitlement commands require a holding scope",
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

fn finance_banking_read_holding_scope(scope: &Scope) -> Result<AuthorityScope, Status> {
    match scope {
        Scope::Holding { holding_id } => Ok(AuthorityScope::Holding {
            holding_id: *holding_id,
        }),
        Scope::Company {
            holding_id,
            company_id,
        } => Ok(AuthorityScope::Company {
            holding_id: *holding_id,
            company_id: *company_id,
        }),
        Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(AuthorityScope::Branch {
            holding_id: *holding_id,
            company_id: *company_id,
            branch_id: *branch_id,
        }),
        Scope::Platform | Scope::Subtree { .. } => Err(Status::permission_denied(
            "Finance banking reads require an explicit holding, company, or branch scope",
        )),
    }
}
fn query_access_scope(context: &erpx_proto::common::QueryContext) -> Result<Scope, Status> {
    let scope = context
        .active_scope
        .as_ref()
        .and_then(|scope| scope.scope.as_ref())
        .ok_or_else(|| Status::permission_denied("query holding scope is required"))?;
    match scope {
        erpx_proto::common::authority_scope::Scope::Platform(_) => Ok(Scope::Platform),
        erpx_proto::common::authority_scope::Scope::Holding(value) => Ok(Scope::Holding {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
        }),
        erpx_proto::common::authority_scope::Scope::Company(value) => Ok(Scope::Company {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
        }),
        erpx_proto::common::authority_scope::Scope::Branch(value) => Ok(Scope::Branch {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
            branch_id: parse_uuid(&value.branch_id, "branch_id")?,
        }),
        erpx_proto::common::authority_scope::Scope::Subtree(value) => Ok(Scope::Subtree {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            organization_node_id: parse_uuid(&value.organization_node_id, "organization_node_id")?,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QualityAuthorizedMode {
    Read,
    Write,
}

#[derive(Debug, Clone)]
struct QualityPreauthorizedAccess {
    principal_ref: String,
    authority_scope: KernelHoldingScope,
    mode: QualityAuthorizedMode,
}

#[async_trait::async_trait]
impl QualityAuthorizationPort for QualityPreauthorizedAccess {
    async fn authorize(
        &self,
        request: QualityAuthorizationRequest<'_>,
    ) -> Result<(), QualityPortError> {
        let correct_mode = match self.mode {
            QualityAuthorizedMode::Read => !request.action.requires_write_entitlement(),
            QualityAuthorizedMode::Write => request.action.requires_write_entitlement(),
        };
        if request.actor.principal_ref != self.principal_ref
            || request.authority_scope != &self.authority_scope
            || !self.authority_scope.contains(request.target_scope)
            || !correct_mode
        {
            return Err(QualityPortError::Denied);
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl QualityEntitlementPort for QualityPreauthorizedAccess {
    async fn ensure_quality_enabled(
        &self,
        scope: &KernelHoldingScope,
        write: bool,
    ) -> Result<(), QualityPortError> {
        let authorized_write = self.mode == QualityAuthorizedMode::Write;
        if write != authorized_write || !self.authority_scope.contains(scope) {
            return Err(QualityPortError::EntitlementDenied);
        }
        Ok(())
    }
}

type QualitySqlxCommandHandler = QualityCommandHandler<
    QualityWriteRepository,
    QualityCommandAdapter,
    QualityPreauthorizedAccess,
    QualityPreauthorizedAccess,
    KernelSystemClock,
>;
type QualitySqlxQueryHandler = QualityQueryHandler<
    QualityReadRepository,
    QualityPreauthorizedAccess,
    QualityPreauthorizedAccess,
>;

#[derive(Clone)]
struct QualityServerApplication {
    router: Option<Arc<HoldingDatabaseRouter>>,
}

impl QualityServerApplication {
    async fn execute_command<C>(
        &self,
        route: QualityHoldingRoute,
        envelope: CqrsCommandEnvelope<C>,
    ) -> Result<
        CqrsCommandReceipt<<QualitySqlxCommandHandler as CommandHandler<C>>::Response>,
        CqrsError,
    >
    where
        QualitySqlxCommandHandler: CommandHandler<C>,
    {
        let correlation_id = envelope.correlation_id;
        let access = QualityPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.active_scope.clone(),
            mode: QualityAuthorizedMode::Write,
        };
        let router = self.router.as_ref().ok_or_else(|| {
            quality_cqrs_failure(
                correlation_id,
                "quality.routing_unavailable",
                KernelErrorCategory::Unavailable,
                "Quality storage is temporarily unavailable.",
                true,
            )
        })?;
        let routed = router
            .route(
                &route.holding_id,
                &route.data_profile_key,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| {
                quality_cqrs_failure(
                    correlation_id,
                    "quality.write_unavailable",
                    KernelErrorCategory::Unavailable,
                    "Quality storage is temporarily unavailable.",
                    true,
                )
            })?;
        let holding_id = Uuid::parse_str(&routed.holding_id).map_err(|_| {
            quality_cqrs_failure(
                correlation_id,
                "quality.invalid_holding_identity",
                KernelErrorCategory::Internal,
                "Quality storage holding identity is invalid.",
                false,
            )
        })?;
        let pool = CqrsWritePool::for_holding(routed.database_pool().clone(), holding_id).map_err(
            |_| {
                quality_cqrs_failure(
                    correlation_id,
                    "quality.invalid_holding_identity",
                    KernelErrorCategory::Internal,
                    "Quality storage holding identity is invalid.",
                    false,
                )
            },
        )?;
        let handler = QualityCommandHandler::new(
            QualityWriteRepository::new(pool.clone()),
            QualityCommandAdapter::new(pool),
            access.clone(),
            access,
            KernelSystemClock,
        );
        let result = handler.handle(envelope).await;
        drop(routed);
        result
    }

    async fn execute_query<Q>(
        &self,
        route: QualityHoldingRoute,
        envelope: CqrsQueryEnvelope<Q>,
    ) -> Result<CqrsQueryResult<<QualitySqlxQueryHandler as QueryHandler<Q>>::Response>, CqrsError>
    where
        QualitySqlxQueryHandler: QueryHandler<Q>,
    {
        let correlation_id = envelope.correlation_id;
        let access = QualityPreauthorizedAccess {
            principal_ref: envelope.actor.principal_ref.clone(),
            authority_scope: envelope.active_scope.clone(),
            mode: QualityAuthorizedMode::Read,
        };
        let router = self.router.as_ref().ok_or_else(|| {
            quality_cqrs_failure(
                correlation_id,
                "quality.routing_unavailable",
                KernelErrorCategory::Unavailable,
                "Quality read storage is temporarily unavailable.",
                true,
            )
        })?;
        let routed = router
            .route(
                &route.holding_id,
                &route.data_profile_key,
                RoutedDatabaseSide::Read,
            )
            .await
            .map_err(|_| {
                quality_cqrs_failure(
                    correlation_id,
                    "quality.read_unavailable",
                    KernelErrorCategory::Unavailable,
                    "Quality read storage is temporarily unavailable.",
                    true,
                )
            })?;
        let holding_id = Uuid::parse_str(&routed.holding_id).map_err(|_| {
            quality_cqrs_failure(
                correlation_id,
                "quality.invalid_holding_identity",
                KernelErrorCategory::Internal,
                "Quality read storage holding identity is invalid.",
                false,
            )
        })?;
        let read_pool = CqrsReadPool::for_holding(routed.database_pool().clone(), holding_id)
            .map_err(|_| {
                quality_cqrs_failure(
                    correlation_id,
                    "quality.invalid_holding_identity",
                    KernelErrorCategory::Internal,
                    "Quality read storage holding identity is invalid.",
                    false,
                )
            })?;
        let repository = QualityReadRepository::new(read_pool).map_err(|_| {
            quality_cqrs_failure(
                correlation_id,
                "quality.read_configuration",
                KernelErrorCategory::Internal,
                "Quality read storage could not be initialized.",
                false,
            )
        })?;
        let handler = QualityQueryHandler::new(repository, access.clone(), access);
        let result = handler.handle(envelope).await;
        drop(routed);
        result
    }
}

#[async_trait::async_trait]
impl QualityCommandApplication for QualityServerApplication {
    async fn create(
        &self,
        route: QualityHoldingRoute,
        command: CqrsCommandEnvelope<erpx_quality::CreateInspection>,
    ) -> Result<CqrsCommandReceipt<erpx_quality::CreateInspectionResult>, CqrsError> {
        self.execute_command(route, command).await
    }

    async fn start(
        &self,
        route: QualityHoldingRoute,
        command: CqrsCommandEnvelope<erpx_quality::StartInspection>,
    ) -> Result<CqrsCommandReceipt<erpx_quality::InspectionMutationResult>, CqrsError> {
        self.execute_command(route, command).await
    }

    async fn record_result(
        &self,
        route: QualityHoldingRoute,
        command: CqrsCommandEnvelope<erpx_quality::RecordInspectionResult>,
    ) -> Result<CqrsCommandReceipt<erpx_quality::InspectionMutationResult>, CqrsError> {
        self.execute_command(route, command).await
    }

    async fn release(
        &self,
        route: QualityHoldingRoute,
        command: CqrsCommandEnvelope<erpx_quality::ReleaseInspection>,
    ) -> Result<CqrsCommandReceipt<erpx_quality::InspectionMutationResult>, CqrsError> {
        self.execute_command(route, command).await
    }

    async fn cancel(
        &self,
        route: QualityHoldingRoute,
        command: CqrsCommandEnvelope<erpx_quality::CancelInspection>,
    ) -> Result<CqrsCommandReceipt<erpx_quality::InspectionMutationResult>, CqrsError> {
        self.execute_command(route, command).await
    }
}

#[async_trait::async_trait]
impl QualityQueryApplication for QualityServerApplication {
    async fn get(
        &self,
        route: QualityHoldingRoute,
        query: CqrsQueryEnvelope<erpx_quality::GetInspection>,
    ) -> Result<CqrsQueryResult<erpx_quality::GetInspectionResult>, CqrsError> {
        self.execute_query(route, query).await
    }

    async fn list(
        &self,
        route: QualityHoldingRoute,
        query: CqrsQueryEnvelope<erpx_quality::ListInspections>,
    ) -> Result<CqrsQueryResult<erpx_quality::InspectionList>, CqrsError> {
        self.execute_query(route, query).await
    }
}

#[derive(Clone)]
struct QualityServerAuthenticator {
    state: AppState,
}

#[async_trait::async_trait]
impl QualityRequestAuthenticator for QualityServerAuthenticator {
    async fn authenticate(
        &self,
        request: QualityAuthenticationRequest,
    ) -> Result<AuthenticatedQualityRequest, QualityAuthenticationError> {
        let mut transport_request = Request::new(());
        *transport_request.metadata_mut() = request.transport_metadata().clone();
        let claims = self
            .state
            .auth
            .authorize(&transport_request)
            .await
            .map_err(quality_authentication_error)?;
        require_actor_match(
            &claims.principal_ref,
            &request.presented_actor().principal_ref,
            "Quality actor must match the authenticated principal",
        )
        .map_err(quality_authentication_error)?;

        let scope =
            explicit_product_scope(quality_access_scope(request.presented_scope()), "Quality")
                .map_err(quality_authentication_error)?;
        let holding_id =
            validate_transport_reference(&request.presented_route().holding_id, "holding_id")
                .map_err(quality_authentication_error)?;
        let customer_id =
            parse_uuid(holding_id, "holding_id").map_err(quality_authentication_error)?;

        match request.access_mode() {
            QualityAccessMode::Read => {
                self.state
                    .auth
                    .authorize_role(&self.state.store, &claims, &scope, "quality.read")
                    .await
                    .map_err(quality_authentication_error)?;
                if !self
                    .state
                    .store
                    .customer_scope_contains(customer_id, &scope)
                    .await
                    .map_err(|_| QualityAuthenticationError::Unavailable)?
                {
                    return Err(QualityAuthenticationError::Denied);
                }
            }
            QualityAccessMode::Write => {
                let subscription_id = request
                    .subscription_id()
                    .ok_or(QualityAuthenticationError::Denied)
                    .and_then(|value| {
                        Uuid::parse_str(value).map_err(|_| QualityAuthenticationError::Denied)
                    })?;
                let context = quality_legacy_command_context(
                    &claims,
                    request.presented_scope(),
                    subscription_id,
                );
                self.state
                    .authorize_entitled_command(
                        &claims,
                        &context,
                        customer_id,
                        "quality",
                        "quality.write",
                    )
                    .await
                    .map_err(quality_authentication_error)?;
                if let Some(code) = request.mfa_code() {
                    self.state
                        .auth
                        .authorize_mfa(&claims, code)
                        .await
                        .map_err(quality_authentication_error)?;
                }
            }
        }

        let session_id = KernelSessionId::try_from_uuid(claims.session_id)
            .map_err(|_| QualityAuthenticationError::Internal)?;
        let kind = auth_actor::kernel_actor_kind(&claims);
        AuthenticatedQualityRequest::try_new(
            KernelActorAttribution {
                principal_ref: claims.principal_ref,
                kind,
                session_id: Some(session_id),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            request.presented_scope().clone(),
            request.presented_route().clone(),
        )
    }
}

fn quality_legacy_command_context(
    claims: &AccessTokenClaims,
    scope: &KernelHoldingScope,
    subscription_id: Uuid,
) -> CommandContext {
    let correlation_id = Uuid::new_v4();
    CommandContext {
        contract_version: CONTRACT_VERSION,
        command_id: Uuid::new_v4(),
        idempotency_key: format!("quality-authorization-{}", Uuid::new_v4().simple()),
        correlation_id,
        causation_id: None,
        actor: ActorAttribution {
            principal_ref: claims.principal_ref.clone(),
            kind: auth_actor::legacy_actor_kind(claims),
            session_id: Some(claims.session_id),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        active_scope: quality_legacy_scope(scope),
        audit: erpx_domain::contracts::AuditContext {
            purpose: "authorize Quality command".into(),
            customer_visible: false,
            module: Some("quality".into()),
            correlation_id,
        },
        expected_aggregate_version: None,
        subscription_id: Some(subscription_id),
    }
}

fn quality_legacy_scope(scope: &KernelHoldingScope) -> AuthorityScope {
    match scope {
        KernelHoldingScope::Platform => AuthorityScope::Platform,
        KernelHoldingScope::Holding { holding_id } => AuthorityScope::Holding {
            holding_id: *holding_id,
        },
        KernelHoldingScope::Company {
            holding_id,
            company_id,
        } => AuthorityScope::Company {
            holding_id: *holding_id,
            company_id: *company_id,
        },
        KernelHoldingScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => AuthorityScope::Branch {
            holding_id: *holding_id,
            company_id: *company_id,
            branch_id: *branch_id,
        },
        KernelHoldingScope::Subtree {
            holding_id,
            organization_node_id,
        } => AuthorityScope::Subtree {
            holding_id: *holding_id,
            organization_node_id: *organization_node_id,
        },
    }
}

fn legacy_kernel_scope(scope: &AuthorityScope) -> KernelHoldingScope {
    match scope {
        AuthorityScope::Platform => KernelHoldingScope::Platform,
        AuthorityScope::Holding { holding_id } => KernelHoldingScope::Holding {
            holding_id: *holding_id,
        },
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => KernelHoldingScope::Company {
            holding_id: *holding_id,
            company_id: *company_id,
        },
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => KernelHoldingScope::Branch {
            holding_id: *holding_id,
            company_id: *company_id,
            branch_id: *branch_id,
        },
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => KernelHoldingScope::Subtree {
            holding_id: *holding_id,
            organization_node_id: *organization_node_id,
        },
    }
}

fn authenticated_directory_replacement_actor(
    claims: &AccessTokenClaims,
    presented_actor: &ActorAttribution,
) -> Result<KernelActorAttribution, Status> {
    require_actor_match(
        &claims.principal_ref,
        &presented_actor.principal_ref,
        "directory replacement actor must match the authenticated principal",
    )?;
    if presented_actor.session_id != Some(claims.session_id)
        || presented_actor.represented_principal_ref.is_some()
        || presented_actor.support_capability_id.is_some()
    {
        return Err(Status::permission_denied(
            "directory replacement requires the direct authenticated command session",
        ));
    }
    Ok(KernelActorAttribution {
        principal_ref: claims.principal_ref.clone(),
        kind: auth_actor::kernel_actor_kind(claims),
        session_id: Some(
            KernelSessionId::try_from_uuid(claims.session_id)
                .map_err(|_| Status::internal("authenticated session attribution is invalid"))?,
        ),
        represented_principal_ref: None,
        support_capability_id: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn authenticated_directory_replacement_decision(
    claims: &AccessTokenClaims,
    presented_actor: &ActorAttribution,
    scope: &AuthorityScope,
    action: DirectoryIdentityReplacementDecisionAction,
    conflict_id: Uuid,
    expected_conflict_version: u64,
    correlation_id: Uuid,
    decided_at: DateTime<Utc>,
) -> Result<DirectoryIdentityReplacementDecision, Status> {
    let actor = authenticated_directory_replacement_actor(claims, presented_actor)?;
    let authority_scope = legacy_kernel_scope(scope);
    let decision_ref = directory_identity_replacement_decision_ref(
        action,
        conflict_id,
        expected_conflict_version,
        correlation_id,
    );
    let proof =
        HumanDecisionProof::try_new(actor, authority_scope.clone(), decision_ref, decided_at)
            .map_err(|_| {
                Status::permission_denied(
                    "directory replacement decisions require an authenticated human",
                )
            })?;
    DirectoryIdentityReplacementDecision::try_new(
        action,
        conflict_id,
        expected_conflict_version,
        correlation_id,
        authority_scope,
        proof,
    )
    .map_err(|_| Status::permission_denied("directory replacement decision proof is invalid"))
}

#[cfg(test)]
mod directory_replacement_decision_boundary_tests {
    use super::*;
    use erpx_identity::{AuthenticatedPrincipalKind, AuthenticatedRealm};

    fn claims(principal_kind: AuthenticatedPrincipalKind) -> AccessTokenClaims {
        let issued_at = Utc::now();
        AccessTokenClaims {
            principal_ref: "holding:replacement-approver".into(),
            session_id: Uuid::from_u128(201),
            active_role: None,
            authorization_context_version: Some(1),
            realm: AuthenticatedRealm::InternalWorkforce,
            principal_kind,
            issued_at,
            expires_at: issued_at + Duration::minutes(5),
        }
    }

    fn presented_actor(claims: &AccessTokenClaims) -> ActorAttribution {
        ActorAttribution {
            principal_ref: claims.principal_ref.clone(),
            kind: ActorKind::HoldingUser,
            session_id: Some(claims.session_id),
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }

    #[test]
    fn authenticated_human_decision_boundary_binds_exact_request_material() {
        let claims = claims(AuthenticatedPrincipalKind::Human);
        let actor = presented_actor(&claims);
        let scope = AuthorityScope::Holding {
            holding_id: Uuid::from_u128(202),
        };
        let conflict_id = Uuid::from_u128(203);
        let correlation_id = Uuid::from_u128(204);
        let decided_at = Utc::now();
        let decision = authenticated_directory_replacement_decision(
            &claims,
            &actor,
            &scope,
            DirectoryIdentityReplacementDecisionAction::Approve,
            conflict_id,
            7,
            correlation_id,
            decided_at,
        )
        .expect("human decision");

        assert_eq!(decision.principal_ref(), claims.principal_ref);
        assert_eq!(decision.session_id().as_uuid(), claims.session_id);
        assert_eq!(
            decision.proof().authority_scope(),
            &legacy_kernel_scope(&scope)
        );
        assert_eq!(decision.decided_at(), decided_at);
        assert_eq!(
            decision.proof().decision_ref(),
            directory_identity_replacement_decision_ref(
                DirectoryIdentityReplacementDecisionAction::Approve,
                conflict_id,
                7,
                correlation_id,
            )
        );
    }

    #[test]
    fn authenticated_decision_boundary_rejects_agent_and_service_before_repository_use() {
        for kind in [
            AuthenticatedPrincipalKind::Agent,
            AuthenticatedPrincipalKind::Service,
        ] {
            let claims = claims(kind);
            let status = authenticated_directory_replacement_decision(
                &claims,
                &presented_actor(&claims),
                &AuthorityScope::Platform,
                DirectoryIdentityReplacementDecisionAction::Reject,
                Uuid::from_u128(205),
                2,
                Uuid::from_u128(206),
                Utc::now(),
            )
            .expect_err("non-human decision");
            assert_eq!(status.code(), tonic::Code::PermissionDenied);
        }
    }

    #[test]
    fn authenticated_decision_boundary_rejects_presented_actor_or_session_substitution() {
        let claims = claims(AuthenticatedPrincipalKind::Human);
        let mut actor = presented_actor(&claims);
        actor.session_id = Some(Uuid::from_u128(207));
        assert!(
            authenticated_directory_replacement_decision(
                &claims,
                &actor,
                &AuthorityScope::Platform,
                DirectoryIdentityReplacementDecisionAction::Approve,
                Uuid::from_u128(208),
                3,
                Uuid::from_u128(209),
                Utc::now(),
            )
            .is_err()
        );
        let mut actor = presented_actor(&claims);
        actor.principal_ref = "holding:another-principal".into();
        assert!(authenticated_directory_replacement_actor(&claims, &actor).is_err());
    }
}

async fn resolve_legacy_holding_workspace(
    state: &AppState,
    scope: &AuthorityScope,
    holding_id: &str,
    data_profile_key: &str,
    write: bool,
) -> Result<ResolvedHoldingWorkspace, Status> {
    let holding_id = holding_id.trim();
    let data_profile_key = data_profile_key.trim();
    let presented_route = match (holding_id.is_empty(), data_profile_key.is_empty()) {
        (true, true) => None,
        (false, false) => Some(PresentedHoldingRoute {
            holding_id: validate_transport_reference(holding_id, "holding_id")?,
            data_profile_key: validate_transport_reference(data_profile_key, "data_profile_key")?,
        }),
        _ => {
            return Err(Status::invalid_argument(
                "presented holding route is incomplete",
            ));
        }
    };
    resolve_holding_workspace(
        state,
        &legacy_kernel_scope(scope),
        presented_route,
        None,
        write,
    )
    .await
}

fn quality_access_scope(scope: &KernelHoldingScope) -> Scope {
    access_scope(&quality_legacy_scope(scope))
}

fn quality_authentication_error(status: Status) -> QualityAuthenticationError {
    match status.code() {
        tonic::Code::Unauthenticated => QualityAuthenticationError::Unauthenticated,
        tonic::Code::PermissionDenied | tonic::Code::InvalidArgument => {
            QualityAuthenticationError::Denied
        }
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => {
            QualityAuthenticationError::Unavailable
        }
        _ => QualityAuthenticationError::Internal,
    }
}

fn quality_cqrs_failure(
    correlation_id: KernelCorrelationId,
    code: &'static str,
    category: KernelErrorCategory,
    safe_message: &'static str,
    retryable: bool,
) -> CqrsError {
    CqrsError::Operation(KernelError {
        code: code.into(),
        category,
        safe_message: safe_message.into(),
        retryable,
        correlation_id,
        context: KernelErrorContext::empty(),
    })
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
            organization_node_id,
        } => format!("subtree/{holding_id}/node/{organization_node_id}"),
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
        ["subtree", holding_id, "node", organization_node_id] => Ok(Scope::Subtree {
            holding_id: parse_uuid(holding_id, "support holding_id")?,
            organization_node_id: parse_uuid(organization_node_id, "support organization_node_id")?,
        }),
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
        | IdentityStoreError::RoleSelectionRequired
        | IdentityStoreError::RoleSelectionInvalid
        | IdentityStoreError::SecurityCommandConflict
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
fn parse_uuid(value: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(value).map_err(|_| Status::invalid_argument(format!("{field} must be a UUID")))
}

fn organization_page_size(value: u32) -> Result<u32, Status> {
    match value {
        0 => Ok(25),
        value if value <= u32::from(MAX_ORGANIZATION_LIST_PAGE_SIZE) => Ok(value),
        _ => Err(Status::invalid_argument(
            "organization page size exceeds the allowed bound",
        )),
    }
}
const fn typed_organization_kind_name(kind: TypedOrganizationNodeKind) -> &'static str {
    match kind {
        TypedOrganizationNodeKind::Holding => "holding",
        TypedOrganizationNodeKind::Company => "company",
        TypedOrganizationNodeKind::Headquarters => "headquarters",
        TypedOrganizationNodeKind::Branch => "branch",
        TypedOrganizationNodeKind::Department => "department",
        TypedOrganizationNodeKind::Team => "team",
    }
}

const fn typed_organization_company_id(
    node_id: Uuid,
    placement: &TypedOrganizationPlacement,
) -> Option<Uuid> {
    match placement {
        TypedOrganizationPlacement::Holding => None,
        TypedOrganizationPlacement::Company { .. } => Some(node_id),
        TypedOrganizationPlacement::Headquarters { company_id, .. }
        | TypedOrganizationPlacement::Branch { company_id, .. }
        | TypedOrganizationPlacement::Department { company_id, .. }
        | TypedOrganizationPlacement::Team { company_id, .. } => Some(*company_id),
    }
}

const fn typed_organization_branch_id(
    node_id: Uuid,
    placement: &TypedOrganizationPlacement,
) -> Option<Uuid> {
    match placement {
        TypedOrganizationPlacement::Branch { .. } => Some(node_id),
        TypedOrganizationPlacement::Department { branch_id, .. }
        | TypedOrganizationPlacement::Team { branch_id, .. } => *branch_id,
        TypedOrganizationPlacement::Holding
        | TypedOrganizationPlacement::Company { .. }
        | TypedOrganizationPlacement::Headquarters { .. } => None,
    }
}
/// Encode the support queue's validated keyset position. The cursor is
/// opaque at the transport boundary and contains no ticket payload, customer
/// secret, or authorization material.
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
        cursor.holding_id,
        cursor.data_profile_key,
        cursor.projection_name,
        cursor.job_key,
        cursor.job_id,
    )
    .map_err(|_| Status::invalid_argument("projection status cursor is invalid"))
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

const MAX_MARKETING_FORM_VALUES: usize = 256;
const MAX_MARKETING_FORM_VALUE_BYTES: usize = 16 * 1024;
const PUBLIC_MARKETING_FORM_REQUESTS_PER_MINUTE: u32 = 30;
const PUBLIC_MARKETING_PREVIEW_REQUESTS_PER_MINUTE: u32 = 120;

fn marketing_form_values(
    values: &[marketing_target::MarketingFormValue],
) -> Result<BTreeMap<String, String>, Status> {
    if values.is_empty() || values.len() > MAX_MARKETING_FORM_VALUES {
        return Err(Status::invalid_argument(
            "marketing form values are out of bounds",
        ));
    }
    let mut normalized = BTreeMap::new();
    for value in values {
        let key = value.key.trim();
        if key.is_empty()
            || key.len() > 255
            || value.value.len() > MAX_MARKETING_FORM_VALUE_BYTES
            || key.chars().any(char::is_control)
        {
            return Err(Status::invalid_argument("marketing form value is invalid"));
        }
        if normalized
            .insert(key.to_owned(), value.value.clone())
            .is_some()
        {
            return Err(Status::invalid_argument(
                "marketing form value keys must be unique",
            ));
        }
    }
    Ok(normalized)
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

const MAX_BOUNDED_JSON_BYTES: usize = 2 * 1024 * 1024;

fn decode_bounded_json<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    field: &'static str,
) -> Result<T, Status> {
    if bytes.is_empty() || bytes.len() > MAX_BOUNDED_JSON_BYTES {
        return Err(Status::invalid_argument(format!(
            "{field} is empty or exceeds its size bound"
        )));
    }
    serde_json::from_slice(bytes)
        .map_err(|_| Status::invalid_argument(format!("{field} is invalid")))
}
fn billing_state_name(state: BillingState) -> &'static str {
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
) -> commercial_target::EntitlementCommandReceipt {
    commercial_target::EntitlementCommandReceipt {
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

#[derive(Clone)]
struct CustomerRpcImpl {
    state: AppState,
}

impl CustomerRpcImpl {
    async fn get_target_customer_access_audit(
        &self,
        request: Request<customer_authority_target::GetCustomerAccessAuditRequest>,
    ) -> Result<Response<customer_authority_target::GetCustomerAccessAuditResponse>, Status> {
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
                        &claims,
                        &Scope::Platform,
                        PLATFORM_CUSTOMER_READ_PERMISSION,
                    )
                    .await?;
            } else {
                self.state
                    .auth
                    .authorize_customer_security_admin(&self.state.store, &claims, id, &scope)
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
                "customer_authority.customer_access_audit",
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
            let mut response = customer_authority_target::GetCustomerAccessAuditResponse {
                customer_id: id.to_string(),
                correlation_id: context.correlation_id.to_string(),
                ..Default::default()
            };
            if let CustomerReadView::Present { data, metadata } = view {
                response.entries = data
                    .entries
                    .into_iter()
                    .map(|entry| {
                        Ok(customer_authority_target::CustomerAccessAuditEntry {
                            audit_id: entry.audit_id.to_string(),
                            actor_ref: entry.actor.principal_ref,
                            session_ref: entry.actor.session_ref.unwrap_or_default(),
                            customer_id: entry.customer_id.to_string(),
                            module: entry.module,
                            scope: serde_json::to_string(&entry.scope).map_err(|_| {
                                Status::internal("customer access-audit scope serialization failed")
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
            Ok(Response::new(response))
        })
    }
}

#[derive(Clone)]
struct CustomerAuthorityQueryComposite<A, G> {
    current: CustomerAuthorityQueryRpc<A, G>,
    audit: CustomerRpcImpl,
}

impl<A, G> CustomerAuthorityQueryComposite<A, G> {
    fn new(current: CustomerAuthorityQueryRpc<A, G>, audit: CustomerRpcImpl) -> Self {
        Self { current, audit }
    }
}

#[tonic::async_trait]
impl<A, G> TargetCustomerAuthorityQueryService for CustomerAuthorityQueryComposite<A, G>
where
    A: CustomerAuthorityQueryApplication,
    G: CustomerAuthorityRequestAuthenticator,
{
    async fn get(
        &self,
        request: Request<customer_authority_target::GetCustomerAuthorityRequest>,
    ) -> Result<Response<customer_authority_target::GetCustomerAuthorityResponse>, Status> {
        self.current.get(request).await
    }

    async fn list(
        &self,
        request: Request<customer_authority_target::ListCustomerAuthoritiesRequest>,
    ) -> Result<Response<customer_authority_target::ListCustomerAuthoritiesResponse>, Status> {
        self.current.list(request).await
    }

    async fn get_customer_access_audit(
        &self,
        request: Request<customer_authority_target::GetCustomerAccessAuditRequest>,
    ) -> Result<Response<customer_authority_target::GetCustomerAccessAuditResponse>, Status> {
        self.audit.get_target_customer_access_audit(request).await
    }
}

impl CustomerRpcImpl {
    async fn get_target_customer360_detail(
        &self,
        request: Request<customer360_target::GetCustomer360DetailRequest>,
    ) -> Result<Response<customer360_target::GetCustomer360DetailResponse>, Status> {
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
                    &claims,
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
                    module: "customer_360.detail".into(),
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
                    customer_detail_section_states(data)
                        .into_iter()
                        .map(|state| customer360_target::Customer360SectionState {
                            section: state.section,
                            state: state.state,
                        })
                        .collect(),
                ),
                CustomerReadView::Absent { .. } => (true, String::new(), false, Vec::new()),
                CustomerReadView::Unsupported { reason, .. } => {
                    (false, reason.clone(), false, Vec::new())
                }
            };
            let section_facts = match &view {
                CustomerReadView::Present { data, .. } => customer_detail_section_facts(data)
                    .into_iter()
                    .map(|fact| customer360_target::Customer360SectionFact {
                        section: fact.section,
                        key: fact.key,
                        value: fact.value,
                    })
                    .collect(),
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
                            .map(|note| customer360_target::CustomerNote {
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
            Ok(Response::new(
                customer360_target::GetCustomer360DetailResponse {
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
                },
            ))
        })
    }
}

#[derive(Clone)]
struct Customer360QueryComposite<A, G> {
    current: Customer360QueryRpc<A, G>,
    detail: CustomerRpcImpl,
}

impl<A, G> Customer360QueryComposite<A, G> {
    fn new(current: Customer360QueryRpc<A, G>, detail: CustomerRpcImpl) -> Self {
        Self { current, detail }
    }
}

#[tonic::async_trait]
impl<A, G> TargetCustomer360QueryService for Customer360QueryComposite<A, G>
where
    A: Customer360QueryApplication,
    G: Customer360RequestAuthenticator,
{
    async fn get_customer360(
        &self,
        request: Request<customer360_target::GetCustomer360Request>,
    ) -> Result<Response<customer360_target::GetCustomer360Response>, Status> {
        self.current.get_customer360(request).await
    }

    async fn get_customer360_detail(
        &self,
        request: Request<customer360_target::GetCustomer360DetailRequest>,
    ) -> Result<Response<customer360_target::GetCustomer360DetailResponse>, Status> {
        self.detail.get_target_customer360_detail(request).await
    }
}

#[tonic::async_trait]
impl erpx_proto::erpx::customer_360::customer360_command_service_server::Customer360CommandService
    for CustomerRpcImpl
{
    async fn add_customer_note(
        &self,
        request: Request<erpx_proto::erpx::customer_360::AddCustomerNoteRequest>,
    ) -> Result<Response<erpx_proto::erpx::customer_360::AddCustomerNoteResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
            let authority_scope = legacy_kernel_scope(&context.active_scope);
            if !matches!(
                authority_scope,
                KernelHoldingScope::Company { .. } | KernelHoldingScope::Branch { .. }
            ) {
                return Err(Status::permission_denied(
                    "customer notes require an exact company or branch scope",
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
                    &claims,
                    &scope,
                    PLATFORM_CUSTOMER_NOTE_WRITE_PERMISSION,
                )
                .await?;
            let customer_id = parse_uuid(&request.get_ref().customer_id, "customer_id")?;
            let note_id = parse_uuid(&request.get_ref().note_id, "note_id")?;
            if !request.get_ref().body.trim().is_empty() {
                return Err(Status::invalid_argument(
                    "inline customer note bodies are not accepted",
                ));
            }
            let subscription_id = context.subscription_id.map(|id| id.to_string());
            let workspace = resolve_holding_workspace(
                &self.state,
                &authority_scope,
                None,
                subscription_id.as_deref(),
                true,
            )
            .await?;
            if workspace.customer_id != customer_id {
                return Err(Status::permission_denied(
                    "customer is outside the active holding workspace",
                ));
            }
            let customer_repository = CustomerAuthorityWriteRepository::new(
                CqrsWritePool::from_control_pg_pool(self.state.store.pools().write.clone()),
            );
            let customer = customer_repository
                .load(
                    &authority_scope,
                    AuthorityCustomerId::try_from_uuid(customer_id)
                        .map_err(|_| Status::invalid_argument("customer_id is invalid"))?,
                )
                .await
                .map_err(|error| match error {
                    erpx_customer_authority::PortError::Denied
                    | erpx_customer_authority::PortError::NotFound => {
                        Status::permission_denied("customer is outside the active authority")
                    }
                    erpx_customer_authority::PortError::Unavailable
                    | erpx_customer_authority::PortError::NotReady(_) => {
                        Status::unavailable("customer authority is unavailable")
                    }
                    _ => Status::internal("customer authority lookup failed"),
                })?
                .ok_or_else(|| {
                    Status::permission_denied("customer is outside the active authority")
                })?;
            let record = Customer360OwnedRecord::create(
                KernelRecordId::try_from_uuid(note_id)
                    .map_err(|_| Status::invalid_argument("note_id is invalid"))?,
                Customer360RecordKind::Note,
                authority_scope.clone(),
                customer
                    .record_ref()
                    .map_err(|_| Status::internal("customer reference is invalid"))?,
                None,
                customer.version(),
                request.get_ref().title.clone(),
                Some(request.get_ref().content_object_ref.clone()),
                request.get_ref().content_sha256.clone(),
                claims.principal_ref.clone(),
                Utc::now(),
            )
            .map_err(|_| Status::invalid_argument("customer note metadata is invalid"))?;
            let router = self.state.holding_router.as_ref().ok_or_else(|| {
                Status::unavailable("customer note holding routing is unavailable")
            })?;
            let routed = router
                .route(
                    &workspace.holding_id,
                    &workspace.data_profile_key,
                    RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| Status::unavailable("customer note storage is unavailable"))?;
            let holding_id = Uuid::parse_str(&workspace.holding_id)
                .map_err(|_| Status::internal("holding workspace identity is invalid"))?;
            let write = CqrsWritePool::for_holding(routed.database_pool().clone(), holding_id)
                .map_err(|_| Status::internal("holding workspace identity is invalid"))?;
            let receipt = Customer360RecordSqlxAdapter::new(write)
                .save_record(Customer360RecordCommand {
                    command_id: KernelCommandId::try_from_uuid(context.command_id)
                        .map_err(|_| Status::invalid_argument("command_id is invalid"))?,
                    correlation_id: KernelCorrelationId::try_from_uuid(context.correlation_id)
                        .map_err(|_| Status::invalid_argument("correlation_id is invalid"))?,
                    idempotency_key: IdempotencyKey::try_new(context.idempotency_key.clone())
                        .map_err(|_| Status::invalid_argument("idempotency_key is invalid"))?,
                    expected_version: context.expected_aggregate_version,
                    actor: KernelActorAttribution {
                        principal_ref: claims.principal_ref.clone(),
                        kind: auth_actor::kernel_actor_kind(&claims),
                        session_id: Some(
                            KernelSessionId::try_from_uuid(claims.session_id)
                                .map_err(|_| Status::internal("session identity is invalid"))?,
                        ),
                        represented_principal_ref: context.actor.represented_principal_ref.clone(),
                        support_capability_id: context.actor.support_capability_id,
                    },
                    audit: KernelAuditContext {
                        purpose: context.audit.purpose.clone(),
                        customer_visible: context.audit.customer_visible,
                        module: context.audit.module.clone(),
                    },
                    authority_scope,
                    record,
                })
                .await
                .map_err(|error| match error {
                    erpx_customer_360::PortError::Denied => {
                        Status::permission_denied("customer note write is denied")
                    }
                    erpx_customer_360::PortError::Unavailable
                    | erpx_customer_360::PortError::NotReady(_) => {
                        Status::unavailable("customer note storage is unavailable")
                    }
                    erpx_customer_360::PortError::Internal => {
                        Status::internal("customer note write failed")
                    }
                })?;
            Ok(Response::new(
                erpx_proto::erpx::customer_360::AddCustomerNoteResponse {
                    note: Some(erpx_proto::erpx::customer_360::CustomerNote {
                        id: receipt.record.id().to_string(),
                        author_ref: receipt.record.created_by().to_owned(),
                        created_at: receipt.record.created_at().to_rfc3339(),
                        // Protected note bodies are returned only by an authorized read model.
                        body: String::new(),
                    }),
                    receipt: Some(erpx_proto::erpx::customer_360::CustomerNoteWriteReceipt {
                        aggregate_type: "customer_360_owned_record".into(),
                        aggregate_id: receipt.record.id().to_string(),
                        aggregate_version: receipt.record.version(),
                        event_id: Uuid::new_v5(&context.command_id, b"c360-state").to_string(),
                        replayed: receipt.replayed,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }
}

fn catalog_price_kind_name(kind: CatalogPriceKind) -> &'static str {
    match kind {
        CatalogPriceKind::BaseFee => "holding_base",
        CatalogPriceKind::Module => "module",
        CatalogPriceKind::SetupFee => "setup_fee",
    }
}

fn catalog_pricing_unit_name(unit: PricingUnit) -> &'static str {
    match unit {
        PricingUnit::Flat => "flat",
        PricingUnit::Seat => "seat",
        PricingUnit::Company => "company",
        PricingUnit::Branch => "branch",
    }
}

fn catalog_interval_name(charge: ChargeKind) -> &'static str {
    match charge {
        ChargeKind::Recurring(BillingInterval::Monthly) => "monthly",
        ChargeKind::Recurring(BillingInterval::Yearly) => "yearly",
        ChargeKind::OneTime => "one_time",
        ChargeKind::Metered => "metered",
    }
}

fn stripe_billing_client() -> Result<StripeHttpClient, Status> {
    let account_ref = std::env::var("ERPX_STRIPE_ACCOUNT_REF")
        .map_err(|_| Status::failed_precondition("Stripe billing is not configured"))?;
    let api_key = std::env::var("ERPX_STRIPE_API_KEY")
        .map_err(|_| Status::failed_precondition("Stripe billing is not configured"))?;
    let config = StripeProviderConfig::pinned(account_ref)
        .map_err(|_| Status::failed_precondition("Stripe billing is not configured"))?;
    let key = StripeApiKey::parse(api_key)
        .map_err(|_| Status::failed_precondition("Stripe billing is not configured"))?;
    StripeHttpClient::with_defaults(config, key)
        .map_err(|_| Status::failed_precondition("Stripe billing is not configured"))
}

/// Checkout and Portal return locations are deployment configuration, never a
/// client-selected URL. The protobuf field is retained for additive client
/// compatibility but deliberately has no authority at this boundary.
fn billing_return_url(path: &str) -> Result<String, Status> {
    let origin = std::env::var("ERPX_PUBLIC_ORIGIN")
        .map_err(|_| Status::failed_precondition("public billing origin is not configured"))?;
    let origin = origin.trim().trim_end_matches('/');
    let is_secure = origin.starts_with("https://");
    let is_loopback =
        origin.starts_with("http://127.0.0.1:") || origin.starts_with("http://localhost:");
    if origin.is_empty()
        || origin.contains('?')
        || origin.contains('#')
        || !(is_secure || is_loopback)
    {
        return Err(Status::failed_precondition(
            "public billing origin is invalid",
        ));
    }
    Ok(format!("{origin}{path}"))
}

fn validate_checkout_items(
    catalog: &erpx_domain::commercial::PublishedCommercialCatalogRevision,
    selections: &[DomainSubscriptionItemSelection],
) -> Result<String, Status> {
    let currency = active_subscription_currency(catalog, selections)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    let mut selected_interval = None;
    let mut base_count = 0_u8;
    for selection in selections {
        let price = catalog
            .price(&selection.item_key)
            .ok_or_else(|| Status::invalid_argument("catalog item is not published"))?;
        let interval = match price.charge {
            ChargeKind::Recurring(interval) => interval,
            ChargeKind::OneTime | ChargeKind::Metered => {
                return Err(Status::invalid_argument(
                    "Checkout accepts recurring SaaS catalog items only",
                ));
            }
        };
        if selected_interval.is_some_and(|selected| selected != interval) {
            return Err(Status::invalid_argument(
                "subscription items must use one billing interval",
            ));
        }
        selected_interval = Some(interval);
        if price.kind == CatalogPriceKind::BaseFee {
            base_count = base_count.saturating_add(1);
            if selection.quantity != 1 {
                return Err(Status::invalid_argument(
                    "holding base subscription quantity must be one",
                ));
            }
        }
    }
    if base_count != 1 {
        return Err(Status::invalid_argument(
            "subscription must contain exactly one holding base item",
        ));
    }
    Ok(currency)
}

async fn authorize_holding_billing_scope(
    state: &AppState,
    claims: &AccessTokenClaims,
    holding_id: &str,
    scope: &Scope,
    holding_permission: &str,
    platform_read_allowed: bool,
) -> Result<erpx_domain::holding_administration::HoldingRegistryBinding, Status> {
    let binding = SqlxHoldingAdministrationStore::new(state.store.clone())
        .customer_holding_binding(holding_id)
        .await
        .map_err(|_| Status::unavailable("holding billing binding is unavailable"))?
        .ok_or_else(|| {
            Status::failed_precondition("holding configuration is incomplete for billing")
        })?;
    match scope {
        Scope::Platform if platform_read_allowed => {
            state
                .auth
                .authorize_role(
                    &state.store,
                    claims,
                    scope,
                    PLATFORM_HOLDING_READ_PERMISSION,
                )
                .await?;
        }
        Scope::Holding { holding_id } if *holding_id == binding.canonical_holding_id => {
            if !state
                .store
                .customer_scope_contains(binding.customer_id, scope)
                .await
                .map_err(|_| Status::internal("customer billing scope lookup failed"))?
            {
                return Err(Status::permission_denied(
                    "customer billing scope is not granted",
                ));
            }
            state
                .auth
                .authorize_role(&state.store, claims, scope, holding_permission)
                .await?;
        }
        Scope::Platform => {
            return Err(Status::permission_denied(
                "platform staff cannot initiate holding payment sessions",
            ));
        }
        Scope::Holding { .. }
        | Scope::Company { .. }
        | Scope::Branch { .. }
        | Scope::Subtree { .. } => {
            return Err(Status::permission_denied(
                "headquarters billing scope is required",
            ));
        }
    }
    Ok(binding)
}

#[derive(Clone)]
struct SaasBillingRpcImpl {
    state: AppState,
}

impl SaasBillingRpcImpl {
    async fn list_catalog(
        &self,
        request: Request<commercial_target::ListSaasCatalogRequest>,
    ) -> Result<Response<commercial_target::ListSaasCatalogResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let query = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            let permission = if scope == Scope::Platform {
                PLATFORM_HOLDING_READ_PERMISSION
            } else {
                COMMERCIAL_BILLING_READ_PERMISSION
            };
            self.state
                .auth
                .authorize_role(&self.state.store, &claims, &scope, permission)
                .await?;
            let requested_currency = request.get_ref().currency.trim();
            let currency = if requested_currency.is_empty() {
                ACTIVE_CATALOG_CURRENCY
            } else {
                requested_currency
            };
            if currency != ACTIVE_CATALOG_CURRENCY {
                return Err(Status::failed_precondition(
                    "only the USD SaaS catalog is active",
                ));
            }
            let catalog = self
                .state
                .commercial
                .get_latest_catalog_revision(currency)
                .await
                .map_err(|_| Status::unavailable("SaaS catalog is unavailable"))?
                .ok_or_else(|| Status::not_found("published USD SaaS catalog was not found"))?;
            let prices = catalog
                .prices()
                .iter()
                .filter(|price| price.amount.currency == currency)
                .map(|price| commercial_target::SaasCatalogPrice {
                    item_key: price.item_key.clone(),
                    display_name: price.display_name.clone(),
                    kind: catalog_price_kind_name(price.kind).into(),
                    pricing_unit: catalog_pricing_unit_name(price.pricing_unit).into(),
                    interval: catalog_interval_name(price.charge).into(),
                    amount_minor: price.amount.amount_minor,
                    currency: price.amount.currency.clone(),
                })
                .collect();
            Ok(Response::new(commercial_target::ListSaasCatalogResponse {
                revision: catalog.revision_number(),
                prices,
                correlation_id: query.correlation_id.to_string(),
            }))
        })
    }

    async fn get_subscription(
        &self,
        request: Request<commercial_target::GetHoldingSubscriptionRequest>,
    ) -> Result<Response<commercial_target::GetHoldingSubscriptionResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let query = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            let binding = authorize_holding_billing_scope(
                &self.state,
                &claims,
                holding_id,
                &scope,
                COMMERCIAL_BILLING_READ_PERMISSION,
                true,
            )
            .await?;
            let administration = SqlxHoldingAdministrationStore::new(self.state.store.clone());
            let subscription_binding = administration
                .holding_subscription_binding(holding_id)
                .await
                .map_err(|_| Status::unavailable("holding subscription is unavailable"))?;
            let (state, currency, grace_until, read_only, items) =
                if let Some(subscription_binding) = subscription_binding {
                    let lifecycle = self
                        .state
                        .commercial
                        .get_billing_lifecycle(
                            binding.customer_id,
                            subscription_binding.subscription_id,
                        )
                        .await
                        .map_err(|_| Status::unavailable("holding billing state is unavailable"))?
                        .map(|(lifecycle, _)| lifecycle);
                    let state = lifecycle
                        .as_ref()
                        .map_or("pending", |lifecycle| billing_state_name(lifecycle.state));
                    let grace_until = lifecycle
                        .as_ref()
                        .and_then(|lifecycle| lifecycle.grace_until)
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_default();
                    let read_only = lifecycle.as_ref().is_some_and(|lifecycle| {
                        matches!(
                            lifecycle.state,
                            BillingState::ReadOnly | BillingState::Canceled
                        )
                    });
                    let items =
                        subscription_binding
                            .item_quantities
                            .into_iter()
                            .map(|(item_key, quantity)| {
                                commercial_target::SubscriptionItemSelection { item_key, quantity }
                            })
                            .collect();
                    (
                        state.to_owned(),
                        subscription_binding.currency,
                        grace_until,
                        read_only,
                        items,
                    )
                } else {
                    (
                        "unconfigured".into(),
                        ACTIVE_CATALOG_CURRENCY.into(),
                        String::new(),
                        false,
                        Vec::new(),
                    )
                };
            Ok(Response::new(
                commercial_target::GetHoldingSubscriptionResponse {
                    subscription: Some(commercial_target::HoldingSubscriptionStatus {
                        holding_id: holding_id.to_owned(),
                        state,
                        currency,
                        grace_until,
                        business_modules_read_only: read_only,
                        items,
                    }),
                    correlation_id: query.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn create_checkout_session(
        &self,
        request: Request<commercial_target::CreateCheckoutSessionRequest>,
    ) -> Result<Response<commercial_target::CreateCheckoutSessionResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let command = command_domain(&request, request.get_ref().context.as_ref())?;
            require_actor_match(
                &claims.principal_ref,
                &command.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?
                    .to_owned();
            let scope = access_scope(&command.active_scope);
            let binding = authorize_holding_billing_scope(
                &self.state,
                &claims,
                &holding_id,
                &scope,
                COMMERCIAL_BILLING_MANAGE_PERMISSION,
                false,
            )
            .await?;
            let selections = request
                .get_ref()
                .items
                .iter()
                .map(|item| DomainSubscriptionItemSelection {
                    item_key: item.item_key.clone(),
                    quantity: item.quantity,
                })
                .collect::<Vec<_>>();
            let catalog = self
                .state
                .commercial
                .get_latest_catalog_revision(ACTIVE_CATALOG_CURRENCY)
                .await
                .map_err(|_| Status::unavailable("SaaS catalog is unavailable"))?
                .ok_or_else(|| Status::not_found("published USD SaaS catalog was not found"))?;
            let currency = validate_checkout_items(&catalog, &selections)?;
            let item_keys = selections
                .iter()
                .map(|item| item.item_key.clone())
                .collect::<Vec<_>>();
            let provider_prices = self
                .state
                .commercial
                .resolve_stripe_checkout_prices(catalog.revision_number(), &item_keys)
                .await
                .map_err(|_| {
                    Status::failed_precondition("SaaS catalog is not activated in Stripe")
                })?;
            let administration = SqlxHoldingAdministrationStore::new(self.state.store.clone());
            let existing_subscription = administration
                .holding_subscription_binding(&holding_id)
                .await
                .map_err(|_| Status::unavailable("holding subscription is unavailable"))?;
            if existing_subscription
                .as_ref()
                .is_some_and(|existing| existing.currency != currency)
            {
                return Err(Status::failed_precondition(
                    "subscription currency cannot be changed by Checkout",
                ));
            }
            let line_items = selections
                .iter()
                .map(|item| {
                    Ok(StripeCheckoutLineItem {
                        price: provider_prices
                            .get(&item.item_key)
                            .cloned()
                            .ok_or_else(|| {
                                Status::failed_precondition("Stripe catalog price is unavailable")
                            })?,
                        quantity: u64::from(item.quantity),
                    })
                })
                .collect::<Result<Vec<_>, Status>>()?;
            let internal_subscription_id = existing_subscription.as_ref().map_or_else(
                || {
                    Uuid::new_v5(
                        &Uuid::NAMESPACE_URL,
                        format!("erpx-subscription:{}:{}", holding_id, command.command_id)
                            .as_bytes(),
                    )
                },
                |existing| existing.subscription_id,
            );
            let selection_payload = serde_json::to_vec(&selections)
                .map_err(|_| Status::internal("checkout selection could not be encoded"))?;
            let selection_digest = hex::encode(Sha256::digest(selection_payload));
            let item_quantities = selections
                .iter()
                .map(|item| (item.item_key.clone(), item.quantity))
                .collect::<BTreeMap<_, _>>();
            let item_quantities = serde_json::to_string(&item_quantities)
                .map_err(|_| Status::internal("checkout selection could not be encoded"))?;
            if item_quantities.len() > 500 {
                return Err(Status::invalid_argument(
                    "checkout selection exceeds Stripe metadata bounds",
                ));
            }
            let metadata = BTreeMap::from([
                ("erpx_holding_id".into(), holding_id.clone()),
                ("erpx_customer_id".into(), binding.customer_id.to_string()),
                (
                    "erpx_subscription_id".into(),
                    internal_subscription_id.to_string(),
                ),
                (
                    "erpx_catalog_revision".into(),
                    catalog.revision_number().to_string(),
                ),
                ("erpx_currency".into(), currency),
                ("erpx_items".into(), item_quantities),
                ("erpx_selection_sha256".into(), selection_digest),
                ("erpx_command_id".into(), command.command_id.to_string()),
            ]);
            let client = stripe_billing_client()?;
            let stripe = client
                .create_checkout_session(
                    StripeCheckoutSessionCreateRequest {
                        mode: StripeCheckoutMode::Subscription,
                        customer: existing_subscription
                            .as_ref()
                            .map(|existing| existing.stripe_customer_ref.clone()),
                        success_url: billing_return_url("/billing/checkout/success")?,
                        cancel_url: billing_return_url("/billing/checkout/canceled")?,
                        line_items,
                        automatic_tax: true,
                        collect_billing_address: true,
                        collect_tax_id: true,
                        subscription_metadata: metadata.clone(),
                        metadata,
                    },
                    &format!("erpx-checkout-{}", command.command_id),
                )
                .await
                .map_err(|_| Status::unavailable("Stripe Checkout is unavailable"))?
                .body;
            let hosted_url = stripe
                .url
                .ok_or_else(|| Status::unavailable("Stripe Checkout returned no hosted URL"))?;
            let expires_at = stripe
                .expires_at
                .and_then(|value| DateTime::<Utc>::from_timestamp(value, 0))
                .map(|value| value.to_rfc3339())
                .unwrap_or_default();
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: Some(binding.customer_id),
                    action: "commercial.billing.checkout_created".into(),
                    purpose: command.audit.purpose,
                    correlation_id: command.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("billing audit failed"))?;
            Ok(Response::new(
                commercial_target::CreateCheckoutSessionResponse {
                    hosted_url,
                    expires_at,
                    correlation_id: command.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn create_customer_portal_session(
        &self,
        request: Request<commercial_target::CreateCustomerPortalSessionRequest>,
    ) -> Result<Response<commercial_target::CreateCustomerPortalSessionResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let command = command_domain(&request, request.get_ref().context.as_ref())?;
            require_actor_match(
                &claims.principal_ref,
                &command.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?
                    .to_owned();
            let scope = access_scope(&command.active_scope);
            let binding = authorize_holding_billing_scope(
                &self.state,
                &claims,
                &holding_id,
                &scope,
                COMMERCIAL_BILLING_MANAGE_PERMISSION,
                false,
            )
            .await?;
            let subscription = SqlxHoldingAdministrationStore::new(self.state.store.clone())
                .holding_subscription_binding(&holding_id)
                .await
                .map_err(|_| Status::unavailable("holding subscription is unavailable"))?
                .ok_or_else(|| Status::failed_precondition("holding has no Stripe customer"))?;
            let client = stripe_billing_client()?;
            let stripe = client
                .create_customer_portal_session(
                    StripeCustomerPortalSessionCreateRequest {
                        customer: subscription.stripe_customer_ref,
                        return_url: billing_return_url("/billing")?,
                    },
                    &format!("erpx-portal-{}", command.command_id),
                )
                .await
                .map_err(|_| Status::unavailable("Stripe Customer Portal is unavailable"))?
                .body;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: Some(binding.customer_id),
                    action: "commercial.billing.portal_created".into(),
                    purpose: command.audit.purpose,
                    correlation_id: command.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("billing audit failed"))?;
            Ok(Response::new(
                commercial_target::CreateCustomerPortalSessionResponse {
                    hosted_url: stripe.url,
                    correlation_id: command.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn list_invoices(
        &self,
        request: Request<commercial_target::ListHoldingInvoicesRequest>,
    ) -> Result<Response<commercial_target::ListHoldingInvoicesResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let query = query_context(&request, Some(&proto_context))?;
            let scope = query_access_scope(&proto_context)?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            authorize_holding_billing_scope(
                &self.state,
                &claims,
                holding_id,
                &scope,
                COMMERCIAL_BILLING_READ_PERMISSION,
                true,
            )
            .await?;
            let page_size = match request.get_ref().page_size {
                0 => 25,
                value if value <= 100 => value,
                _ => return Err(Status::invalid_argument("invoice page size exceeds 100")),
            };
            let after = optional_registry_filter(&request.get_ref().after_invoice_id);
            let page = SqlxHoldingAdministrationStore::new(self.state.store.clone())
                .list_invoices(holding_id, after.as_deref(), page_size)
                .await
                .map_err(|_| Status::unavailable("holding invoices are unavailable"))?;
            Ok(Response::new(
                commercial_target::ListHoldingInvoicesResponse {
                    invoices: page
                        .invoices
                        .into_iter()
                        .map(|invoice| commercial_target::HoldingInvoiceSummary {
                            invoice_id: invoice.invoice_id,
                            state: invoice.state,
                            currency: invoice.currency,
                            amount_due_minor: invoice.amount_due_minor,
                            amount_paid_minor: invoice.amount_paid_minor,
                            due_at: invoice
                                .due_at
                                .map(|value| value.to_rfc3339())
                                .unwrap_or_default(),
                            hosted_invoice_available: invoice.hosted_invoice_available,
                        })
                        .collect(),
                    has_more: page.has_more,
                    next_after_invoice_id: page.next_after_invoice_id.unwrap_or_default(),
                    correlation_id: query.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn grant_grace_extension(
        &self,
        request: Request<commercial_target::GrantBillingGraceExtensionRequest>,
    ) -> Result<Response<commercial_target::GrantBillingGraceExtensionResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let command = command_domain(&request, request.get_ref().context.as_ref())?;
            require_actor_match(
                &claims.principal_ref,
                &command.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let scope = access_scope(&command.active_scope);
            require_platform_scope(&scope)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims,
                    &scope,
                    COMMERCIAL_BILLING_GRACE_EXTEND_PERMISSION,
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(&claims, &request.get_ref().mfa_code)
                .await?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            let cycle_id = parse_uuid(
                &request.get_ref().delinquency_cycle_id,
                "delinquency_cycle_id",
            )?;
            let days = request.get_ref().extension_days;
            if !(1..=7).contains(&days) {
                return Err(Status::invalid_argument(
                    "billing grace extension must be from one to seven days",
                ));
            }
            let reason = validate_transport_reference(&request.get_ref().reason, "reason")?;
            let extension = SqlxHoldingAdministrationStore::new(self.state.store.clone())
                .grant_billing_grace_extension(HoldingBillingGraceExtensionGrant {
                    holding_id,
                    delinquency_cycle_id: cycle_id,
                    extension: Duration::days(i64::from(days)),
                    granted_at: Utc::now(),
                    granted_by: &claims.principal_ref,
                    reason,
                    correlation_id: command.correlation_id,
                })
                .await
                .map_err(|error| commercial_persistence_status(error, command.correlation_id))?;
            Ok(Response::new(
                commercial_target::GrantBillingGraceExtensionResponse {
                    extension_id: extension.id.to_string(),
                    extended_until: extension.extended_until.to_rfc3339(),
                    correlation_id: command.correlation_id.to_string(),
                },
            ))
        })
    }
}

#[derive(Clone)]
struct PlatformOperationsRpcImpl {
    state: AppState,
}

async fn authorize_branch_administration<T>(
    state: &AppState,
    claims: &AccessTokenClaims,
    request: &Request<T>,
    scope: &Scope,
    holding_key: &str,
    holding_id: Uuid,
    mutation: bool,
) -> Result<(), Status> {
    let exact_holding = Scope::Holding { holding_id };
    if !scope.contains(&exact_holding) {
        return Err(Status::permission_denied(
            "branch administration requires exact holding or entered-holding scope",
        ));
    }
    state
        .auth
        .authorize_role_for_holding(
            &state.store,
            claims,
            scope,
            if mutation {
                "branch.infrastructure.provision"
            } else {
                "organization.read"
            },
            Some(holding_key),
        )
        .await?;
    if matches!(scope, Scope::Platform)
        || claims.principal_ref == erpx_identity::DEVELOPMENT_CONTROLLER_PRINCIPAL
    {
        validate_holding_admin_context(state, request, claims, holding_key, holding_id).await?;
    }
    Ok(())
}

async fn require_branch_capacity(store: &SqlxControlPlane, holding_id: &str) -> Result<(), Status> {
    let available = persistence::branch_capacity_available(store, holding_id)
        .await
        .map_err(|_| Status::unavailable("subscription branch capacity is unavailable"))?;
    available
        .then_some(())
        .ok_or_else(|| Status::failed_precondition("subscription branch capacity is exhausted"))
}

/// Resolve the customer authority behind an opaque physical holding route.
///
/// Owner-service transports need the registry holding and data-profile key to
/// select storage, while authorization is evaluated against the customer and
/// canonical organization scope. Keeping this join on the server prevents a
/// browser or native client from asserting either side of that relationship.
pub(crate) async fn customer_id_for_holding_route(
    state: &AppState,
    holding_id: &str,
    data_profile_key: &str,
) -> Result<Uuid, Status> {
    let holding_id = validate_transport_reference(holding_id, "holding_id")?;
    Uuid::parse_str(holding_id)
        .map_err(|_| Status::invalid_argument("holding_id must be a UUID"))?;
    let data_profile_key = validate_transport_reference(data_profile_key, "data_profile_key")?;
    let binding = SqlxHoldingAdministrationStore::new(state.store.clone())
        .customer_holding_binding(holding_id)
        .await
        .map_err(|_| Status::unavailable("holding route authority is unavailable"))?
        .ok_or_else(|| Status::permission_denied("holding route is not authorized"))?;
    if binding.data_profile_key != data_profile_key {
        return Err(Status::permission_denied("holding route is not authorized"));
    }
    Ok(binding.customer_id)
}

async fn branch_provisioning_record(
    store: &SqlxBranchProvisioningStore,
    job: erpx_infrastructure::BranchProvisioningJob,
) -> Result<platform_admin_target::HoldingBranchProvisioningRecord, Status> {
    let route = store
        .route(&job.holding_id, job.branch_id)
        .await
        .map_err(|_| Status::unavailable("branch route status is unavailable"))?;
    Ok(platform_admin_target::HoldingBranchProvisioningRecord {
        job_id: job.job_id.to_string(),
        holding_id: job.holding_id,
        branch_id: job.branch_id.to_string(),
        branch_code: job.branch_code,
        branch_name: job.branch_name,
        enabled_business_units: job
            .enabled_business_units
            .iter()
            .map(|unit| unit.as_str().to_owned())
            .collect(),
        state: job.state.as_str().into(),
        attempt_count: job.attempt_count,
        failure_category: route
            .as_ref()
            .and_then(|route| route.failure_category.clone())
            .unwrap_or_default(),
        created_at: job.created_at.to_rfc3339(),
        updated_at: job.updated_at.to_rfc3339(),
        completed_at: job
            .completed_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        route_version: route.as_ref().map_or(0, |route| route.route_version),
        correlation_id: job.correlation_id.to_string(),
    })
}

impl PlatformOperationsRpcImpl {
    pub(crate) async fn resolve_active_holding_route(
        &self,
        request: Request<platform_admin_target::ResolveActiveHoldingRouteRequest>,
    ) -> Result<Response<platform_admin_target::ResolveActiveHoldingRouteResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor,
                "query actor must match the authenticated principal",
            )?;
            let scope = query_access_scope(&proto_context)?;
            let canonical_holding_id = match &scope {
                Scope::Platform => {
                    return Err(Status::failed_precondition(
                        "an active holding scope is required",
                    ));
                }
                Scope::Holding { holding_id }
                | Scope::Company { holding_id, .. }
                | Scope::Branch { holding_id, .. }
                | Scope::Subtree { holding_id, .. } => *holding_id,
            };
            let store = SqlxHoldingAdministrationStore::new(self.state.store.clone());
            let holding_id = store
                .holding_id_for_root_holding(canonical_holding_id)
                .await
                .map_err(|_| Status::unavailable("holding route registry unavailable"))?
                .ok_or_else(|| Status::failed_precondition("holding route is not configured"))?;
            let binding = store
                .customer_holding_binding(&holding_id)
                .await
                .map_err(|_| Status::unavailable("holding route registry unavailable"))?
                .ok_or_else(|| Status::failed_precondition("holding route is not configured"))?;
            if binding.canonical_holding_id != canonical_holding_id {
                return Err(Status::permission_denied(
                    "active scope does not match the resolved holding route",
                ));
            }
            let mut authorized = false;
            // Physical routing is not business authorization. Permit an active
            // holding-scoped role to resolve its own route when it can read at
            // least one certified workspace; each owner service still applies
            // its action-specific authorization to the subsequent query.
            for permission in [
                "security.read",
                "holding.directory.read",
                "operations.read",
                "finance.read",
                "sales.read",
                "procurement.read",
                "inventory.read",
                "assets.read",
                "maintenance.read",
                "quality.read",
                "marketing-crm.read",
                "hr-payroll.read",
                "point-of-sale.read",
                "legal.read",
                "support.ticket.read",
            ] {
                match self
                    .state
                    .auth
                    .authorize_role_for_holding(
                        &self.state.store,
                        &claims,
                        &scope,
                        permission,
                        Some(&holding_id),
                    )
                    .await
                {
                    Ok(()) => {
                        authorized = true;
                        break;
                    }
                    Err(status) if status.code() == tonic::Code::PermissionDenied => {}
                    Err(status) => return Err(status),
                }
            }
            if !authorized {
                return Err(Status::permission_denied(
                    "active role does not authorize holding routing",
                ));
            }
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: Some(binding.customer_id),
                    action: "holding-route.resolve".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "active holding route resolution".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("holding route audit failed"))?;
            Ok(Response::new(
                platform_admin_target::ResolveActiveHoldingRouteResponse {
                    route: Some(platform_admin_target::ActiveHoldingRoute {
                        holding_id: binding.holding_id,
                        canonical_holding_id: binding.canonical_holding_id.to_string(),
                        data_profile_key: binding.data_profile_key,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn list_holdings(
        &self,
        request: Request<platform_admin_target::ListPlatformHoldingsRequest>,
    ) -> Result<Response<platform_admin_target::ListPlatformHoldingsResponse>, Status> {
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
                    &claims,
                    &scope,
                    PLATFORM_HOLDING_READ_PERMISSION,
                )
                .await?;

            let page_size = platform_holding_page_size(request.get_ref().page_size)?;
            let input = request.get_ref();
            let page = SqlxHoldingAdministrationStore::new(self.state.store.clone())
                .list_holdings(&HoldingRegistryFilter {
                    search: optional_registry_filter(&input.search),
                    lifecycle: optional_registry_filter(&input.lifecycle),
                    billing_state: optional_registry_filter(&input.billing_state),
                    plan: optional_registry_filter(&input.plan),
                    module: optional_registry_filter(&input.module),
                    health: optional_registry_filter(&input.health),
                    after_holding_id: optional_registry_filter(&input.after_holding_id),
                    page_size,
                })
                .await
                .map_err(|_| Status::internal("platform holding registry read failed"))?;
            let holdings = page
                .holdings
                .into_iter()
                .map(platform_holding_summary)
                .collect();
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.holding.list".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "platform operations holding list".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("platform holding audit failed"))?;
            Ok(Response::new(
                platform_admin_target::ListPlatformHoldingsResponse {
                    holdings,
                    has_more: page.has_more,
                    next_after_holding_id: page.next_after_holding_id.unwrap_or_default(),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn get_holding(
        &self,
        request: Request<platform_admin_target::GetPlatformHoldingRequest>,
    ) -> Result<Response<platform_admin_target::GetPlatformHoldingResponse>, Status> {
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
                    &claims,
                    &scope,
                    PLATFORM_HOLDING_READ_PERMISSION,
                )
                .await?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            let holding = SqlxHoldingAdministrationStore::new(self.state.store.clone())
                .get_holding_summary(holding_id)
                .await
                .map_err(|_| Status::internal("platform holding registry read failed"))?
                .ok_or_else(|| Status::not_found("platform holding not found"))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.holding.get".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "platform operations holding detail".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("platform holding audit failed"))?;
            Ok(Response::new(
                platform_admin_target::GetPlatformHoldingResponse {
                    holding: Some(platform_holding_summary(holding)),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn list_holding_control_history(
        &self,
        request: Request<platform_admin_target::ListHoldingControlHistoryRequest>,
    ) -> Result<Response<platform_admin_target::ListHoldingControlHistoryResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(&proto_context))?;
            require_platform_scope(&query_access_scope(&proto_context)?)?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            let administration = SqlxHoldingAdministrationStore::new(self.state.store.clone());
            let binding = administration
                .customer_holding_binding(holding_id)
                .await
                .map_err(|_| Status::unavailable("holding binding unavailable"))?
                .ok_or_else(|| Status::failed_precondition("holding binding is incomplete"))?;
            self.state
                .auth
                .authorize_role_for_holding(
                    &self.state.store,
                    &claims,
                    &Scope::Holding {
                        holding_id: binding.canonical_holding_id,
                    },
                    PLATFORM_HOLDING_HISTORY_READ_PERMISSION,
                    Some(holding_id),
                )
                .await?;
            let input = request.get_ref();
            let cursor = match (
                input.before_occurred_at.trim().is_empty(),
                input.before_event_id.trim().is_empty(),
            ) {
                (true, true) => None,
                (false, false) => Some(erpx_infrastructure::HoldingControlHistoryCursor {
                    occurred_at: DateTime::parse_from_rfc3339(&input.before_occurred_at)
                        .map_err(|_| {
                            Status::invalid_argument("history cursor time must be RFC 3339")
                        })?
                        .with_timezone(&Utc),
                    event_id: parse_uuid(&input.before_event_id, "before_event_id")?,
                }),
                _ => {
                    return Err(Status::invalid_argument(
                        "history cursor requires both occurred_at and event_id",
                    ));
                }
            };
            let page_size = match input.page_size {
                0 => 50,
                value if value <= 100 => value,
                _ => return Err(Status::invalid_argument("history page size exceeds 100")),
            };
            let page = administration
                .list_control_history(holding_id, cursor, page_size)
                .await
                .map_err(|_| Status::unavailable("holding control history unavailable"))?;
            Ok(Response::new(
                platform_admin_target::ListHoldingControlHistoryResponse {
                    entries: page
                        .entries
                        .into_iter()
                        .map(|entry| platform_admin_target::HoldingControlHistoryEntry {
                            event_id: entry.event_id.to_string(),
                            event_category: entry.event_category,
                            safe_action: entry.safe_action,
                            aggregate_version: entry.aggregate_version,
                            actor_ref: entry.actor_ref,
                            correlation_id: entry.correlation_id.to_string(),
                            occurred_at: entry.occurred_at.to_rfc3339(),
                        })
                        .collect(),
                    has_more: page.has_more,
                    next_before_occurred_at: page
                        .next_cursor
                        .map(|cursor| cursor.occurred_at.to_rfc3339())
                        .unwrap_or_default(),
                    next_before_event_id: page
                        .next_cursor
                        .map(|cursor| cursor.event_id.to_string())
                        .unwrap_or_default(),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn begin_holding_admin_context(
        &self,
        request: Request<platform_admin_target::BeginHoldingAdminContextRequest>,
    ) -> Result<Response<platform_admin_target::BeginHoldingAdminContextResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            let reason = validate_transport_reference(&request.get_ref().reason, "reason")?;
            let administration = SqlxHoldingAdministrationStore::new(self.state.store.clone());
            let binding = administration
                .customer_holding_binding(holding_id)
                .await
                .map_err(|_| Status::unavailable("holding binding unavailable"))?
                .ok_or_else(|| Status::failed_precondition("holding binding is incomplete"))?;
            self.state
                .auth
                .authorize_role_for_holding(
                    &self.state.store,
                    &claims,
                    &Scope::Holding {
                        holding_id: binding.canonical_holding_id,
                    },
                    PLATFORM_HOLDING_ENTER_PERMISSION,
                    Some(holding_id),
                )
                .await?;
            self.state
                .auth
                .authorize_mfa_for_holding(
                    &self.state.store,
                    &claims,
                    holding_id,
                    &request.get_ref().mfa_code,
                    context.correlation_id,
                )
                .await?;
            let at = Utc::now();
            let device_id = self
                .state
                .auth
                .identity
                .active_session_device_id(claims.session_id, at)
                .await
                .map_err(map_identity_auth_error)?;
            let administrative_context = administration
                .begin_context(HoldingAdminContextIssue {
                    holding_id,
                    principal_ref: &claims.principal_ref,
                    device_id,
                    session_id: claims.session_id,
                    reason,
                    correlation_id: context.correlation_id,
                    at,
                })
                .await
                .map_err(|_| {
                    Status::failed_precondition(
                        "holding configuration is incomplete or unavailable",
                    )
                })?;
            Ok(Response::new(
                platform_admin_target::BeginHoldingAdminContextResponse {
                    administrative_context: Some(holding_admin_context_message(
                        &administrative_context,
                        at,
                    )),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn get_holding_admin_context(
        &self,
        request: Request<platform_admin_target::GetHoldingAdminContextRequest>,
    ) -> Result<Response<platform_admin_target::GetHoldingAdminContextResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let query = query_context(&request, Some(&proto_context))?;
            require_platform_scope(&query_access_scope(&proto_context)?)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims,
                    &Scope::Platform,
                    PLATFORM_HOLDING_ENTER_PERMISSION,
                )
                .await?;
            let context_id = parse_uuid(
                &request.get_ref().administrative_context_id,
                "administrative_context_id",
            )?;
            let at = Utc::now();
            let device_id = self
                .state
                .auth
                .identity
                .active_session_device_id(claims.session_id, at)
                .await
                .map_err(map_identity_auth_error)?;
            let administrative_context =
                SqlxHoldingAdministrationStore::new(self.state.store.clone())
                    .get_context(context_id)
                    .await
                    .map_err(|_| Status::unavailable("holding administration context unavailable"))?
                    .ok_or_else(|| Status::not_found("holding administration context not found"))?;
            let requested_scope = HoldingOrganizationScope::Holding {
                holding_id: administrative_context.canonical_holding_id,
            };
            administrative_context
                .authorize(&HoldingAdminAuthorizationRequest {
                    principal_ref: &claims.principal_ref,
                    device_id,
                    session_id: claims.session_id,
                    holding_id: &administrative_context.holding_id,
                    requested_scope: &requested_scope,
                    module: None,
                    entitlements: &[],
                    identity_active: true,
                    session_active: true,
                    directory_allows: true,
                    platform_role_active: true,
                    at,
                })
                .map_err(|_| Status::permission_denied("holding administration context denied"))?;
            Ok(Response::new(
                platform_admin_target::GetHoldingAdminContextResponse {
                    administrative_context: Some(holding_admin_context_message(
                        &administrative_context,
                        at,
                    )),
                    correlation_id: query.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn end_holding_admin_context(
        &self,
        request: Request<platform_admin_target::EndHoldingAdminContextRequest>,
    ) -> Result<Response<platform_admin_target::EndHoldingAdminContextResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            require_platform_scope(&access_scope(&context.active_scope))?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let context_id = parse_uuid(
                &request.get_ref().administrative_context_id,
                "administrative_context_id",
            )?;
            let reason = validate_transport_reference(&request.get_ref().reason, "reason")?;
            let at = Utc::now();
            let device_id = self
                .state
                .auth
                .identity
                .active_session_device_id(claims.session_id, at)
                .await
                .map_err(map_identity_auth_error)?;
            let ended = SqlxHoldingAdministrationStore::new(self.state.store.clone())
                .end_context(
                    context_id,
                    &claims.principal_ref,
                    device_id,
                    claims.session_id,
                    reason,
                    at,
                )
                .await
                .map_err(|_| Status::unavailable("holding administration context unavailable"))?;
            Ok(Response::new(
                platform_admin_target::EndHoldingAdminContextResponse {
                    ended,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn discover_holding_organization_units(
        &self,
        request: Request<platform_admin_target::DiscoverHoldingOrganizationUnitsRequest>,
    ) -> Result<Response<platform_admin_target::DiscoverHoldingOrganizationUnitsResponse>, Status>
    {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let query = query_context(&request, Some(&proto_context))?;
            require_platform_scope(&query_access_scope(&proto_context)?)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims,
                    &Scope::Platform,
                    PLATFORM_HOLDING_ENTER_PERMISSION,
                )
                .await?;
            let context_id = request
                .metadata()
                .get("x-erpx-holding-admin-context")
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| {
                    Status::permission_denied("holding administration context is required")
                })
                .and_then(|value| parse_uuid(value, "administrative_context_id"))?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            let at = Utc::now();
            let device_id = self
                .state
                .auth
                .identity
                .active_session_device_id(claims.session_id, at)
                .await
                .map_err(map_identity_auth_error)?;
            let administration = SqlxHoldingAdministrationStore::new(self.state.store.clone());
            let administrative_context = administration
                .get_context(context_id)
                .await
                .map_err(|_| Status::unavailable("holding administration context unavailable"))?
                .ok_or_else(|| Status::not_found("holding administration context not found"))?;
            let requested_scope = HoldingOrganizationScope::Holding {
                holding_id: administrative_context.canonical_holding_id,
            };
            administrative_context
                .authorize(&HoldingAdminAuthorizationRequest {
                    principal_ref: &claims.principal_ref,
                    device_id,
                    session_id: claims.session_id,
                    holding_id,
                    requested_scope: &requested_scope,
                    module: None,
                    entitlements: &[],
                    identity_active: true,
                    session_active: true,
                    directory_allows: true,
                    platform_role_active: true,
                    at,
                })
                .map_err(|_| Status::permission_denied("holding administration context denied"))?;
            let registry = SqlxHoldingStoreRegistry::new(self.state.store.clone());
            let registration = registry
                .get(holding_id)
                .await
                .map_err(|_| Status::unavailable("holding registry is unavailable"))?
                .ok_or_else(|| Status::not_found("holding is not registered"))?;
            if registration.state != "active"
                || holding_holding_scope_id(&registration.data_profile_key)
                    != administrative_context.canonical_holding_id
            {
                return Err(Status::failed_precondition(
                    "holding organization route is unavailable",
                ));
            }
            let router = self
                .state
                .holding_router
                .as_ref()
                .ok_or_else(|| Status::unavailable("holding routing is not configured"))?;
            let read = router
                .route(
                    holding_id,
                    &registration.data_profile_key,
                    RoutedDatabaseSide::Read,
                )
                .await
                .map_err(|_| Status::unavailable("holding read route is unavailable"))?;
            let holding_uuid = Uuid::parse_str(holding_id)
                .map_err(|_| Status::failed_precondition("holding identity is invalid"))?;
            let pool = CqrsReadPool::for_holding(read.database_pool().clone(), holding_uuid)
                .map_err(|_| Status::unavailable("holding read authority is unavailable"))?;
            let store = OrganizationReadRepository::new(pool)
                .map_err(|_| Status::unavailable("organization read model is unavailable"))?;
            let page_size = organization_page_size(request.get_ref().page_size)?;
            let result = store
                .list(
                    &erpx_kernel::AuthorityScope::Holding {
                        holding_id: administrative_context.canonical_holding_id,
                    },
                    &OrganizationNodeListFilter {
                        page_size: u16::try_from(page_size).map_err(|_| {
                            Status::invalid_argument("page_size is outside its bound")
                        })?,
                        cursor: (!request.get_ref().after_cursor.trim().is_empty())
                            .then(|| request.get_ref().after_cursor.clone()),
                        ..OrganizationNodeListFilter::default()
                    },
                    &QueryConsistency::Eventual,
                )
                .await;
            read.close().await;
            let page = result
                .map_err(|_| Status::unavailable("organization read model is unavailable"))?
                .value;
            administration
                .record_context_access(
                    &administrative_context,
                    "organization",
                    "organization.units.discover",
                    at,
                )
                .await
                .map_err(|_| Status::unavailable("holding access audit unavailable"))?;
            Ok(Response::new(
                platform_admin_target::DiscoverHoldingOrganizationUnitsResponse {
                    units: page
                        .nodes
                        .into_iter()
                        .map(|node| platform_admin_target::HoldingOrganizationUnit {
                            kind: typed_organization_kind_name(node.kind).into(),
                            holding_id: administrative_context.canonical_holding_id.to_string(),
                            company_id: typed_organization_company_id(
                                node.node_id.as_uuid(),
                                &node.placement,
                            )
                            .map(|id| id.to_string())
                            .unwrap_or_default(),
                            branch_id: typed_organization_branch_id(
                                node.node_id.as_uuid(),
                                &node.placement,
                            )
                            .map(|id| id.to_string())
                            .unwrap_or_default(),
                            code: node.code,
                            name: node.name,
                        })
                        .collect(),
                    has_more: page.next_cursor.is_some(),
                    next_cursor: page.next_cursor.unwrap_or_default(),
                    correlation_id: query.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn begin_holding_provisioning(
        &self,
        request: Request<platform_admin_target::BeginHoldingProvisioningRequest>,
    ) -> Result<Response<platform_admin_target::BeginHoldingProvisioningResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let target_holding =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            self.state
                .auth
                .authorize_role_for_holding(
                    &self.state.store,
                    &claims,
                    &scope,
                    PLATFORM_HOLDING_PROVISION_PERMISSION,
                    Some(target_holding),
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
                    PLATFORM_HOLDING_PROVISION_PERMISSION,
                    &context.actor.principal_ref,
                )
                .await?;
            let command_owner = self
                .state
                .store
                .command_ownership_mode()
                .await
                .map_err(|_| {
                    Status::failed_precondition(
                        "authoritative ERPX command ownership is unavailable",
                    )
                })?;
            if command_owner != "erpx_only" {
                return Err(Status::failed_precondition(
                    "authoritative ERPX command ownership is unavailable",
                ));
            }
            self.state
                .auth
                .authorize_mfa_for_holding(
                    &self.state.store,
                    &claims,
                    target_holding,
                    command_mfa_code(request.get_ref().context.as_ref()),
                    context.correlation_id,
                )
                .await?;
            let input = request.into_inner();
            let registration = HoldingStoreRegistration {
                holding_id: validate_transport_reference(&input.holding_id, "holding_id")?.into(),
                data_profile_key: validate_transport_reference(
                    &input.data_profile_key,
                    "data_profile_key",
                )?
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
            let holding_id = registration.holding_id.clone();
            HoldingProvisioner::new(self.state.store.clone())
                .begin(registration)
                .await
                .map_err(|_| {
                    Status::failed_precondition("holding provisioning request rejected")
                })?;
            HoldingProvisioner::new(self.state.store.clone())
                .register_owner_seed(
                    holding_id.as_str(),
                    &owner_principal_ref,
                    &owner_approver_ref,
                )
                .await
                .map_err(|_| Status::failed_precondition("holding owner seed was rejected"))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: context.actor.principal_ref,
                    customer_account_id: None,
                    action: "platform.holding.provision.begin".into(),
                    purpose: context.audit.purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("platform audit persistence unavailable"))?;
            Ok(Response::new(
                platform_admin_target::BeginHoldingProvisioningResponse {
                    holding_id,
                    state: "provisioning".into(),
                    registry_version: 1,
                    accepted: true,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn provision_holding_resources(
        &self,
        request: Request<platform_admin_target::ProvisionHoldingResourcesRequest>,
    ) -> Result<Response<platform_admin_target::ProvisionHoldingResourcesResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let target_holding =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            self.state
                .auth
                .authorize_role_for_holding(
                    &self.state.store,
                    &claims,
                    &scope,
                    PLATFORM_HOLDING_PROVISION_PERMISSION,
                    Some(target_holding),
                )
                .await?;
            self.state
                .auth
                .authorize_mfa_for_holding(
                    &self.state.store,
                    &claims,
                    target_holding,
                    command_mfa_code(request.get_ref().context.as_ref()),
                    context.correlation_id,
                )
                .await?;
            let input = request.into_inner();
            let holding_id = validate_transport_reference(&input.holding_id, "holding_id")?;
            let owner_seed = HoldingProvisioner::new(self.state.store.clone())
                .get_owner_seed(holding_id)
                .await
                .map_err(|_| Status::unavailable("holding owner seed unavailable"))?
                .ok_or_else(|| Status::failed_precondition("holding owner seed is missing"))?;
            let database_config = HoldingDatabaseProvisioningConfig::from_env().map_err(|_| {
                Status::unavailable("holding provisioning configuration unavailable")
            })?;
            let provisioner =
                HoldingDatabaseProvisioner::new(self.state.store.clone(), database_config)
                    .map_err(|_| {
                        Status::unavailable("holding provisioning configuration unavailable")
                    })?;
            provisioner
                .provision_databases(holding_id)
                .await
                .map_err(|_| Status::unavailable("holding database provisioning unavailable"))?;
            provisioner
                .initialize_holding_organization(
                    holding_id,
                    owner_seed.holding_id,
                    &owner_seed.principal_ref,
                )
                .await
                .map_err(|_| {
                    Status::unavailable("holding organization initialization unavailable")
                })?;
            let storage = self.state.storage.clone();
            let storage_health = provisioner
                .provision_storage_namespace(holding_id, move |namespace| async move {
                    storage.ensure_namespace(&namespace).await
                })
                .await
                .map_err(|_| Status::unavailable("holding storage provisioning unavailable"))?;
            let registration = SqlxHoldingStoreRegistry::new(self.state.store.clone())
                .get(holding_id)
                .await
                .map_err(|_| Status::unavailable("holding registry unavailable"))?
                .ok_or_else(|| Status::not_found("holding not found"))?;
            let owner_receipt = seed_holding_owner(
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
                    action: "platform.holding.owner.seed".into(),
                    purpose: "holding owner bootstrap".into(),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("platform audit persistence unavailable"))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: context.actor.principal_ref,
                    customer_account_id: None,
                    action: "platform.holding.provision.resources".into(),
                    purpose: context.audit.purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("platform audit persistence unavailable"))?;
            Ok(Response::new(
                platform_admin_target::ProvisionHoldingResourcesResponse {
                    holding_id: holding_id.into(),
                    state: registration.state,
                    databases_ready: true,
                    storage_ready: storage_health.reachable,
                    correlation_id: context.correlation_id.to_string(),
                    owner_seeded: owner_receipt.seeded,
                    owner_principal_ref: owner_receipt.principal_ref,
                    owner_recovery_code: owner_receipt.recovery_code.unwrap_or_default(),
                },
            ))
        })
    }

    pub(crate) async fn activate_holding(
        &self,
        request: Request<platform_admin_target::TransitionHoldingStateRequest>,
    ) -> Result<Response<platform_admin_target::TransitionHoldingStateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            let registration = SqlxHoldingStoreRegistry::new(self.state.store.clone())
                .get(holding_id)
                .await
                .map_err(|_| Status::unavailable("holding registry unavailable"))?
                .ok_or_else(|| Status::not_found("holding not found"))?;
            validate_controller_holding_admin_context(
                &self.state,
                &request,
                &claims,
                holding_id,
                holding_holding_scope_id(&registration.data_profile_key),
            )
            .await?;
            self.state
                .auth
                .authorize_role_for_holding(
                    &self.state.store,
                    &claims,
                    &Scope::Holding {
                        holding_id: holding_holding_scope_id(&registration.data_profile_key),
                    },
                    PLATFORM_HOLDING_PROVISION_PERMISSION,
                    Some(holding_id),
                )
                .await?;
            self.state
                .auth
                .authorize_mfa_for_holding(
                    &self.state.store,
                    &claims,
                    holding_id,
                    command_mfa_code(request.get_ref().context.as_ref()),
                    context.correlation_id,
                )
                .await?;
            let at = Utc::now();
            let receipt = self
                .state
                .platform_completion
                .transition_lifecycle(PlatformLifecycleApiRequest {
                    command: HoldingLifecycleCommand {
                        binding: HoldingBinding::new(
                            holding_id.to_owned(),
                            registration.data_profile_key.clone(),
                        )
                        .map_err(|_| Status::failed_precondition("holding binding is invalid"))?,
                        action: HoldingLifecycleAction::Activate,
                        actor_ref: context.actor.principal_ref.clone(),
                        correlation_id: context.correlation_id,
                        idempotency_key: context.idempotency_key.clone(),
                        expected_registry_version: Some(registration.registry_version),
                        at,
                    },
                    proof: platform_authorization_proof(
                        &context.actor.principal_ref,
                        PLATFORM_HOLDING_PROVISION_PERMISSION,
                        scope,
                        at,
                    ),
                })
                .await?
                .into_inner();
            Ok(Response::new(
                platform_admin_target::TransitionHoldingStateResponse {
                    holding_id: receipt.holding_id,
                    state: receipt.state.as_str().into(),
                    registry_version: receipt.registry_version,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn suspend_holding(
        &self,
        request: Request<platform_admin_target::TransitionHoldingStateRequest>,
    ) -> Result<Response<platform_admin_target::TransitionHoldingStateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            let registration = SqlxHoldingStoreRegistry::new(self.state.store.clone())
                .get(holding_id)
                .await
                .map_err(|_| Status::unavailable("holding registry unavailable"))?
                .ok_or_else(|| Status::not_found("holding not found"))?;
            validate_controller_holding_admin_context(
                &self.state,
                &request,
                &claims,
                holding_id,
                holding_holding_scope_id(&registration.data_profile_key),
            )
            .await?;
            self.state
                .auth
                .authorize_role_for_holding(
                    &self.state.store,
                    &claims,
                    &Scope::Holding {
                        holding_id: holding_holding_scope_id(&registration.data_profile_key),
                    },
                    PLATFORM_HOLDING_SUSPEND_PERMISSION,
                    Some(holding_id),
                )
                .await?;
            self.state
                .auth
                .authorize_mfa_for_holding(
                    &self.state.store,
                    &claims,
                    holding_id,
                    command_mfa_code(request.get_ref().context.as_ref()),
                    context.correlation_id,
                )
                .await?;
            let at = Utc::now();
            let receipt = self
                .state
                .platform_completion
                .transition_lifecycle(PlatformLifecycleApiRequest {
                    command: HoldingLifecycleCommand {
                        binding: HoldingBinding::new(
                            holding_id.to_owned(),
                            registration.data_profile_key.clone(),
                        )
                        .map_err(|_| Status::failed_precondition("holding binding is invalid"))?,
                        action: HoldingLifecycleAction::Suspend,
                        actor_ref: context.actor.principal_ref.clone(),
                        correlation_id: context.correlation_id,
                        idempotency_key: context.idempotency_key.clone(),
                        expected_registry_version: Some(registration.registry_version),
                        at,
                    },
                    proof: platform_authorization_proof(
                        &context.actor.principal_ref,
                        PLATFORM_HOLDING_SUSPEND_PERMISSION,
                        scope,
                        at,
                    ),
                })
                .await?
                .into_inner();
            Ok(Response::new(
                platform_admin_target::TransitionHoldingStateResponse {
                    holding_id: receipt.holding_id,
                    state: receipt.state.as_str().into(),
                    registry_version: receipt.registry_version,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn enter_holding_recovery(
        &self,
        request: Request<platform_admin_target::TransitionHoldingStateRequest>,
    ) -> Result<Response<platform_admin_target::TransitionHoldingStateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            let registration = SqlxHoldingStoreRegistry::new(self.state.store.clone())
                .get(holding_id)
                .await
                .map_err(|_| Status::unavailable("holding registry unavailable"))?
                .ok_or_else(|| Status::not_found("holding not found"))?;
            validate_controller_holding_admin_context(
                &self.state,
                &request,
                &claims,
                holding_id,
                holding_holding_scope_id(&registration.data_profile_key),
            )
            .await?;
            self.state
                .auth
                .authorize_role_for_holding(
                    &self.state.store,
                    &claims,
                    &Scope::Holding {
                        holding_id: holding_holding_scope_id(&registration.data_profile_key),
                    },
                    PLATFORM_HOLDING_RECOVERY_PERMISSION,
                    Some(holding_id),
                )
                .await?;
            self.state
                .auth
                .authorize_mfa_for_holding(
                    &self.state.store,
                    &claims,
                    holding_id,
                    command_mfa_code(request.get_ref().context.as_ref()),
                    context.correlation_id,
                )
                .await?;
            let at = Utc::now();
            let receipt = self
                .state
                .platform_completion
                .transition_lifecycle(PlatformLifecycleApiRequest {
                    command: HoldingLifecycleCommand {
                        binding: HoldingBinding::new(
                            holding_id.to_owned(),
                            registration.data_profile_key.clone(),
                        )
                        .map_err(|_| Status::failed_precondition("holding binding is invalid"))?,
                        action: HoldingLifecycleAction::EnterRecovery,
                        actor_ref: context.actor.principal_ref.clone(),
                        correlation_id: context.correlation_id,
                        idempotency_key: context.idempotency_key.clone(),
                        expected_registry_version: Some(registration.registry_version),
                        at,
                    },
                    proof: platform_authorization_proof(
                        &context.actor.principal_ref,
                        PLATFORM_HOLDING_RECOVERY_PERMISSION,
                        scope,
                        at,
                    ),
                })
                .await?
                .into_inner();
            Ok(Response::new(
                platform_admin_target::TransitionHoldingStateResponse {
                    holding_id: receipt.holding_id,
                    state: receipt.state.as_str().into(),
                    registry_version: receipt.registry_version,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn schedule_holding_backup(
        &self,
        request: Request<platform_admin_target::ScheduleHoldingBackupRequest>,
    ) -> Result<Response<platform_admin_target::ScheduleHoldingBackupResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
            require_platform_scope(&scope)?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let target_holding =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            let requested_holding = validate_transport_reference(
                &request.get_ref().data_profile_key,
                "data_profile_key",
            )?;
            let registration = SqlxHoldingStoreRegistry::new(self.state.store.clone())
                .get(target_holding)
                .await
                .map_err(|_| Status::unavailable("holding registry unavailable"))?
                .ok_or_else(|| Status::not_found("holding not found"))?;
            if registration.data_profile_key != requested_holding {
                return Err(Status::permission_denied(
                    "holding and holding target do not match",
                ));
            }
            validate_controller_holding_admin_context(
                &self.state,
                &request,
                &claims,
                target_holding,
                holding_holding_scope_id(&registration.data_profile_key),
            )
            .await?;
            self.state
                .auth
                .authorize_role_for_holding(
                    &self.state.store,
                    &claims,
                    &Scope::Holding {
                        holding_id: holding_holding_scope_id(&registration.data_profile_key),
                    },
                    PLATFORM_HOLDING_BACKUP_PERMISSION,
                    Some(target_holding),
                )
                .await?;
            self.state
                .auth
                .authorize_mfa_for_holding(
                    &self.state.store,
                    &claims,
                    target_holding,
                    command_mfa_code(request.get_ref().context.as_ref()),
                    context.correlation_id,
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
            let holding_id = validate_transport_reference(&input.holding_id, "holding_id")?;
            let data_profile_key =
                validate_transport_reference(&input.data_profile_key, "data_profile_key")?;
            let kind = match validate_transport_reference(&input.kind, "kind")? {
                "full" => PlatformBackupKind::Full,
                "restore_test" => PlatformBackupKind::RestoreTest,
                "pitr" => {
                    return Err(Status::failed_precondition(
                        "holding PITR is unsupported; use cluster-scoped PITR",
                    ));
                }
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
                        binding: HoldingBinding::new(
                            holding_id.to_owned(),
                            data_profile_key.to_owned(),
                        )
                        .map_err(|_| Status::failed_precondition("holding binding is invalid"))?,
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
                        PLATFORM_HOLDING_BACKUP_PERMISSION,
                        scope,
                        at,
                    ),
                })
                .await?
                .into_inner();
            Ok(Response::new(
                platform_admin_target::ScheduleHoldingBackupResponse {
                    job_id: receipt.id.to_string(),
                    holding_id: receipt.holding_id,
                    kind: receipt.kind,
                    state: receipt.state,
                    available_at: receipt.available_at.to_rfc3339(),
                    max_attempts: u32::from(receipt.max_attempts),
                    duplicate: receipt.duplicate,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn list_holding_backup_jobs(
        &self,
        request: Request<platform_admin_target::ListHoldingBackupJobsRequest>,
    ) -> Result<Response<platform_admin_target::ListHoldingBackupJobsResponse>, Status> {
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
                    &claims,
                    &scope,
                    PLATFORM_HOLDING_READ_PERMISSION,
                )
                .await?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            let page_size = platform_holding_page_size(request.get_ref().page_size)?;
            let jobs = HoldingProvisioner::new(self.state.store.clone())
                .list_backup_jobs(holding_id, page_size)
                .await
                .map_err(|_| Status::internal("backup job history read failed"))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.holding.backup.list".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "platform holding backup history".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("platform audit persistence unavailable"))?;
            Ok(Response::new(
                platform_admin_target::ListHoldingBackupJobsResponse {
                    jobs: jobs
                        .into_iter()
                        .map(|job| platform_admin_target::HoldingBackupJobSummary {
                            job_id: job.id.to_string(),
                            holding_id: job.holding_id,
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
                },
            ))
        })
    }

    pub(crate) async fn schedule_cluster_base_backup(
        &self,
        request: Request<platform_admin_target::ScheduleClusterBaseBackupRequest>,
    ) -> Result<Response<platform_admin_target::ScheduleClusterBaseBackupResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
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
                    &claims,
                    &scope,
                    PLATFORM_HOLDING_BACKUP_PERMISSION,
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(&claims, &request.get_ref().mfa_code)
                .await?;
            let input = request.get_ref();
            let retention_until = DateTime::parse_from_rfc3339(&input.retention_until)
                .map_err(|_| Status::invalid_argument("retention_until is invalid"))?
                .with_timezone(&Utc);
            let at = Utc::now();
            let chain = ClusterPitrRepository::new(self.state.store.pools().write.clone())
                .schedule_base_backup(
                    validate_transport_reference(&input.cluster_ref, "cluster_ref")?,
                    validate_transport_reference(&input.storage_namespace, "storage_namespace")?,
                    validate_transport_reference(&input.chain_ref, "chain_ref")?,
                    retention_until,
                    at,
                )
                .await
                .map_err(|error| Status::failed_precondition(error.to_string()))?;
            Ok(Response::new(
                platform_admin_target::ScheduleClusterBaseBackupResponse {
                    chain_id: chain.id.to_string(),
                    cluster_ref: chain.cluster_ref,
                    chain_ref: chain.chain_ref,
                    state: chain.state,
                    retention_until: chain.retention_until.to_rfc3339(),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn list_cluster_recovery_points(
        &self,
        request: Request<platform_admin_target::ListClusterRecoveryPointsRequest>,
    ) -> Result<Response<platform_admin_target::ListClusterRecoveryPointsResponse>, Status> {
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
                    &claims,
                    &scope,
                    PLATFORM_HOLDING_READ_PERMISSION,
                )
                .await?;
            let input = request.get_ref();
            let page_size = match input.page_size {
                0 => 100,
                value if value <= MAX_CLUSTER_PITR_PAGE_SIZE => value,
                _ => return Err(Status::invalid_argument("page_size exceeds the PITR bound")),
            };
            let recovery_points =
                ClusterPitrRepository::new(self.state.store.pools().write.clone())
                    .list_recovery_points(
                        validate_transport_reference(&input.cluster_ref, "cluster_ref")?,
                        page_size,
                    )
                    .await
                    .map_err(|_| Status::unavailable("cluster PITR read unavailable"))?
                    .into_iter()
                    .map(|point| platform_admin_target::ClusterRecoveryPointSummary {
                        recovery_point_id: point.id.to_string(),
                        chain_id: point.chain_id.to_string(),
                        label: point.label,
                        target_time: point
                            .target_time
                            .map(|value| value.to_rfc3339())
                            .unwrap_or_default(),
                        target_lsn: point.target_lsn.unwrap_or_default(),
                        timeline_id: point.timeline_id,
                        wal_continuity_verified: point.wal_continuity_verified,
                        state: point.state,
                        created_at: point.created_at.to_rfc3339(),
                    })
                    .collect();
            Ok(Response::new(
                platform_admin_target::ListClusterRecoveryPointsResponse {
                    recovery_points,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn start_cluster_restore_rehearsal(
        &self,
        request: Request<platform_admin_target::StartClusterRestoreRehearsalRequest>,
    ) -> Result<Response<platform_admin_target::StartClusterRestoreRehearsalResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
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
                    &claims,
                    &scope,
                    PLATFORM_HOLDING_BACKUP_PERMISSION,
                )
                .await?;
            self.state
                .auth
                .authorize_mfa(&claims, &request.get_ref().mfa_code)
                .await?;
            let input = request.get_ref();
            let recovery_point_id = Uuid::parse_str(input.recovery_point_id.trim())
                .map_err(|_| Status::invalid_argument("recovery_point_id is invalid"))?;
            let rehearsal = ClusterPitrRepository::new(self.state.store.pools().write.clone())
                .schedule_restore_rehearsal(
                    recovery_point_id,
                    validate_transport_reference(&input.restore_target_ref, "restore_target_ref")?,
                    &claims.principal_ref,
                    context.correlation_id,
                    Utc::now(),
                )
                .await
                .map_err(|error| Status::failed_precondition(error.to_string()))?;
            Ok(Response::new(
                platform_admin_target::StartClusterRestoreRehearsalResponse {
                    rehearsal_id: rehearsal.id.to_string(),
                    recovery_point_id: rehearsal.recovery_point_id.to_string(),
                    restore_target_ref: rehearsal.restore_target_ref,
                    state: rehearsal.state,
                    created_at: rehearsal.created_at.to_rfc3339(),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn get_cluster_restore_rehearsal(
        &self,
        request: Request<platform_admin_target::GetClusterRestoreRehearsalRequest>,
    ) -> Result<Response<platform_admin_target::GetClusterRestoreRehearsalResponse>, Status> {
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
                    &claims,
                    &scope,
                    PLATFORM_HOLDING_READ_PERMISSION,
                )
                .await?;
            let rehearsal_id = Uuid::parse_str(request.get_ref().rehearsal_id.trim())
                .map_err(|_| Status::invalid_argument("rehearsal_id is invalid"))?;
            let result = ClusterPitrRepository::new(self.state.store.pools().write.clone())
                .read_restore_rehearsal(rehearsal_id)
                .await
                .map_err(|_| Status::not_found("restore rehearsal not found"))?;
            Ok(Response::new(
                platform_admin_target::GetClusterRestoreRehearsalResponse {
                    rehearsal_id: result.id.to_string(),
                    verification_complete: result.state == "completed",
                    state: result.state,
                    verification_checks: result.verification_checks,
                    failure_category: result.failure_category.unwrap_or_default(),
                    updated_at: result.updated_at.to_rfc3339(),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn start_projection_rebuild(
        &self,
        request: Request<platform_admin_target::StartProjectionRebuildRequest>,
    ) -> Result<Response<platform_admin_target::StartProjectionRebuildResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
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
                    &claims,
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
            let holding_id = validate_transport_reference(&input.holding_id, "holding_id")?;
            let projection_name = projection_rebuild_name(&input.projection_name)?;
            let candidate_read_database_key = validate_transport_reference(
                &input.candidate_read_database_key,
                "candidate_read_database_key",
            )?;
            let source_version = projection_rebuild_source_version(input.source_version)?;
            let registry = Arc::new(SqlxHoldingStoreRegistry::new(self.state.store.clone()));
            let registration = match registry.get(holding_id).await {
                Ok(Some(registration)) if registration.state == "active" => registration,
                Ok(Some(_)) => {
                    return Err(Status::failed_precondition(
                        "projection rebuild holding is not active",
                    ));
                }
                Ok(None) => return Err(Status::not_found("projection rebuild holding not found")),
                Err(_) => return Err(Status::unavailable("holding registry unavailable")),
            };
            let router = self
                .state
                .holding_router
                .as_deref()
                .ok_or_else(|| Status::unavailable("holding routing is not configured"))?;
            let observed_source_version = operations_source_high_watermark(
                router,
                holding_id,
                &registration.data_profile_key,
            )
            .await
            .map_err(|_| Status::unavailable("Operations source is unavailable"))?;
            if observed_source_version != source_version {
                return Err(Status::failed_precondition(
                    "projection source_version is no longer current",
                ));
            }
            let binding = HoldingHoldingBinding::new(
                registration.holding_id.clone(),
                registration.data_profile_key.clone(),
            )
            .map_err(|_| Status::failed_precondition("holding projection binding is invalid"))?;
            let scheduler_authorization = ProjectionSchedulerAuthorization::new(
                claims.principal_ref.clone(),
                context.audit.purpose.clone(),
                context.correlation_id,
            )
            .map_err(|_| Status::internal("projection scheduler authorization is invalid"))?;
            let scheduler = HoldingProjectionSchedulerRepository::new(
                self.state.store.pools().write.clone(),
                registry,
                scheduler_authorization,
            );
            let policy = ProjectionPolicy::new(500, 1, 5, 5, 300)
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
            Ok(Response::new(
                platform_admin_target::StartProjectionRebuildResponse {
                    holding_id: holding_id.into(),
                    projection_name: projection_name.into(),
                    state: "rebuilding".into(),
                    source_version,
                    correlation_id: context.correlation_id.to_string(),
                    generation,
                },
            ))
        })
    }

    pub(crate) async fn activate_projection_rebuild(
        &self,
        request: Request<platform_admin_target::ActivateProjectionRebuildRequest>,
    ) -> Result<Response<platform_admin_target::ActivateProjectionRebuildResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
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
                    &claims,
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
            let holding_id = validate_transport_reference(&input.holding_id, "holding_id")?;
            let projection_name = projection_rebuild_name(&input.projection_name)?;
            let candidate_read_database_key = validate_transport_reference(
                &input.candidate_read_database_key,
                "candidate_read_database_key",
            )?;
            let generation = projection_rebuild_generation(input.generation)?;
            let router = self
                .state
                .holding_router
                .as_deref()
                .ok_or_else(|| Status::unavailable("holding routing is not configured"))?;
            let registry = Arc::new(SqlxHoldingStoreRegistry::new(Arc::clone(&self.state.store)));
            let registration = registry
                .get(holding_id)
                .await
                .map_err(|_| Status::unavailable("holding registry unavailable"))?
                .ok_or_else(|| Status::failed_precondition("holding is not registered"))?;
            if projection_name != OPERATIONS_READ_PROJECTION {
                return Err(Status::invalid_argument(
                    "projection activation is not supported for this projection",
                ));
            }
            let source_version = router
                .replacement_read_source_version(
                    holding_id,
                    candidate_read_database_key,
                    generation,
                )
                .await
                .map_err(|_| Status::failed_precondition("projection generation is not ready"))?
                .ok_or_else(|| Status::failed_precondition("projection generation is not ready"))?;
            let write = router
                .route(
                    holding_id,
                    &registration.data_profile_key,
                    RoutedDatabaseSide::Write,
                )
                .await
                .map_err(|_| Status::unavailable("holding write database unavailable"))?;
            let mut source_lock = match write.acquire_source_rebuild_lock().await {
                Ok(lock) => lock,
                Err(_) => return Err(Status::unavailable("projection source fence unavailable")),
            };
            let current_source_version = source_lock
                .source_event_high_watermark(
                    HoldingSourceProjection::Operations,
                    &registration.data_profile_key,
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
                        holding_id,
                        candidate_read_database_key,
                        generation,
                        "source moved before manual activation",
                    )
                    .await
                    .map_err(|_| Status::unavailable("projection stale transition failed"))?;
                let binding = HoldingHoldingBinding::new(
                    registration.holding_id.clone(),
                    registration.data_profile_key.clone(),
                )
                .map_err(|_| {
                    Status::failed_precondition("holding projection binding is invalid")
                })?;
                let scheduler_authorization = ProjectionSchedulerAuthorization::new(
                    claims.principal_ref.clone(),
                    context.audit.purpose.clone(),
                    context.correlation_id,
                )
                .map_err(|_| Status::internal("projection scheduler authorization is invalid"))?;
                let scheduler = HoldingProjectionSchedulerRepository::new(
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
                    holding_id,
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
            Ok(Response::new(
                platform_admin_target::ActivateProjectionRebuildResponse {
                    holding_id: holding_id.into(),
                    projection_name: projection_name.into(),
                    state: "activated".into(),
                    generation,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn list_projection_statuses(
        &self,
        request: Request<platform_admin_target::ListProjectionStatusesRequest>,
    ) -> Result<Response<platform_admin_target::ListProjectionStatusesResponse>, Status> {
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
                    &claims,
                    &scope,
                    PLATFORM_HOLDING_READ_PERMISSION,
                )
                .await?;

            let input = request.get_ref();
            let holding_id = (!input.holding_id.trim().is_empty())
                .then(|| validate_transport_reference(&input.holding_id, "holding_id"))
                .transpose()?;
            let data_profile_key = (!input.data_profile_key.trim().is_empty())
                .then(|| validate_transport_reference(&input.data_profile_key, "data_profile_key"))
                .transpose()?;
            if data_profile_key.is_some() && holding_id.is_none() {
                return Err(Status::invalid_argument(
                    "data_profile_key requires holding_id",
                ));
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
                HoldingProjectionStatusReadRepository::new(self.state.store.pools().read.clone());
            let page = status_read
                .list_status_page(
                    holding_id,
                    data_profile_key,
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
                    platform_admin_target::ProjectionStatus {
                        holding_id: status.holding_id,
                        data_profile_key: status.data_profile_key,
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
            Ok(Response::new(
                platform_admin_target::ListProjectionStatusesResponse {
                    statuses,
                    has_more: page.has_more,
                    next_cursor: page
                        .next_cursor
                        .as_ref()
                        .map(encode_projection_status_cursor)
                        .unwrap_or_default(),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn list_platform_audit(
        &self,
        request: Request<platform_admin_target::ListPlatformAuditRequest>,
    ) -> Result<Response<platform_admin_target::ListPlatformAuditResponse>, Status> {
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
                    &claims,
                    &scope,
                    PLATFORM_HOLDING_READ_PERMISSION,
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
            Ok(Response::new(
                platform_admin_target::ListPlatformAuditResponse {
                    entries: page
                        .entries
                        .into_iter()
                        .map(|entry| platform_admin_target::PlatformAuditEntry {
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
                },
            ))
        })
    }

    pub(crate) async fn replay_dead_letter_outbox(
        &self,
        request: Request<platform_admin_target::ReplayDeadLetterOutboxRequest>,
    ) -> Result<Response<platform_admin_target::ReplayDeadLetterOutboxResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
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
                    &claims,
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
            Ok(Response::new(
                platform_admin_target::ReplayDeadLetterOutboxResponse {
                    event_id: receipt.event_id.to_string(),
                    state: if receipt.replayed {
                        "already_requeued".into()
                    } else {
                        "requeued".into()
                    },
                    correlation_id: receipt.correlation_id.to_string(),
                    replayed: receipt.replayed,
                },
            ))
        })
    }

    pub(crate) async fn list_platform_integrations(
        &self,
        request: Request<platform_admin_target::ListPlatformIntegrationsRequest>,
    ) -> Result<Response<platform_admin_target::ListPlatformIntegrationsResponse>, Status> {
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
                    &claims,
                    &scope,
                    PLATFORM_HOLDING_READ_PERMISSION,
                )
                .await?;
            let checked_at = Utc::now();
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
            let observations = vec![
                PlatformIntegrationObservation {
                    integration_key: "control_plane".into(),
                    state: control_plane_state.into(),
                    capability: "SQLx control-plane write/read pools".into(),
                    checked_at,
                },
                PlatformIntegrationObservation {
                    integration_key: "identity".into(),
                    state: identity_state.into(),
                    capability: "SQLx identity, session, and recovery boundary".into(),
                    checked_at,
                },
                PlatformIntegrationObservation {
                    integration_key: "holding_routing".into(),
                    state: if self.state.holding_router.is_some() {
                        "configured"
                    } else {
                        "not_configured"
                    }
                    .into(),
                    capability: "physical holding write/read database routing".into(),
                    checked_at,
                },
                PlatformIntegrationObservation {
                    integration_key: "object_storage".into(),
                    state: storage.0.into(),
                    capability: "MinIO-compatible holding object storage".into(),
                    checked_at,
                },
                PlatformIntegrationObservation {
                    integration_key: "billing".into(),
                    state: "not_probed".into(),
                    capability: "Stripe catalog, webhook, and entitlement boundary".into(),
                    checked_at,
                },
            ];
            let integrations = self
                .state
                .store
                .record_platform_integration_observations(&observations, &claims.principal_ref)
                .await
                .map_err(|_| Status::unavailable("platform integration persistence unavailable"))?
                .into_iter()
                .map(
                    |observation| platform_admin_target::PlatformIntegrationStatus {
                        integration_key: observation.integration_key,
                        state: observation.state,
                        capability: observation.capability,
                        checked_at: observation.checked_at.to_rfc3339(),
                    },
                )
                .collect();
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
            Ok(Response::new(
                platform_admin_target::ListPlatformIntegrationsResponse {
                    integrations,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn get_platform_settings(
        &self,
        request: Request<platform_admin_target::GetPlatformSettingsRequest>,
    ) -> Result<Response<platform_admin_target::GetPlatformSettingsResponse>, Status> {
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
                    &claims,
                    &scope,
                    PLATFORM_HOLDING_READ_PERMISSION,
                )
                .await?;
            let command_owner = self
                .state
                .store
                .command_ownership_mode()
                .await
                .map_err(|_| {
                    Status::failed_precondition(
                        "authoritative ERPX command ownership is unavailable",
                    )
                })?;
            if command_owner != "erpx_only" {
                return Err(Status::failed_precondition(
                    "authoritative ERPX command ownership is unavailable",
                ));
            }
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
            Ok(Response::new(
                platform_admin_target::GetPlatformSettingsResponse {
                    settings: Some(platform_admin_target::PlatformSettingsSummary {
                        contract_version: u32::from(CONTRACT_VERSION).to_string(),
                        transport: "tonic gRPC and gRPC-Web".into(),
                        holding_isolation: "physical per-holding write/read PostgreSQL databases"
                            .into(),
                        aggregate_authority: "PostgreSQL aggregate state with outbox projection"
                            .into(),
                        billing_authority: "Stripe payment and subscription state".into(),
                        storage_authority: "MinIO-compatible object storage".into(),
                        command_owner: format!("ERPX (persisted {command_owner} authority)"),
                    }),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn provision_holding_branch(
        &self,
        request: Request<platform_admin_target::ProvisionHoldingBranchRequest>,
    ) -> Result<Response<platform_admin_target::ProvisionHoldingBranchResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let holding_key =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?
                    .to_owned();
            let registration = SqlxHoldingStoreRegistry::new(self.state.store.clone())
                .get(&holding_key)
                .await
                .map_err(|_| Status::unavailable("holding registry is unavailable"))?
                .ok_or_else(|| Status::not_found("holding is not registered"))?;
            if registration.state != "active" {
                return Err(Status::failed_precondition("holding is not active"));
            }
            let holding_id = holding_holding_scope_id(&registration.data_profile_key);
            let scope = access_scope(&context.active_scope);
            authorize_branch_administration(
                &self.state,
                &claims,
                &request,
                &scope,
                &holding_key,
                holding_id,
                true,
            )
            .await?;
            self.state
                .auth
                .authorize_mfa_for_holding(
                    &self.state.store,
                    &claims,
                    &holding_key,
                    &request.get_ref().mfa_code,
                    context.correlation_id,
                )
                .await?;
            require_branch_capacity(&self.state.store, &holding_key).await?;
            let input = request.into_inner();
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let branch_id = parse_uuid(&input.branch_id, "branch_id")?;
            let target_scope = Scope::Branch {
                holding_id,
                company_id,
                branch_id,
            };
            if !scope.contains(&target_scope) {
                return Err(Status::permission_denied(
                    "branch provisioning scope is outside the authorized scope",
                ));
            }
            let units = input
                .enabled_business_units
                .iter()
                .map(|unit| {
                    BusinessUnitId::parse(unit.trim()).map_err(|_| {
                        Status::invalid_argument("enabled business unit is unsupported")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let store = SqlxBranchProvisioningStore::new(self.state.store.clone());
            let job = store
                .enqueue(&erpx_infrastructure::ProvisionBranchRequest {
                    holding_key,
                    data_profile_key: registration.data_profile_key,
                    holding_id,
                    company_id,
                    branch_id,
                    branch_code: input.branch_code,
                    branch_name: input.branch_name,
                    registration_number: input.registration_number,
                    profile_key: validate_transport_reference(
                        &input.database_profile_key,
                        "database_profile_key",
                    )?
                    .into(),
                    enabled_business_units: units,
                    idempotency_key: context.idempotency_key,
                    requested_by: claims.principal_ref,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|error| match error {
                    PersistenceError::InvalidData(message) => Status::failed_precondition(message),
                    _ => Status::unavailable("branch provisioning queue is unavailable"),
                })?;
            let provisioning = branch_provisioning_record(&store, job).await?;
            Ok(Response::new(
                platform_admin_target::ProvisionHoldingBranchResponse {
                    provisioning: Some(provisioning),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn get_holding_branch_provisioning(
        &self,
        request: Request<platform_admin_target::GetHoldingBranchProvisioningRequest>,
    ) -> Result<Response<platform_admin_target::GetHoldingBranchProvisioningResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let query = query_context(&request, Some(&proto_context))?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            let registration = SqlxHoldingStoreRegistry::new(self.state.store.clone())
                .get(holding_id)
                .await
                .map_err(|_| Status::unavailable("holding registry is unavailable"))?
                .ok_or_else(|| Status::not_found("holding is not registered"))?;
            authorize_branch_administration(
                &self.state,
                &claims,
                &request,
                &query_access_scope(&proto_context)?,
                holding_id,
                holding_holding_scope_id(&registration.data_profile_key),
                false,
            )
            .await?;
            let store = SqlxBranchProvisioningStore::new(self.state.store.clone());
            let job = store
                .get_job(holding_id, parse_uuid(&request.get_ref().job_id, "job_id")?)
                .await
                .map_err(|_| Status::unavailable("branch provisioning status is unavailable"))?
                .ok_or_else(|| Status::not_found("branch provisioning was not found"))?;
            Ok(Response::new(
                platform_admin_target::GetHoldingBranchProvisioningResponse {
                    provisioning: Some(branch_provisioning_record(&store, job).await?),
                    correlation_id: query.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn list_holding_branch_provisionings(
        &self,
        request: Request<platform_admin_target::ListHoldingBranchProvisioningsRequest>,
    ) -> Result<Response<platform_admin_target::ListHoldingBranchProvisioningsResponse>, Status>
    {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let query = query_context(&request, Some(&proto_context))?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            let registration = SqlxHoldingStoreRegistry::new(self.state.store.clone())
                .get(holding_id)
                .await
                .map_err(|_| Status::unavailable("holding registry is unavailable"))?
                .ok_or_else(|| Status::not_found("holding is not registered"))?;
            authorize_branch_administration(
                &self.state,
                &claims,
                &request,
                &query_access_scope(&proto_context)?,
                holding_id,
                holding_holding_scope_id(&registration.data_profile_key),
                false,
            )
            .await?;
            let page_size = match request.get_ref().page_size {
                0 => 50,
                value if value <= 100 => value,
                _ => return Err(Status::invalid_argument("page_size must be at most 100")),
            };
            let store = SqlxBranchProvisioningStore::new(self.state.store.clone());
            let jobs = store
                .list_jobs(holding_id, page_size)
                .await
                .map_err(|_| Status::unavailable("branch provisioning list is unavailable"))?;
            let mut provisionings = Vec::with_capacity(jobs.len());
            for job in jobs {
                provisionings.push(branch_provisioning_record(&store, job).await?);
            }
            Ok(Response::new(
                platform_admin_target::ListHoldingBranchProvisioningsResponse {
                    provisionings,
                    correlation_id: query.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn retry_holding_branch_provisioning(
        &self,
        request: Request<platform_admin_target::RetryHoldingBranchProvisioningRequest>,
    ) -> Result<Response<platform_admin_target::RetryHoldingBranchProvisioningResponse>, Status>
    {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            let registration = SqlxHoldingStoreRegistry::new(self.state.store.clone())
                .get(holding_id)
                .await
                .map_err(|_| Status::unavailable("holding registry is unavailable"))?
                .ok_or_else(|| Status::not_found("holding is not registered"))?;
            authorize_branch_administration(
                &self.state,
                &claims,
                &request,
                &access_scope(&context.active_scope),
                holding_id,
                holding_holding_scope_id(&registration.data_profile_key),
                true,
            )
            .await?;
            self.state
                .auth
                .authorize_mfa_for_holding(
                    &self.state.store,
                    &claims,
                    holding_id,
                    &request.get_ref().mfa_code,
                    context.correlation_id,
                )
                .await?;
            require_branch_capacity(&self.state.store, holding_id).await?;
            let store = SqlxBranchProvisioningStore::new(self.state.store.clone());
            let job = store
                .retry(
                    holding_id,
                    parse_uuid(&request.get_ref().job_id, "job_id")?,
                    context.correlation_id,
                    Utc::now(),
                )
                .await
                .map_err(|_| Status::failed_precondition("branch provisioning is not retryable"))?;
            Ok(Response::new(
                platform_admin_target::RetryHoldingBranchProvisioningResponse {
                    provisioning: Some(branch_provisioning_record(&store, job).await?),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    pub(crate) async fn suspend_holding_branch_route(
        &self,
        request: Request<platform_admin_target::SuspendHoldingBranchRouteRequest>,
    ) -> Result<Response<platform_admin_target::SuspendHoldingBranchRouteResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor must match the authenticated principal",
            )?;
            let holding_id =
                validate_transport_reference(&request.get_ref().holding_id, "holding_id")?;
            let registration = SqlxHoldingStoreRegistry::new(self.state.store.clone())
                .get(holding_id)
                .await
                .map_err(|_| Status::unavailable("holding registry is unavailable"))?
                .ok_or_else(|| Status::not_found("holding is not registered"))?;
            authorize_branch_administration(
                &self.state,
                &claims,
                &request,
                &access_scope(&context.active_scope),
                holding_id,
                holding_holding_scope_id(&registration.data_profile_key),
                true,
            )
            .await?;
            self.state
                .auth
                .authorize_mfa_for_holding(
                    &self.state.store,
                    &claims,
                    holding_id,
                    &request.get_ref().mfa_code,
                    context.correlation_id,
                )
                .await?;
            let branch_id = parse_uuid(&request.get_ref().branch_id, "branch_id")?;
            let route = SqlxBranchProvisioningStore::new(self.state.store.clone())
                .suspend(holding_id, branch_id, Utc::now())
                .await
                .map_err(|_| Status::failed_precondition("active branch route was not found"))?;
            Ok(Response::new(
                platform_admin_target::SuspendHoldingBranchRouteResponse {
                    holding_id: route.holding_id.to_string(),
                    branch_id: route.branch_id.to_string(),
                    state: route.state.as_str().into(),
                    route_version: route.route_version,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }
}

#[derive(Clone)]
struct PlatformAdministrationCommandComposite<A, G> {
    current: PlatformAdministrationCommandRpc<A, G>,
    operations: PlatformOperationsRpcImpl,
    migration: migration::MigrationRpcImpl,
    data_plane: data_plane_rpc::DataPlaneControlRpcImpl,
}

#[derive(Clone)]
struct PlatformAdministrationQueryComposite<A, G> {
    current: PlatformAdministrationQueryRpc<A, G>,
    operations: PlatformOperationsRpcImpl,
    migration: migration::MigrationRpcImpl,
    data_plane: data_plane_rpc::DataPlaneControlRpcImpl,
}

macro_rules! forward_platform_operation_command {
    ($name:ident, $target_request:ty, $target_response:ty) => {
        fn $name<'life0, 'async_trait>(
            &'life0 self,
            request: Request<$target_request>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<Response<$target_response>, Status>>
                    + Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move { self.operations.$name(request).await })
        }
    };
}

macro_rules! forward_platform_operation_query {
    ($target_name:ident, $legacy_name:ident, $target_request:ty, $target_response:ty) => {
        fn $target_name<'life0, 'async_trait>(
            &'life0 self,
            request: Request<$target_request>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<Response<$target_response>, Status>>
                    + Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move { self.operations.$legacy_name(request).await })
        }
    };
}

macro_rules! forward_migration_command {
    ($name:ident, $target_request:ty, $target_response:ty) => {
        fn $name<'life0, 'async_trait>(
            &'life0 self,
            request: Request<$target_request>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<Response<$target_response>, Status>>
                    + Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move { self.migration.$name(request).await })
        }
    };
}

macro_rules! forward_data_plane_rpc {
    ($name:ident, $target_request:ty, $target_response:ty) => {
        fn $name<'life0, 'async_trait>(
            &'life0 self,
            request: Request<$target_request>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<Response<$target_response>, Status>>
                    + Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move { self.data_plane.$name(request).await })
        }
    };
}

#[tonic::async_trait]
impl<A, G>
    platform_admin_target::platform_administration_command_service_server::PlatformAdministrationCommandService
    for PlatformAdministrationCommandComposite<A, G>
where
    A: contexts::platform_administration_cqrs::PlatformAdministrationCommandApplication,
    G: contexts::platform_administration_cqrs::PlatformAdministrationRequestAuthenticator,
{
    async fn register_holding(
        &self,
        request: Request<platform_admin_target::RegisterPlatformHoldingRequest>,
    ) -> Result<Response<platform_admin_target::RegisterPlatformHoldingResponse>, Status> {
        self.current.register_holding(request).await
    }

    async fn transition_holding(
        &self,
        request: Request<platform_admin_target::TransitionPlatformHoldingRequest>,
    ) -> Result<Response<platform_admin_target::TransitionPlatformHoldingResponse>, Status> {
        self.current.transition_holding(request).await
    }

    async fn register_connector(
        &self,
        request: Request<platform_admin_target::RegisterPlatformConnectorRequest>,
    ) -> Result<Response<platform_admin_target::RegisterPlatformConnectorResponse>, Status> {
        self.current.register_connector(request).await
    }

    async fn transition_connector(
        &self,
        request: Request<platform_admin_target::TransitionPlatformConnectorRequest>,
    ) -> Result<Response<platform_admin_target::TransitionPlatformConnectorResponse>, Status> {
        self.current.transition_connector(request).await
    }

    async fn issue_admin_context(
        &self,
        request: Request<platform_admin_target::IssuePlatformHoldingAdminContextRequest>,
    ) -> Result<Response<platform_admin_target::IssuePlatformHoldingAdminContextResponse>, Status> {
        self.current.issue_admin_context(request).await
    }

    async fn revoke_admin_context(
        &self,
        request: Request<platform_admin_target::RevokePlatformHoldingAdminContextRequest>,
    ) -> Result<Response<platform_admin_target::RevokePlatformHoldingAdminContextResponse>, Status> {
        self.current.revoke_admin_context(request).await
    }

    forward_platform_operation_command!(begin_holding_admin_context, platform_admin_target::BeginHoldingAdminContextRequest, platform_admin_target::BeginHoldingAdminContextResponse);
    forward_platform_operation_command!(end_holding_admin_context, platform_admin_target::EndHoldingAdminContextRequest, platform_admin_target::EndHoldingAdminContextResponse);
    forward_platform_operation_command!(begin_holding_provisioning, platform_admin_target::BeginHoldingProvisioningRequest, platform_admin_target::BeginHoldingProvisioningResponse);
    forward_platform_operation_command!(provision_holding_resources, platform_admin_target::ProvisionHoldingResourcesRequest, platform_admin_target::ProvisionHoldingResourcesResponse);
    forward_platform_operation_command!(activate_holding, platform_admin_target::TransitionHoldingStateRequest, platform_admin_target::TransitionHoldingStateResponse);
    forward_platform_operation_command!(suspend_holding, platform_admin_target::TransitionHoldingStateRequest, platform_admin_target::TransitionHoldingStateResponse);
    forward_platform_operation_command!(enter_holding_recovery, platform_admin_target::TransitionHoldingStateRequest, platform_admin_target::TransitionHoldingStateResponse);
    forward_platform_operation_command!(schedule_holding_backup, platform_admin_target::ScheduleHoldingBackupRequest, platform_admin_target::ScheduleHoldingBackupResponse);
    forward_platform_operation_command!(schedule_cluster_base_backup, platform_admin_target::ScheduleClusterBaseBackupRequest, platform_admin_target::ScheduleClusterBaseBackupResponse);
    forward_platform_operation_command!(start_cluster_restore_rehearsal, platform_admin_target::StartClusterRestoreRehearsalRequest, platform_admin_target::StartClusterRestoreRehearsalResponse);
    forward_platform_operation_command!(start_projection_rebuild, platform_admin_target::StartProjectionRebuildRequest, platform_admin_target::StartProjectionRebuildResponse);
    forward_platform_operation_command!(activate_projection_rebuild, platform_admin_target::ActivateProjectionRebuildRequest, platform_admin_target::ActivateProjectionRebuildResponse);
    forward_platform_operation_command!(replay_dead_letter_outbox, platform_admin_target::ReplayDeadLetterOutboxRequest, platform_admin_target::ReplayDeadLetterOutboxResponse);
    forward_platform_operation_command!(provision_holding_branch, platform_admin_target::ProvisionHoldingBranchRequest, platform_admin_target::ProvisionHoldingBranchResponse);
    forward_platform_operation_command!(retry_holding_branch_provisioning, platform_admin_target::RetryHoldingBranchProvisioningRequest, platform_admin_target::RetryHoldingBranchProvisioningResponse);
    forward_platform_operation_command!(suspend_holding_branch_route, platform_admin_target::SuspendHoldingBranchRouteRequest, platform_admin_target::SuspendHoldingBranchRouteResponse);
    forward_migration_command!(queue_migration_work, platform_admin_target::QueueMigrationWorkRequest, platform_admin_target::QueueMigrationWorkResponse);
    forward_migration_command!(begin_migration_run, platform_admin_target::BeginMigrationRunRequest, platform_admin_target::BeginMigrationRunResponse);
    forward_migration_command!(advance_migration_phase, platform_admin_target::AdvanceMigrationPhaseRequest, platform_admin_target::AdvanceMigrationPhaseResponse);
    forward_data_plane_rpc!(save_holding_infrastructure_profile, platform_admin_target::SaveHoldingInfrastructureProfileRequest, platform_admin_target::SaveHoldingInfrastructureProfileResponse);
    forward_data_plane_rpc!(save_business_unit_configuration, platform_admin_target::SaveBusinessUnitConfigurationRequest, platform_admin_target::SaveBusinessUnitConfigurationResponse);
    forward_data_plane_rpc!(issue_data_plane_enrollment, platform_admin_target::IssueDataPlaneEnrollmentRequest, platform_admin_target::IssueDataPlaneEnrollmentResponse);
    forward_data_plane_rpc!(propose_desired_configuration, platform_admin_target::ProposeDesiredConfigurationRequest, platform_admin_target::ProposeDesiredConfigurationResponse);
    forward_data_plane_rpc!(decide_desired_configuration, platform_admin_target::DecideDesiredConfigurationRequest, platform_admin_target::DecideDesiredConfigurationResponse);
    forward_data_plane_rpc!(request_root_access, platform_admin_target::RequestRootAccessRequest, platform_admin_target::RequestRootAccessResponse);
    forward_data_plane_rpc!(decide_root_access, platform_admin_target::DecideRootAccessRequest, platform_admin_target::DecideRootAccessResponse);
    forward_data_plane_rpc!(revoke_root_access, platform_admin_target::RevokeRootAccessRequest, platform_admin_target::RevokeRootAccessResponse);
    forward_data_plane_rpc!(save_corporate_identity_draft, platform_admin_target::SaveCorporateIdentityDraftRequest, platform_admin_target::SaveCorporateIdentityDraftResponse);
    forward_data_plane_rpc!(publish_corporate_identity_draft, platform_admin_target::PublishCorporateIdentityDraftRequest, platform_admin_target::PublishCorporateIdentityDraftResponse);
    forward_data_plane_rpc!(rollback_corporate_identity, platform_admin_target::RollbackCorporateIdentityRequest, platform_admin_target::RollbackCorporateIdentityResponse);
}

#[tonic::async_trait]
impl<A, G>
    platform_admin_target::platform_administration_query_service_server::PlatformAdministrationQueryService
    for PlatformAdministrationQueryComposite<A, G>
where
    A: contexts::platform_administration_cqrs::PlatformAdministrationQueryApplication,
    G: contexts::platform_administration_cqrs::PlatformAdministrationRequestAuthenticator,
{
    async fn get_holding(
        &self,
        request: Request<platform_admin_target::GetPlatformAdministrationHoldingRequest>,
    ) -> Result<Response<platform_admin_target::GetPlatformAdministrationHoldingResponse>, Status> {
        self.current.get_holding(request).await
    }

    async fn list_holdings(
        &self,
        request: Request<platform_admin_target::ListPlatformAdministrationHoldingsRequest>,
    ) -> Result<Response<platform_admin_target::ListPlatformAdministrationHoldingsResponse>, Status> {
        self.current.list_holdings(request).await
    }

    async fn get_connector(
        &self,
        request: Request<platform_admin_target::GetPlatformAdministrationConnectorRequest>,
    ) -> Result<Response<platform_admin_target::GetPlatformAdministrationConnectorResponse>, Status> {
        self.current.get_connector(request).await
    }

    async fn list_connectors(
        &self,
        request: Request<platform_admin_target::ListPlatformAdministrationConnectorsRequest>,
    ) -> Result<Response<platform_admin_target::ListPlatformAdministrationConnectorsResponse>, Status> {
        self.current.list_connectors(request).await
    }

    async fn get_admin_context(
        &self,
        request: Request<platform_admin_target::GetPlatformHoldingAdminContextRequest>,
    ) -> Result<Response<platform_admin_target::GetPlatformHoldingAdminContextResponse>, Status> {
        self.current.get_admin_context(request).await
    }

    forward_platform_operation_query!(list_operational_holdings, list_holdings, platform_admin_target::ListPlatformHoldingsRequest, platform_admin_target::ListPlatformHoldingsResponse);
    forward_platform_operation_query!(get_operational_holding, get_holding, platform_admin_target::GetPlatformHoldingRequest, platform_admin_target::GetPlatformHoldingResponse);
    forward_platform_operation_query!(resolve_active_holding_route, resolve_active_holding_route, platform_admin_target::ResolveActiveHoldingRouteRequest, platform_admin_target::ResolveActiveHoldingRouteResponse);
    forward_platform_operation_query!(list_holding_control_history, list_holding_control_history, platform_admin_target::ListHoldingControlHistoryRequest, platform_admin_target::ListHoldingControlHistoryResponse);
    forward_platform_operation_query!(get_holding_admin_context, get_holding_admin_context, platform_admin_target::GetHoldingAdminContextRequest, platform_admin_target::GetHoldingAdminContextResponse);
    forward_platform_operation_query!(discover_holding_organization_units, discover_holding_organization_units, platform_admin_target::DiscoverHoldingOrganizationUnitsRequest, platform_admin_target::DiscoverHoldingOrganizationUnitsResponse);
    forward_platform_operation_query!(list_holding_backup_jobs, list_holding_backup_jobs, platform_admin_target::ListHoldingBackupJobsRequest, platform_admin_target::ListHoldingBackupJobsResponse);
    forward_platform_operation_query!(list_cluster_recovery_points, list_cluster_recovery_points, platform_admin_target::ListClusterRecoveryPointsRequest, platform_admin_target::ListClusterRecoveryPointsResponse);
    forward_platform_operation_query!(get_cluster_restore_rehearsal, get_cluster_restore_rehearsal, platform_admin_target::GetClusterRestoreRehearsalRequest, platform_admin_target::GetClusterRestoreRehearsalResponse);
    forward_platform_operation_query!(list_projection_statuses, list_projection_statuses, platform_admin_target::ListProjectionStatusesRequest, platform_admin_target::ListProjectionStatusesResponse);
    forward_platform_operation_query!(list_platform_audit, list_platform_audit, platform_admin_target::ListPlatformAuditRequest, platform_admin_target::ListPlatformAuditResponse);
    forward_platform_operation_query!(list_platform_integrations, list_platform_integrations, platform_admin_target::ListPlatformIntegrationsRequest, platform_admin_target::ListPlatformIntegrationsResponse);
    forward_platform_operation_query!(get_platform_settings, get_platform_settings, platform_admin_target::GetPlatformSettingsRequest, platform_admin_target::GetPlatformSettingsResponse);
    forward_platform_operation_query!(get_holding_branch_provisioning, get_holding_branch_provisioning, platform_admin_target::GetHoldingBranchProvisioningRequest, platform_admin_target::GetHoldingBranchProvisioningResponse);
    forward_platform_operation_query!(list_holding_branch_provisionings, list_holding_branch_provisionings, platform_admin_target::ListHoldingBranchProvisioningsRequest, platform_admin_target::ListHoldingBranchProvisioningsResponse);

    async fn list_migration_statuses(
        &self,
        request: Request<platform_admin_target::ListMigrationStatusesRequest>,
    ) -> Result<Response<platform_admin_target::ListMigrationStatusesResponse>, Status> {
        self.migration.list_migration_statuses(request).await
    }

    forward_data_plane_rpc!(get_holding_infrastructure, platform_admin_target::GetHoldingInfrastructureRequest, platform_admin_target::GetHoldingInfrastructureResponse);
    forward_data_plane_rpc!(get_business_unit_configuration, platform_admin_target::GetBusinessUnitConfigurationRequest, platform_admin_target::GetBusinessUnitConfigurationResponse);
    forward_data_plane_rpc!(list_root_access_requests, platform_admin_target::ListRootAccessRequestsRequest, platform_admin_target::ListRootAccessRequestsResponse);
    forward_data_plane_rpc!(get_resolved_corporate_identity, platform_admin_target::GetResolvedCorporateIdentityRequest, platform_admin_target::GetResolvedCorporateIdentityResponse);
    forward_data_plane_rpc!(get_corporate_identity_draft, platform_admin_target::GetCorporateIdentityDraftRequest, platform_admin_target::GetCorporateIdentityDraftResponse);
    forward_data_plane_rpc!(list_corporate_identity_history, platform_admin_target::ListCorporateIdentityHistoryRequest, platform_admin_target::ListCorporateIdentityHistoryResponse);
    forward_data_plane_rpc!(get_navigation_manifest, platform_admin_target::GetNavigationManifestRequest, platform_admin_target::GetNavigationManifestResponse);
}

#[derive(Clone)]
struct DirectoryRpcImpl {
    state: AppState,
    runtime_enabled: bool,
}

impl DirectoryRpcImpl {
    async fn authorize_directory_role(
        &self,
        claims: &AccessTokenClaims,
        scope: &Scope,
        permission: &str,
    ) -> Result<Option<String>, Status> {
        match scope {
            Scope::Platform => {
                self.state
                    .auth
                    .authorize_role(&self.state.store, claims, scope, permission)
                    .await?;
                Ok(None)
            }
            Scope::Holding { holding_id } => {
                let holding_id = SqlxHoldingAdministrationStore::new(self.state.store.clone())
                    .holding_id_for_root_holding(*holding_id)
                    .await
                    .map_err(|_| Status::unavailable("directory holding binding unavailable"))?
                    .ok_or_else(|| {
                        Status::failed_precondition("directory holding is not bound to a holding")
                    })?;
                self.state
                    .auth
                    .authorize_role_for_holding(
                        &self.state.store,
                        claims,
                        scope,
                        permission,
                        Some(&holding_id),
                    )
                    .await?;
                Ok(Some(holding_id))
            }
            Scope::Company { .. } | Scope::Branch { .. } | Scope::Subtree { .. } => Err(
                Status::permission_denied("directory administration requires holding scope"),
            ),
        }
    }

    async fn authorize_directory_mfa(
        &self,
        claims: &AccessTokenClaims,
        holding_id: Option<&str>,
        presented_code: &str,
        correlation_id: Uuid,
    ) -> Result<(), Status> {
        if let Some(holding_id) = holding_id {
            self.state
                .auth
                .authorize_mfa_for_holding(
                    &self.state.store,
                    claims,
                    holding_id,
                    presented_code,
                    correlation_id,
                )
                .await
        } else {
            self.state.auth.authorize_mfa(claims, presented_code).await
        }
    }

    async fn validate_directory_admin_context<T>(
        &self,
        request: &Request<T>,
        claims: &AccessTokenClaims,
        scope: &Scope,
        target_holding: Option<&str>,
    ) -> Result<(), Status> {
        match (scope, target_holding) {
            (Scope::Holding { holding_id }, Some(holding_key)) => {
                validate_controller_holding_admin_context(
                    &self.state,
                    request,
                    claims,
                    holding_key,
                    *holding_id,
                )
                .await
            }
            _ if claims.principal_ref == erpx_identity::DEVELOPMENT_CONTROLLER_PRINCIPAL => Err(
                Status::permission_denied("holding-bound directory context is required"),
            ),
            _ => Ok(()),
        }
    }
}

impl DirectoryRpcImpl {
    async fn list_directory_connectors(
        &self,
        request: Request<identity_target::ListDirectoryConnectorsRequest>,
    ) -> Result<Response<identity_target::ListDirectoryConnectorsResponse>, Status> {
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
            self.authorize_directory_role(&claims, &scope, directory_permission(&scope, "read"))
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
            Ok(Response::new(
                identity_target::ListDirectoryConnectorsResponse {
                    connectors,
                    correlation_id: context.correlation_id.to_string(),
                    readiness_state: if self.runtime_enabled {
                        "enabled".into()
                    } else {
                        "disabled".into()
                    },
                    readiness_detail: if self.runtime_enabled {
                        "connector runtime enabled".into()
                    } else {
                        "development profile: LDAP, AD DS, and UCS workers are disabled".into()
                    },
                },
            ))
        })
    }

    async fn save_directory_connector(
        &self,
        request: Request<identity_target::SaveDirectoryConnectorRequest>,
    ) -> Result<Response<identity_target::SaveDirectoryConnectorResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(&proto_context))?;
            let scope = access_scope(&context.active_scope);
            if matches!(
                scope,
                Scope::Company { .. } | Scope::Branch { .. } | Scope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "directory administration requires platform or holding scope",
                ));
            }
            let target_holding = self
                .authorize_directory_role(&claims, &scope, directory_permission(&scope, "manage"))
                .await?;
            self.validate_directory_admin_context(
                &request,
                &claims,
                &scope,
                target_holding.as_deref(),
            )
            .await?;
            let input = request.get_ref();
            if input.enabled {
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref,
                        customer_account_id: None,
                        action: "platform.directory.activation.rejected".into(),
                        purpose: "Connector activation requires the lifecycle transition RPC"
                            .into(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::internal("directory audit failed"))?;
                return Err(Status::failed_precondition(
                    "save creates or updates a non-active connector; use the audited lifecycle transition RPC to activate it",
                ));
            }
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
            if !input.directory_authority_fingerprint.trim().is_empty() {
                return Err(Status::invalid_argument(
                    "directory authority fingerprint is observed by preflight and cannot be supplied by a client",
                ));
            }
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            let persisted_authority_fingerprint = if expected_revision == 0 {
                None
            } else {
                Some(
                    directory
                        .connector(connector_id)
                        .await
                        .map_err(|_| Status::not_found("directory connector not found"))?
                        .directory_authority_fingerprint,
                )
                .flatten()
            };
            let provider = parse_directory_provider(&input.provider)?;
            let connector = DirectoryConnector {
                id: connector_id,
                provider,
                scope: parse_directory_scope(&input.scope_key, &input.data_profile_key)?,
                endpoint: input.endpoint.trim().into(),
                base_dn: input.base_dn.trim().into(),
                tls: DirectoryTlsPolicy {
                    mode: parse_directory_tls(&input.tls_mode)?,
                    ca_certificate_sha256: input.ca_certificate_sha256.trim().into(),
                    server_name: input.server_name.trim().into(),
                    minimum_tls_version: input.minimum_tls_version.trim().into(),
                },
                feed: parse_directory_feed(&input.feed_kind)?,
                credential_ref: if input.credential_profile_key.trim().is_empty() {
                    input.credential_ref.trim().into()
                } else {
                    input.credential_profile_key.trim().into()
                },
                credential_profile_key: if input.credential_profile_key.trim().is_empty() {
                    input.credential_ref.trim().into()
                } else {
                    input.credential_profile_key.trim().into()
                },
                parent_connector_id: if input.parent_connector_id.trim().is_empty() {
                    None
                } else {
                    Some(parse_uuid(
                        &input.parent_connector_id,
                        "parent_connector_id",
                    )?)
                },
                directory_authority_fingerprint: persisted_authority_fingerprint,
                fallback_to_holding: input.fallback_to_holding,
                enabled: false,
                max_import_users: input.max_import_users,
                revision,
            };
            if let DirectoryScope::Branch {
                company_id,
                branch_id,
                ..
            } = &connector.scope
                && (input.company_id.trim() != company_id.to_string()
                    || input.branch_id.trim() != branch_id.to_string())
            {
                return Err(Status::invalid_argument(
                    "branch connector hierarchy fields do not match scope_key",
                ));
            }
            directory_scope_access(&scope, &connector.scope)?;
            self.authorize_directory_mfa(
                &claims,
                target_holding.as_deref(),
                &request.get_ref().mfa_code,
                context.correlation_id,
            )
            .await?;
            directory
                .validate_branch_parent_authority(&connector, false)
                .await
                .map_err(|error| Status::failed_precondition(error.to_string()))?;
            directory
                .save_connector(&connector, Utc::now())
                .await
                .map_err(|error| Status::invalid_argument(error.to_string()))?;
            let user_filter = if input.user_filter.trim().is_empty() {
                match provider {
                    DirectoryProvider::GenericLdap
                    | DirectoryProvider::ActiveDirectoryDomainServices => "(objectClass=person)",
                    DirectoryProvider::UniventionCorporateServer => "(objectClass=*)",
                }
            } else {
                input.user_filter.trim()
            };
            let page_size = if input.page_size == 0 {
                500
            } else {
                input.page_size
            };
            let attribute_mapping = parse_directory_json_object(
                &input.attribute_mapping_json,
                "attribute_mapping_json",
            )?;
            let feed_configuration = parse_directory_json_object(
                &input.feed_configuration_json,
                "feed_configuration_json",
            )?;
            let ca_reference = if input.ca_reference.trim().is_empty() {
                "system-trust"
            } else {
                input.ca_reference.trim()
            };
            directory
                .save_connector_runtime_settings(
                    connector_id,
                    user_filter,
                    page_size,
                    &attribute_mapping,
                    ca_reference,
                    &feed_configuration,
                    Utc::now(),
                )
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
            Ok(Response::new(
                identity_target::SaveDirectoryConnectorResponse {
                    connector: Some(directory_connector_summary(snapshot)),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn start_directory_import(
        &self,
        request: Request<identity_target::StartDirectoryImportRequest>,
    ) -> Result<Response<identity_target::StartDirectoryImportResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(&proto_context))?;
            let scope = access_scope(&context.active_scope);
            if matches!(
                scope,
                Scope::Company { .. } | Scope::Branch { .. } | Scope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "directory administration requires platform or holding scope",
                ));
            }
            let target_holding = self
                .authorize_directory_role(&claims, &scope, directory_permission(&scope, "manage"))
                .await?;
            self.validate_directory_admin_context(
                &request,
                &claims,
                &scope,
                target_holding.as_deref(),
            )
            .await?;
            if !self.runtime_enabled {
                self.state
                    .store
                    .record(AuditEvent {
                        actor_ref: claims.principal_ref,
                        customer_account_id: None,
                        action: "platform.directory.import.rejected".into(),
                        purpose: "Directory runtime is disabled in the development profile".into(),
                        correlation_id: context.correlation_id,
                    })
                    .await
                    .map_err(|_| Status::internal("directory audit failed"))?;
                return Err(Status::failed_precondition(
                    "directory runtime is disabled in the development profile",
                ));
            }
            let connector_id = Uuid::parse_str(request.get_ref().connector_id.trim())
                .map_err(|_| Status::invalid_argument("connector_id is invalid"))?;
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            let connector = directory
                .connector(connector_id)
                .await
                .map_err(|_| Status::not_found("directory connector not found"))?;
            directory_scope_access(&scope, &connector.scope)?;
            self.authorize_directory_mfa(
                &claims,
                target_holding.as_deref(),
                &request.get_ref().mfa_code,
                context.correlation_id,
            )
            .await?;
            directory
                .validate_branch_parent_authority(&connector, true)
                .await
                .map_err(|error| Status::failed_precondition(error.to_string()))?;
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
            Ok(Response::new(
                identity_target::StartDirectoryImportResponse {
                    connector_id: connector_id.to_string(),
                    page_count: plan.page_count(),
                    status: "queued".into(),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn get_directory_import_status(
        &self,
        request: Request<identity_target::GetDirectoryImportStatusRequest>,
    ) -> Result<Response<identity_target::GetDirectoryImportStatusResponse>, Status> {
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
            self.authorize_directory_role(&claims, &scope, directory_permission(&scope, "read"))
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
            Ok(Response::new(
                identity_target::GetDirectoryImportStatusResponse {
                    status: Some(directory_import_status_message(status)),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn list_directory_role_mappings(
        &self,
        request: Request<identity_target::ListDirectoryRoleMappingsRequest>,
    ) -> Result<Response<identity_target::ListDirectoryRoleMappingsResponse>, Status> {
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
            self.authorize_directory_role(&claims, &scope, directory_permission(&scope, "read"))
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
                    |mapping: DirectoryRoleMapping| identity_target::DirectoryRoleMappingMessage {
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
            Ok(Response::new(
                identity_target::ListDirectoryRoleMappingsResponse {
                    mappings,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn list_directory_identity_links(
        &self,
        request: Request<identity_target::ListDirectoryIdentityLinksRequest>,
    ) -> Result<Response<identity_target::ListDirectoryIdentityLinksResponse>, Status> {
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
            self.authorize_directory_role(&claims, &scope, directory_permission(&scope, "read"))
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
                .map(|link| identity_target::DirectoryIdentityLinkSummary {
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
            Ok(Response::new(
                identity_target::ListDirectoryIdentityLinksResponse {
                    links,
                    has_more,
                    next_after_identity_link,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn save_directory_role_mapping(
        &self,
        request: Request<identity_target::SaveDirectoryRoleMappingRequest>,
    ) -> Result<Response<identity_target::SaveDirectoryRoleMappingResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(&proto_context))?;
            let scope = access_scope(&context.active_scope);
            if matches!(
                scope,
                Scope::Company { .. } | Scope::Branch { .. } | Scope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "directory administration requires platform or holding scope",
                ));
            }
            let target_holding = self
                .authorize_directory_role(&claims, &scope, directory_permission(&scope, "manage"))
                .await?;
            self.validate_directory_admin_context(
                &request,
                &claims,
                &scope,
                target_holding.as_deref(),
            )
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
            let mapping_scope = parse_directory_scope(&input.scope_key, "")?;
            let mapping = DirectoryRoleMapping {
                connector_id,
                group_ref: input.group_ref.trim().into(),
                role_ref: input.role_ref.trim().into(),
                target_scope: match &mapping_scope {
                    DirectoryScope::Platform => erpx_identity::DirectoryRoleTargetScope::Platform,
                    DirectoryScope::Holding { .. } => {
                        erpx_identity::DirectoryRoleTargetScope::Holding
                    }
                    DirectoryScope::Branch { .. } => {
                        erpx_identity::DirectoryRoleTargetScope::Branch
                    }
                },
                scope: mapping_scope,
                requires_mfa: input.requires_mfa,
                active: input.active,
                revision,
            };
            let role_template_allowed = match &mapping.scope {
                DirectoryScope::Platform => directory_role_template_is_allowed(
                    ConfigurationLayer::Platform,
                    &mapping.role_ref,
                ),
                DirectoryScope::Holding { .. } | DirectoryScope::Branch { .. } => {
                    directory_role_template_is_allowed(
                        ConfigurationLayer::Holding,
                        &mapping.role_ref,
                    ) || BusinessUnitRoleTemplate::parse(&mapping.role_ref).is_ok()
                }
            };
            if !role_template_allowed {
                return Err(Status::invalid_argument(
                    "role_ref must reference an allow-listed ERPX role template",
                ));
            }
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
            self.authorize_directory_mfa(
                &claims,
                target_holding.as_deref(),
                &request.get_ref().mfa_code,
                context.correlation_id,
            )
            .await?;
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
            Ok(Response::new(
                identity_target::SaveDirectoryRoleMappingResponse {
                    mapping: Some(identity_target::DirectoryRoleMappingMessage {
                        connector_id: mapping.connector_id.to_string(),
                        group_ref: mapping.group_ref,
                        role_ref: mapping.role_ref,
                        scope_key: mapping.scope.canonical_key().unwrap_or_default(),
                        requires_mfa: mapping.requires_mfa,
                        active: mapping.active,
                        revision: mapping.revision,
                    }),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn preflight_directory_connector(
        &self,
        request: Request<identity_target::PreflightDirectoryConnectorRequest>,
    ) -> Result<Response<identity_target::PreflightDirectoryConnectorResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(&proto_context))?;
            let scope = access_scope(&context.active_scope);
            let target_holding = self
                .authorize_directory_role(&claims, &scope, directory_permission(&scope, "manage"))
                .await?;
            self.validate_directory_admin_context(
                &request,
                &claims,
                &scope,
                target_holding.as_deref(),
            )
            .await?;
            if !self.runtime_enabled {
                return Err(Status::failed_precondition(
                    "directory runtime is not configured",
                ));
            }
            let connector_id = Uuid::parse_str(request.get_ref().connector_id.trim())
                .map_err(|_| Status::invalid_argument("connector_id is invalid"))?;
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            let connector = directory
                .connector(connector_id)
                .await
                .map_err(|_| Status::not_found("directory connector not found"))?;
            directory_scope_access(&scope, &connector.scope)?;
            self.authorize_directory_mfa(
                &claims,
                target_holding.as_deref(),
                &request.get_ref().mfa_code,
                context.correlation_id,
            )
            .await?;
            directory
                .validate_branch_parent_authority(&connector, false)
                .await
                .map_err(|error| Status::failed_precondition(error.to_string()))?;
            let observed_at = Utc::now();
            let mut probe = preflight_directory_connector(
                &connector,
                self.state.auth.directory_secrets.as_ref(),
            )
            .await;
            let mut parent_authority_mismatch = false;
            if let (Some(evidence), Some(parent_id)) =
                (probe.as_ref().ok(), connector.parent_connector_id)
            {
                let parent = directory
                    .connector(parent_id)
                    .await
                    .map_err(|_| Status::failed_precondition("holding connector is unavailable"))?;
                parent_authority_mismatch = parent.directory_authority_fingerprint.as_deref()
                    != Some(evidence.authority_fingerprint.as_str())
            }
            if parent_authority_mismatch {
                probe = Err(LiveDirectoryError::Protocol);
            }
            let failure_category = probe.as_ref().err().map(|error| match error {
                LiveDirectoryError::Configuration => "configuration",
                LiveDirectoryError::Transport => "transport",
                LiveDirectoryError::InvalidCredentials => "invalid_credentials",
                LiveDirectoryError::AccountDenied => "account_denied",
                LiveDirectoryError::Protocol => "protocol",
                LiveDirectoryError::ResponseTooLarge => "response_too_large",
                LiveDirectoryError::TrustEvidenceUnavailable => "trust_evidence",
                LiveDirectoryError::NoMessage => "no_message",
                LiveDirectoryError::AlreadyCommitted(_) => "already_committed",
                LiveDirectoryError::SequenceOverflow => "sequence_overflow",
            });
            let lifecycle = directory
                .record_preflight_evidence(
                    connector_id,
                    probe.is_ok(),
                    failure_category,
                    probe
                        .as_ref()
                        .ok()
                        .map(|evidence| evidence.authority_fingerprint.as_str()),
                    context.correlation_id,
                    observed_at,
                )
                .await
                .map_err(|error| Status::failed_precondition(error.to_string()))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.directory.preflight".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "directory connector preflight".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("directory audit failed"))?;
            if let Some(category) = failure_category {
                return Err(Status::failed_precondition(format!(
                    "directory connector preflight failed: {category}"
                )));
            }
            Ok(Response::new(
                identity_target::PreflightDirectoryConnectorResponse {
                    connector_id: connector_id.to_string(),
                    lifecycle: lifecycle.as_str().into(),
                    tls_verified: true,
                    bind_verified: true,
                    feed_verified: true,
                    observed_at: observed_at.to_rfc3339(),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn transition_directory_connector_lifecycle(
        &self,
        request: Request<identity_target::TransitionDirectoryConnectorLifecycleRequest>,
    ) -> Result<Response<identity_target::TransitionDirectoryConnectorLifecycleResponse>, Status>
    {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(&proto_context))?;
            let scope = access_scope(&context.active_scope);
            let target_holding = self
                .authorize_directory_role(
                    &claims,
                    &scope,
                    directory_permission(
                        &scope,
                        if request.get_ref().target_lifecycle.trim() == "active" {
                            "activate"
                        } else {
                            "manage"
                        },
                    ),
                )
                .await?;
            self.validate_directory_admin_context(
                &request,
                &claims,
                &scope,
                target_holding.as_deref(),
            )
            .await?;
            let target = parse_directory_lifecycle(&request.get_ref().target_lifecycle)?;
            if target == DirectoryConnectorLifecycle::Active && !self.runtime_enabled {
                return Err(Status::failed_precondition(
                    "directory runtime is not configured",
                ));
            }
            let connector_id = Uuid::parse_str(request.get_ref().connector_id.trim())
                .map_err(|_| Status::invalid_argument("connector_id is invalid"))?;
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            let connector = directory
                .connector(connector_id)
                .await
                .map_err(|_| Status::not_found("directory connector not found"))?;
            directory_scope_access(&scope, &connector.scope)?;
            self.authorize_directory_mfa(
                &claims,
                target_holding.as_deref(),
                &request.get_ref().mfa_code,
                context.correlation_id,
            )
            .await?;
            if target == DirectoryConnectorLifecycle::Active {
                directory
                    .validate_branch_parent_authority(&connector, true)
                    .await
                    .map_err(|error| Status::failed_precondition(error.to_string()))?;
            }
            let lifecycle = directory
                .transition_lifecycle(
                    connector_id,
                    target,
                    &claims.principal_ref,
                    context.correlation_id,
                    Utc::now(),
                )
                .await
                .map_err(|error| Status::failed_precondition(error.to_string()))?;
            Ok(Response::new(
                identity_target::TransitionDirectoryConnectorLifecycleResponse {
                    connector_id: connector_id.to_string(),
                    lifecycle: lifecycle.as_str().into(),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn link_directory_principal(
        &self,
        request: Request<identity_target::LinkDirectoryPrincipalRequest>,
    ) -> Result<Response<identity_target::LinkDirectoryPrincipalResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(&proto_context))?;
            let scope = access_scope(&context.active_scope);
            let target_holding = self
                .authorize_directory_role(&claims, &scope, directory_permission(&scope, "manage"))
                .await?;
            self.validate_directory_admin_context(
                &request,
                &claims,
                &scope,
                target_holding.as_deref(),
            )
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
            self.authorize_directory_mfa(
                &claims,
                target_holding.as_deref(),
                &request.get_ref().mfa_code,
                context.correlation_id,
            )
            .await?;
            let binding = directory
                .link_principal(
                    connector_id,
                    input.external_subject.trim(),
                    input.principal_ref.trim(),
                    &claims.principal_ref,
                    context.correlation_id,
                    Utc::now(),
                )
                .await
                .map_err(|error| Status::failed_precondition(error.to_string()))?;
            Ok(Response::new(
                identity_target::LinkDirectoryPrincipalResponse {
                    binding_id: binding.binding_id.to_string(),
                    connector_id: binding.connector_id.to_string(),
                    external_subject: binding.external_object_id,
                    principal_ref: binding.principal_ref,
                    revision: binding.revision,
                    linked_at: binding.linked_at.to_rfc3339(),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn unlink_directory_principal(
        &self,
        request: Request<identity_target::UnlinkDirectoryPrincipalRequest>,
    ) -> Result<Response<identity_target::UnlinkDirectoryPrincipalResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(&proto_context))?;
            let scope = access_scope(&context.active_scope);
            let target_holding = self
                .authorize_directory_role(&claims, &scope, directory_permission(&scope, "manage"))
                .await?;
            self.validate_directory_admin_context(
                &request,
                &claims,
                &scope,
                target_holding.as_deref(),
            )
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
            self.authorize_directory_mfa(
                &claims,
                target_holding.as_deref(),
                &request.get_ref().mfa_code,
                context.correlation_id,
            )
            .await?;
            directory
                .unlink_principal(
                    connector_id,
                    input.external_subject.trim(),
                    &claims.principal_ref,
                    context.correlation_id,
                    Utc::now(),
                )
                .await
                .map_err(|error| Status::failed_precondition(error.to_string()))?;
            Ok(Response::new(
                identity_target::UnlinkDirectoryPrincipalResponse {
                    connector_id: connector_id.to_string(),
                    external_subject: input.external_subject.trim().into(),
                    unlinked: true,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn get_directory_connector_readiness(
        &self,
        request: Request<identity_target::GetDirectoryConnectorReadinessRequest>,
    ) -> Result<Response<identity_target::GetDirectoryConnectorReadinessResponse>, Status> {
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
            self.authorize_directory_role(&claims, &scope, directory_permission(&scope, "read"))
                .await?;
            let connector_id = Uuid::parse_str(request.get_ref().connector_id.trim())
                .map_err(|_| Status::invalid_argument("connector_id is invalid"))?;
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            let connector = directory
                .connector(connector_id)
                .await
                .map_err(|_| Status::not_found("directory connector not found"))?;
            directory_scope_access(&scope, &connector.scope)?;
            let readiness = directory
                .connector_readiness(connector_id)
                .await
                .map_err(|_| Status::internal("directory readiness read failed"))?;
            Ok(Response::new(
                identity_target::GetDirectoryConnectorReadinessResponse {
                    connector_id: connector_id.to_string(),
                    lifecycle: readiness.lifecycle.as_str().into(),
                    readiness: readiness.readiness(Utc::now()).into(),
                    last_preflight_at: readiness
                        .last_preflight_at
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_default(),
                    last_import_completed_at: readiness
                        .last_import_completed_at
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_default(),
                    last_reconciled_at: readiness
                        .last_reconciled_at
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_default(),
                    last_heartbeat_at: readiness
                        .last_heartbeat_at
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_default(),
                    unresolved_conflicts: readiness.unresolved_conflicts,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn reconcile_directory_connector(
        &self,
        request: Request<identity_target::ReconcileDirectoryConnectorRequest>,
    ) -> Result<Response<identity_target::ReconcileDirectoryConnectorResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(&proto_context))?;
            let scope = access_scope(&context.active_scope);
            let target_holding = self
                .authorize_directory_role(&claims, &scope, directory_permission(&scope, "manage"))
                .await?;
            self.validate_directory_admin_context(
                &request,
                &claims,
                &scope,
                target_holding.as_deref(),
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
            self.authorize_directory_mfa(
                &claims,
                target_holding.as_deref(),
                &request.get_ref().mfa_code,
                context.correlation_id,
            )
            .await?;
            let readiness = directory
                .connector_readiness(connector_id)
                .await
                .map_err(|_| Status::internal("directory reconciliation read failed"))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.directory.reconcile".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "directory reconciliation view".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("directory audit failed"))?;
            Ok(Response::new(
                identity_target::ReconcileDirectoryConnectorResponse {
                    connector_id: connector_id.to_string(),
                    state: if readiness.unresolved_conflicts == 0 {
                        "reconciled"
                    } else {
                        "conflicts_require_resolution"
                    }
                    .into(),
                    unresolved_conflicts: readiness.unresolved_conflicts,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn list_directory_conflicts(
        &self,
        request: Request<identity_target::ListDirectoryConflictsRequest>,
    ) -> Result<Response<identity_target::ListDirectoryConflictsResponse>, Status> {
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
                    "directory conflict review requires platform or holding scope",
                ));
            }
            self.authorize_directory_role(&claims, &scope, directory_permission(&scope, "read"))
                .await?;
            let connector_id = Uuid::parse_str(request.get_ref().connector_id.trim())
                .map_err(|_| Status::invalid_argument("connector_id is invalid"))?;
            let page_size = match request.get_ref().page_size {
                0 => 50,
                value if value <= 100 => value,
                _ => {
                    return Err(Status::invalid_argument(
                        "page_size must be between 1 and 100",
                    ));
                }
            };
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            let connector = directory
                .connector(connector_id)
                .await
                .map_err(|_| Status::not_found("directory connector not found"))?;
            directory_scope_access(&scope, &connector.scope)?;
            let conflicts = directory
                .list_identity_conflicts(connector_id, page_size)
                .await
                .map_err(|_| Status::internal("directory conflict read failed"))?
                .into_iter()
                .map(directory_conflict_summary)
                .collect();
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: directory_audit_customer(&self.state.store, &scope)
                        .await?,
                    action: directory_audit_action(&scope, "identity_conflict.list"),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "directory identity conflict review".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("directory audit failed"))?;
            Ok(Response::new(
                identity_target::ListDirectoryConflictsResponse {
                    conflicts,
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn propose_directory_identity_replacement(
        &self,
        request: Request<identity_target::ProposeDirectoryIdentityReplacementRequest>,
    ) -> Result<Response<identity_target::ProposeDirectoryIdentityReplacementResponse>, Status>
    {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(&proto_context))?;
            let scope = access_scope(&context.active_scope);
            if matches!(
                scope,
                Scope::Company { .. } | Scope::Branch { .. } | Scope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "directory identity replacement requires platform or holding scope",
                ));
            }
            let proposer = authenticated_directory_replacement_actor(&claims, &context.actor)?;
            if matches!(scope, Scope::Platform) {
                self.state
                    .auth
                    .authorize_role(
                        &self.state.store,
                        &claims,
                        &scope,
                        PLATFORM_DIRECTORY_MANAGE_PERMISSION,
                    )
                    .await?;
            } else {
                authorize_directory_replacement_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    true,
                )
                .await?;
            }
            self.state
                .auth
                .authorize_mfa(&claims, &request.get_ref().mfa_code)
                .await?;
            let conflict_id = Uuid::parse_str(request.get_ref().conflict_id.trim())
                .map_err(|_| Status::invalid_argument("conflict_id is invalid"))?;
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            let existing = directory
                .identity_conflict(conflict_id)
                .await
                .map_err(|_| Status::not_found("directory conflict not found"))?;
            let connector = directory
                .connector(existing.connector_id)
                .await
                .map_err(|_| Status::not_found("directory connector not found"))?;
            directory_scope_access(&scope, &connector.scope)?;
            let conflict = directory
                .propose_identity_replacement(
                    conflict_id,
                    request.get_ref().expected_conflict_version,
                    &proposer,
                    context.correlation_id,
                    Utc::now(),
                )
                .await
                .map_err(|error| Status::failed_precondition(error.to_string()))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: directory_audit_customer(&self.state.store, &scope)
                        .await?,
                    action: directory_audit_action(&scope, "identity_replacement.propose"),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "directory identity replacement proposal".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("directory audit failed"))?;
            Ok(Response::new(
                identity_target::ProposeDirectoryIdentityReplacementResponse {
                    conflict: Some(directory_conflict_summary(conflict)),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn approve_directory_identity_replacement(
        &self,
        request: Request<identity_target::ApproveDirectoryIdentityReplacementRequest>,
    ) -> Result<Response<identity_target::ApproveDirectoryIdentityReplacementResponse>, Status>
    {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(&proto_context))?;
            let scope = access_scope(&context.active_scope);
            if matches!(
                scope,
                Scope::Company { .. } | Scope::Branch { .. } | Scope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "directory identity replacement requires platform or holding scope",
                ));
            }
            let conflict_id = Uuid::parse_str(request.get_ref().conflict_id.trim())
                .map_err(|_| Status::invalid_argument("conflict_id is invalid"))?;
            let decision = authenticated_directory_replacement_decision(
                &claims,
                &context.actor,
                &context.active_scope,
                DirectoryIdentityReplacementDecisionAction::Approve,
                conflict_id,
                request.get_ref().expected_conflict_version,
                context.correlation_id,
                Utc::now(),
            )?;
            if matches!(scope, Scope::Platform) {
                self.state
                    .auth
                    .authorize_role(
                        &self.state.store,
                        &claims,
                        &scope,
                        PLATFORM_DIRECTORY_MANAGE_PERMISSION,
                    )
                    .await?;
            } else {
                authorize_directory_replacement_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    false,
                )
                .await?;
            }
            self.state
                .auth
                .authorize_mfa(&claims, &request.get_ref().mfa_code)
                .await?;
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            let existing = directory
                .identity_conflict(conflict_id)
                .await
                .map_err(|_| Status::not_found("directory conflict not found"))?;
            let connector = directory
                .connector(existing.connector_id)
                .await
                .map_err(|_| Status::not_found("directory connector not found"))?;
            directory_scope_access(&scope, &connector.scope)?;
            let conflict = directory
                .approve_identity_replacement(&decision)
                .await
                .map_err(|error| Status::failed_precondition(error.to_string()))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: decision.principal_ref().to_owned(),
                    customer_account_id: directory_audit_customer(&self.state.store, &scope)
                        .await?,
                    action: directory_audit_action(&scope, "identity_replacement.approve"),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "directory identity replacement approval".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("directory audit failed"))?;
            Ok(Response::new(
                identity_target::ApproveDirectoryIdentityReplacementResponse {
                    conflict: Some(directory_conflict_summary(conflict)),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn reject_directory_identity_replacement(
        &self,
        request: Request<identity_target::RejectDirectoryIdentityReplacementRequest>,
    ) -> Result<Response<identity_target::RejectDirectoryIdentityReplacementResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let context = command_domain(&request, Some(&proto_context))?;
            let scope = access_scope(&context.active_scope);
            if matches!(
                scope,
                Scope::Company { .. } | Scope::Branch { .. } | Scope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "directory identity replacement requires platform or holding scope",
                ));
            }
            let conflict_id = Uuid::parse_str(request.get_ref().conflict_id.trim())
                .map_err(|_| Status::invalid_argument("conflict_id is invalid"))?;
            let decision = authenticated_directory_replacement_decision(
                &claims,
                &context.actor,
                &context.active_scope,
                DirectoryIdentityReplacementDecisionAction::Reject,
                conflict_id,
                request.get_ref().expected_conflict_version,
                context.correlation_id,
                Utc::now(),
            )?;
            if matches!(scope, Scope::Platform) {
                self.state
                    .auth
                    .authorize_role(
                        &self.state.store,
                        &claims,
                        &scope,
                        PLATFORM_DIRECTORY_MANAGE_PERMISSION,
                    )
                    .await?;
            } else {
                authorize_directory_replacement_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &scope,
                    false,
                )
                .await?;
            }
            self.state
                .auth
                .authorize_mfa(&claims, &request.get_ref().mfa_code)
                .await?;
            let directory = SqlxDirectoryStore::new(self.state.store.pools().write.clone());
            let existing = directory
                .identity_conflict(conflict_id)
                .await
                .map_err(|_| Status::not_found("directory conflict not found"))?;
            let connector = directory
                .connector(existing.connector_id)
                .await
                .map_err(|_| Status::not_found("directory connector not found"))?;
            directory_scope_access(&scope, &connector.scope)?;
            let conflict = directory
                .reject_identity_replacement(&decision)
                .await
                .map_err(|error| Status::failed_precondition(error.to_string()))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: decision.principal_ref().to_owned(),
                    customer_account_id: directory_audit_customer(&self.state.store, &scope)
                        .await?,
                    action: directory_audit_action(&scope, "identity_replacement.reject"),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "directory identity replacement rejection".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::internal("directory audit failed"))?;
            Ok(Response::new(
                identity_target::RejectDirectoryIdentityReplacementResponse {
                    conflict: Some(directory_conflict_summary(conflict)),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }
}

#[derive(Clone)]
struct RoleRpcImpl {
    state: AppState,
}

impl RoleRpcImpl {
    async fn list_expiring_roles(
        &self,
        request: Request<identity_target::ListExpiringRolesRequest>,
    ) -> Result<Response<identity_target::ListExpiringRolesResponse>, Status> {
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
                .authorize_role(&self.state.store, &claims, &scope, "role.assignment.read")
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
            Ok(Response::new(identity_target::ListExpiringRolesResponse {
                assignments: assignments.into_iter().map(to_role_message).collect(),
            }))
        })
    }

    async fn list_role_review(
        &self,
        request: Request<identity_target::ListRoleReviewRequest>,
    ) -> Result<Response<identity_target::ListRoleReviewResponse>, Status> {
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
                .authorize_role(&self.state.store, &claims, &scope, "role.assignment.review")
                .await?;
            let within_days = request.get_ref().within_days;
            if within_days == 0 || within_days > MAX_ROLE_REVIEW_DAYS {
                return Err(Status::invalid_argument(format!(
                    "within_days must be between 1 and {MAX_ROLE_REVIEW_DAYS}"
                )));
            }
            let queue = self
                .state
                .roles
                .review_queue(Duration::days(i64::from(within_days)))
                .await
                .map_err(|error| errors::application(error, context.correlation_id))?;
            let items = queue
                .items
                .into_iter()
                .filter(|item| scope.contains(&item.assignment.scope))
                .map(to_role_review_message)
                .collect();
            Ok(Response::new(identity_target::ListRoleReviewResponse {
                evaluated_at: queue.evaluated_at.to_rfc3339(),
                expiry_horizon_until: queue.expiry_horizon_until.to_rfc3339(),
                items,
            }))
        })
    }
}

#[derive(Clone)]
struct IdentityAccessCommandComposite<A, G> {
    current: IdentityAccessCommandRpc<A, G>,
    directory: DirectoryRpcImpl,
}

impl<A, G> IdentityAccessCommandComposite<A, G> {
    fn new(current: IdentityAccessCommandRpc<A, G>, directory: DirectoryRpcImpl) -> Self {
        Self { current, directory }
    }
}

#[tonic::async_trait]
impl<A, G> TargetIdentityAccessCommandService for IdentityAccessCommandComposite<A, G>
where
    A: IdentityAccessCommandApplication,
    G: IdentityAccessRequestAuthenticator,
{
    async fn save_directory_connector(
        &self,
        request: Request<identity_target::SaveDirectoryConnectorRequest>,
    ) -> Result<Response<identity_target::SaveDirectoryConnectorResponse>, Status> {
        self.directory.save_directory_connector(request).await
    }

    async fn start_directory_import(
        &self,
        request: Request<identity_target::StartDirectoryImportRequest>,
    ) -> Result<Response<identity_target::StartDirectoryImportResponse>, Status> {
        self.directory.start_directory_import(request).await
    }

    async fn save_directory_role_mapping(
        &self,
        request: Request<identity_target::SaveDirectoryRoleMappingRequest>,
    ) -> Result<Response<identity_target::SaveDirectoryRoleMappingResponse>, Status> {
        self.directory.save_directory_role_mapping(request).await
    }

    async fn preflight_directory_connector(
        &self,
        request: Request<identity_target::PreflightDirectoryConnectorRequest>,
    ) -> Result<Response<identity_target::PreflightDirectoryConnectorResponse>, Status> {
        self.directory.preflight_directory_connector(request).await
    }

    async fn transition_directory_connector_lifecycle(
        &self,
        request: Request<identity_target::TransitionDirectoryConnectorLifecycleRequest>,
    ) -> Result<Response<identity_target::TransitionDirectoryConnectorLifecycleResponse>, Status>
    {
        self.directory
            .transition_directory_connector_lifecycle(request)
            .await
    }

    async fn link_directory_principal(
        &self,
        request: Request<identity_target::LinkDirectoryPrincipalRequest>,
    ) -> Result<Response<identity_target::LinkDirectoryPrincipalResponse>, Status> {
        self.directory.link_directory_principal(request).await
    }

    async fn unlink_directory_principal(
        &self,
        request: Request<identity_target::UnlinkDirectoryPrincipalRequest>,
    ) -> Result<Response<identity_target::UnlinkDirectoryPrincipalResponse>, Status> {
        self.directory.unlink_directory_principal(request).await
    }

    async fn reconcile_directory_connector(
        &self,
        request: Request<identity_target::ReconcileDirectoryConnectorRequest>,
    ) -> Result<Response<identity_target::ReconcileDirectoryConnectorResponse>, Status> {
        self.directory.reconcile_directory_connector(request).await
    }

    async fn propose_directory_identity_replacement(
        &self,
        request: Request<identity_target::ProposeDirectoryIdentityReplacementRequest>,
    ) -> Result<Response<identity_target::ProposeDirectoryIdentityReplacementResponse>, Status>
    {
        self.directory
            .propose_directory_identity_replacement(request)
            .await
    }

    async fn approve_directory_identity_replacement(
        &self,
        request: Request<identity_target::ApproveDirectoryIdentityReplacementRequest>,
    ) -> Result<Response<identity_target::ApproveDirectoryIdentityReplacementResponse>, Status>
    {
        self.directory
            .approve_directory_identity_replacement(request)
            .await
    }

    async fn reject_directory_identity_replacement(
        &self,
        request: Request<identity_target::RejectDirectoryIdentityReplacementRequest>,
    ) -> Result<Response<identity_target::RejectDirectoryIdentityReplacementResponse>, Status> {
        self.directory
            .reject_directory_identity_replacement(request)
            .await
    }

    async fn renew_role(
        &self,
        request: Request<identity_target::RenewRoleRequest>,
    ) -> Result<Response<identity_target::RenewRoleResponse>, Status> {
        self.current.renew_role(request).await
    }

    async fn recover_final_role(
        &self,
        request: Request<identity_target::RecoverFinalRoleRequest>,
    ) -> Result<Response<identity_target::RecoverFinalRoleResponse>, Status> {
        self.current.recover_final_role(request).await
    }

    async fn create_principal(
        &self,
        request: Request<identity_target::CreateIdentityAccessPrincipalRequest>,
    ) -> Result<Response<identity_target::IdentityAccessPrincipalCommandResponse>, Status> {
        self.current.create_principal(request).await
    }

    async fn activate_principal(
        &self,
        request: Request<identity_target::IdentityAccessPrincipalMutationRequest>,
    ) -> Result<Response<identity_target::IdentityAccessPrincipalCommandResponse>, Status> {
        self.current.activate_principal(request).await
    }

    async fn suspend_principal(
        &self,
        request: Request<identity_target::IdentityAccessPrincipalMutationRequest>,
    ) -> Result<Response<identity_target::IdentityAccessPrincipalCommandResponse>, Status> {
        self.current.suspend_principal(request).await
    }

    async fn reactivate_principal(
        &self,
        request: Request<identity_target::IdentityAccessPrincipalMutationRequest>,
    ) -> Result<Response<identity_target::IdentityAccessPrincipalCommandResponse>, Status> {
        self.current.reactivate_principal(request).await
    }

    async fn disable_principal(
        &self,
        request: Request<identity_target::IdentityAccessPrincipalMutationRequest>,
    ) -> Result<Response<identity_target::IdentityAccessPrincipalCommandResponse>, Status> {
        self.current.disable_principal(request).await
    }

    async fn assign_role(
        &self,
        request: Request<identity_target::AssignIdentityAccessRoleRequest>,
    ) -> Result<Response<identity_target::IdentityAccessPrincipalCommandResponse>, Status> {
        self.current.assign_role(request).await
    }

    async fn revoke_role(
        &self,
        request: Request<identity_target::RevokeIdentityAccessRoleRequest>,
    ) -> Result<Response<identity_target::IdentityAccessPrincipalCommandResponse>, Status> {
        self.current.revoke_role(request).await
    }

    async fn define_role(
        &self,
        request: Request<identity_target::DefineIdentityAccessRoleRequest>,
    ) -> Result<Response<identity_target::IdentityAccessRoleCommandResponse>, Status> {
        self.current.define_role(request).await
    }

    async fn set_role_permissions(
        &self,
        request: Request<identity_target::SetIdentityAccessRolePermissionsRequest>,
    ) -> Result<Response<identity_target::IdentityAccessRoleCommandResponse>, Status> {
        self.current.set_role_permissions(request).await
    }
}

#[derive(Clone)]
struct IdentityAccessQueryComposite<A, G> {
    current: IdentityAccessQueryRpc<A, G>,
    directory: DirectoryRpcImpl,
    roles: RoleRpcImpl,
}

impl<A, G> IdentityAccessQueryComposite<A, G> {
    fn new(
        current: IdentityAccessQueryRpc<A, G>,
        directory: DirectoryRpcImpl,
        roles: RoleRpcImpl,
    ) -> Self {
        Self {
            current,
            directory,
            roles,
        }
    }
}

#[tonic::async_trait]
impl<A, G> TargetIdentityAccessQueryService for IdentityAccessQueryComposite<A, G>
where
    A: IdentityAccessQueryApplication,
    G: IdentityAccessRequestAuthenticator,
{
    async fn list_directory_connectors(
        &self,
        request: Request<identity_target::ListDirectoryConnectorsRequest>,
    ) -> Result<Response<identity_target::ListDirectoryConnectorsResponse>, Status> {
        self.directory.list_directory_connectors(request).await
    }

    async fn get_directory_import_status(
        &self,
        request: Request<identity_target::GetDirectoryImportStatusRequest>,
    ) -> Result<Response<identity_target::GetDirectoryImportStatusResponse>, Status> {
        self.directory.get_directory_import_status(request).await
    }

    async fn list_directory_role_mappings(
        &self,
        request: Request<identity_target::ListDirectoryRoleMappingsRequest>,
    ) -> Result<Response<identity_target::ListDirectoryRoleMappingsResponse>, Status> {
        self.directory.list_directory_role_mappings(request).await
    }

    async fn list_directory_identity_links(
        &self,
        request: Request<identity_target::ListDirectoryIdentityLinksRequest>,
    ) -> Result<Response<identity_target::ListDirectoryIdentityLinksResponse>, Status> {
        self.directory.list_directory_identity_links(request).await
    }

    async fn get_directory_connector_readiness(
        &self,
        request: Request<identity_target::GetDirectoryConnectorReadinessRequest>,
    ) -> Result<Response<identity_target::GetDirectoryConnectorReadinessResponse>, Status> {
        self.directory
            .get_directory_connector_readiness(request)
            .await
    }

    async fn list_directory_conflicts(
        &self,
        request: Request<identity_target::ListDirectoryConflictsRequest>,
    ) -> Result<Response<identity_target::ListDirectoryConflictsResponse>, Status> {
        self.directory.list_directory_conflicts(request).await
    }

    async fn list_expiring_roles(
        &self,
        request: Request<identity_target::ListExpiringRolesRequest>,
    ) -> Result<Response<identity_target::ListExpiringRolesResponse>, Status> {
        self.roles.list_expiring_roles(request).await
    }

    async fn list_role_review(
        &self,
        request: Request<identity_target::ListRoleReviewRequest>,
    ) -> Result<Response<identity_target::ListRoleReviewResponse>, Status> {
        self.roles.list_role_review(request).await
    }

    async fn get_principal(
        &self,
        request: Request<identity_target::GetIdentityAccessPrincipalRequest>,
    ) -> Result<Response<identity_target::GetIdentityAccessPrincipalResponse>, Status> {
        self.current.get_principal(request).await
    }

    async fn list_principals(
        &self,
        request: Request<identity_target::ListIdentityAccessPrincipalsRequest>,
    ) -> Result<Response<identity_target::ListIdentityAccessPrincipalsResponse>, Status> {
        self.current.list_principals(request).await
    }

    async fn get_role(
        &self,
        request: Request<identity_target::GetIdentityAccessRoleRequest>,
    ) -> Result<Response<identity_target::GetIdentityAccessRoleResponse>, Status> {
        self.current.get_role(request).await
    }

    async fn list_roles(
        &self,
        request: Request<identity_target::ListIdentityAccessRolesRequest>,
    ) -> Result<Response<identity_target::ListIdentityAccessRolesResponse>, Status> {
        self.current.list_roles(request).await
    }
}

async fn with_crm_read_store<T, F, Fut>(
    router: &HoldingDatabaseRouter,
    holding_id: &str,
    data_profile_key: &str,
    correlation_id: Uuid,
    operation: F,
) -> Result<T, Status>
where
    F: FnOnce(SqlxMarketingStore) -> Fut,
    Fut: Future<Output = Result<T, Status>>,
{
    let read = router
        .route(holding_id, data_profile_key, RoutedDatabaseSide::Read)
        .await
        .map_err(|_| Status::unavailable("holding read database is unavailable"))?;
    let result = async {
        let store = SqlxMarketingStore::from_pools(
            read.database_pool(),
            read.database_pool(),
            data_profile_key,
        )
        .map_err(|error| crm_persistence_status(error, correlation_id))?;
        operation(store).await
    }
    .await;
    read.close().await;
    result
}

async fn with_crm_write_store<T, F, Fut>(
    router: &HoldingDatabaseRouter,
    holding_id: &str,
    data_profile_key: &str,
    correlation_id: Uuid,
    operation: F,
) -> Result<T, Status>
where
    F: FnOnce(SqlxMarketingCrmStore) -> Fut,
    Fut: Future<Output = Result<T, Status>>,
{
    let write = router
        .route(holding_id, data_profile_key, RoutedDatabaseSide::Write)
        .await
        .map_err(|_| Status::unavailable("holding write database is unavailable"))?;
    let result = async {
        let store = SqlxMarketingCrmStore::from_pool(write.database_pool(), data_profile_key)
            .map_err(|error| crm_persistence_status(error, correlation_id))?;
        operation(store).await
    }
    .await;
    write.close().await;
    result
}

#[derive(Clone)]
struct CrmRpcImpl {
    state: AppState,
}

impl CrmRpcImpl {
    async fn list_crm_read_rows(
        &self,
        request: Request<crm_target::ListCrmReadRowsRequest>,
    ) -> Result<Response<crm_target::ListCrmReadRowsResponse>, Status> {
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
            let read_scope = finance_banking_read_holding_scope(&scope)?;
            let input = request.get_ref();
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &read_scope,
                &input.holding_id,
                &input.data_profile_key,
                false,
            )
            .await?;
            self.state
                .auth
                .authorize_role_for_holding(
                    &self.state.store,
                    &claims,
                    &scope,
                    "marketing-crm.read",
                    Some(&workspace.holding_id),
                )
                .await?;
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
            if !self
                .state
                .store
                .customer_scope_contains(workspace.customer_id, &scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.holding_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("holding database routing is not configured")
            })?;
            let correlation_id = context.correlation_id;
            let page = with_crm_read_store(
                router,
                &workspace.holding_id,
                &workspace.data_profile_key,
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
                    Ok::<crm_target::CrmReadRow, Status>(crm_target::CrmReadRow {
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
            Ok(Response::new(crm_target::ListCrmReadRowsResponse {
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
}

impl CrmRpcImpl {
    async fn save_crm_handoff(
        &self,
        request: Request<crm_target::SaveCrmHandoffRequest>,
    ) -> Result<Response<crm_target::SaveCrmHandoffResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let mut context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "CRM mutations require an explicit holding, company, or branch scope",
                ));
            }
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &context.active_scope,
                &request.get_ref().holding_id,
                &request.get_ref().data_profile_key,
                true,
            )
            .await?;
            context.subscription_id = workspace.subscription_id;
            let handoff: AcceptedLeadHandoff =
                decode_bounded_json(&request.get_ref().handoff_json, "handoff_json")?;
            handoff
                .validate()
                .map_err(|_| Status::failed_precondition("CRM handoff is invalid"))?;
            if !context.active_scope.contains(&handoff.scope)
                || handoff.command_id != context.command_id
            {
                return Err(Status::permission_denied("CRM handoff binding is invalid"));
            }
            self.state
                .authorize_entitled_command_for_holding(
                    &claims,
                    &context,
                    workspace.customer_id,
                    "crm",
                    "marketing-crm.write",
                    &workspace.holding_id,
                )
                .await?;
            let router = self.state.holding_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("holding database routing is not configured")
            })?;
            let correlation_id = context.correlation_id;
            let command_context = context.clone();
            let receipt = with_crm_write_store(
                router,
                &workspace.holding_id,
                &workspace.data_profile_key,
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
                    customer_account_id: Some(workspace.customer_id),
                    action: "crm.handoff.saved".into(),
                    purpose: "crm.write".into(),
                    correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("CRM audit persistence unavailable"))?;
            Ok(Response::new(crm_target::SaveCrmHandoffResponse {
                receipt: Some(crm_target::CrmWriteReceipt {
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
        request: Request<crm_target::SaveCrmOpportunityRequest>,
    ) -> Result<Response<crm_target::SaveCrmOpportunityResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let mut context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "CRM mutations require an explicit holding, company, or branch scope",
                ));
            }
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &context.active_scope,
                &request.get_ref().holding_id,
                &request.get_ref().data_profile_key,
                true,
            )
            .await?;
            context.subscription_id = workspace.subscription_id;
            let opportunity: CrmOpportunity =
                decode_bounded_json(&request.get_ref().opportunity_json, "opportunity_json")?;
            opportunity
                .validate()
                .map_err(|_| Status::failed_precondition("CRM opportunity is invalid"))?;
            if !context.active_scope.contains(&opportunity.scope)
                || opportunity.command_id != context.command_id
            {
                return Err(Status::permission_denied(
                    "CRM opportunity binding is invalid",
                ));
            }
            self.state
                .authorize_entitled_command_for_holding(
                    &claims,
                    &context,
                    workspace.customer_id,
                    "crm",
                    "marketing-crm.write",
                    &workspace.holding_id,
                )
                .await?;
            let router = self.state.holding_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("holding database routing is not configured")
            })?;
            let correlation_id = context.correlation_id;
            let command_context = context.clone();
            let expected = request
                .get_ref()
                .has_expected_version
                .then_some(request.get_ref().expected_version);
            let receipt = with_crm_write_store(
                router,
                &workspace.holding_id,
                &workspace.data_profile_key,
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
                    customer_account_id: Some(workspace.customer_id),
                    action: "crm.opportunity.saved".into(),
                    purpose: "crm.write".into(),
                    correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("CRM audit persistence unavailable"))?;
            Ok(Response::new(crm_target::SaveCrmOpportunityResponse {
                receipt: Some(crm_target::CrmWriteReceipt {
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
        request: Request<crm_target::RecordCrmAttributionRequest>,
    ) -> Result<Response<crm_target::RecordCrmAttributionResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let mut context = command_domain(&request, request.get_ref().context.as_ref())?;
            let scope = access_scope(&context.active_scope);
            if matches!(scope, Scope::Platform | Scope::Subtree { .. }) {
                return Err(Status::permission_denied(
                    "CRM mutations require an explicit holding, company, or branch scope",
                ));
            }
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &context.active_scope,
                &request.get_ref().holding_id,
                &request.get_ref().data_profile_key,
                true,
            )
            .await?;
            context.subscription_id = workspace.subscription_id;
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
            self.state
                .authorize_entitled_command_for_holding(
                    &claims,
                    &context,
                    workspace.customer_id,
                    "crm",
                    "marketing-crm.write",
                    &workspace.holding_id,
                )
                .await?;
            let router = self.state.holding_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("holding database routing is not configured")
            })?;
            let correlation_id = context.correlation_id;
            let command_context = context.clone();
            let expected = request.get_ref().expected_opportunity_version;
            let receipt = with_crm_write_store(
                router,
                &workspace.holding_id,
                &workspace.data_profile_key,
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
                    customer_account_id: Some(workspace.customer_id),
                    action: "crm.attribution.recorded".into(),
                    purpose: "crm.write".into(),
                    correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("CRM audit persistence unavailable"))?;
            Ok(Response::new(crm_target::RecordCrmAttributionResponse {
                receipt: Some(crm_target::CrmAttributionWriteReceipt {
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

#[derive(Clone)]
struct CrmCommandComposite<A, G> {
    current: CrmCommandRpc<A, G>,
    ancillary: CrmRpcImpl,
}

impl<A, G> CrmCommandComposite<A, G> {
    fn new(current: CrmCommandRpc<A, G>, ancillary: CrmRpcImpl) -> Self {
        Self { current, ancillary }
    }
}

#[tonic::async_trait]
impl<A, G> TargetCrmCommandService for CrmCommandComposite<A, G>
where
    A: CrmCommandApplication,
    G: CrmRequestAuthenticator,
{
    async fn save_crm_handoff(
        &self,
        request: Request<crm_target::SaveCrmHandoffRequest>,
    ) -> Result<Response<crm_target::SaveCrmHandoffResponse>, Status> {
        self.ancillary.save_crm_handoff(request).await
    }

    async fn save_crm_opportunity(
        &self,
        request: Request<crm_target::SaveCrmOpportunityRequest>,
    ) -> Result<Response<crm_target::SaveCrmOpportunityResponse>, Status> {
        self.ancillary.save_crm_opportunity(request).await
    }

    async fn record_crm_attribution(
        &self,
        request: Request<crm_target::RecordCrmAttributionRequest>,
    ) -> Result<Response<crm_target::RecordCrmAttributionResponse>, Status> {
        self.ancillary.record_crm_attribution(request).await
    }

    async fn accept_marketing_lead(
        &self,
        request: Request<crm_target::AcceptMarketingLeadRequest>,
    ) -> Result<Response<crm_target::AcceptMarketingLeadResponse>, Status> {
        self.current.accept_marketing_lead(request).await
    }

    async fn create_opportunity(
        &self,
        request: Request<crm_target::CreateCrmOpportunityRequest>,
    ) -> Result<Response<crm_target::CrmOpportunityMutationResponse>, Status> {
        self.current.create_opportunity(request).await
    }

    async fn transition_opportunity(
        &self,
        request: Request<crm_target::TransitionCrmOpportunityRequest>,
    ) -> Result<Response<crm_target::CrmOpportunityMutationResponse>, Status> {
        self.current.transition_opportunity(request).await
    }
}

#[derive(Clone)]
struct CrmQueryComposite<A, G> {
    current: CrmQueryRpc<A, G>,
    ancillary: CrmRpcImpl,
}

impl<A, G> CrmQueryComposite<A, G> {
    fn new(current: CrmQueryRpc<A, G>, ancillary: CrmRpcImpl) -> Self {
        Self { current, ancillary }
    }
}

#[tonic::async_trait]
impl<A, G> TargetCrmQueryService for CrmQueryComposite<A, G>
where
    A: CrmQueryApplication,
    G: CrmRequestAuthenticator,
{
    async fn list_crm_read_rows(
        &self,
        request: Request<crm_target::ListCrmReadRowsRequest>,
    ) -> Result<Response<crm_target::ListCrmReadRowsResponse>, Status> {
        self.ancillary.list_crm_read_rows(request).await
    }

    async fn get_crm_lead(
        &self,
        request: Request<crm_target::GetCrmLeadRequest>,
    ) -> Result<Response<crm_target::GetCrmLeadResponse>, Status> {
        self.current.get_crm_lead(request).await
    }

    async fn get_opportunity(
        &self,
        request: Request<crm_target::GetCrmOpportunityRequest>,
    ) -> Result<Response<crm_target::GetCrmOpportunityResponse>, Status> {
        self.current.get_opportunity(request).await
    }

    async fn list_opportunities(
        &self,
        request: Request<crm_target::ListCrmOpportunitiesRequest>,
    ) -> Result<Response<crm_target::ListCrmOpportunitiesResponse>, Status> {
        self.current.list_opportunities(request).await
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
) -> Result<AuthorityScope, Status> {
    match context.active_scope {
        AuthorityScope::Platform | AuthorityScope::Subtree { .. } => Err(
            Status::permission_denied("Marketing content writes require an explicit holding scope"),
        ),
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
    match context.active_scope {
        AuthorityScope::Holding { holding_id } => Ok(holding_id),
        AuthorityScope::Platform
        | AuthorityScope::Company { .. }
        | AuthorityScope::Branch { .. }
        | AuthorityScope::Subtree { .. } => Err(Status::permission_denied(
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
) -> marketing_target::MarketingProviderAccountWriteReceipt {
    marketing_target::MarketingProviderAccountWriteReceipt {
        provider_account_id: receipt.provider_account_id.to_string(),
        provider_account_ref: receipt.provider_account_ref,
        holding_id: receipt.holding_id,
        data_profile_key: receipt.data_profile_key,
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
) -> marketing_target::MarketingContentWriteReceipt {
    marketing_target::MarketingContentWriteReceipt {
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
    holding_id: &str,
    data_profile_key: &str,
    correlation_id: Uuid,
) -> Result<
    (
        RoutedHoldingDatabase,
        RoutedHoldingDatabase,
        SqlxMarketingContentWriteAdapter,
    ),
    Status,
> {
    let (write, read) =
        route_marketing_databases(state, holding_id, data_profile_key, correlation_id).await?;
    let store = match SqlxMarketingContentStore::new(
        write.database_pool().clone(),
        read.database_pool().clone(),
        data_profile_key.to_owned(),
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
    holding_id: &str,
    data_profile_key: &str,
    correlation_id: Uuid,
) -> Result<
    (
        RoutedHoldingDatabase,
        RoutedHoldingDatabase,
        SqlxMarketingStore,
    ),
    Status,
> {
    let (write, read) =
        route_marketing_databases(state, holding_id, data_profile_key, correlation_id).await?;
    let store = match SqlxMarketingStore::new(
        write.database_pool().clone(),
        read.database_pool().clone(),
        data_profile_key.to_owned(),
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
    holding_id: &str,
    data_profile_key: &str,
    correlation_id: Uuid,
) -> Result<(RoutedHoldingDatabase, RoutedHoldingDatabase), Status> {
    let router = state
        .holding_router
        .as_ref()
        .ok_or_else(|| Status::failed_precondition("holding database routing is not configured"))?;
    let write = router
        .route(holding_id, data_profile_key, RoutedDatabaseSide::Write)
        .await
        .map_err(|error| marketing_content_persistence_status(error, correlation_id))?;
    let read = match router
        .route(holding_id, data_profile_key, RoutedDatabaseSide::Read)
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

impl MarketingContentRpcImpl {
    async fn submit_marketing_form(
        &self,
        request: Request<marketing_target::SubmitMarketingFormRequest>,
    ) -> Result<Response<marketing_target::SubmitMarketingFormResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let mut context = command_domain(&request, Some(proto_context))?;
            if matches!(
                context.active_scope,
                AuthorityScope::Platform | AuthorityScope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "Marketing form submission requires an explicit holding scope",
                ));
            }
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &context.active_scope,
                &input.holding_id,
                &input.data_profile_key,
                true,
            )
            .await?;
            context.subscription_id = workspace.subscription_id;
            let holding_id = workspace.holding_id.as_str();
            let data_profile_key = workspace.data_profile_key.as_str();
            let form_id = parse_uuid(&input.form_id, "form_id")?;
            let submission_id = parse_uuid(&input.submission_id, "submission_id")?;
            let values = marketing_form_values(&input.values)?;
            let form = ContentFormRef::new(form_id, context.active_scope.clone())
                .map_err(|_| Status::invalid_argument("form scope is invalid"))?;
            let customer_id = workspace.customer_id;
            self.state
                .authorize_entitled_command_for_holding(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing-crm.write",
                    holding_id,
                )
                .await?;
            let correlation_id = context.correlation_id;
            let (write, read, port) = route_marketing_content_store(
                &self.state,
                holding_id,
                data_profile_key,
                correlation_id,
            )
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
            Ok(Response::new(
                marketing_target::SubmitMarketingFormResponse {
                    receipt: Some(marketing_target::MarketingContentWriteReceipt {
                        aggregate_type: receipt.aggregate_type,
                        aggregate_id: receipt.aggregate_id.to_string(),
                        version: receipt.version,
                        idempotency_key: receipt.idempotency_key,
                        event_id: receipt.event_id.to_string(),
                        replayed: receipt.replayed,
                    }),
                    correlation_id: correlation_id.to_string(),
                },
            ))
        })
    }

    async fn get_marketing_delivery_read(
        &self,
        request: Request<marketing_target::GetMarketingDeliveryReadRequest>,
    ) -> Result<Response<marketing_target::GetMarketingDeliveryReadResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = query_context(&request, Some(proto_context))?;
            let caller_scope = query_access_scope(proto_context)?;
            let read_scope = finance_banking_read_holding_scope(&caller_scope)?;
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &read_scope,
                &input.holding_id,
                &input.data_profile_key,
                false,
            )
            .await?;
            let delivery_id = parse_uuid(&input.delivery_id, "delivery_id")?;
            self.state
                .auth
                .authorize_role_for_holding(
                    &self.state.store,
                    &claims,
                    &caller_scope,
                    "marketing-crm.read",
                    Some(&workspace.holding_id),
                )
                .await?;
            if !self
                .state
                .store
                .customer_scope_contains(workspace.customer_id, &caller_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let (write, read) = route_marketing_databases(
                &self.state,
                &workspace.holding_id,
                &workspace.data_profile_key,
                context.correlation_id,
            )
            .await?;
            let result = async {
                let store = SqlxMarketingDeliveryStore::from_pools(
                    write.database_pool(),
                    read.database_pool(),
                    &workspace.data_profile_key,
                )
                .map_err(|error| {
                    marketing_content_persistence_status(error, context.correlation_id)
                })?;
                let dispatch = store.get(delivery_id).await.map_err(|error| {
                    marketing_content_persistence_status(error, context.correlation_id)
                })?;
                let Some(dispatch) = dispatch else {
                    return Ok::<marketing_target::GetMarketingDeliveryReadResponse, Status>(
                        marketing_target::GetMarketingDeliveryReadResponse {
                            delivery: None,
                            correlation_id: context.correlation_id.to_string(),
                        },
                    );
                };
                let holding_id = match dispatch.scope {
                    AuthorityScope::Holding { holding_id }
                    | AuthorityScope::Company { holding_id, .. }
                    | AuthorityScope::Branch { holding_id, .. }
                    | AuthorityScope::Subtree { holding_id, .. } => holding_id,
                    AuthorityScope::Platform => {
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
                Ok(marketing_target::GetMarketingDeliveryReadResponse {
                    delivery: Some(marketing_target::MarketingDeliveryRead {
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
                    customer_account_id: Some(workspace.customer_id),
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
        request: Request<marketing_target::ListMarketingContentReadRowsRequest>,
    ) -> Result<Response<marketing_target::ListMarketingContentReadRowsResponse>, Status> {
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
            let read_scope = finance_banking_read_holding_scope(&caller_scope)?;
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &read_scope,
                &input.holding_id,
                &input.data_profile_key,
                false,
            )
            .await?;
            self.state
                .auth
                .authorize_role_for_holding(
                    &self.state.store,
                    &claims,
                    &caller_scope,
                    "marketing-crm.read",
                    Some(&workspace.holding_id),
                )
                .await?;
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
            if !self
                .state
                .store
                .customer_scope_contains(workspace.customer_id, &caller_scope)
                .await
                .map_err(|_| Status::internal("customer scope lookup failed"))?
            {
                return Err(Status::permission_denied("customer_scope_not_granted"));
            }
            let router = self.state.holding_router.as_ref().ok_or_else(|| {
                Status::failed_precondition("holding database routing is not configured")
            })?;
            let correlation_id = context.correlation_id;
            let read = router
                .route(
                    &workspace.holding_id,
                    &workspace.data_profile_key,
                    RoutedDatabaseSide::Read,
                )
                .await
                .map_err(|_| Status::unavailable("holding read database is unavailable"))?;
            let result = async {
                let store = SqlxMarketingContentStore::new(
                    read.database_pool().clone(),
                    read.database_pool().clone(),
                    &workspace.data_profile_key,
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
                    mapped.push(marketing_target::MarketingContentReadRow {
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
                        customer_account_id: Some(workspace.customer_id),
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
                Ok::<marketing_target::ListMarketingContentReadRowsResponse, Status>(
                    marketing_target::ListMarketingContentReadRowsResponse {
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
        request: Request<marketing_target::EraseMarketingProfileRequest>,
    ) -> Result<Response<marketing_target::EraseMarketingProfileResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let mut context = command_domain(&request, Some(proto_context))?;
            if matches!(
                context.active_scope,
                AuthorityScope::Platform | AuthorityScope::Subtree { .. }
            ) {
                return Err(Status::permission_denied(
                    "Marketing profile erasure requires an explicit holding scope",
                ));
            }
            if input.reason.trim().is_empty()
                || input.reason.len() > erpx_domain::marketing::MAX_MARKETING_PROFILE_ERASURE_REASON
                || input.reason.chars().any(char::is_control)
            {
                return Err(Status::invalid_argument("erasure reason is invalid"));
            }
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &context.active_scope,
                &input.holding_id,
                &input.data_profile_key,
                true,
            )
            .await?;
            context.subscription_id = workspace.subscription_id;
            let holding_id = workspace.holding_id.as_str();
            let data_profile_key = workspace.data_profile_key.as_str();
            let profile_id = parse_uuid(&input.profile_id, "profile_id")?;
            let erased_at = if input.erased_at.trim().is_empty() {
                Utc::now()
            } else {
                marketing_expiry(&input.erased_at)?
            };
            let erasure = MarketingProfileErasure::new(
                profile_id,
                context.active_scope.clone(),
                erased_at,
                input.reason.trim(),
            )
            .map_err(|_| Status::invalid_argument("profile erasure is invalid"))?;
            let customer_id = workspace.customer_id;
            self.state
                .authorize_entitled_command_for_holding(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing.profile.erase",
                    holding_id,
                )
                .await?;
            let correlation = MarketingProfileErasureCorrelation::new(
                context.correlation_id,
                context.causation_id,
            )
            .map_err(|_| Status::invalid_argument("erasure correlation is invalid"))?;
            let command = EraseMarketingProfileCommand::new(
                erasure,
                context.active_scope.clone(),
                context.idempotency_key.clone(),
                correlation,
            )
            .map_err(|error| errors::application(error, context.correlation_id))?;
            let correlation_id = context.correlation_id;
            let (write, read, store) = route_marketing_profile_store(
                &self.state,
                holding_id,
                data_profile_key,
                correlation_id,
            )
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
            Ok(Response::new(
                marketing_target::EraseMarketingProfileResponse {
                    receipt: Some(marketing_target::MarketingProfileErasureReceipt {
                        profile_id: receipt.profile_id.to_string(),
                        idempotency_key: receipt.idempotency_key,
                        replayed: receipt.replayed,
                    }),
                    correlation_id: correlation_id.to_string(),
                },
            ))
        })
    }

    async fn issue_marketing_preview(
        &self,
        request: Request<marketing_target::IssueMarketingPreviewRequest>,
    ) -> Result<Response<marketing_target::IssueMarketingPreviewResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let mut context = command_domain(&request, Some(proto_context))?;
            let scope = marketing_scope(&context)?;
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &context.active_scope,
                &input.holding_id,
                &input.data_profile_key,
                true,
            )
            .await?;
            context.subscription_id = workspace.subscription_id;
            let holding_id = workspace.holding_id.as_str();
            let data_profile_key = workspace.data_profile_key.as_str();
            let template_id = parse_uuid(&input.template_id, "template_id")?;
            let template =
                ContentTemplateRef::new(template_id, input.template_version, scope.clone())
                    .map_err(|_| Status::invalid_argument("template reference is invalid"))?;
            let expires_at = marketing_expiry(&input.expires_at)?;
            let customer_id = workspace.customer_id;
            self.state
                .authorize_entitled_command_for_holding(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing-crm.write",
                    holding_id,
                )
                .await?;
            let issued_at = Utc::now();
            let preview = PreviewGrant::new(Uuid::new_v4(), template, issued_at, expires_at)
                .map_err(|_| Status::invalid_argument("preview expiry is invalid"))?;
            let token = marketing_opaque_secret();
            let correlation_id = context.correlation_id;
            let (write, read, port) = route_marketing_content_store(
                &self.state,
                holding_id,
                data_profile_key,
                correlation_id,
            )
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
            Ok(Response::new(
                marketing_target::IssueMarketingPreviewResponse {
                    receipt: Some(marketing_receipt(receipt)),
                    preview_id: preview.id.to_string(),
                    token,
                    expires_at: preview.expires_at.to_rfc3339(),
                    correlation_id: correlation_id.to_string(),
                },
            ))
        })
    }

    async fn revoke_marketing_preview(
        &self,
        request: Request<marketing_target::RevokeMarketingPreviewRequest>,
    ) -> Result<Response<marketing_target::RevokeMarketingPreviewResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let mut context = command_domain(&request, Some(proto_context))?;
            let scope = marketing_scope(&context)?;
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &context.active_scope,
                &input.holding_id,
                &input.data_profile_key,
                true,
            )
            .await?;
            context.subscription_id = workspace.subscription_id;
            let holding_id = workspace.holding_id.as_str();
            let data_profile_key = workspace.data_profile_key.as_str();
            let preview_id = parse_uuid(&input.preview_id, "preview_id")?;
            let customer_id = workspace.customer_id;
            self.state
                .authorize_entitled_command_for_holding(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing-crm.write",
                    holding_id,
                )
                .await?;
            let correlation_id = context.correlation_id;
            let (write, read, port) = route_marketing_content_store(
                &self.state,
                holding_id,
                data_profile_key,
                correlation_id,
            )
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
            Ok(Response::new(
                marketing_target::RevokeMarketingPreviewResponse {
                    receipt: Some(marketing_receipt(receipt)),
                    correlation_id: correlation_id.to_string(),
                },
            ))
        })
    }

    async fn issue_marketing_domain_verification(
        &self,
        request: Request<marketing_target::IssueMarketingDomainVerificationRequest>,
    ) -> Result<Response<marketing_target::IssueMarketingDomainVerificationResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let mut context = command_domain(&request, Some(proto_context))?;
            let scope = marketing_scope(&context)?;
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &context.active_scope,
                &input.holding_id,
                &input.data_profile_key,
                true,
            )
            .await?;
            context.subscription_id = workspace.subscription_id;
            let holding_id = workspace.holding_id.as_str();
            let data_profile_key = workspace.data_profile_key.as_str();
            let domain_id = parse_uuid(&input.domain_id, "domain_id")?;
            let customer_id = workspace.customer_id;
            self.state
                .authorize_entitled_command_for_holding(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing-crm.write",
                    holding_id,
                )
                .await?;
            let challenge = marketing_opaque_secret();
            let correlation_id = context.correlation_id;
            let (write, read, port) = route_marketing_content_store(
                &self.state,
                holding_id,
                data_profile_key,
                correlation_id,
            )
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
            Ok(Response::new(
                marketing_target::IssueMarketingDomainVerificationResponse {
                    receipt: Some(marketing_receipt(receipt)),
                    challenge,
                    correlation_id: correlation_id.to_string(),
                },
            ))
        })
    }

    async fn verify_marketing_domain(
        &self,
        request: Request<marketing_target::VerifyMarketingDomainRequest>,
    ) -> Result<Response<marketing_target::VerifyMarketingDomainResponse>, Status> {
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
            let mut context = command_domain(&request, Some(proto_context))?;
            let scope = marketing_scope(&context)?;
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &context.active_scope,
                &input.holding_id,
                &input.data_profile_key,
                true,
            )
            .await?;
            context.subscription_id = workspace.subscription_id;
            let holding_id = workspace.holding_id.as_str();
            let data_profile_key = workspace.data_profile_key.as_str();
            let domain_id = parse_uuid(&input.domain_id, "domain_id")?;
            let customer_id = workspace.customer_id;
            self.state
                .authorize_entitled_command_for_holding(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing-crm.write",
                    holding_id,
                )
                .await?;
            let correlation_id = context.correlation_id;
            let (write, read, port) = route_marketing_content_store(
                &self.state,
                holding_id,
                data_profile_key,
                correlation_id,
            )
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
            Ok(Response::new(
                marketing_target::VerifyMarketingDomainResponse {
                    receipt: Some(marketing_receipt(receipt)),
                    correlation_id: correlation_id.to_string(),
                },
            ))
        })
    }

    async fn bind_marketing_domain(
        &self,
        request: Request<marketing_target::BindMarketingDomainRequest>,
    ) -> Result<Response<marketing_target::BindMarketingDomainResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let mut context = command_domain(&request, Some(proto_context))?;
            let scope = marketing_scope(&context)?;
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &context.active_scope,
                &input.holding_id,
                &input.data_profile_key,
                true,
            )
            .await?;
            context.subscription_id = workspace.subscription_id;
            let holding_id = workspace.holding_id.as_str();
            let data_profile_key = workspace.data_profile_key.as_str();
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
            let customer_id = workspace.customer_id;
            self.state
                .authorize_entitled_command_for_holding(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing-crm.write",
                    holding_id,
                )
                .await?;
            let correlation_id = context.correlation_id;
            let (write, read, port) = route_marketing_content_store(
                &self.state,
                holding_id,
                data_profile_key,
                correlation_id,
            )
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
            Ok(Response::new(
                marketing_target::BindMarketingDomainResponse {
                    receipt: Some(marketing_receipt(receipt)),
                    correlation_id: correlation_id.to_string(),
                },
            ))
        })
    }

    async fn configure_marketing_provider_account(
        &self,
        request: Request<marketing_target::ConfigureMarketingProviderAccountRequest>,
    ) -> Result<Response<marketing_target::ConfigureMarketingProviderAccountResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let mut context = command_domain(&request, Some(proto_context))?;
            let holding_uuid = provider_account_scope(&context)?;
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &context.active_scope,
                &input.holding_id,
                &input.data_profile_key,
                true,
            )
            .await?;
            context.subscription_id = workspace.subscription_id;
            let holding_id = workspace.holding_id.as_str();
            let account_id = parse_uuid(&input.provider_account_id, "provider_account_id")?;
            let customer_id = workspace.customer_id;
            self.state
                .authorize_entitled_command_for_holding(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing.provider.configure",
                    holding_id,
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
                holding_uuid,
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
            Ok(Response::new(
                marketing_target::ConfigureMarketingProviderAccountResponse {
                    receipt: Some(marketing_provider_receipt(receipt)),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn acknowledge_marketing_provider_verification(
        &self,
        request: Request<marketing_target::AcknowledgeMarketingProviderVerificationRequest>,
    ) -> Result<Response<marketing_target::AcknowledgeMarketingProviderVerificationResponse>, Status>
    {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let mut context = command_domain(&request, Some(proto_context))?;
            provider_account_scope(&context)?;
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &context.active_scope,
                &input.holding_id,
                &input.data_profile_key,
                true,
            )
            .await?;
            context.subscription_id = workspace.subscription_id;
            let holding_id = workspace.holding_id.as_str();
            let data_profile_key = workspace.data_profile_key.as_str();
            let account_id = parse_uuid(&input.provider_account_id, "provider_account_id")?;
            let verification_reference = validate_transport_reference(
                &input.verification_reference,
                "verification_reference",
            )?;
            let customer_id = workspace.customer_id;
            self.state
                .authorize_entitled_command_for_holding(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing.provider.verify",
                    holding_id,
                )
                .await?;
            let command = AcknowledgeMarketingProviderVerificationCommand::new(
                account_id,
                holding_id,
                data_profile_key,
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
                marketing_target::AcknowledgeMarketingProviderVerificationResponse {
                    receipt: Some(marketing_provider_receipt(receipt)),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn set_marketing_provider_account_state(
        &self,
        request: Request<marketing_target::SetMarketingProviderAccountStateRequest>,
    ) -> Result<Response<marketing_target::SetMarketingProviderAccountStateResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let proto_context = input
                .context
                .as_ref()
                .ok_or_else(|| Status::failed_precondition("command context is required"))?;
            let mut context = command_domain(&request, Some(proto_context))?;
            provider_account_scope(&context)?;
            let workspace = resolve_legacy_holding_workspace(
                &self.state,
                &context.active_scope,
                &input.holding_id,
                &input.data_profile_key,
                true,
            )
            .await?;
            context.subscription_id = workspace.subscription_id;
            let holding_id = workspace.holding_id.as_str();
            let data_profile_key = workspace.data_profile_key.as_str();
            let account_id = parse_uuid(&input.provider_account_id, "provider_account_id")?;
            let customer_id = workspace.customer_id;
            self.state
                .authorize_entitled_command_for_holding(
                    &claims,
                    &context,
                    customer_id,
                    "marketing",
                    "marketing.provider.configure",
                    holding_id,
                )
                .await?;
            let command = SetMarketingProviderAccountStateCommand::new(
                account_id,
                holding_id,
                data_profile_key,
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
            Ok(Response::new(
                marketing_target::SetMarketingProviderAccountStateResponse {
                    receipt: Some(marketing_provider_receipt(receipt)),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }
}

#[derive(Clone)]
struct MarketingCommandComposite<A, G> {
    current: MarketingCommandRpc<A, G>,
    ancillary: MarketingContentRpcImpl,
}

impl<A, G> MarketingCommandComposite<A, G> {
    fn new(current: MarketingCommandRpc<A, G>, ancillary: MarketingContentRpcImpl) -> Self {
        Self { current, ancillary }
    }
}

#[tonic::async_trait]
impl<A, G> TargetMarketingCommandService for MarketingCommandComposite<A, G>
where
    A: MarketingCommandApplication,
    G: MarketingRequestAuthenticator,
{
    async fn submit_marketing_form(
        &self,
        request: Request<marketing_target::SubmitMarketingFormRequest>,
    ) -> Result<Response<marketing_target::SubmitMarketingFormResponse>, Status> {
        self.ancillary.submit_marketing_form(request).await
    }

    async fn erase_marketing_profile(
        &self,
        request: Request<marketing_target::EraseMarketingProfileRequest>,
    ) -> Result<Response<marketing_target::EraseMarketingProfileResponse>, Status> {
        self.ancillary.erase_marketing_profile(request).await
    }

    async fn issue_marketing_preview(
        &self,
        request: Request<marketing_target::IssueMarketingPreviewRequest>,
    ) -> Result<Response<marketing_target::IssueMarketingPreviewResponse>, Status> {
        self.ancillary.issue_marketing_preview(request).await
    }

    async fn revoke_marketing_preview(
        &self,
        request: Request<marketing_target::RevokeMarketingPreviewRequest>,
    ) -> Result<Response<marketing_target::RevokeMarketingPreviewResponse>, Status> {
        self.ancillary.revoke_marketing_preview(request).await
    }

    async fn issue_marketing_domain_verification(
        &self,
        request: Request<marketing_target::IssueMarketingDomainVerificationRequest>,
    ) -> Result<Response<marketing_target::IssueMarketingDomainVerificationResponse>, Status> {
        self.ancillary
            .issue_marketing_domain_verification(request)
            .await
    }

    async fn verify_marketing_domain(
        &self,
        request: Request<marketing_target::VerifyMarketingDomainRequest>,
    ) -> Result<Response<marketing_target::VerifyMarketingDomainResponse>, Status> {
        self.ancillary.verify_marketing_domain(request).await
    }

    async fn bind_marketing_domain(
        &self,
        request: Request<marketing_target::BindMarketingDomainRequest>,
    ) -> Result<Response<marketing_target::BindMarketingDomainResponse>, Status> {
        self.ancillary.bind_marketing_domain(request).await
    }

    async fn configure_marketing_provider_account(
        &self,
        request: Request<marketing_target::ConfigureMarketingProviderAccountRequest>,
    ) -> Result<Response<marketing_target::ConfigureMarketingProviderAccountResponse>, Status> {
        self.ancillary
            .configure_marketing_provider_account(request)
            .await
    }

    async fn acknowledge_marketing_provider_verification(
        &self,
        request: Request<marketing_target::AcknowledgeMarketingProviderVerificationRequest>,
    ) -> Result<Response<marketing_target::AcknowledgeMarketingProviderVerificationResponse>, Status>
    {
        self.ancillary
            .acknowledge_marketing_provider_verification(request)
            .await
    }

    async fn set_marketing_provider_account_state(
        &self,
        request: Request<marketing_target::SetMarketingProviderAccountStateRequest>,
    ) -> Result<Response<marketing_target::SetMarketingProviderAccountStateResponse>, Status> {
        self.ancillary
            .set_marketing_provider_account_state(request)
            .await
    }

    async fn create_lead_candidate(
        &self,
        request: Request<marketing_target::CreateLeadCandidateRequest>,
    ) -> Result<Response<marketing_target::MarketingLeadCandidateMutationResponse>, Status> {
        self.current.create_lead_candidate(request).await
    }

    async fn capture_engagement(
        &self,
        request: Request<marketing_target::CaptureMarketingEngagementRequest>,
    ) -> Result<Response<marketing_target::MarketingLeadCandidateMutationResponse>, Status> {
        self.current.capture_engagement(request).await
    }

    async fn qualify_lead_candidate(
        &self,
        request: Request<marketing_target::QualifyLeadCandidateRequest>,
    ) -> Result<Response<marketing_target::MarketingLeadCandidateMutationResponse>, Status> {
        self.current.qualify_lead_candidate(request).await
    }

    async fn reject_lead_candidate(
        &self,
        request: Request<marketing_target::RejectLeadCandidateRequest>,
    ) -> Result<Response<marketing_target::MarketingLeadCandidateMutationResponse>, Status> {
        self.current.reject_lead_candidate(request).await
    }
}

#[derive(Clone)]
struct MarketingQueryComposite<A, G> {
    current: MarketingQueryRpc<A, G>,
    ancillary: MarketingContentRpcImpl,
}

impl<A, G> MarketingQueryComposite<A, G> {
    fn new(current: MarketingQueryRpc<A, G>, ancillary: MarketingContentRpcImpl) -> Self {
        Self { current, ancillary }
    }
}

#[tonic::async_trait]
impl<A, G> TargetMarketingQueryService for MarketingQueryComposite<A, G>
where
    A: MarketingQueryApplication,
    G: MarketingRequestAuthenticator,
{
    async fn get_marketing_delivery_read(
        &self,
        request: Request<marketing_target::GetMarketingDeliveryReadRequest>,
    ) -> Result<Response<marketing_target::GetMarketingDeliveryReadResponse>, Status> {
        self.ancillary.get_marketing_delivery_read(request).await
    }

    async fn list_marketing_content_read_rows(
        &self,
        request: Request<marketing_target::ListMarketingContentReadRowsRequest>,
    ) -> Result<Response<marketing_target::ListMarketingContentReadRowsResponse>, Status> {
        self.ancillary
            .list_marketing_content_read_rows(request)
            .await
    }

    async fn get_lead_candidate(
        &self,
        request: Request<marketing_target::GetLeadCandidateRequest>,
    ) -> Result<Response<marketing_target::GetLeadCandidateResponse>, Status> {
        self.current.get_lead_candidate(request).await
    }

    async fn list_lead_candidates(
        &self,
        request: Request<marketing_target::ListLeadCandidatesRequest>,
    ) -> Result<Response<marketing_target::ListLeadCandidatesResponse>, Status> {
        self.current.list_lead_candidates(request).await
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
        let scope = access_scope(&context.active_scope);
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
            .authorize_role(&self.store, claims, &scope, permission)
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

#[derive(Clone)]
struct CommercialCommandComposite<A, G> {
    current: CommercialCommandRpc<A, G>,
    entitlement: EntitlementRpcImpl,
    billing: SaasBillingRpcImpl,
}

impl<A, G> CommercialCommandComposite<A, G> {
    fn new(
        current: CommercialCommandRpc<A, G>,
        entitlement: EntitlementRpcImpl,
        billing: SaasBillingRpcImpl,
    ) -> Self {
        Self {
            current,
            entitlement,
            billing,
        }
    }
}

#[tonic::async_trait]
impl<A, G> TargetCommercialCommandService for CommercialCommandComposite<A, G>
where
    A: CommercialCommandApplication,
    G: CommercialRequestAuthenticator,
{
    async fn allocate_company_entitlement(
        &self,
        request: Request<commercial_target::AllocateCompanyEntitlementRequest>,
    ) -> Result<Response<commercial_target::AllocateCompanyEntitlementResponse>, Status> {
        self.entitlement.allocate_company_entitlement(request).await
    }

    async fn activate_company_entitlement(
        &self,
        request: Request<commercial_target::ActivateCompanyEntitlementRequest>,
    ) -> Result<Response<commercial_target::ActivateCompanyEntitlementResponse>, Status> {
        self.entitlement.activate_company_entitlement(request).await
    }

    async fn activate_branch_entitlement(
        &self,
        request: Request<commercial_target::ActivateBranchEntitlementRequest>,
    ) -> Result<Response<commercial_target::ActivateBranchEntitlementResponse>, Status> {
        self.entitlement.activate_branch_entitlement(request).await
    }

    async fn create_checkout_session(
        &self,
        request: Request<commercial_target::CreateCheckoutSessionRequest>,
    ) -> Result<Response<commercial_target::CreateCheckoutSessionResponse>, Status> {
        self.billing.create_checkout_session(request).await
    }

    async fn create_customer_portal_session(
        &self,
        request: Request<commercial_target::CreateCustomerPortalSessionRequest>,
    ) -> Result<Response<commercial_target::CreateCustomerPortalSessionResponse>, Status> {
        self.billing.create_customer_portal_session(request).await
    }

    async fn grant_grace_extension(
        &self,
        request: Request<commercial_target::GrantBillingGraceExtensionRequest>,
    ) -> Result<Response<commercial_target::GrantBillingGraceExtensionResponse>, Status> {
        self.billing.grant_grace_extension(request).await
    }

    async fn create_catalog_plan(
        &self,
        request: Request<commercial_target::CreateCommercialCatalogPlanRequest>,
    ) -> Result<Response<commercial_target::CommercialCatalogPlanCommandResponse>, Status> {
        self.current.create_catalog_plan(request).await
    }

    async fn publish_catalog_plan(
        &self,
        request: Request<commercial_target::PublishCommercialCatalogPlanRequest>,
    ) -> Result<Response<commercial_target::CommercialCatalogPlanCommandResponse>, Status> {
        self.current.publish_catalog_plan(request).await
    }

    async fn create_subscription(
        &self,
        request: Request<commercial_target::CreateCommercialSubscriptionRequest>,
    ) -> Result<Response<commercial_target::CommercialSubscriptionCommandResponse>, Status> {
        self.current.create_subscription(request).await
    }

    async fn apply_billing_event(
        &self,
        request: Request<commercial_target::ApplyCommercialBillingEventRequest>,
    ) -> Result<Response<commercial_target::CommercialSubscriptionCommandResponse>, Status> {
        self.current.apply_billing_event(request).await
    }

    async fn create_entitlement(
        &self,
        request: Request<commercial_target::CreateCommercialEntitlementRequest>,
    ) -> Result<Response<commercial_target::CommercialEntitlementCommandResponse>, Status> {
        self.current.create_entitlement(request).await
    }

    async fn apply_entitlement_change(
        &self,
        request: Request<commercial_target::ApplyCommercialEntitlementChangeRequest>,
    ) -> Result<Response<commercial_target::CommercialEntitlementCommandResponse>, Status> {
        self.current.apply_entitlement_change(request).await
    }
}

#[derive(Clone)]
struct CommercialQueryComposite<A, G> {
    current: CommercialQueryRpc<A, G>,
    entitlement: EntitlementRpcImpl,
    billing: SaasBillingRpcImpl,
}

impl<A, G> CommercialQueryComposite<A, G> {
    fn new(
        current: CommercialQueryRpc<A, G>,
        entitlement: EntitlementRpcImpl,
        billing: SaasBillingRpcImpl,
    ) -> Self {
        Self {
            current,
            entitlement,
            billing,
        }
    }
}

#[tonic::async_trait]
impl<A, G> TargetCommercialQueryService for CommercialQueryComposite<A, G>
where
    A: CommercialQueryApplication,
    G: CommercialRequestAuthenticator,
{
    async fn check_entitlement(
        &self,
        request: Request<commercial_target::CheckEntitlementRequest>,
    ) -> Result<Response<commercial_target::CheckEntitlementResponse>, Status> {
        self.entitlement.check_entitlement(request).await
    }

    async fn list_saas_catalog(
        &self,
        request: Request<commercial_target::ListSaasCatalogRequest>,
    ) -> Result<Response<commercial_target::ListSaasCatalogResponse>, Status> {
        self.billing.list_catalog(request).await
    }

    async fn get_holding_subscription(
        &self,
        request: Request<commercial_target::GetHoldingSubscriptionRequest>,
    ) -> Result<Response<commercial_target::GetHoldingSubscriptionResponse>, Status> {
        self.billing.get_subscription(request).await
    }

    async fn list_holding_invoices(
        &self,
        request: Request<commercial_target::ListHoldingInvoicesRequest>,
    ) -> Result<Response<commercial_target::ListHoldingInvoicesResponse>, Status> {
        self.billing.list_invoices(request).await
    }

    async fn get_catalog_plan(
        &self,
        request: Request<commercial_target::GetCommercialCatalogPlanRequest>,
    ) -> Result<Response<commercial_target::GetCommercialCatalogPlanResponse>, Status> {
        self.current.get_catalog_plan(request).await
    }

    async fn list_catalog_plans(
        &self,
        request: Request<commercial_target::ListCommercialCatalogPlansRequest>,
    ) -> Result<Response<commercial_target::ListCommercialCatalogPlansResponse>, Status> {
        self.current.list_catalog_plans(request).await
    }

    async fn get_subscription(
        &self,
        request: Request<commercial_target::GetCommercialSubscriptionRequest>,
    ) -> Result<Response<commercial_target::GetCommercialSubscriptionResponse>, Status> {
        self.current.get_subscription(request).await
    }

    async fn list_subscriptions(
        &self,
        request: Request<commercial_target::ListCommercialSubscriptionsRequest>,
    ) -> Result<Response<commercial_target::ListCommercialSubscriptionsResponse>, Status> {
        self.current.list_subscriptions(request).await
    }

    async fn get_entitlement(
        &self,
        request: Request<commercial_target::GetCommercialEntitlementRequest>,
    ) -> Result<Response<commercial_target::GetCommercialEntitlementResponse>, Status> {
        self.current.get_entitlement(request).await
    }

    async fn list_entitlements(
        &self,
        request: Request<commercial_target::ListCommercialEntitlementsRequest>,
    ) -> Result<Response<commercial_target::ListCommercialEntitlementsResponse>, Status> {
        self.current.list_entitlements(request).await
    }
}

impl EntitlementRpcImpl {
    async fn check_entitlement(
        &self,
        request: Request<commercial_target::CheckEntitlementRequest>,
    ) -> Result<Response<commercial_target::CheckEntitlementResponse>, Status> {
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
                return Ok(Response::new(commercial_target::CheckEntitlementResponse {
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
                    return Ok(Response::new(commercial_target::CheckEntitlementResponse {
                        allowed: false,
                        reason: "support_scope_not_granted".into(),
                    }));
                }
                let Some(device_ref) = (!input.support_device_ref.trim().is_empty())
                    .then_some(input.support_device_ref.clone())
                else {
                    return Ok(Response::new(commercial_target::CheckEntitlementResponse {
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
                    return Ok(Response::new(commercial_target::CheckEntitlementResponse {
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
                return Ok(Response::new(commercial_target::CheckEntitlementResponse {
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
                return Ok(Response::new(commercial_target::CheckEntitlementResponse {
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
                return Ok(Response::new(commercial_target::CheckEntitlementResponse {
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
                return Ok(Response::new(commercial_target::CheckEntitlementResponse {
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
            Ok(Response::new(commercial_target::CheckEntitlementResponse {
                allowed: decision.is_ok(),
                reason: decision
                    .err()
                    .map_or_else(|| "allowed".into(), |error| error.to_string()),
            }))
        })
    }

    async fn allocate_company_entitlement(
        &self,
        request: Request<commercial_target::AllocateCompanyEntitlementRequest>,
    ) -> Result<Response<commercial_target::AllocateCompanyEntitlementResponse>, Status> {
        grpc_handler!(request, {
            let claims = self.auth.authorize(&request).await?;
            let context = command_domain(&request, request.get_ref().context.as_ref())?;
            let input = request.into_inner();
            self.auth.authorize_mfa(&claims, &input.mfa_code).await?;
            let customer_id = parse_uuid(&input.customer_id, "customer_id")?;
            let company_id = parse_uuid(&input.company_id, "company_id")?;
            let module_key =
                validate_transport_reference(&input.module_key, "module_key")?.to_owned();
            let scope = access_scope(&context.active_scope);
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
            Ok(Response::new(
                commercial_target::AllocateCompanyEntitlementResponse {
                    receipt: Some(entitlement_command_receipt(&receipt)),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn activate_company_entitlement(
        &self,
        request: Request<commercial_target::ActivateCompanyEntitlementRequest>,
    ) -> Result<Response<commercial_target::ActivateCompanyEntitlementResponse>, Status> {
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
            let scope = access_scope(&context.active_scope);
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
            Ok(Response::new(
                commercial_target::ActivateCompanyEntitlementResponse {
                    receipt: Some(entitlement_command_receipt(&receipt)),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }

    async fn activate_branch_entitlement(
        &self,
        request: Request<commercial_target::ActivateBranchEntitlementRequest>,
    ) -> Result<Response<commercial_target::ActivateBranchEntitlementResponse>, Status> {
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
            let scope = access_scope(&context.active_scope);
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
            Ok(Response::new(
                commercial_target::ActivateBranchEntitlementResponse {
                    receipt: Some(entitlement_command_receipt(&receipt)),
                    correlation_id: context.correlation_id.to_string(),
                },
            ))
        })
    }
}

fn to_role_message(assignment: RoleAssignment) -> identity_target::RoleAssignment {
    let effective = assignment.is_active_at(Utc::now());
    identity_target::RoleAssignment {
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

fn to_role_review_message(item: RoleReviewItem) -> identity_target::RoleReviewItem {
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
    identity_target::RoleReviewItem {
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

#[derive(Debug, Deserialize)]
struct ActivateBody {
    session_id: String,
}

#[derive(Debug, Serialize)]
struct AccountsBody {
    accounts: Vec<erpx_identity::AccountSessionSummary>,
}

const BROWSER_SESSION_ID_COOKIE: &str = "erpx_session_id";
const BROWSER_REFRESH_COOKIE: &str = "erpx_refresh_token";
const BROWSER_SESSION_BUNDLE_COOKIE: &str = "erpx_session_bundle";
const MAX_BROWSER_ACCOUNTS: usize = 5;
const MAX_BROWSER_BUNDLE_ENCODED_BYTES: usize = 3_800;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BrowserSessionEntry {
    principal_ref: String,
    session_id: String,
    refresh_token: String,
    refresh_expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BrowserSessionBundle {
    version: u8,
    active_session_id: String,
    mru_session_ids: Vec<String>,
    accounts: Vec<BrowserSessionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionBody {
    principal_ref: String,
    session_id: String,
    access_token: String,
    refresh_token: String,
    access_expires_at: String,
    refresh_expires_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_role_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_role_selection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    authorization_context_version: Option<u64>,
}

fn issued_session_body(session: &erpx_identity::IssuedSession) -> SessionBody {
    SessionBody {
        principal_ref: session.access_token().claims().principal_ref.clone(),
        session_id: session.record().id().to_string(),
        access_token: session.access_token().token().to_owned(),
        refresh_token: session.refresh_token().as_str().to_owned(),
        access_expires_at: session.access_token().claims().expires_at.to_rfc3339(),
        refresh_expires_at: session.record().refresh_expires_at().to_rfc3339(),
        active_role_source: session
            .access_token()
            .claims()
            .active_role
            .as_ref()
            .map(|role| role.source.clone()),
        active_role_selection_id: session
            .access_token()
            .claims()
            .active_role
            .as_ref()
            .map(|role| role.selection_id.clone()),
        authorization_context_version: session
            .access_token()
            .claims()
            .authorization_context_version,
    }
}

fn rotated_session_body(session: &erpx_identity::RotatedSession) -> SessionBody {
    SessionBody {
        principal_ref: session.access_token().claims().principal_ref.clone(),
        session_id: session.record().id().to_string(),
        access_token: session.access_token().token().to_owned(),
        refresh_token: session.refresh_token().as_str().to_owned(),
        access_expires_at: session.access_token().claims().expires_at.to_rfc3339(),
        refresh_expires_at: session.record().refresh_expires_at().to_rfc3339(),
        active_role_source: session
            .access_token()
            .claims()
            .active_role
            .as_ref()
            .map(|role| role.source.clone()),
        active_role_selection_id: session
            .access_token()
            .claims()
            .active_role
            .as_ref()
            .map(|role| role.selection_id.clone()),
        authorization_context_version: session
            .access_token()
            .claims()
            .authorization_context_version,
    }
}

fn request_cookie<'a>(headers: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    headers.get("cookie")?.split(';').find_map(|part| {
        let (candidate, value) = part.trim().split_once('=')?;
        (candidate == name).then_some(value)
    })
}

fn valid_browser_refresh_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn validate_browser_bundle(bundle: &BrowserSessionBundle) -> bool {
    if bundle.version != 2
        || bundle.accounts.is_empty()
        || bundle.accounts.len() > MAX_BROWSER_ACCOUNTS
        || bundle.mru_session_ids.len() != bundle.accounts.len()
        || Uuid::parse_str(&bundle.active_session_id).is_err()
    {
        return false;
    }
    let mut principals = BTreeSet::new();
    let mut sessions = BTreeSet::new();
    for account in &bundle.accounts {
        if account.principal_ref.trim().is_empty()
            || account.principal_ref.len() > 256
            || account.principal_ref.chars().any(char::is_control)
            || Uuid::parse_str(&account.session_id).is_err()
            || !valid_browser_refresh_token(&account.refresh_token)
            || DateTime::parse_from_rfc3339(&account.refresh_expires_at).is_err()
            || !principals.insert(account.principal_ref.as_str())
            || !sessions.insert(account.session_id.as_str())
        {
            return false;
        }
    }
    if !sessions.contains(bundle.active_session_id.as_str()) {
        return false;
    }
    let mru = bundle
        .mru_session_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    mru.len() == sessions.len() && mru == sessions
}

fn decode_browser_bundle(headers: &HashMap<String, String>) -> Option<BrowserSessionBundle> {
    if let Some(encoded) = request_cookie(headers, BROWSER_SESSION_BUNDLE_COOKIE) {
        if encoded.is_empty() || encoded.len() > MAX_BROWSER_BUNDLE_ENCODED_BYTES {
            return None;
        }
        let bytes = URL_SAFE_NO_PAD.decode(encoded).ok()?;
        let bundle = serde_json::from_slice::<BrowserSessionBundle>(&bytes).ok()?;
        return validate_browser_bundle(&bundle).then_some(bundle);
    }

    // One-time migration from the original two-cookie session format.
    let session_id = request_cookie(headers, BROWSER_SESSION_ID_COOKIE)?;
    let refresh_token = request_cookie(headers, BROWSER_REFRESH_COOKIE)?;
    Uuid::parse_str(session_id).ok()?;
    if !valid_browser_refresh_token(refresh_token) {
        return None;
    }
    let bundle = BrowserSessionBundle {
        version: 2,
        active_session_id: session_id.to_owned(),
        mru_session_ids: vec![session_id.to_owned()],
        accounts: vec![BrowserSessionEntry {
            principal_ref: "legacy-session".into(),
            session_id: session_id.to_owned(),
            refresh_token: refresh_token.to_owned(),
            refresh_expires_at: (Utc::now() + Duration::days(30)).to_rfc3339(),
        }],
    };
    validate_browser_bundle(&bundle).then_some(bundle)
}

fn encode_browser_bundle(bundle: &BrowserSessionBundle) -> Option<String> {
    validate_browser_bundle(bundle).then_some(())?;
    let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(bundle).ok()?);
    (encoded.len() <= MAX_BROWSER_BUNDLE_ENCODED_BYTES).then_some(encoded)
}

fn browser_refresh_request(headers: &HashMap<String, String>) -> Option<RefreshBody> {
    let bundle = decode_browser_bundle(headers)?;
    let account = bundle
        .accounts
        .iter()
        .find(|account| account.session_id == bundle.active_session_id)?;
    Some(RefreshBody {
        session_id: account.session_id.clone(),
        refresh_token: account.refresh_token.clone(),
    })
}

fn cookie_secure_suffix(headers: &HashMap<String, String>) -> &'static str {
    if headers
        .get("origin")
        .is_some_and(|origin| origin.starts_with("https://"))
        || headers
            .get("x-forwarded-proto")
            .is_some_and(|proto| proto.eq_ignore_ascii_case("https"))
    {
        "; Secure"
    } else {
        ""
    }
}

#[cfg(test)]
fn browser_session_cookie_headers(
    session: &SessionBody,
    request_headers: &HashMap<String, String>,
) -> String {
    browser_session_cookie_update(session, request_headers).0
}

fn browser_session_cookie_update(
    session: &SessionBody,
    request_headers: &HashMap<String, String>,
) -> (String, Vec<Uuid>) {
    let mut bundle = decode_browser_bundle(request_headers).unwrap_or(BrowserSessionBundle {
        version: 2,
        active_session_id: session.session_id.clone(),
        mru_session_ids: Vec::new(),
        accounts: Vec::new(),
    });
    let mut replaced = Vec::new();
    bundle.accounts.retain(|account| {
        let duplicate = account.principal_ref == session.principal_ref
            || account.session_id == session.session_id;
        if duplicate
            && account.session_id != session.session_id
            && let Ok(session_id) = Uuid::parse_str(&account.session_id)
        {
            replaced.push(session_id);
        }
        !duplicate
    });
    let retained_session_ids = bundle
        .accounts
        .iter()
        .map(|account| account.session_id.as_str())
        .collect::<BTreeSet<_>>();
    bundle
        .mru_session_ids
        .retain(|session_id| retained_session_ids.contains(session_id.as_str()));
    bundle.accounts.push(BrowserSessionEntry {
        principal_ref: session.principal_ref.clone(),
        session_id: session.session_id.clone(),
        refresh_token: session.refresh_token.clone(),
        refresh_expires_at: session.refresh_expires_at.clone(),
    });
    bundle
        .mru_session_ids
        .retain(|session_id| session_id != &session.session_id);
    bundle.mru_session_ids.insert(0, session.session_id.clone());
    bundle.active_session_id = session.session_id.clone();
    while bundle.accounts.len() > MAX_BROWSER_ACCOUNTS {
        let Some(evicted_id) = bundle.mru_session_ids.pop() else {
            break;
        };
        bundle
            .accounts
            .retain(|account| account.session_id != evicted_id);
        if let Ok(session_id) = Uuid::parse_str(&evicted_id) {
            replaced.push(session_id);
        }
    }
    let encoded = loop {
        if let Some(encoded) = encode_browser_bundle(&bundle) {
            break Some(encoded);
        }
        if bundle.accounts.len() <= 1 {
            break None;
        }
        let Some(evicted_id) = bundle.mru_session_ids.pop() else {
            break None;
        };
        bundle
            .accounts
            .retain(|account| account.session_id != evicted_id);
        if let Ok(session_id) = Uuid::parse_str(&evicted_id) {
            replaced.push(session_id);
        }
    };
    let Some(encoded) = encoded else {
        return (
            clear_browser_session_cookie_headers(request_headers),
            replaced,
        );
    };
    let max_age = bundle
        .accounts
        .iter()
        .filter_map(|account| DateTime::parse_from_rfc3339(&account.refresh_expires_at).ok())
        .map(|expires| {
            expires
                .with_timezone(&Utc)
                .signed_duration_since(Utc::now())
                .num_seconds()
        })
        .max()
        .unwrap_or(0)
        .clamp(0, 31_536_000);
    let secure = cookie_secure_suffix(request_headers);
    (
        format!(
            "Set-Cookie: {BROWSER_SESSION_BUNDLE_COOKIE}={encoded}; Path=/auth; Max-Age={max_age}; HttpOnly; SameSite=Strict{secure}\r\nSet-Cookie: {BROWSER_SESSION_ID_COOKIE}=; Path=/auth; Max-Age=0; HttpOnly; SameSite=Strict{secure}\r\nSet-Cookie: {BROWSER_REFRESH_COOKIE}=; Path=/auth; Max-Age=0; HttpOnly; SameSite=Strict{secure}\r\n"
        ),
        replaced,
    )
}

fn browser_remove_session_cookie_headers(
    request_headers: &HashMap<String, String>,
    removed_session_id: &str,
) -> String {
    let Some(mut bundle) = decode_browser_bundle(request_headers) else {
        return clear_browser_session_cookie_headers(request_headers);
    };
    bundle
        .accounts
        .retain(|account| account.session_id != removed_session_id);
    bundle
        .mru_session_ids
        .retain(|session_id| session_id != removed_session_id);
    if bundle.accounts.is_empty() {
        return clear_browser_session_cookie_headers(request_headers);
    }
    if bundle.active_session_id == removed_session_id {
        bundle.active_session_id = bundle.mru_session_ids[0].clone();
    }
    let Some(encoded) = encode_browser_bundle(&bundle) else {
        return clear_browser_session_cookie_headers(request_headers);
    };
    let max_age = bundle
        .accounts
        .iter()
        .filter_map(|account| DateTime::parse_from_rfc3339(&account.refresh_expires_at).ok())
        .map(|expires| {
            expires
                .with_timezone(&Utc)
                .signed_duration_since(Utc::now())
                .num_seconds()
        })
        .max()
        .unwrap_or(0)
        .clamp(0, 31_536_000);
    let secure = cookie_secure_suffix(request_headers);
    format!(
        "Set-Cookie: {BROWSER_SESSION_BUNDLE_COOKIE}={encoded}; Path=/auth; Max-Age={max_age}; HttpOnly; SameSite=Strict{secure}\r\nSet-Cookie: {BROWSER_SESSION_ID_COOKIE}=; Path=/auth; Max-Age=0; HttpOnly; SameSite=Strict{secure}\r\nSet-Cookie: {BROWSER_REFRESH_COOKIE}=; Path=/auth; Max-Age=0; HttpOnly; SameSite=Strict{secure}\r\n"
    )
}

fn browser_bundle_cookie_headers(
    bundle: &BrowserSessionBundle,
    request_headers: &HashMap<String, String>,
) -> String {
    let Some(encoded) = encode_browser_bundle(bundle) else {
        return clear_browser_session_cookie_headers(request_headers);
    };
    let max_age = bundle
        .accounts
        .iter()
        .filter_map(|account| DateTime::parse_from_rfc3339(&account.refresh_expires_at).ok())
        .map(|expires| {
            expires
                .with_timezone(&Utc)
                .signed_duration_since(Utc::now())
                .num_seconds()
        })
        .max()
        .unwrap_or(0)
        .clamp(0, 31_536_000);
    let secure = cookie_secure_suffix(request_headers);
    format!(
        "Set-Cookie: {BROWSER_SESSION_BUNDLE_COOKIE}={encoded}; Path=/auth; Max-Age={max_age}; HttpOnly; SameSite=Strict{secure}\r\nSet-Cookie: {BROWSER_SESSION_ID_COOKIE}=; Path=/auth; Max-Age=0; HttpOnly; SameSite=Strict{secure}\r\nSet-Cookie: {BROWSER_REFRESH_COOKIE}=; Path=/auth; Max-Age=0; HttpOnly; SameSite=Strict{secure}\r\n"
    )
}

fn clear_browser_session_cookie_headers(request_headers: &HashMap<String, String>) -> String {
    let secure = cookie_secure_suffix(request_headers);
    format!(
        "Set-Cookie: {BROWSER_SESSION_BUNDLE_COOKIE}=; Path=/auth; Max-Age=0; HttpOnly; SameSite=Strict{secure}\r\nSet-Cookie: {BROWSER_SESSION_ID_COOKIE}=; Path=/auth; Max-Age=0; HttpOnly; SameSite=Strict{secure}\r\nSet-Cookie: {BROWSER_REFRESH_COOKIE}=; Path=/auth; Max-Age=0; HttpOnly; SameSite=Strict{secure}\r\n"
    )
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
        | IdentityStoreError::RoleSelectionRequired
        | IdentityStoreError::RoleSelectionInvalid
        | IdentityStoreError::Primitive(_) => ("401 Unauthorized", "authentication_failed"),
        IdentityStoreError::SecurityCommandConflict => {
            ("409 Conflict", "security_command_conflict")
        }
        IdentityStoreError::Database(_) => ("503 Service Unavailable", "identity unavailable"),
        IdentityStoreError::InvalidRecord(_) => {
            ("500 Internal Server Error", "identity_record_invalid")
        }
    }
}

#[derive(Clone)]
struct HttpServiceContext {
    store: Arc<SqlxControlPlane>,
    registry: Arc<SqlxHoldingStoreRegistry>,
    auth: Arc<AuthContext>,
    commercial: Arc<SqlxCommercialStore>,
    holding_router: Option<Arc<HoldingDatabaseRouter>>,
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
    registry: &'a SqlxHoldingStoreRegistry,
    router: Option<&'a HoldingDatabaseRouter>,
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
    let loopback = hostname == "localhost" || hostname.parse::<std::net::IpAddr>().is_ok();
    let local_profile = std::env::var("ERPX_DEVELOPMENT_PROFILE").as_deref() == Ok("local");
    if hostname.is_empty()
        || hostname.len() > 253
        || (loopback && !local_profile)
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
    data_profile_key: &str,
    hostname: &str,
) -> Result<String, &'static str> {
    let key = std::env::var("ERPX_MARKETING_ABUSE_FINGERPRINT_KEY")
        .map_err(|_| "marketing abuse fingerprinting is not configured")?;
    if key.trim().is_empty() {
        return Err("marketing abuse fingerprinting is not configured");
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes())
        .map_err(|_| "marketing abuse fingerprinting is not configured")?;
    mac.update(b"erpx-marketing-public-abuse\0");
    mac.update(data_profile_key.as_bytes());
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
    let holding_id = match SqlxCorporateIdentityStore::new(Arc::new(control.clone()))
        .holding_id_for_verified_hostname(hostname)
        .await
    {
        Ok(Some(holding_id)) => holding_id.hyphenated().to_string(),
        Ok(None) => {
            return Some((
                "404 Not Found",
                "application/json",
                "{\"error\":\"marketing_domain_not_bound\"}".into(),
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
    let registration = match registry.get(&holding_id).await {
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
            "{\"error\":\"holding_routing_unavailable\"}".into(),
        ));
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
    let fingerprint = match marketing_abuse_fingerprint(headers, &holding_id, hostname) {
        Ok(fingerprint) => fingerprint,
        Err(_) => {
            return Some((
                "503 Service Unavailable",
                "application/json",
                "{\"error\":\"marketing_abuse_service_unavailable\"}".into(),
            ));
        }
    };
    let holding_binding_id = marketing_abuse_binding_id("holding", &holding_id);
    let domain_binding_id = marketing_abuse_binding_id("hostname", &hostname.to_ascii_lowercase());
    let policy = match AnonymousMarketingAbusePolicy::new(
        holding_binding_id,
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
        holding_binding_id,
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
                holding_id: holding_id.clone(),
                data_profile_key: registration.data_profile_key.clone(),
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
            &holding_id,
            &registration.data_profile_key,
            RoutedDatabaseSide::Write,
        )
        .await
    {
        Ok(database) => database,
        Err(_) => {
            return Some((
                "503 Service Unavailable",
                "application/json",
                "{\"error\":\"holding_write_unavailable\"}".into(),
            ));
        }
    };
    let read = match router
        .route(
            &holding_id,
            &registration.data_profile_key,
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
                "{\"error\":\"holding_read_unavailable\"}".into(),
            ));
        }
    };
    let store = match SqlxMarketingContentStore::new(
        write.database_pool().clone(),
        read.database_pool().clone(),
        registration.data_profile_key.clone(),
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
                principal_ref: format!("public-form:{holding_id}"),
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
        let token = headers
            .get("x-erpx-preview-token")
            .map(String::as_str)
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
        let Some(version) = template.versions.get(&binding.template.version) else {
            write.close().await;
            read.close().await;
            return Some((
                "404 Not Found",
                "application/json",
                "{\"error\":\"preview_content_not_found\"}".into(),
            ));
        };
        let english = version.variant(ContentLocale::En);
        let thai = version.variant(ContentLocale::Th);
        match serde_json::to_string(&serde_json::json!({
            "version": version.version,
            "variants": {
                "en": { "title": &english.title, "body": &english.body },
                "th": { "title": &thai.title, "body": &thai.body },
            },
            "seo": {
                "title": { "en": &version.seo.title.en, "th": &version.seo.title.th },
                "description": {
                    "en": &version.seo.description.en,
                    "th": &version.seo.description.th,
                },
                "canonical_path": &version.seo.canonical_path,
                "no_index": version.seo.no_index,
            },
        })) {
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
        holding_router,
        storage,
        readiness,
        marketing_callback_secrets,
    } = context;
    let (method, path, headers, body) = read_http_request(&mut stream).await?;
    let _request_observation = operational_metrics::RequestObservation::start();
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
    if path == "/branding/bootstrap" {
        let (status, response_body) = if method != "GET" {
            (
                "405 Method Not Allowed",
                serde_json::json!({"error":"method_not_allowed"}).to_string(),
            )
        } else {
            let requested_host = headers
                .get("host")
                .and_then(|value| public_brand_hostname(value));
            let identity_store = SqlxCorporateIdentityStore::new(store.clone());
            let identity = match requested_host {
                Some(hostname) => identity_store
                    .resolved_for_verified_hostname(&hostname)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_else(
                        erpx_domain::corporate_identity::ResolvedCorporateIdentity::neutral_erpx,
                    ),
                None => erpx_domain::corporate_identity::ResolvedCorporateIdentity::neutral_erpx(),
            };
            match serde_json::to_string(&identity) {
                Ok(body)
                    if body.len() <= erpx_domain::corporate_identity::MAX_BRAND_BOOTSTRAP_BYTES =>
                {
                    ("200 OK", body)
                }
                _ => (
                    "200 OK",
                    serde_json::to_string(
                        &erpx_domain::corporate_identity::ResolvedCorporateIdentity::neutral_erpx(),
                    )?,
                ),
            }
        };
        return write_public_brand_response(
            &mut stream,
            status,
            &response_body,
            correlation_id,
            cors_origin.as_deref(),
        )
        .await;
    }
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
        let cookie_headers = if passkey_response.status.starts_with("200") {
            if let Ok(session) = serde_json::from_str::<SessionBody>(&passkey_response.body) {
                let (headers, replaced) = browser_session_cookie_update(&session, &headers);
                for replaced_session_id in replaced {
                    let _ = auth
                        .identity
                        .revoke_session(replaced_session_id, Utc::now())
                        .await;
                }
                headers
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        return write_http_response_with_headers(
            &mut stream,
            passkey_response.status,
            "application/json",
            &passkey_response.body,
            correlation_id,
            cors_origin.as_deref(),
            &cookie_headers,
        )
        .await;
    }
    if let Some(identity_response) =
        identity_http::handle(&method, &path, &headers, &body, auth.clone()).await?
    {
        let cookie_headers = if identity_response.status.starts_with("200") {
            if let Ok(session) = serde_json::from_str::<SessionBody>(&identity_response.body) {
                let (headers, replaced) = browser_session_cookie_update(&session, &headers);
                for replaced_session_id in replaced {
                    let _ = auth
                        .identity
                        .revoke_session(replaced_session_id, Utc::now())
                        .await;
                }
                headers
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        return write_http_response_with_headers(
            &mut stream,
            identity_response.status,
            "application/json",
            &identity_response.body,
            correlation_id,
            cors_origin.as_deref(),
            &cookie_headers,
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
                router: holding_router.as_deref(),
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
                    router: holding_router.as_deref(),
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
                holding_router.as_deref(),
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
    let requested_auth_session_id = match path.as_str() {
        "/auth/revoke" => serde_json::from_slice::<RevokeBody>(&body)
            .ok()
            .map(|input| input.session_id),
        "/auth/activate" => serde_json::from_slice::<ActivateBody>(&body)
            .ok()
            .map(|input| input.session_id),
        _ => None,
    };
    let (status, content_type, body) = match path.as_str() {
        "/healthz" | "/health" if method == "GET" => (
            "200 OK",
            "application/json",
            serde_json::json!({
                "status": "ok",
                "storage": storage.health_signal(),
            })
            .to_string(),
        ),
        "/readyz" | "/ready" if method == "GET" => {
            let system_readiness = readiness.observe().await;
            let control_plane_ready = store.check_ready().await.is_ok();
            let identity_ready = auth.identity.check_ready().await.is_ok();
            storage.refresh_readiness().await;
            let storage_ready = storage.configuration_ready();
            let ready = control_plane_ready
                && identity_ready
                && storage_ready
                && (!readiness.required || system_readiness.is_ready());
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
                            "required": readiness.required,
                            "worker_heartbeat": system_readiness.worker_heartbeat.as_str(),
                            "holding_routing": system_readiness.holding_routing.as_str(),
                            "projection_freshness": system_readiness.projection_freshness.as_str(),
                            "backup_pitr": system_readiness.backup_pitr.as_str(),
                        },
                    })
                    .to_string(),
                )
            } else {
                operational_metrics::readiness_failure();
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
                            "required": readiness.required,
                            "worker_heartbeat": system_readiness.worker_heartbeat.as_str(),
                            "holding_routing": system_readiness.holding_routing.as_str(),
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
            operational_metrics::render(store.as_ref()).await,
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
        "/healthz" | "/health" | "/readyz" | "/ready" | "/metrics" => (
            "405 Method Not Allowed",
            "application/json",
            "{\"error\":\"method_not_allowed\"}".to_owned(),
        ),
        "/auth/accounts" if method == "GET" => match decode_browser_bundle(&headers) {
            None => (
                "401 Unauthorized",
                "application/json",
                "{\"error\":\"browser_session_unavailable\"}".to_owned(),
            ),
            Some(bundle) => {
                let session_ids = bundle
                    .mru_session_ids
                    .iter()
                    .filter_map(|session_id| Uuid::parse_str(session_id).ok())
                    .collect::<Vec<_>>();
                let active_session_id = Uuid::parse_str(&bundle.active_session_id)
                    .expect("validated browser bundle active session ID");
                match auth
                    .identity
                    .account_session_summaries(&session_ids, active_session_id, Utc::now())
                    .await
                {
                    Ok(accounts) => (
                        "200 OK",
                        "application/json",
                        serde_json::to_string(&AccountsBody { accounts })?,
                    ),
                    Err(_) => (
                        "503 Service Unavailable",
                        "application/json",
                        "{\"error\":\"accounts_unavailable\"}".to_owned(),
                    ),
                }
            }
        },
        "/auth/activate" if method == "POST" => {
            let input = serde_json::from_slice::<ActivateBody>(&body).ok();
            let bundle = decode_browser_bundle(&headers);
            match input.zip(bundle).and_then(|(input, bundle)| {
                let account = bundle
                    .accounts
                    .into_iter()
                    .find(|account| account.session_id == input.session_id)?;
                Some(account)
            }) {
                None => (
                    "400 Bad Request",
                    "application/json",
                    "{\"error\":\"account_not_in_bundle\"}".to_owned(),
                ),
                Some(account) => {
                    let session_id = Uuid::parse_str(&account.session_id)
                        .expect("validated browser bundle session ID");
                    match auth
                        .identity
                        .refresh(&auth.signer, session_id, &account.refresh_token, Utc::now())
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
                    }
                }
            }
        }
        "/auth/restore" if method == "POST" => match browser_refresh_request(&headers) {
            None => (
                "401 Unauthorized",
                "application/json",
                "{\"error\":\"browser_session_unavailable\"}".to_owned(),
            ),
            Some(input) => {
                let session_id = Uuid::parse_str(&input.session_id)
                    .expect("browser cookie session ID was validated");
                match auth
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
                }
            }
        },
        "/auth/clear" if method == "POST" => (
            "200 OK",
            "application/json",
            "{\"cleared\":true}".to_owned(),
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
                            Ok(_) => {
                                let _ = auth
                                    .identity
                                    .revoke_development_contexts_for_session(
                                        session_id,
                                        "controller_session_revoked",
                                        Utc::now(),
                                    )
                                    .await;
                                (
                                    "200 OK",
                                    "application/json",
                                    "{\"revoked\":true}".to_owned(),
                                )
                            }
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
        "/auth/login" | "/auth/refresh" | "/auth/revoke" | "/auth/restore" | "/auth/activate"
        | "/auth/accounts" | "/auth/clear" => (
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
    let (cookie_headers, replaced_sessions) = if status.starts_with("200") {
        if let Ok(session) = serde_json::from_str::<SessionBody>(&body) {
            browser_session_cookie_update(&session, &headers)
        } else if path == "/auth/revoke" {
            let removed = requested_auth_session_id.clone().unwrap_or_default();
            (
                browser_remove_session_cookie_headers(&headers, &removed),
                Vec::new(),
            )
        } else if path == "/auth/clear" {
            let active = browser_refresh_request(&headers)
                .map(|input| input.session_id)
                .unwrap_or_default();
            (
                browser_remove_session_cookie_headers(&headers, &active),
                Vec::new(),
            )
        } else if path == "/auth/accounts"
            && request_cookie(&headers, BROWSER_SESSION_BUNDLE_COOKIE).is_none()
        {
            (
                decode_browser_bundle(&headers)
                    .map(|bundle| browser_bundle_cookie_headers(&bundle, &headers))
                    .unwrap_or_default(),
                Vec::new(),
            )
        } else {
            (String::new(), Vec::new())
        }
    } else if matches!(path.as_str(), "/auth/restore" | "/auth/activate")
        && status.starts_with("401")
    {
        let removed = if path == "/auth/activate" {
            requested_auth_session_id.clone()
        } else {
            browser_refresh_request(&headers).map(|input| input.session_id)
        }
        .unwrap_or_default();
        (
            browser_remove_session_cookie_headers(&headers, &removed),
            Vec::new(),
        )
    } else {
        (String::new(), Vec::new())
    };
    for replaced_session_id in replaced_sessions {
        let _ = auth
            .identity
            .revoke_session(replaced_session_id, Utc::now())
            .await;
    }
    write_http_response_with_headers(
        &mut stream,
        status,
        content_type,
        &body,
        correlation_id,
        cors_origin.as_deref(),
        &cookie_headers,
    )
    .await
}

#[derive(Debug, Deserialize)]
struct ObjectUploadBody {
    holding_id: String,
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
    holding_id: String,
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
    registry: &'a SqlxHoldingStoreRegistry,
    auth: &'a AuthContext,
    store: &'a SqlxControlPlane,
    commercial: &'a SqlxCommercialStore,
    router: Option<&'a HoldingDatabaseRouter>,
    storage: &'a StorageComposition,
}

struct ObjectPolicyRequest<'a> {
    holding_id: &'a str,
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
        let request = HoldingObjectUploadRequest {
            holding_id: input.holding_id,
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
            context.router,
            ObjectPolicyRequest {
                holding_id: &request.holding_id,
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
    let request = HoldingObjectDownloadRequest {
        holding_id: input.holding_id,
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
        context.router,
        ObjectPolicyRequest {
            holding_id: &request.holding_id,
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
    registry: &SqlxHoldingStoreRegistry,
    auth: &AuthContext,
    store: &SqlxControlPlane,
    commercial: &SqlxCommercialStore,
    router: Option<&HoldingDatabaseRouter>,
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
        .get(policy.holding_id)
        .await
        .map_err(|_| ObjectAuthorizationError::Denied)?
    else {
        return Err(ObjectAuthorizationError::Denied);
    };
    if registration.state != "active" || registration.storage_namespace != policy.storage_namespace
    {
        return Err(ObjectAuthorizationError::Denied);
    }
    let holding_id = Uuid::parse_str(&registration.data_profile_key)
        .map_err(|_| ObjectAuthorizationError::Denied)?;
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
        .authorize_role(store, &claims, &scope, policy.permission)
        .await
        .is_ok();
    let support_capability_id = headers
        .get("x-erpx-support-capability-id")
        .filter(|value| !value.is_empty())
        .map(|value| Uuid::parse_str(value))
        .transpose()
        .map_err(|_| ObjectAuthorizationError::Denied)?;
    let support_material = if let Some(capability_id) = support_capability_id {
        auth.authorize_role(store, &claims, &Scope::Platform, "support.session.request")
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
        let authority_scope = match scope {
            Scope::Company {
                holding_id,
                company_id,
            } => KernelHoldingScope::Company {
                holding_id,
                company_id,
            },
            Scope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => KernelHoldingScope::Branch {
                holding_id,
                company_id,
                branch_id,
            },
            _ => return Err(ObjectAuthorizationError::Denied),
        };
        let router = router.ok_or(ObjectAuthorizationError::Denied)?;
        let routed = router
            .route(
                policy.holding_id,
                &registration.data_profile_key,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| ObjectAuthorizationError::Denied)?;
        let holding_id =
            Uuid::parse_str(policy.holding_id).map_err(|_| ObjectAuthorizationError::Denied)?;
        let write = CqrsWritePool::for_holding(routed.database_pool().clone(), holding_id)
            .map_err(|_| ObjectAuthorizationError::Denied)?;
        let repository = SupportWriteRepository::new(write);
        let ticket = erpx_support::WriteRepository::load_ticket(
            &repository,
            &authority_scope,
            erpx_support::SupportTicketId::try_from_uuid(ticket_id)
                .map_err(|_| ObjectAuthorizationError::Denied)?,
        )
        .await
        .map_err(|_| ObjectAuthorizationError::Denied)?
        .ok_or(ObjectAuthorizationError::Denied)?;
        let normalized_session = erpx_support::WriteRepository::load_session(
            &repository,
            &authority_scope,
            erpx_support::SessionId::try_from_uuid(session_id)
                .map_err(|_| ObjectAuthorizationError::Denied)?,
        )
        .await
        .map_err(|_| ObjectAuthorizationError::Denied)?
        .ok_or(ObjectAuthorizationError::Denied)?;
        let normalized_capability = erpx_support::WriteRepository::load_capability(
            &repository,
            &authority_scope,
            erpx_support::CapabilityId::try_from_uuid(capability_id)
                .map_err(|_| ObjectAuthorizationError::Denied)?,
        )
        .await
        .map_err(|_| ObjectAuthorizationError::Denied)?
        .ok_or(ObjectAuthorizationError::Denied)?;
        let legacy_mode = match normalized_session.mode() {
            erpx_support::SupportMode::DiagnosticRead => SupportMode::DiagnosticRead,
            erpx_support::SupportMode::HoldingMutation => SupportMode::HoldingMutation,
            erpx_support::SupportMode::Impersonation => SupportMode::Impersonation,
        };
        let mode_name = match legacy_mode {
            SupportMode::DiagnosticRead => "diagnostic_read",
            SupportMode::HoldingMutation => "holding_mutation",
            SupportMode::Impersonation => "impersonation",
        };
        if ticket.id().as_uuid() != ticket_id
            || ticket.customer_account().as_uuid() != customer_id
            || ticket.status() == erpx_support::TicketStatus::Closed
            || ticket.scope() != &authority_scope
            || normalized_session.ticket_id().as_uuid() != ticket_id
            || normalized_session.customer_account().as_uuid() != customer_id
            || normalized_session.agent().as_uuid().to_string() != claims.principal_ref
            || normalized_session.scope() != &authority_scope
            || !normalized_session.is_active(at)
            || normalized_session.module() != "documents"
            || !normalized_session
                .permissions()
                .iter()
                .any(|permission| permission == policy.permission)
            || normalized_capability.id().as_uuid() != capability_id
            || normalized_capability.session_id().as_uuid() != session_id
            || normalized_capability.ticket_id().as_uuid() != ticket_id
            || normalized_capability.customer_account().as_uuid() != customer_id
            || normalized_capability.agent().as_uuid().to_string() != claims.principal_ref
            || normalized_capability.device_ref() != device_ref
            || normalized_capability.mode() != normalized_session.mode()
            || normalized_capability.scope() != &authority_scope
            || normalized_capability.module() != "documents"
            || normalized_capability.revocation().is_some()
            || normalized_capability.issued_at() > at
            || at >= normalized_capability.expires_at()
            || !normalized_capability
                .permissions()
                .iter()
                .any(|permission| permission == policy.permission)
            || mode != mode_name
            || approved_scope != scope
        {
            return Err(ObjectAuthorizationError::Denied);
        }
        let canonical_scope = approved_scope.canonical_key();
        let evaluation_session = erpx_domain::support::SupportSession {
            id: session_id,
            ticket_id,
            customer_account_id: customer_id,
            agent_ref: claims.principal_ref.clone(),
            represented_holding_user: normalized_session
                .represented_principal()
                .map(|principal| principal.as_uuid().to_string()),
            mode: legacy_mode,
            permissions: normalized_session.permissions().to_vec(),
            scope: canonical_scope.clone(),
            module: normalized_session.module().into(),
            reason: normalized_session.reason().into(),
            requested_at: normalized_session.requested_at(),
            expires_at: normalized_session.expires_at(),
            approved_by: normalized_session.approved_by().map(str::to_owned),
            approved_at: normalized_session.approved_at(),
            revoked_at: normalized_session.revoked_at(),
        };
        evaluation_session
            .validate()
            .map_err(|_| ObjectAuthorizationError::Denied)?;
        let binding = erpx_domain::support::SupportCapabilityBinding::new(
            customer_id,
            claims.principal_ref.clone(),
            device_ref.to_owned(),
            session_id,
            ticket_id,
            legacy_mode,
            normalized_capability.permissions().to_vec(),
            canonical_scope,
            normalized_capability.module().to_owned(),
        )
        .map_err(|_| ObjectAuthorizationError::Denied)?;
        let evaluation_capability = erpx_domain::support::SupportCapability::from_persisted(
            capability_id,
            binding,
            normalized_capability.issued_at(),
            normalized_capability.expires_at(),
            *normalized_capability.secret_hash_sha256(),
            None,
            None,
        )
        .map_err(|_| ObjectAuthorizationError::Denied)?;
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
                .represented_holding_user
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
            matches!(session.mode, SupportMode::HoldingMutation),
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
            holding_id: policy.holding_id,
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
    router: Option<&HoldingDatabaseRouter>,
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
            &binding.holding_id.to_string(),
            &binding.data_profile_key,
            RoutedDatabaseSide::Write,
        )
        .await
        .map_err(|_| MarketingCallbackHttpError::Persistence)?;
    let read = match router
        .route(
            &binding.holding_id.to_string(),
            &binding.data_profile_key,
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
        binding.data_profile_key.clone(),
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
        AuthorityScope::Holding { holding_id }
        | AuthorityScope::Company { holding_id, .. }
        | AuthorityScope::Branch { holding_id, .. }
        | AuthorityScope::Subtree { holding_id, .. } => Some(*holding_id),
        AuthorityScope::Platform => None,
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
    write_http_response_with_headers(
        stream,
        status,
        content_type,
        body,
        correlation_id,
        cors_origin,
        "",
    )
    .await
}

async fn write_http_response_with_headers(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
    correlation_id: Uuid,
    cors_origin: Option<&str>,
    additional_headers: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    operational_metrics::response(status);
    let response_body = errors::http_body(status, body, correlation_id);
    let cors_headers = cors_origin.map_or_else(String::new, |origin| {
        format!(
            "Access-Control-Allow-Origin: {origin}\r\nAccess-Control-Allow-Credentials: true\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Cache-Control, Content-Type, X-Erpx-Access-Token, X-Erpx-Actor, X-Erpx-Session, X-Erpx-Dev-Persona-Context, X-Erpx-Correlation-Id\r\nAccess-Control-Expose-Headers: X-Correlation-Id\r\nAccess-Control-Max-Age: 600\r\nVary: Origin\r\n"
        )
    });
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nX-Correlation-Id: {correlation_id}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n{cors_headers}{additional_headers}Connection: close\r\n\r\n{response_body}",
        response_body.len(),
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}

fn public_brand_hostname(host: &str) -> Option<String> {
    let host = host.trim();
    if host.is_empty() || host.len() > 260 || host.chars().any(char::is_control) {
        return None;
    }
    let host = if host.starts_with('[') {
        return None;
    } else {
        host.split_once(':').map_or(host, |(hostname, _)| hostname)
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

async fn write_public_brand_response(
    stream: &mut TcpStream,
    status: &str,
    body: &str,
    correlation_id: Uuid,
    cors_origin: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    operational_metrics::response(status);
    let cors_headers = cors_origin.map_or_else(String::new, |origin| {
        format!(
            "Access-Control-Allow-Origin: {origin}\r\nAccess-Control-Expose-Headers: ETag, X-Correlation-Id\r\nVary: Host, Origin\r\n"
        )
    });
    let digest = format!("{:x}", Sha256::digest(body.as_bytes()));
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nX-Correlation-Id: {correlation_id}\r\nETag: \"{digest}\"\r\nCache-Control: public, max-age=300, stale-if-error=86400\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\n{cors_headers}Connection: close\r\n\r\n{body}",
        body.len(),
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
    "http://127.0.0.1:3004",
    "http://localhost:3004",
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

fn configure_holding_router(
    store: Arc<SqlxControlPlane>,
) -> Result<Option<Arc<HoldingDatabaseRouter>>, Box<dyn std::error::Error>> {
    let admin = std::env::var("ERPX_HOLDING_ADMIN_DATABASE_URL").ok();
    let template = std::env::var("ERPX_HOLDING_DATABASE_URL_TEMPLATE").ok();
    match (admin, template) {
        (None, None) => Ok(None),
        (Some(admin_database_url), Some(database_url_template))
            if !admin_database_url.trim().is_empty()
                && !database_url_template.trim().is_empty() =>
        {
            let config = HoldingDatabaseProvisioningConfig {
                admin_database_url,
                database_url_template,
            };
            let router = HoldingDatabaseRouter::new(store, config)?;
            Ok(Some(Arc::new(router)))
        }
        _ => Err("holding database routing configuration is incomplete".into()),
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

fn configure_directory_secret_resolver()
-> Result<Arc<dyn DirectorySecretResolver>, Box<dyn std::error::Error>> {
    match std::env::var("ERPX_DIRECTORY_SECRET_RESOLVER").as_deref() {
        Ok("local") | Err(std::env::VarError::NotPresent) => {
            let root = std::env::var_os("ERPX_DIRECTORY_SECRET_ROOT")
                .map(std::path::PathBuf::from)
                .unwrap_or(std::env::current_dir()?.join(".runtime/erpx/secrets/directory"));
            Ok(Arc::new(LocalDirectorySecretResolver::new(root)?))
        }
        Ok("https") | Ok("secret-manager") => Ok(Arc::new(HttpsDirectorySecretResolver::new(
            &std::env::var("ERPX_DIRECTORY_SECRET_MANAGER_ENDPOINT")?,
            std::env::var("ERPX_DIRECTORY_SECRET_MANAGER_TOKEN")?,
        )?)),
        Ok("unconfigured") => Ok(Arc::new(UnconfiguredDirectorySecretResolver)),
        Ok(_) => Err(
            "ERPX_DIRECTORY_SECRET_RESOLVER must be local, https, secret-manager, or unconfigured"
                .into(),
        ),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err("ERPX_DIRECTORY_SECRET_RESOLVER is invalid".into())
        }
    }
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let log_format = std::env::var("ERPX_LOG_FORMAT").unwrap_or_else(|_| "pretty".into());
    if log_format == "json" {
        tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_span_list(true)
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    } else if log_format == "pretty" {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    } else {
        return Err("ERPX_LOG_FORMAT must be pretty or json".into());
    }
    tracing::info!("ERPX target service catalog authority selected");
    // The local profile controls local composition/readiness and is not an
    // authorization switch. Development persona access remains a separate,
    // explicitly requested feature whose full set of gates is validated
    // below. This lets `cargo xtask dev` run locally without implicitly
    // enabling the controller surface.
    let development_requested = std::env::var("ERPX_DEV_PERSONA_MODE").as_deref() == Ok("enabled")
        || std::env::var("ERPX_DEVELOPMENT_CLUSTER").as_deref() == Ok("local-ucs-lab")
        || [
            "ERPX_DEV_CONTROLLER_ROLE_PROFILE",
            "ERPX_DEV_CONTROLLER_TARGET_HOLDING",
            "ERPX_DEV_CONTROLLER_MFA_BYPASS",
            "ERPX_DEV_CONTROLLER_SOD_BYPASS",
        ]
        .into_iter()
        .any(|key| std::env::var_os(key).is_some());
    if development_requested {
        identity_http::development_gates()
            .map_err(|error| format!("development access startup gate failed: {error}"))?;
    }
    let development_controller = identity_http::development_controller_role_config()
        .map_err(|error| format!("development controller startup gate failed: {error}"))?;
    let storage = Arc::new(
        StorageComposition::from_env()
            .map_err(|error| format!("object storage configuration failed: {error}"))?,
    );
    let database_config = ControlPlaneDatabaseConfig::from_env()?;
    let pools = ControlPlanePools::connect(&database_config)
        .await
        .map_err(|error| format!("control-plane database connection failed: {error}"))?;
    erpx_migration::migrate_control_plane(&pools.write)
        .await
        .map_err(|error| format!("control-write migration failed: {error}"))?;
    erpx_migration::migrate_control_read(&pools.read)
        .await
        .map_err(|error| format!("control-read migration failed: {error}"))?;
    apply_identity_migration(&pools.write)
        .await
        .map_err(|error| format!("identity migration failed: {error}"))?;
    let access_token_key = std::env::var("ERPX_ACCESS_TOKEN_SIGNING_KEY")
        .map_err(|_| "ERPX_ACCESS_TOKEN_SIGNING_KEY is required")?;
    let access_token_signer = Arc::new(AccessTokenSigner::new(access_token_key.as_bytes())?);
    let store = Arc::new(SqlxControlPlane::new(pools));
    store
        .require_erpx_command_owner()
        .await
        .map_err(|error| format!("control-plane command-owner check failed: {error}"))?;
    let commercial = Arc::new(SqlxCommercialStore::new(&store));
    let holding_router = configure_holding_router(store.clone())?;
    let marketing_callback_secrets = configure_marketing_callback_secret_resolver()?;
    let identity = Arc::new(SqlxIdentityStore::new(store.pools().write.clone()));
    let directory_secrets = configure_directory_secret_resolver()?;
    let passkeys = configure_passkey_service((*identity).clone())?;
    let totp_keys = configure_totp_key_resolver()?;
    let auth = Arc::new(AuthContext {
        identity,
        directory: Arc::new(SqlxDirectoryStore::new(store.pools().write.clone())),
        directory_secrets,
        signer: access_token_signer.clone(),
        passkeys,
        totp_keys,
        development_controller,
    });
    let audit: Arc<dyn erpx_application::AuditSink> = store.clone();
    let clock = Arc::new(SystemClock);
    let state = AppState {
        auth: auth.clone(),
        store: store.clone(),
        commercial: commercial.clone(),
        holding_router,
        storage: storage.clone(),
        roles: Arc::new(RoleLifecycleUseCase::new(store.clone(), clock, audit)),
        platform_completion: Arc::new(PlatformCompletionApi::new(Arc::new(
            PlatformControlStore::new(store.clone()),
        ))),
        migration: Arc::new(SqlxG13MigrationStore::new(store.clone())),
    };
    let data_plane_broker = Arc::new(data_plane_rpc::DataPlaneBroker::new(
        erpx_infrastructure::SqlxDataPlaneControlStore::new(store.clone()),
    )?);
    data_plane_rpc::spawn_agent_broker(data_plane_broker.clone())
        .await
        .map_err(|error| format!("data-plane broker startup failed: {error}"))?;
    let http_addr: SocketAddr = std::env::var("ERPX_HTTP_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50052".into())
        .parse()?;
    let holding_registry = Arc::new(SqlxHoldingStoreRegistry::new(state.store.clone()));
    let readiness = Arc::new(ServerReadinessAdapter::from_env(
        state.store.clone(),
        holding_registry.clone(),
    ));
    let http_listener = TcpListener::bind(http_addr).await?;
    tokio::spawn(serve_http_health(
        http_listener,
        HttpServiceContext {
            store: state.store.clone(),
            registry: holding_registry,
            auth: state.auth.clone(),
            commercial: state.commercial.clone(),
            holding_router: state.holding_router.clone(),
            storage: state.storage.clone(),
            readiness: readiness.clone(),
            marketing_callback_secrets,
        },
    ));
    tracing::info!(%http_addr, "ERPX HTTP health and metrics listening");
    let address = std::env::var("ERPX_GRPC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50051".into())
        .parse()?;
    let quality_application = Arc::new(QualityServerApplication {
        router: state.holding_router.clone(),
    });
    let quality_authenticator = Arc::new(QualityServerAuthenticator {
        state: state.clone(),
    });
    let commercial_provider_key = CommercialProviderKey::from_env()?;
    let commercial_application = Arc::new(CommercialServerApplication::new(
        &state,
        commercial_provider_key.clone(),
    ));
    let commercial_authenticator = Arc::new(CommercialServerAuthenticator::new(
        state.clone(),
        commercial_provider_key,
    ));
    let identity_access_application = Arc::new(IdentityServerApplication::new(&state));
    let identity_access_authenticator = Arc::new(IdentityServerAuthenticator::new(state.clone()));
    let organization_application = Arc::new(OrganizationServerApplication::new(
        state.holding_router.clone(),
    ));
    let organization_authenticator = Arc::new(OrganizationServerAuthenticator::new(state.clone()));
    let platform_administration_application =
        Arc::new(PlatformAdministrationServerApplication::new(&state));
    let platform_administration_authenticator = Arc::new(
        PlatformAdministrationServerAuthenticator::new(state.clone()),
    );
    let brand_application = Arc::new(BrandServerApplication::new(&state));
    let brand_authenticator = Arc::new(BrandServerAuthenticator::new(state.clone()));
    let customer_authority_application = Arc::new(CustomerAuthorityServerApplication::new(&state));
    let customer_authority_authenticator =
        Arc::new(CustomerAuthorityServerAuthenticator::new(state.clone()));
    let wave6_application = Arc::new(Wave6HoldingApplication::new(&state));
    let wave6_authenticator = Arc::new(Wave6ServerAuthenticator::new(state.clone()));
    let wave7_application = Arc::new(Wave7HoldingApplication::new(&state));
    let wave7_authenticator = Arc::new(Wave7ServerAuthenticator::new(state.clone()));
    let wave8_application = Arc::new(Wave8HoldingApplication::new(state.clone())?);
    let wave8_authenticator = Arc::new(Wave8ServerAuthenticator::new(state.clone()));
    let wave9_application = Arc::new(Wave9Application::new(&state));
    let wave9_authenticator = Arc::new(Wave9ServerAuthenticator::new(state.clone()));
    let future_agent_projects = future_agent_projects::tonic_services(state.clone());
    tracing::info!(%address, "ERPX Tonic server listening");
    tonic::transport::Server::builder()
        .accept_http1(true)
        .layer(tonic::service::InterceptorLayer::new(move |request| {
            transport_interceptor(request, access_token_signer.as_ref())
        }))
        .layer(grpc_web_cors_layer())
        .layer(tonic_web::GrpcWebLayer::new())
        .add_service(
            erpx_proto::erpx::reference::reference_query_service_server::ReferenceQueryServiceServer::new(
                ReferenceQueryRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::collaboration::collaboration_command_service_server::CollaborationCommandServiceServer::new(
                CollaborationCommandRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::collaboration::collaboration_query_service_server::CollaborationQueryServiceServer::new(
                CollaborationQueryRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::collaboration::collaboration_stream_service_server::CollaborationStreamServiceServer::new(
                CollaborationStreamRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::platform_administration::data_plane_stream_service_server::DataPlaneStreamServiceServer::new(
                DataPlaneStreamRpc::new(state.clone(), readiness),
            ),
        )
        .add_service(
            erpx_proto::erpx::operations::operations_command_service_server::OperationsCommandServiceServer::new(
                OperationsCommandRpc::new(
                    wave9_application.clone(),
                    wave9_authenticator.clone(),
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::operations::operations_query_service_server::OperationsQueryServiceServer::new(
                OperationsQueryRpc::new(
                    wave9_application.clone(),
                    wave9_authenticator.clone(),
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::customer_360::customer360_command_service_server::Customer360CommandServiceServer::new(
                CustomerRpcImpl {
                    state: state.clone(),
                },
            ),
        )
        .add_service(
            erpx_proto::erpx::customer_360::customer360_query_service_server::Customer360QueryServiceServer::new(
                Customer360QueryComposite::new(
                    Customer360QueryRpc::new(wave9_application, wave9_authenticator),
                    CustomerRpcImpl { state: state.clone() },
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::quality::quality_command_service_server::QualityCommandServiceServer::new(
                QualityCommandRpc::new(quality_application.clone(), quality_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::quality::quality_query_service_server::QualityQueryServiceServer::new(
                QualityQueryRpc::new(quality_application, quality_authenticator),
            ),
        )
        .add_service(
            erpx_proto::erpx::commercial::commercial_command_service_server::CommercialCommandServiceServer::new(
                CommercialCommandComposite::new(
                    CommercialCommandRpc::new(
                        commercial_application.clone(),
                        commercial_authenticator.clone(),
                    ),
                    EntitlementRpcImpl {
                        auth: auth.clone(),
                        store: store.clone(),
                        commercial: commercial.clone(),
                    },
                    SaasBillingRpcImpl { state: state.clone() },
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::commercial::commercial_query_service_server::CommercialQueryServiceServer::new(
                CommercialQueryComposite::new(
                    CommercialQueryRpc::new(commercial_application, commercial_authenticator),
                    EntitlementRpcImpl {
                        auth: auth.clone(),
                        store: store.clone(),
                        commercial: commercial.clone(),
                    },
                    SaasBillingRpcImpl { state: state.clone() },
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::identity_access::identity_access_command_service_server::IdentityAccessCommandServiceServer::new(
                IdentityAccessCommandComposite::new(
                    IdentityAccessCommandRpc::new(
                        identity_access_application.clone(),
                        identity_access_authenticator.clone(),
                    ),
                    DirectoryRpcImpl {
                        state: state.clone(),
                        runtime_enabled: std::env::var("ERPX_DIRECTORY_RUNTIME").as_deref()
                            == Ok("enabled"),
                    },
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::identity_access::identity_access_query_service_server::IdentityAccessQueryServiceServer::new(
                IdentityAccessQueryComposite::new(
                    IdentityAccessQueryRpc::new(
                        identity_access_application,
                        identity_access_authenticator,
                    ),
                    DirectoryRpcImpl {
                        state: state.clone(),
                        runtime_enabled: std::env::var("ERPX_DIRECTORY_RUNTIME").as_deref()
                            == Ok("enabled"),
                    },
                    RoleRpcImpl { state: state.clone() },
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::organization::organization_command_service_server::OrganizationCommandServiceServer::new(
                OrganizationCommandRpc::new(
                    organization_application.clone(),
                    organization_authenticator.clone(),
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::organization::organization_query_service_server::OrganizationQueryServiceServer::new(
                OrganizationQueryRpc::new(
                    organization_application,
                    organization_authenticator,
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::platform_administration::platform_administration_command_service_server::PlatformAdministrationCommandServiceServer::new(
                PlatformAdministrationCommandComposite {
                    current: PlatformAdministrationCommandRpc::new(
                        platform_administration_application.clone(),
                        platform_administration_authenticator.clone(),
                    ),
                    operations: PlatformOperationsRpcImpl { state: state.clone() },
                    migration: migration::MigrationRpcImpl { state: state.clone() },
                    data_plane: data_plane_rpc::DataPlaneControlRpcImpl {
                        state: state.clone(),
                        broker: data_plane_broker.clone(),
                    },
                },
            ),
        )
        .add_service(
            erpx_proto::erpx::platform_administration::platform_administration_query_service_server::PlatformAdministrationQueryServiceServer::new(
                PlatformAdministrationQueryComposite {
                    current: PlatformAdministrationQueryRpc::new(
                        platform_administration_application,
                        platform_administration_authenticator,
                    ),
                    operations: PlatformOperationsRpcImpl { state: state.clone() },
                    migration: migration::MigrationRpcImpl { state: state.clone() },
                    data_plane: data_plane_rpc::DataPlaneControlRpcImpl {
                        state: state.clone(),
                        broker: data_plane_broker,
                    },
                },
            ),
        )
        .add_service(
            erpx_proto::erpx::brand::brand_command_service_server::BrandCommandServiceServer::new(
                BrandCommandRpc::new(brand_application.clone(), brand_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::brand::brand_query_service_server::BrandQueryServiceServer::new(
                BrandQueryRpc::new(brand_application, brand_authenticator),
            ),
        )
        .add_service(
            erpx_proto::erpx::customer_authority::customer_authority_command_service_server::CustomerAuthorityCommandServiceServer::new(
                CustomerAuthorityCommandRpc::new(
                    customer_authority_application.clone(),
                    customer_authority_authenticator.clone(),
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::customer_authority::customer_authority_query_service_server::CustomerAuthorityQueryServiceServer::new(
                CustomerAuthorityQueryComposite::new(
                    CustomerAuthorityQueryRpc::new(
                        customer_authority_application,
                        customer_authority_authenticator,
                    ),
                    CustomerRpcImpl { state: state.clone() },
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::marketing::marketing_command_service_server::MarketingCommandServiceServer::new(
                MarketingCommandComposite::new(
                    MarketingCommandRpc::new(wave6_application.clone(), wave6_authenticator.clone()),
                    MarketingContentRpcImpl { state: state.clone() },
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::marketing::marketing_query_service_server::MarketingQueryServiceServer::new(
                MarketingQueryComposite::new(
                    MarketingQueryRpc::new(wave6_application.clone(), wave6_authenticator.clone()),
                    MarketingContentRpcImpl { state: state.clone() },
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::crm::crm_command_service_server::CrmCommandServiceServer::new(
                CrmCommandComposite::new(
                    CrmCommandRpc::new(wave6_application.clone(), wave6_authenticator.clone()),
                    CrmRpcImpl { state: state.clone() },
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::crm::crm_query_service_server::CrmQueryServiceServer::new(
                CrmQueryComposite::new(
                    CrmQueryRpc::new(wave6_application.clone(), wave6_authenticator.clone()),
                    CrmRpcImpl { state: state.clone() },
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::procurement::procurement_command_service_server::ProcurementCommandServiceServer::new(
                ProcurementCommandRpc::new(wave6_application.clone(), wave6_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::procurement::procurement_query_service_server::ProcurementQueryServiceServer::new(
                ProcurementQueryRpc::new(wave6_application.clone(), wave6_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::inventory::inventory_command_service_server::InventoryCommandServiceServer::new(
                InventoryCommandRpc::new(wave6_application.clone(), wave6_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::inventory::inventory_query_service_server::InventoryQueryServiceServer::new(
                InventoryQueryRpc::new(wave6_application.clone(), wave6_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::assets::assets_command_service_server::AssetsCommandServiceServer::new(
                AssetsCommandRpc::new(wave6_application.clone(), wave6_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::assets::assets_query_service_server::AssetsQueryServiceServer::new(
                AssetsQueryRpc::new(wave6_application.clone(), wave6_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::maintenance::maintenance_command_service_server::MaintenanceCommandServiceServer::new(
                MaintenanceCommandRpc::new(wave6_application.clone(), wave6_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::maintenance::maintenance_query_service_server::MaintenanceQueryServiceServer::new(
                MaintenanceQueryRpc::new(wave6_application, wave6_authenticator),
            ),
        )
        .add_service(
            erpx_proto::erpx::sales::sales_command_service_server::SalesCommandServiceServer::new(
                SalesCommandRpc::new(wave7_application.clone(), wave7_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::sales::sales_query_service_server::SalesQueryServiceServer::new(
                SalesQueryRpc::new(wave7_application.clone(), wave7_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::people_payroll::people_payroll_command_service_server::PeoplePayrollCommandServiceServer::new(
                PeoplePayrollCommandRpc::new(
                    wave7_application.clone(),
                    wave7_authenticator.clone(),
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::people_payroll::people_payroll_query_service_server::PeoplePayrollQueryServiceServer::new(
                PeoplePayrollQueryRpc::new(
                    wave7_application.clone(),
                    wave7_authenticator.clone(),
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::pos::point_of_sale_command_service_server::PointOfSaleCommandServiceServer::new(
                PosCommandRpc::new(wave7_application.clone(), wave7_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::pos::point_of_sale_query_service_server::PointOfSaleQueryServiceServer::new(
                PosQueryRpc::new(wave7_application, wave7_authenticator),
            ),
        )
        .add_service(
            erpx_proto::erpx::finance::finance_command_service_server::FinanceCommandServiceServer::new(
                FinanceCommandRpc::new(wave8_application.clone(), wave8_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::finance::finance_query_service_server::FinanceQueryServiceServer::new(
                FinanceQueryRpc::new(wave8_application.clone(), wave8_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::legal::legal_command_service_server::LegalCommandServiceServer::new(
                LegalCommandRpc::new(wave8_application.clone(), wave8_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::legal::legal_query_service_server::LegalQueryServiceServer::new(
                LegalQueryRpc::new(wave8_application.clone(), wave8_authenticator.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::support::support_command_service_server::SupportCommandServiceServer::new(
                SupportCommandRpc::new(
                    wave8_application.clone(),
                    wave8_authenticator.clone(),
                    wave8_application.clone(),
                    wave8_application.clone(),
                ),
            ),
        )
        .add_service(
            erpx_proto::erpx::support::support_query_service_server::SupportQueryServiceServer::new(
                SupportQueryRpc::new(wave8_application.clone(), wave8_authenticator.clone()),
            ),
        )
        .add_service(future_agent_projects.agent_command)
        .add_service(future_agent_projects.agent_query)
        .add_service(future_agent_projects.projects_command)
        .add_service(future_agent_projects.projects_query)
        .add_service(
            erpx_proto::erpx::manufacturing::manufacturing_command_service_server::ManufacturingCommandServiceServer::new(
                future_manufacturing_planning::ManufacturingCommandRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::manufacturing::manufacturing_query_service_server::ManufacturingQueryServiceServer::new(
                future_manufacturing_planning::ManufacturingQueryRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::planning::planning_command_service_server::PlanningCommandServiceServer::new(
                future_manufacturing_planning::PlanningCommandRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::planning::planning_query_service_server::PlanningQueryServiceServer::new(
                future_manufacturing_planning::PlanningQueryRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::benefits::benefits_command_service_server::BenefitsCommandServiceServer::new(
                future_benefits::BenefitsCommandRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::benefits::benefits_query_service_server::BenefitsQueryServiceServer::new(
                future_benefits::BenefitsQueryRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::learning::learning_command_service_server::LearningCommandServiceServer::new(
                future_learning::LearningCommandRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::learning::learning_query_service_server::LearningQueryServiceServer::new(
                future_learning::LearningQueryRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::field_service::field_service_command_service_server::FieldServiceCommandServiceServer::new(
                future_field_service::FieldServiceCommandRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::field_service::field_service_query_service_server::FieldServiceQueryServiceServer::new(
                future_field_service::FieldServiceQueryRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::ecommerce::ecommerce_command_service_server::EcommerceCommandServiceServer::new(
                future_ecommerce::EcommerceCommandRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::ecommerce::ecommerce_query_service_server::EcommerceQueryServiceServer::new(
                future_ecommerce::EcommerceQueryRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::talent::talent_command_service_server::TalentCommandServiceServer::new(
                future_talent::TalentCommandRpc::new(state.clone()),
            ),
        )
        .add_service(
            erpx_proto::erpx::talent::talent_query_service_server::TalentQueryServiceServer::new(
                future_talent::TalentQueryRpc::new(state.clone()),
            ),
        )
        .serve(address)
        .await?;
    Ok(())
}

#[cfg(test)]
mod target_service_registration_tests {
    #[test]
    fn current_context_servers_register_only_target_packages() {
        let source = include_str!("lib.rs");
        let current_services = [
            ("identity_access", "identity_access"),
            ("platform_administration", "platform_administration"),
            ("organization", "organization"),
            ("commercial", "commercial"),
            ("brand", "brand"),
            ("customer_authority", "customer_authority"),
            ("customer_360", "customer360"),
            ("support", "support"),
            ("finance", "finance"),
            ("sales", "sales"),
            ("pos", "point_of_sale"),
            ("procurement", "procurement"),
            ("inventory", "inventory"),
            ("assets", "assets"),
            ("maintenance", "maintenance"),
            ("quality", "quality"),
            ("operations", "operations"),
            ("people_payroll", "people_payroll"),
            ("legal", "legal"),
            ("marketing", "marketing"),
            ("crm", "crm"),
        ];
        for (module, service) in current_services {
            for kind in ["command", "query"] {
                let target =
                    format!("erpx_proto::erpx::{module}::{service}_{kind}_service_server::");
                assert!(
                    source.contains(&target),
                    "missing target {kind} service registration for {module}"
                );
            }
        }
        assert!(!source.contains(concat!(
            "erpx_proto::",
            "platform::operations_",
            "command_service_server"
        )));
        assert!(!source.contains(concat!(
            "erpx_proto::",
            "platform::finance_",
            "query_service_server"
        )));
        assert!(!source.contains(concat!("CustomerNoteCommand", "ServiceServer::new")));
        assert!(!source.contains(concat!("add_optional_", "service")));
        assert!(!source.contains(concat!("ERPX_V1_SERVICE_", "AUTHORITY")));
    }
}

#[cfg(test)]
mod browser_session_cookie_tests {
    use super::*;

    fn session(principal_ref: &str) -> SessionBody {
        SessionBody {
            principal_ref: principal_ref.into(),
            session_id: Uuid::new_v4().to_string(),
            access_token: "access-secret".into(),
            refresh_token: format!("refresh-{}", Uuid::new_v4().simple()),
            access_expires_at: (Utc::now() + Duration::minutes(5)).to_rfc3339(),
            refresh_expires_at: (Utc::now() + Duration::days(30)).to_rfc3339(),
            active_role_source: None,
            active_role_selection_id: None,
            authorization_context_version: Some(1),
        }
    }

    fn request_headers_from_set_cookie(set_cookie: &str) -> HashMap<String, String> {
        let value = set_cookie
            .lines()
            .find_map(|line| {
                line.strip_prefix(&format!("Set-Cookie: {BROWSER_SESSION_BUNDLE_COOKIE}="))
                    .and_then(|value| value.split(';').next())
            })
            .expect("v2 bundle cookie");
        HashMap::from([(
            "cookie".into(),
            format!("{BROWSER_SESSION_BUNDLE_COOKIE}={value}"),
        )])
    }

    #[test]
    fn browser_session_cookie_is_http_only_strict_and_round_trips() {
        let session_id = Uuid::new_v4().to_string();
        let refresh_token = "safe_url_refresh_token-123".to_owned();
        let session = SessionBody {
            principal_ref: "local:test".into(),
            session_id: session_id.clone(),
            access_token: "access-secret".into(),
            refresh_token: refresh_token.clone(),
            access_expires_at: (Utc::now() + Duration::minutes(5)).to_rfc3339(),
            refresh_expires_at: (Utc::now() + Duration::days(30)).to_rfc3339(),
            active_role_source: None,
            active_role_selection_id: None,
            authorization_context_version: Some(1),
        };
        let headers = HashMap::from([("origin".into(), "http://localhost:3004".into())]);
        let cookie_headers = browser_session_cookie_headers(&session, &headers);

        assert!(cookie_headers.contains("HttpOnly; SameSite=Strict"));
        assert!(!cookie_headers.contains("access-secret"));
        assert!(!cookie_headers.contains(&refresh_token));
        assert!(cookie_headers.contains(BROWSER_SESSION_BUNDLE_COOKIE));
        assert!(!cookie_headers.contains("Secure"));

        let bundle_cookie = cookie_headers
            .lines()
            .find_map(|line| {
                line.strip_prefix(&format!("Set-Cookie: {BROWSER_SESSION_BUNDLE_COOKIE}="))
                    .and_then(|value| value.split(';').next())
            })
            .expect("v2 bundle cookie");
        let request_headers = HashMap::from([(
            "cookie".into(),
            format!("{BROWSER_SESSION_BUNDLE_COOKIE}={bundle_cookie}"),
        )]);
        let restored = browser_refresh_request(&request_headers).expect("valid browser session");
        assert_eq!(restored.session_id, session_id);
        assert_eq!(restored.refresh_token, refresh_token);
    }

    #[test]
    fn malformed_browser_cookie_fails_closed_and_https_requires_secure() {
        let invalid = HashMap::from([(
            "cookie".into(),
            format!("{BROWSER_SESSION_ID_COOKIE}=not-a-uuid; {BROWSER_REFRESH_COOKIE}=bad%0d%0a"),
        )]);
        assert!(browser_refresh_request(&invalid).is_none());

        let headers = HashMap::from([("origin".into(), "https://console.erpx.test".into())]);
        assert_eq!(cookie_secure_suffix(&headers), "; Secure");
        let cleared = clear_browser_session_cookie_headers(&headers);
        assert!(cleared.contains("Max-Age=0; HttpOnly; SameSite=Strict; Secure"));
    }

    #[test]
    fn browser_bundle_caps_at_five_accounts_and_evicts_lru() {
        let mut headers = HashMap::new();
        let mut issued = Vec::new();
        let mut evicted = Vec::new();
        for index in 0..6 {
            let next = session(&format!("local:principal-{index}"));
            issued.push(next.session_id.clone());
            let (set_cookie, replaced) = browser_session_cookie_update(&next, &headers);
            evicted.extend(replaced);
            headers = request_headers_from_set_cookie(&set_cookie);
        }

        let bundle = decode_browser_bundle(&headers).expect("bounded v2 bundle");
        assert_eq!(bundle.accounts.len(), MAX_BROWSER_ACCOUNTS);
        assert_eq!(bundle.active_session_id, issued[5]);
        assert!(
            !bundle
                .accounts
                .iter()
                .any(|account| account.session_id == issued[0])
        );
        assert!(evicted.contains(&Uuid::parse_str(&issued[0]).expect("first session ID")));
    }

    #[test]
    fn duplicate_principal_replacement_is_atomic_and_returns_old_session() {
        let first = session("local:duplicate");
        let (set_cookie, replaced) = browser_session_cookie_update(&first, &HashMap::new());
        assert!(replaced.is_empty());
        let headers = request_headers_from_set_cookie(&set_cookie);

        let replacement = session("local:duplicate");
        let (set_cookie, replaced) = browser_session_cookie_update(&replacement, &headers);
        let bundle = decode_browser_bundle(&request_headers_from_set_cookie(&set_cookie))
            .expect("replacement bundle");
        assert_eq!(bundle.accounts.len(), 1);
        assert_eq!(bundle.active_session_id, replacement.session_id);
        assert_eq!(
            replaced,
            vec![Uuid::parse_str(&first.session_id).expect("old session ID")]
        );
    }

    #[test]
    fn removing_active_account_chooses_next_mru_without_clearing_others() {
        let first = session("local:first");
        let second = session("local:second");
        let third = session("local:third");
        let (cookie, _) = browser_session_cookie_update(&first, &HashMap::new());
        let (cookie, _) =
            browser_session_cookie_update(&second, &request_headers_from_set_cookie(&cookie));
        let (cookie, _) =
            browser_session_cookie_update(&third, &request_headers_from_set_cookie(&cookie));
        let headers = request_headers_from_set_cookie(&cookie);

        let removed = browser_remove_session_cookie_headers(&headers, &third.session_id);
        let bundle = decode_browser_bundle(&request_headers_from_set_cookie(&removed))
            .expect("MRU fallback bundle");
        assert_eq!(bundle.accounts.len(), 2);
        assert_eq!(bundle.active_session_id, second.session_id);
        assert!(
            bundle
                .accounts
                .iter()
                .any(|account| account.session_id == first.session_id)
        );
    }

    #[test]
    fn malformed_or_oversized_v2_cookie_never_falls_back_to_legacy_material() {
        let legacy_session = Uuid::new_v4();
        for malformed in [
            "not-base64".to_owned(),
            "a".repeat(MAX_BROWSER_BUNDLE_ENCODED_BYTES + 1),
        ] {
            let headers = HashMap::from([(
                "cookie".into(),
                format!(
                    "{BROWSER_SESSION_BUNDLE_COOKIE}={malformed}; {BROWSER_SESSION_ID_COOKIE}={legacy_session}; {BROWSER_REFRESH_COOKIE}=legacy-refresh"
                ),
            )]);
            assert!(decode_browser_bundle(&headers).is_none());
            assert!(browser_refresh_request(&headers).is_none());
        }
    }
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
    use erpx_domain::support::{SupportCapability, SupportCapabilityBinding};

    const SUPPORT_DIAGNOSTICS_PERMISSION: &str = "diagnostics:read";

    #[test]
    fn local_runtime_profiles_use_the_xtask_readiness_barrier() {
        assert!(!system_readiness_required(Some("local")));
        assert!(!system_readiness_required(Some("local-fresh-state")));
        assert!(system_readiness_required(None));
        assert!(system_readiness_required(Some("production")));
        assert_eq!(
            ServerReadinessAdapter::not_probed_composition(),
            WorkerReadinessComposition {
                worker_heartbeat: ReadinessState::NotProbed,
                holding_routing: ReadinessState::NotProbed,
                projection_freshness: ReadinessState::NotProbed,
                backup_pitr: ReadinessState::NotProbed,
            }
        );
    }

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
    fn platform_holding_page_size_is_defaulted_and_bounded() {
        assert_eq!(platform_holding_page_size(0).expect("default"), 25);
        assert_eq!(
            platform_holding_page_size(MAX_HOLDING_REGISTRY_PAGE_SIZE).expect("maximum"),
            MAX_HOLDING_REGISTRY_PAGE_SIZE
        );
        assert!(platform_holding_page_size(MAX_HOLDING_REGISTRY_PAGE_SIZE + 1).is_err());
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
            "holding-opaque",
            "holding-opaque",
            "operations",
            "job-1",
            Uuid::from_u128(41),
        )
        .expect("cursor");
        let encoded = encode_projection_status_cursor(&cursor);
        assert!(!encoded.contains("holding-opaque"));
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
            projection_rebuild_name(erpx_infrastructure::HOLDING_PROJECTION_STATUS_READ_PROJECTION)
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
                SupportMode::HoldingMutation,
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
                    organization_node_id: branch_id,
                }
            )
            .is_err()
        );
    }
}

#[cfg(test)]
mod marketing_content_transport_tests {
    use super::*;

    #[test]
    fn form_values_are_bounded_string_maps() {
        let values = marketing_form_values(&[marketing_target::MarketingFormValue {
            key: "email".into(),
            value: "customer@example.com".into(),
        }])
        .expect("valid values");
        assert_eq!(
            values.get("email").map(String::as_str),
            Some("customer@example.com")
        );
        assert!(marketing_form_values(&[]).is_err());
        assert!(
            marketing_form_values(&[marketing_target::MarketingFormValue {
                key: "email".into(),
                value: "a".repeat(MAX_MARKETING_FORM_VALUE_BYTES + 1),
            }])
            .is_err()
        );
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
        let holding = marketing_abuse_binding_id("holding", "holding-ref-1");
        assert_eq!(
            holding,
            marketing_abuse_binding_id("holding", "holding-ref-1")
        );
        assert_ne!(
            holding,
            marketing_abuse_binding_id("holding", "holding-ref-2")
        );
        assert_ne!(holding, Uuid::nil());
        assert!(!format!("{holding:?}").contains("holding-ref-1"));
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
mod holding_owner_seed_tests {
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
        let provisioner = HoldingProvisioner::new(store.clone());
        let holding_id = "00000000-0000-4000-8000-000000000901";
        provisioner
            .begin(HoldingStoreRegistration {
                holding_id: holding_id.into(),
                data_profile_key: "holding-owner-transaction".into(),
                write_database_key: "holding-owner-transaction-write".into(),
                read_database_key: "holding-owner-transaction-read".into(),
                storage_namespace: "holding-owner-transaction-storage".into(),
                credential_key_ref: "holding-owner-transaction-key".into(),
                backup_chain_ref: "holding-owner-transaction-chain".into(),
                state: "provisioning".into(),
                registry_version: 1,
            })
            .await
            .expect("begin provisioning");
        let seed = provisioner
            .register_owner_seed(holding_id, "owner-transaction", "approver-transaction")
            .await
            .expect("owner seed");
        let receipt = seed_holding_owner(&store, &seed, "grantor-transaction", Utc::now())
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
        let replay = seed_holding_owner(&store, &seed, "grantor-transaction", Utc::now())
            .await
            .expect("replay owner seed");
        assert!(replay.seeded);
        assert!(replay.recovery_code.is_none());
        let read_table: Option<String> =
            sqlx::query_scalar("SELECT to_regclass('erpx_control.holding_owner_seeds')::text")
                .fetch_one(&store.pools().read)
                .await
                .expect("read isolation query");
        assert_eq!(read_table, None);
    }
}

#[cfg(test)]
mod directory_scope_tests {
    use super::*;

    #[test]
    fn directory_scope_accepts_platform_and_matching_opaque_holding_only() {
        let data_profile_key = "holding-ref-42";
        let holding_scope = DirectoryScope::Holding {
            data_profile_key: data_profile_key.into(),
        };
        let holding_id = holding_scope
            .holding_id()
            .expect("holding scope has stable identity");

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
                data_profile_key: "holding-ref-42".into()
            }
        );
        assert_eq!(
            parse_directory_scope("holding-ref-42", "holding-ref-42").expect("legacy"),
            DirectoryScope::Holding {
                data_profile_key: "holding-ref-42".into()
            }
        );
        assert!(parse_directory_scope("holding:other", "holding-ref-42").is_err());
    }
}

#[cfg(test)]
mod phase_one_subtree_ingress_tests {
    use super::*;

    #[test]
    fn exact_subtree_requires_an_explicit_pilot_opt_in() {
        let subtree = Scope::Subtree {
            holding_id: Uuid::from_u128(1),
            organization_node_id: Uuid::from_u128(4),
        };
        assert!(!exact_subtree_ingress_allowed(&subtree, false));
        assert!(exact_subtree_ingress_allowed(&subtree, true));
        assert!(exact_subtree_ingress_allowed(
            &Scope::Company {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2),
            },
            false,
        ));
    }

    #[test]
    fn custom_ingresses_use_default_deny_and_only_pilot_paths_opt_in() {
        let wave9 = include_str!("wave9_composition.rs");
        let customer_360 = wave9
            .split("impl Customer360RequestAuthenticator")
            .nth(1)
            .and_then(|tail| tail.split("fn map_operations_auth").next())
            .expect("Customer 360 authenticator");
        assert!(customer_360.contains(".authorize_role("));
        assert!(!customer_360.contains("with_exact_subtree"));

        let target = include_str!("target_transport.rs");
        let stream = target
            .split("impl DataPlaneStreamService")
            .nth(1)
            .and_then(|tail| tail.split("fn readiness_state").next())
            .expect("data-plane stream ingress");
        assert!(stream.contains(".authorize_role_for_holding("));
        assert!(!stream.contains("with_exact_subtree"));

        let pilot = target
            .split("async fn authorize_known_holding_request")
            .nth(1)
            .and_then(|tail| tail.split("fn collaboration_action").next())
            .expect("Collaboration/Item Directory pilot authorization");
        assert!(pilot.contains("authorize_role_for_holding_with_exact_subtree"));

        let wave4 = include_str!("wave4_composition.rs");
        let organization = wave4
            .split("async fn authenticate_context_with_permissions")
            .nth(1)
            .and_then(|tail| tail.split("fn legacy_command_context").next())
            .expect("Organization context authenticator");
        assert!(organization.contains("product == \"organization\""));
        assert!(organization.contains("authorize_role_for_holding_with_exact_subtree"));
    }
}
