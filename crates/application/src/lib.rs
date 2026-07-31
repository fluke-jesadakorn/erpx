#![doc = "ERPX use cases and ports. The application layer owns orchestration, not persistence or transport."]

pub mod marketing_privacy;
pub mod marketing_provider_onboarding;
pub mod support_workflow;

pub use marketing_privacy::{
    AuthorizedMarketingProfilePrivacyPort, EraseMarketingProfileCommand,
    MarketingProfileErasureCorrelation, MarketingProfileErasureReceipt,
    MarketingProfilePrivacyFailure, MarketingProfilePrivacyPort,
    authorize_and_erase_marketing_profile, redacted_marketing_profile_privacy_error,
};
pub use marketing_provider_onboarding::{
    AcknowledgeMarketingProviderVerificationCommand, ConfigureMarketingProviderAccountCommand,
    DEFAULT_MARKETING_PROVIDER_PRIORITY, MARKETING_PROVIDER_CHANNELS,
    MAX_MARKETING_PROVIDER_CHANNELS, MAX_MARKETING_PROVIDER_PRIORITY,
    MAX_MARKETING_PROVIDER_REFERENCE_BYTES, MAX_MARKETING_PROVIDER_ROUTE_BYTES,
    MarketingProviderAccountWriteReceipt, MarketingProviderChannelRoute,
    MarketingProviderOnboardingError, SetMarketingProviderAccountStateCommand,
};
pub use support_workflow::{
    PersistApprovedSupportGrantCommand, RevokeSupportGrantCommand,
    SUPPORT_WORKFLOW_HIGH_RISK_MODES, SupportWorkflowPort, SupportWorkflowReceipt,
    SupportWorkflowService,
};

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
pub use erpx_domain::customer::{
    CustomerAccessAuditSummaryReadDto, CustomerCommercialReadDto, CustomerContactsReadDto,
    CustomerEntitlementsReadDto, CustomerGlobalReadAuditDetailReadDto, CustomerHealthReadDto,
    CustomerNote, CustomerNoteError, CustomerNoteReadDto, CustomerNotesReadDto,
    CustomerOnboardingReadDto, CustomerOrganizationReadDto, CustomerReadFreshness,
    CustomerReadMetadata, CustomerReadView, CustomerSupportSummaryReadDto, GlobalReadActor,
    GlobalReadScope,
};
pub use erpx_domain::support::{
    CustomerVisibleTicket, CustomerVisibleTicketMessage, SupportDomainPolicy,
    SupportIntersectionDenial, SupportIntersectionRequest, SupportMessageAudience,
    SupportTicketAssignment, SupportTicketCommand, SupportTicketEscalation, SupportTicketMessage,
    evaluate_support_intersection,
};
use erpx_domain::{
    access::{
        AssignmentError, RoleAssignment, RoleRecoveryRequest, RoleReviewQueue, Scope,
        build_role_review_queue,
    },
    commercial::{BillingState, EntitlementDecision, ModuleEntitlement, authorize_module},
    contracts::{CommandContext, TenantScope},
    cqrs::OutboxEvent,
    customer::Customer360,
    marketing_content::{
        ContentForm, ContentFormRef, ContentTemplateRef, CustomDomain, CustomDomainBinding,
        MarketingContentError, PreviewGrant,
    },
    support::{SupportError, SupportSession, SupportTicket},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("authorization denied: {0}")]
    Authorization(String),
    #[error(transparent)]
    Assignment(#[from] AssignmentError),
    #[error(transparent)]
    RoleReview(#[from] erpx_domain::access::RoleReviewError),
    #[error(transparent)]
    Support(#[from] SupportError),
    #[error("repository error: {0}")]
    Repository(String),
}

const MARKETING_CONTENT_HASH_BYTES: usize = 64;
const MARKETING_CONTENT_MAX_SUBMISSION_VALUES: usize = 128;
const MARKETING_CONTENT_MAX_VALUE_BYTES: usize = 16 * 1024;

/// The transport-neutral result of an authoritative Marketing content write.
/// It contains only stable application data; SQLx transactions, rows, and
/// persistence error types must not cross this port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingContentWriteReceipt {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub version: u64,
    pub idempotency_key: String,
    pub event_id: Uuid,
    pub replayed: bool,
}

/// Application commands for the Marketing content write port. Constructors
/// validate shape, bounded inputs, and domain invariants available from the
/// supplied snapshot. Authorization, tenant routing, entitlement, current
/// aggregate state, and persistence policy remain at the authoritative
/// boundary that implements [`MarketingContentWritePort`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitPublishedFormCommand {
    pub form: ContentFormRef,
    pub submission_id: Uuid,
    pub values: std::collections::BTreeMap<String, String>,
    pub received_at: DateTime<Utc>,
    pub context: CommandContext,
}

impl SubmitPublishedFormCommand {
    /// Builds a submission from an authoritative published-form snapshot.
    /// The snapshot check is a domain invariant, not an authorization check;
    /// the write adapter must still re-read authoritative state before commit.
    pub fn from_published_form(
        form: &ContentForm,
        submission_id: Uuid,
        values: std::collections::BTreeMap<String, String>,
        received_at: DateTime<Utc>,
        context: CommandContext,
    ) -> Result<Self, ApplicationError> {
        let form_ref = form.reference().map_err(marketing_command_error)?;
        form.validate_submission(&form_ref, &values)
            .map_err(marketing_command_error)?;
        Self::new(form_ref, submission_id, values, received_at, context)
    }

    /// Builds a submission when the authoritative boundary will resolve the
    /// published form itself. This does not claim that the referenced form is
    /// currently published.
    pub fn new(
        form: ContentFormRef,
        submission_id: Uuid,
        values: std::collections::BTreeMap<String, String>,
        received_at: DateTime<Utc>,
        context: CommandContext,
    ) -> Result<Self, ApplicationError> {
        form_reference_is_valid(&form)?;
        identifier_is_present(submission_id, "submission_id")?;
        bounded_submission_values(&values)?;
        command_context_is_valid(&context)?;
        Ok(Self {
            form,
            submission_id,
            values,
            received_at,
            context,
        })
    }

    pub fn idempotency_key(&self) -> &str {
        &self.context.idempotency_key
    }

    pub fn correlation_id(&self) -> Uuid {
        self.context.correlation_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuePreviewCommand {
    pub preview: PreviewGrant,
    /// A SHA-256 digest produced by the credential boundary. Raw bearer
    /// tokens are intentionally not represented by this application command.
    pub token_hash: String,
    pub context: CommandContext,
}

impl IssuePreviewCommand {
    pub fn new(
        preview: PreviewGrant,
        token_hash: impl Into<String>,
        context: CommandContext,
    ) -> Result<Self, ApplicationError> {
        preview.validate().map_err(marketing_command_error)?;
        let token_hash = sha256_hex(token_hash.into(), "preview token hash")?;
        command_context_is_valid(&context)?;
        Ok(Self {
            preview,
            token_hash,
            context,
        })
    }

    pub fn idempotency_key(&self) -> &str {
        &self.context.idempotency_key
    }

    pub fn correlation_id(&self) -> Uuid {
        self.context.correlation_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevokePreviewCommand {
    pub preview_id: Uuid,
    pub scope: TenantScope,
    pub revoked_at: DateTime<Utc>,
    pub context: CommandContext,
}

impl RevokePreviewCommand {
    pub fn new(
        preview_id: Uuid,
        scope: TenantScope,
        revoked_at: DateTime<Utc>,
        context: CommandContext,
    ) -> Result<Self, ApplicationError> {
        identifier_is_present(preview_id, "preview_id")?;
        tenant_scope_is_valid(&scope)?;
        command_context_is_valid(&context)?;
        Ok(Self {
            preview_id,
            scope,
            revoked_at,
            context,
        })
    }

    pub fn idempotency_key(&self) -> &str {
        &self.context.idempotency_key
    }

    pub fn correlation_id(&self) -> Uuid {
        self.context.correlation_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpirePreviewCommand {
    pub preview_id: Uuid,
    pub scope: TenantScope,
    pub expired_at: DateTime<Utc>,
    pub context: CommandContext,
}

impl ExpirePreviewCommand {
    pub fn new(
        preview_id: Uuid,
        scope: TenantScope,
        expired_at: DateTime<Utc>,
        context: CommandContext,
    ) -> Result<Self, ApplicationError> {
        identifier_is_present(preview_id, "preview_id")?;
        tenant_scope_is_valid(&scope)?;
        command_context_is_valid(&context)?;
        Ok(Self {
            preview_id,
            scope,
            expired_at,
            context,
        })
    }

    pub fn idempotency_key(&self) -> &str {
        &self.context.idempotency_key
    }

    pub fn correlation_id(&self) -> Uuid {
        self.context.correlation_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueDomainVerificationCommand {
    pub domain_id: Uuid,
    pub scope: TenantScope,
    /// A SHA-256 digest produced by the DNS/HTTP challenge boundary. Raw
    /// challenge material is intentionally not represented here.
    pub verification_hash: String,
    pub context: CommandContext,
}

impl IssueDomainVerificationCommand {
    pub fn new(
        domain_id: Uuid,
        scope: TenantScope,
        verification_hash: impl Into<String>,
        context: CommandContext,
    ) -> Result<Self, ApplicationError> {
        identifier_is_present(domain_id, "custom_domain_id")?;
        tenant_scope_is_valid(&scope)?;
        let verification_hash = sha256_hex(verification_hash.into(), "domain verification hash")?;
        command_context_is_valid(&context)?;
        Ok(Self {
            domain_id,
            scope,
            verification_hash,
            context,
        })
    }

    pub fn idempotency_key(&self) -> &str {
        &self.context.idempotency_key
    }

    pub fn correlation_id(&self) -> Uuid {
        self.context.correlation_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyDomainCommand {
    pub domain_id: Uuid,
    pub scope: TenantScope,
    /// A SHA-256 digest of the observed challenge proof. Raw proof material
    /// is intentionally not represented here.
    pub proof_hash: String,
    pub context: CommandContext,
}

impl VerifyDomainCommand {
    pub fn new(
        domain_id: Uuid,
        scope: TenantScope,
        proof_hash: impl Into<String>,
        context: CommandContext,
    ) -> Result<Self, ApplicationError> {
        identifier_is_present(domain_id, "custom_domain_id")?;
        tenant_scope_is_valid(&scope)?;
        let proof_hash = sha256_hex(proof_hash.into(), "domain verification proof hash")?;
        command_context_is_valid(&context)?;
        Ok(Self {
            domain_id,
            scope,
            proof_hash,
            context,
        })
    }

    pub fn idempotency_key(&self) -> &str {
        &self.context.idempotency_key
    }

    pub fn correlation_id(&self) -> Uuid {
        self.context.correlation_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindDomainCommand {
    pub binding: CustomDomainBinding,
    pub context: CommandContext,
}

impl BindDomainCommand {
    pub fn new(
        binding: CustomDomainBinding,
        context: CommandContext,
    ) -> Result<Self, ApplicationError> {
        binding_is_valid(&binding)?;
        command_context_is_valid(&context)?;
        Ok(Self { binding, context })
    }

    /// Creates the binding through the domain rule so a caller cannot build a
    /// cross-scope binding merely by assembling application DTO fields.
    pub fn from_domain(
        domain: &CustomDomain,
        template: ContentTemplateRef,
        bound_at: DateTime<Utc>,
        context: CommandContext,
    ) -> Result<Self, ApplicationError> {
        domain.validate().map_err(marketing_command_error)?;
        let binding = domain
            .bind_template(&template, bound_at)
            .map_err(marketing_command_error)?;
        Self::new(binding, context)
    }

    pub fn idempotency_key(&self) -> &str {
        &self.context.idempotency_key
    }

    pub fn correlation_id(&self) -> Uuid {
        self.context.correlation_id
    }
}

/// Compatibility aliases make the same contract discoverable to adapters
/// that name inbound values as requests rather than commands.
pub type SubmitPublishedFormRequest = SubmitPublishedFormCommand;
pub type IssuePreviewRequest = IssuePreviewCommand;
pub type RevokePreviewRequest = RevokePreviewCommand;
pub type ExpirePreviewRequest = ExpirePreviewCommand;
pub type IssueDomainVerificationRequest = IssueDomainVerificationCommand;
pub type VerifyDomainRequest = VerifyDomainCommand;
pub type BindDomainRequest = BindDomainCommand;

/// Transport-neutral orchestration port for Marketing content writes.
/// Implementations own authoritative authorization/context enforcement,
/// tenant routing, aggregate re-read, transactionality, idempotency, and
/// audit/outbox persistence. This trait intentionally exposes no database,
/// object-storage, or transport types.
#[async_trait]
pub trait MarketingContentWritePort: Send + Sync {
    async fn submit_published_form(
        &self,
        command: SubmitPublishedFormCommand,
    ) -> Result<MarketingContentWriteReceipt, ApplicationError>;

    async fn issue_preview(
        &self,
        command: IssuePreviewCommand,
    ) -> Result<MarketingContentWriteReceipt, ApplicationError>;

    async fn revoke_preview(
        &self,
        command: RevokePreviewCommand,
    ) -> Result<MarketingContentWriteReceipt, ApplicationError>;

    async fn expire_preview(
        &self,
        command: ExpirePreviewCommand,
    ) -> Result<MarketingContentWriteReceipt, ApplicationError>;

    async fn issue_domain_verification(
        &self,
        command: IssueDomainVerificationCommand,
    ) -> Result<MarketingContentWriteReceipt, ApplicationError>;

    async fn verify_domain(
        &self,
        command: VerifyDomainCommand,
    ) -> Result<MarketingContentWriteReceipt, ApplicationError>;

    async fn bind_domain(
        &self,
        command: BindDomainCommand,
    ) -> Result<MarketingContentWriteReceipt, ApplicationError>;
}

fn marketing_command_error(error: MarketingContentError) -> ApplicationError {
    ApplicationError::Repository(format!("invalid marketing content command: {error}"))
}

fn command_context_is_valid(context: &CommandContext) -> Result<(), ApplicationError> {
    context
        .validate()
        .map_err(|error| ApplicationError::Repository(format!("invalid command context: {error}")))
}

fn identifier_is_present(value: Uuid, field: &'static str) -> Result<(), ApplicationError> {
    if value.is_nil() {
        return Err(ApplicationError::Repository(format!(
            "invalid marketing content command: {field} cannot be nil"
        )));
    }
    Ok(())
}

fn tenant_scope_is_valid(scope: &TenantScope) -> Result<(), ApplicationError> {
    // The domain exposes validation through its tenant-bound references rather
    // than a public free function. A synthetic reference keeps this check
    // structural without adding authorization policy to the application port.
    ContentFormRef::new(Uuid::from_u128(1), scope.clone())
        .map(|_| ())
        .map_err(marketing_command_error)
}

fn form_reference_is_valid(form: &ContentFormRef) -> Result<(), ApplicationError> {
    ContentFormRef::new(form.form_id, form.scope.clone())
        .map(|_| ())
        .map_err(marketing_command_error)
}

fn bounded_submission_values(
    values: &std::collections::BTreeMap<String, String>,
) -> Result<(), ApplicationError> {
    if values.len() > MARKETING_CONTENT_MAX_SUBMISSION_VALUES {
        return Err(ApplicationError::Repository(
            "invalid marketing content command: form values exceed the bounded maximum".into(),
        ));
    }
    if values
        .values()
        .any(|value| value.len() > MARKETING_CONTENT_MAX_VALUE_BYTES)
    {
        return Err(ApplicationError::Repository(
            "invalid marketing content command: form value exceeds the bounded maximum".into(),
        ));
    }
    Ok(())
}

fn sha256_hex(value: String, field: &'static str) -> Result<String, ApplicationError> {
    if value.len() != MARKETING_CONTENT_HASH_BYTES
        || !value.chars().all(|character| character.is_ascii_hexdigit())
    {
        return Err(ApplicationError::Repository(format!(
            "invalid marketing content command: {field} must be a SHA-256 hex digest"
        )));
    }
    Ok(value.to_ascii_lowercase())
}

fn binding_is_valid(binding: &CustomDomainBinding) -> Result<(), ApplicationError> {
    identifier_is_present(binding.domain_id, "custom_domain_id")?;
    tenant_scope_is_valid(&binding.domain_scope)?;
    binding
        .template
        .validate()
        .map_err(marketing_command_error)?;
    if !binding.domain_scope.contains(&binding.template.scope) {
        return Err(marketing_command_error(
            MarketingContentError::DomainScopeMismatch,
        ));
    }
    Ok(())
}

#[async_trait]
pub trait CustomerReadStore: Send + Sync {
    async fn list_customers(
        &self,
        search: Option<&str>,
    ) -> Result<Vec<Customer360>, ApplicationError>;
    async fn get_customer(&self, id: Uuid) -> Result<Option<Customer360>, ApplicationError>;
}

/// Read-only secondary views for the platform Customer 360 projection. The
/// port intentionally returns a typed view envelope so transport adapters can
/// distinguish absent data, stale-but-usable projections, and views that have
/// no corresponding control read model yet. Authorization is outside this
/// read port.
#[async_trait]
pub trait Customer360DetailReadStore: Send + Sync {
    async fn get_customer_organization(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerOrganizationReadDto>, ApplicationError>;
    async fn get_customer_contacts(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerContactsReadDto>, ApplicationError>;
    async fn get_customer_commercial(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerCommercialReadDto>, ApplicationError>;
    async fn get_customer_entitlements(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerEntitlementsReadDto>, ApplicationError>;
    async fn get_customer_onboarding(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerOnboardingReadDto>, ApplicationError>;
    async fn get_customer_health(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerHealthReadDto>, ApplicationError>;
    async fn get_customer_support_summary(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerSupportSummaryReadDto>, ApplicationError>;
    async fn get_customer_access_audit_summary(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerAccessAuditSummaryReadDto>, ApplicationError>;
    async fn get_customer_notes(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerNotesReadDto>, ApplicationError>;
}

/// Authenticated, append-only Customer 360 note command. The scope is carried
/// into the durable event so the write adapter can enforce customer binding
/// before the aggregate/outbox transaction commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddCustomerNoteCommand {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub note: CustomerNote,
    pub purpose: String,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerNoteWriteReceipt {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub event_id: Uuid,
    pub idempotency_key: String,
    pub replayed: bool,
}

impl AddCustomerNoteCommand {
    pub fn new(
        command_id: Uuid,
        idempotency_key: impl Into<String>,
        note: CustomerNote,
        purpose: impl Into<String>,
        correlation_id: Uuid,
    ) -> Result<Self, ApplicationError> {
        let idempotency_key = idempotency_key.into();
        let purpose = purpose.into();
        if command_id.is_nil() || correlation_id.is_nil() {
            return Err(ApplicationError::Repository(
                "customer note command IDs are required".into(),
            ));
        }
        if idempotency_key.trim().is_empty() || idempotency_key.len() > 255 {
            return Err(ApplicationError::Repository(
                "customer note idempotency key is invalid".into(),
            ));
        }
        if purpose.trim().is_empty() || purpose.chars().any(char::is_control) {
            return Err(ApplicationError::Repository(
                "customer note purpose is invalid".into(),
            ));
        }
        // Re-run the domain validation at the command boundary so a command
        // cannot be assembled from an unchecked deserialized note.
        CustomerNote::new(
            note.id,
            note.customer_id,
            note.author_ref.clone(),
            note.scope.clone(),
            note.created_at,
            note.body.clone(),
        )
        .map_err(|error| ApplicationError::Repository(error.to_string()))?;
        Ok(Self {
            command_id,
            idempotency_key,
            note,
            purpose,
            correlation_id,
        })
    }

    /// Build the generic command mutation used by the SQLx control-plane
    /// adapter. Aggregate state, event payload, audit attribution, scope, and
    /// idempotency are intentionally derived from the same validated note.
    pub fn into_mutation(&self) -> Result<CommandMutation, ApplicationError> {
        let payload = serde_json::to_value(&self.note)
            .map_err(|error| ApplicationError::Repository(error.to_string()))?;
        let request_material = serde_json::json!({
            "aggregate_type": "customer_note",
            "aggregate_id": self.note.id,
            "note": payload.clone(),
            "author_ref": self.note.author_ref.clone(),
            "scope": &self.note.scope,
            "purpose": self.purpose,
            "correlation_id": self.correlation_id,
        });
        let request_hash = hex::encode(Sha256::digest(
            serde_json::to_vec(&request_material)
                .map_err(|error| ApplicationError::Repository(error.to_string()))?,
        ));
        let event_id = Uuid::new_v4();
        let response = CustomerNoteWriteReceipt {
            aggregate_type: "customer_note".into(),
            aggregate_id: self.note.id,
            aggregate_version: 1,
            event_id,
            idempotency_key: self.idempotency_key.clone(),
            replayed: false,
        };
        Ok(CommandMutation {
            command_id: self.command_id,
            idempotency_key: self.idempotency_key.clone(),
            request_hash,
            aggregate_type: "customer_note".into(),
            aggregate_id: self.note.id,
            expected_aggregate_version: None,
            aggregate_state: payload.clone(),
            event: OutboxEvent {
                id: event_id,
                aggregate_type: "customer_note".into(),
                aggregate_id: self.note.id,
                aggregate_version: 1,
                event_type: "customer.note_added".into(),
                payload,
                occurred_at: self.note.created_at,
            },
            event_version: 1,
            tenant_scope: Some(
                serde_json::to_value(&self.note.scope)
                    .map_err(|error| ApplicationError::Repository(error.to_string()))?,
            ),
            actor: Some(serde_json::json!({
                "principal_ref": self.note.author_ref,
            })),
            causation_id: None,
            audit: AuditEvent {
                actor_ref: self.note.author_ref.clone(),
                customer_account_id: Some(self.note.customer_id),
                action: "customer_note_added".into(),
                purpose: self.purpose.clone(),
                correlation_id: self.correlation_id,
            },
            response: serde_json::to_value(response)
                .map_err(|error| ApplicationError::Repository(error.to_string()))?,
        })
    }
}

#[async_trait]
pub trait Customer360NoteWritePort: Send + Sync {
    async fn append_customer_note(
        &self,
        command: AddCustomerNoteCommand,
    ) -> Result<CustomerNoteWriteReceipt, ApplicationError>;
}

#[derive(Debug, Clone)]
pub struct Customer360NoteService<W> {
    writer: Arc<W>,
}

impl<W> Customer360NoteService<W>
where
    W: Customer360NoteWritePort + 'static,
{
    pub fn new(writer: Arc<W>) -> Self {
        Self { writer }
    }

    pub async fn append(
        &self,
        command: AddCustomerNoteCommand,
    ) -> Result<CustomerNoteWriteReceipt, ApplicationError> {
        self.writer.append_customer_note(command).await
    }
}

/// Transport-neutral application seam for the authoritative support
/// intersection. Main/server adapters resolve current identity, role,
/// capability, entitlement, and represented-user state, then call this pure
/// evaluator before dispatching any support action.
#[derive(Debug, Default, Clone, Copy)]
pub struct SupportIntersectionService;

impl SupportIntersectionService {
    pub fn authorize(
        &self,
        request: &SupportIntersectionRequest<'_>,
    ) -> Result<(), SupportIntersectionDenial> {
        evaluate_support_intersection(request)
    }
}

pub const CUSTOMER_GLOBAL_READ_AUDIT_MAX_PAGE_SIZE: u32 = 100;

/// The customer security administrator's requested customer scope is explicit
/// in the query contract. This port carries scope for the authoritative
/// authorization boundary; the read repository does not make that decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerSecurityAdminScope {
    pub customer_id: Uuid,
    /// `true` is reserved for the platform-console scope. Tenant security
    /// administrators always carry an explicit holding/company/branch scope.
    pub platform: bool,
    pub holding_id: Option<Uuid>,
    pub company_id: Option<Uuid>,
    pub branch_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerGlobalReadAuditQueryContext {
    pub requester: GlobalReadActor,
    pub purpose: String,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerGlobalReadAuditDetailCursor {
    pub occurred_at: DateTime<Utc>,
    pub audit_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerGlobalReadAuditDetailQuery {
    pub customer_id: Uuid,
    pub security_admin_scope: CustomerSecurityAdminScope,
    pub audit: CustomerGlobalReadAuditQueryContext,
    pub before: Option<CustomerGlobalReadAuditDetailCursor>,
    pub limit: u32,
}

impl CustomerGlobalReadAuditDetailQuery {
    pub fn validate(&self) -> Result<(), String> {
        if self.customer_id.is_nil() {
            return Err("customer ID is required".into());
        }
        if self.security_admin_scope.customer_id != self.customer_id {
            return Err("security administrator scope does not match customer".into());
        }
        if self.security_admin_scope.platform
            && (self.security_admin_scope.holding_id.is_some()
                || self.security_admin_scope.company_id.is_some()
                || self.security_admin_scope.branch_id.is_some())
        {
            return Err("platform scope cannot carry organization identifiers".into());
        }
        if !self.security_admin_scope.platform {
            let valid_tenant_scope = matches!(
                (
                    self.security_admin_scope.holding_id,
                    self.security_admin_scope.company_id,
                    self.security_admin_scope.branch_id,
                ),
                (Some(_), None, None) | (Some(_), Some(_), None) | (Some(_), Some(_), Some(_))
            );
            if !valid_tenant_scope {
                return Err("tenant security scope is incomplete".into());
            }
        }
        if self.audit.requester.principal_ref.trim().is_empty() {
            return Err("requester principal is required".into());
        }
        if self.audit.purpose.trim().is_empty() {
            return Err("audit purpose is required".into());
        }
        if self.audit.correlation_id.is_nil() {
            return Err("audit correlation ID is required".into());
        }
        Ok(())
    }

    pub fn bounded_limit(&self) -> u32 {
        self.limit
            .clamp(1, CUSTOMER_GLOBAL_READ_AUDIT_MAX_PAGE_SIZE)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerGlobalReadAuditDetailPage {
    pub entries: Vec<CustomerGlobalReadAuditDetailReadDto>,
    pub next_before: Option<CustomerGlobalReadAuditDetailCursor>,
}

/// Read port for customer-security audit detail. Callers should use
/// `CustomerGlobalReadAuditDetailService` so the query itself is recorded in
/// the application audit sink before the source records are returned.
#[async_trait]
pub trait CustomerGlobalReadAuditDetailStore: Send + Sync {
    async fn get_customer_global_read_audit_details(
        &self,
        query: &CustomerGlobalReadAuditDetailQuery,
    ) -> Result<CustomerReadView<CustomerGlobalReadAuditDetailPage>, ApplicationError>;
}

pub struct CustomerGlobalReadAuditDetailService<R> {
    reader: Arc<R>,
    audit: Arc<dyn AuditSink>,
}

impl<R> CustomerGlobalReadAuditDetailService<R>
where
    R: CustomerGlobalReadAuditDetailStore + 'static,
{
    pub fn new(reader: Arc<R>, audit: Arc<dyn AuditSink>) -> Self {
        Self { reader, audit }
    }

    pub async fn list(
        &self,
        query: CustomerGlobalReadAuditDetailQuery,
    ) -> Result<CustomerReadView<CustomerGlobalReadAuditDetailPage>, ApplicationError> {
        query.validate().map_err(|error| {
            ApplicationError::Repository(format!("invalid audit query: {error}"))
        })?;
        self.audit
            .record(AuditEvent {
                actor_ref: query.audit.requester.principal_ref.clone(),
                customer_account_id: Some(query.customer_id),
                action: "customer_global_read_audit_detail".into(),
                purpose: query.audit.purpose.clone(),
                correlation_id: query.audit.correlation_id,
            })
            .await?;
        self.reader
            .get_customer_global_read_audit_details(&query)
            .await
    }
}

#[async_trait]
pub trait SupportStore: Send + Sync {
    async fn create_ticket(&self, ticket: SupportTicket) -> Result<(), ApplicationError>;
    async fn get_ticket(&self, id: Uuid) -> Result<Option<SupportTicket>, ApplicationError>;
    async fn save_ticket(&self, ticket: SupportTicket) -> Result<(), ApplicationError>;
    async fn create_session(&self, session: SupportSession) -> Result<(), ApplicationError>;
    async fn get_session(&self, id: Uuid) -> Result<Option<SupportSession>, ApplicationError>;
    async fn save_session(&self, session: SupportSession) -> Result<(), ApplicationError>;
}

#[async_trait]
pub trait RoleAssignmentStore: Send + Sync {
    async fn create(&self, assignment: RoleAssignment) -> Result<(), ApplicationError>;
    async fn get(&self, id: Uuid) -> Result<Option<RoleAssignment>, ApplicationError>;
    async fn list(&self, limit: u32) -> Result<Vec<RoleAssignment>, ApplicationError>;
}

#[async_trait]
pub trait AuditSink: Send + Sync {
    async fn record(&self, event: AuditEvent) -> Result<(), ApplicationError>;
}

/// The application-level command mutation passed to a transactional unit of
/// work. It deliberately contains only stable domain values and JSON context;
/// persistence-specific transaction types remain in infrastructure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandMutation {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub request_hash: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub expected_aggregate_version: Option<u64>,
    pub aggregate_state: Value,
    pub event: OutboxEvent,
    pub event_version: u32,
    pub tenant_scope: Option<Value>,
    pub actor: Option<Value>,
    pub causation_id: Option<Uuid>,
    pub audit: AuditEvent,
    pub response: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandReceipt {
    pub command_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub event_id: Uuid,
    pub response: Value,
    pub replayed: bool,
}

#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn execute_command(
        &self,
        mutation: CommandMutation,
    ) -> Result<CommandReceipt, ApplicationError>;
}

#[derive(Debug, Clone)]
pub struct ApplicationCommandService<U> {
    unit_of_work: Arc<U>,
}

impl<U> ApplicationCommandService<U>
where
    U: CommandUnitOfWork + 'static,
{
    pub fn new(unit_of_work: Arc<U>) -> Self {
        Self { unit_of_work }
    }

    pub async fn execute(
        &self,
        mutation: CommandMutation,
    ) -> Result<CommandReceipt, ApplicationError> {
        self.unit_of_work.execute_command(mutation).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub actor_ref: String,
    pub customer_account_id: Option<Uuid>,
    pub action: String,
    pub purpose: String,
    pub correlation_id: Uuid,
}

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub struct PlatformCustomerService<R> {
    reader: Arc<R>,
    audit: Arc<dyn AuditSink>,
}

impl<R> PlatformCustomerService<R>
where
    R: CustomerReadStore + 'static,
{
    pub fn new(reader: Arc<R>, audit: Arc<dyn AuditSink>) -> Self {
        Self { reader, audit }
    }

    pub async fn list(&self, search: Option<&str>) -> Result<Vec<Customer360>, ApplicationError> {
        self.reader.list_customers(search).await
    }

    pub async fn inspect(
        &self,
        actor_ref: &str,
        customer_id: Uuid,
    ) -> Result<Customer360, ApplicationError> {
        let customer = self
            .reader
            .get_customer(customer_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        self.audit
            .record(AuditEvent {
                actor_ref: actor_ref.to_string(),
                customer_account_id: Some(customer_id),
                action: "global_read".into(),
                purpose: "platform_customer_console".into(),
                correlation_id: Uuid::new_v4(),
            })
            .await?;
        Ok(customer)
    }
}

pub struct SupportService<S, C> {
    store: Arc<S>,
    clock: Arc<C>,
    audit: Arc<dyn AuditSink>,
}

/// Support command orchestration that writes through the application command
/// unit of work. The legacy `SupportService` remains available to isolated
/// unit tests and read-only paths while production command handlers use this
/// adapter so aggregate state, outbox, idempotency, and audit commit together.
pub struct AuthoritativeSupportService<S, C, U> {
    store: Arc<S>,
    clock: Arc<C>,
    commands: Arc<U>,
}

impl<S, C, U> AuthoritativeSupportService<S, C, U>
where
    S: SupportStore + 'static,
    C: Clock + 'static,
    U: CommandUnitOfWork + 'static,
{
    pub fn new(store: Arc<S>, clock: Arc<C>, commands: Arc<U>) -> Self {
        Self {
            store,
            clock,
            commands,
        }
    }

    pub async fn open_ticket(
        &self,
        context: &CommandContext,
        customer_account_id: Uuid,
        severity: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<SupportTicket, ApplicationError> {
        let ticket = SupportTicket {
            id: stable_support_ticket_id(context, "new", customer_account_id, None),
            customer_account_id,
            status: erpx_domain::support::TicketStatus::New,
            severity: severity.into(),
            description: description.into(),
            recurrence_of: None,
            messages: Vec::new(),
            assignment: None,
            escalation: None,
            sla_deadline: None,
        };
        let receipt = execute_support_command(
            &self.commands,
            context,
            "support_ticket",
            ticket.id,
            1,
            "support_ticket.created",
            &ticket,
            customer_account_id,
            "support_ticket_created",
        )
        .await?;
        if receipt.replayed {
            return self
                .store
                .get_ticket(receipt.aggregate_id)
                .await?
                .ok_or_else(|| {
                    ApplicationError::Repository(
                        "replayed support ticket command has no rehydratable aggregate".into(),
                    )
                });
        }
        Ok(ticket)
    }

    /// Create a new support aggregate linked to a closed predecessor. The
    /// predecessor remains terminal and the new ticket is emitted through the
    /// same authoritative command/outbox unit of work as ordinary creation.
    pub async fn open_ticket_recurrence(
        &self,
        context: &CommandContext,
        closed_ticket_id: Uuid,
        severity: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<SupportTicket, ApplicationError> {
        let closed = self
            .store
            .get_ticket(closed_ticket_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        let ticket = closed.recurrence(
            stable_support_ticket_id(
                context,
                "recurrence",
                closed.customer_account_id,
                Some(closed.id),
            ),
            severity,
            description,
        )?;
        let receipt = execute_support_command(
            &self.commands,
            context,
            "support_ticket",
            ticket.id,
            1,
            "support_ticket.recurrence_created",
            &ticket,
            ticket.customer_account_id,
            "support_ticket_recurrence_created",
        )
        .await?;
        if receipt.replayed {
            return self
                .store
                .get_ticket(receipt.aggregate_id)
                .await?
                .ok_or_else(|| {
                    ApplicationError::Repository(
                        "replayed support ticket command has no rehydratable aggregate".into(),
                    )
                });
        }
        Ok(ticket)
    }

    /// Advance a support ticket through the frozen lifecycle using the same
    /// authoritative command/outbox unit of work as ticket creation.
    pub async fn transition_ticket(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        next_status: erpx_domain::support::TicketStatus,
    ) -> Result<SupportTicket, ApplicationError> {
        self.apply_ticket_command(
            context,
            ticket_id,
            SupportTicketCommand::Transition { next_status },
        )
        .await
    }

    /// Apply a support-ticket metadata or lifecycle command through the
    /// authoritative command unit of work. The aggregate is validated before
    /// the command is emitted, so the persisted JSON payload and outbox event
    /// carry the same audience-separated state.
    pub async fn apply_ticket_command(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        command: SupportTicketCommand,
    ) -> Result<SupportTicket, ApplicationError> {
        let mut ticket = self
            .store
            .get_ticket(ticket_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        let event_type = command.event_type();
        let action = command.audit_action();
        ticket.apply_command(command, &context.tenant_scope, self.clock.now())?;
        let aggregate_version = next_command_version(context)?;
        execute_support_command(
            &self.commands,
            context,
            "support_ticket",
            ticket.id,
            aggregate_version,
            event_type,
            &ticket,
            ticket.customer_account_id,
            action,
        )
        .await?;
        Ok(ticket)
    }

    pub async fn add_customer_reply(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        body: impl Into<String>,
    ) -> Result<SupportTicket, ApplicationError> {
        self.apply_ticket_command(
            context,
            ticket_id,
            SupportTicketCommand::CustomerReply {
                message_id: Uuid::new_v4(),
                author_ref: context.actor.principal_ref.clone(),
                body: body.into(),
                created_at: self.clock.now(),
            },
        )
        .await
    }

    pub async fn add_internal_note(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        body: impl Into<String>,
    ) -> Result<SupportTicket, ApplicationError> {
        self.apply_ticket_command(
            context,
            ticket_id,
            SupportTicketCommand::InternalNote {
                message_id: Uuid::new_v4(),
                author_ref: context.actor.principal_ref.clone(),
                body: body.into(),
                created_at: self.clock.now(),
            },
        )
        .await
    }

    pub async fn assign_ticket(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        assignee_ref: impl Into<String>,
        assignment_scope: erpx_domain::contracts::TenantScope,
    ) -> Result<SupportTicket, ApplicationError> {
        let assignment = SupportTicketAssignment::new(
            assignee_ref,
            context.actor.principal_ref.clone(),
            assignment_scope,
            self.clock.now(),
        )?;
        self.apply_ticket_command(
            context,
            ticket_id,
            SupportTicketCommand::Assign { assignment },
        )
        .await
    }

    pub async fn escalate_ticket(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        level: u8,
        target_ref: impl Into<String>,
        reason: impl Into<String>,
    ) -> Result<SupportTicket, ApplicationError> {
        let escalation = SupportTicketEscalation::new(
            level,
            target_ref,
            reason,
            context.actor.principal_ref.clone(),
            self.clock.now(),
        )?;
        self.apply_ticket_command(
            context,
            ticket_id,
            SupportTicketCommand::Escalate { escalation },
        )
        .await
    }

    pub async fn set_sla_deadline(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        deadline: DateTime<Utc>,
    ) -> Result<SupportTicket, ApplicationError> {
        self.apply_ticket_command(
            context,
            ticket_id,
            SupportTicketCommand::SetSlaDeadline { deadline },
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn request_session(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        customer_account_id: Uuid,
        agent_ref: &str,
        mode: erpx_domain::support::SupportMode,
        permissions: Vec<String>,
        scope: &str,
        module: &str,
        reason: &str,
        duration: Duration,
        represented_tenant_user: Option<String>,
    ) -> Result<SupportSession, ApplicationError> {
        let ticket = self
            .store
            .get_ticket(ticket_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        if ticket.customer_account_id != customer_account_id {
            return Err(ApplicationError::Authorization(
                "ticket/customer scope mismatch".into(),
            ));
        }
        let session = SupportSession::request_with_module(
            ticket_id,
            customer_account_id,
            agent_ref,
            mode,
            permissions,
            scope,
            module,
            reason,
            self.clock.now(),
            duration,
            represented_tenant_user,
        )?;
        execute_support_command(
            &self.commands,
            context,
            "support_session",
            session.id,
            1,
            "support_session.requested",
            &session,
            customer_account_id,
            "support_session_requested",
        )
        .await?;
        Ok(session)
    }

    pub async fn approve_session(
        &self,
        context: &CommandContext,
        session_id: Uuid,
        customer_approver: &str,
    ) -> Result<SupportSession, ApplicationError> {
        let mut session = self
            .store
            .get_session(session_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        session.approve(customer_approver, self.clock.now())?;
        let aggregate_version = next_command_version(context)?;
        execute_support_command(
            &self.commands,
            context,
            "support_session",
            session.id,
            aggregate_version,
            "support_session.approved",
            &session,
            session.customer_account_id,
            "support_session_approved",
        )
        .await?;
        Ok(session)
    }

    pub async fn revoke_session(
        &self,
        context: &CommandContext,
        session_id: Uuid,
    ) -> Result<(), ApplicationError> {
        let mut session = self
            .store
            .get_session(session_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        session.revoke(self.clock.now());
        let aggregate_version = next_command_version(context)?;
        execute_support_command(
            &self.commands,
            context,
            "support_session",
            session.id,
            aggregate_version,
            "support_session.revoked",
            &session,
            session.customer_account_id,
            "support_session_revoked",
        )
        .await
        .map(|_| ())
    }
}

fn next_command_version(context: &CommandContext) -> Result<u64, ApplicationError> {
    context
        .expected_aggregate_version
        .and_then(|version| version.checked_add(1))
        .ok_or_else(|| {
            ApplicationError::Conflict(
                "an expected aggregate version is required for an existing support aggregate"
                    .into(),
            )
        })
}

/// Derive a stable aggregate identity from non-secret command identity and
/// ticket lineage. The raw idempotency key is never persisted in the UUID or
/// exposed; only its SHA-256 digest contributes to the UUID bytes. Repeating
/// a create or recurrence command with the same idempotency key and lineage
/// therefore targets the same aggregate even if a transport rebuilds the
/// surrounding command context.
fn stable_support_ticket_id(
    context: &CommandContext,
    kind: &str,
    customer_account_id: Uuid,
    predecessor_id: Option<Uuid>,
) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(b"erpx.support.ticket.aggregate.v1");
    digest.update([0]);
    digest.update(kind.as_bytes());
    digest.update([0]);
    digest.update(context.idempotency_key.as_bytes());
    digest.update([0]);
    digest.update(customer_account_id.as_bytes());
    if let Some(predecessor_id) = predecessor_id {
        digest.update([0]);
        digest.update(predecessor_id.as_bytes());
    }
    let digest = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    // UUID v4/variant bits keep this deterministic identifier valid for
    // systems that validate UUID version metadata.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[allow(clippy::too_many_arguments)]
async fn execute_support_command<U, T>(
    commands: &Arc<U>,
    context: &CommandContext,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    event_type: &str,
    state: &T,
    customer_account_id: Uuid,
    action: &str,
) -> Result<CommandReceipt, ApplicationError>
where
    U: CommandUnitOfWork + 'static,
    T: serde::Serialize,
{
    let aggregate_state = serde_json::to_value(state)
        .map_err(|error| ApplicationError::Repository(error.to_string()))?;
    let request_material = serde_json::json!({
        // The idempotency key is the retry identity. Command IDs may be
        // regenerated by a transport retry and must not turn a replay into a
        // conflicting request hash.
        "idempotency_key": context.idempotency_key,
        "aggregate_type": aggregate_type,
        "aggregate_id": aggregate_id,
        "aggregate_version": aggregate_version,
        "event_type": event_type,
        "state": aggregate_state,
    });
    let mut digest = Sha256::new();
    digest.update(
        serde_json::to_vec(&request_material)
            .map_err(|error| ApplicationError::Repository(error.to_string()))?,
    );
    let request_hash = hex::encode(digest.finalize());
    let audit = AuditEvent {
        actor_ref: context.actor.principal_ref.clone(),
        customer_account_id: Some(customer_account_id),
        action: action.into(),
        purpose: context.audit.purpose.clone(),
        correlation_id: context.correlation_id,
    };
    let event = OutboxEvent {
        id: Uuid::new_v4(),
        aggregate_type: aggregate_type.into(),
        aggregate_id,
        aggregate_version,
        event_type: event_type.into(),
        payload: aggregate_state.clone(),
        occurred_at: Utc::now(),
    };
    commands
        .execute_command(CommandMutation {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            request_hash,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected_aggregate_version: context.expected_aggregate_version,
            aggregate_state,
            event,
            event_version: 1,
            tenant_scope: Some(
                serde_json::to_value(&context.tenant_scope)
                    .map_err(|error| ApplicationError::Repository(error.to_string()))?,
            ),
            actor: Some(
                serde_json::to_value(&context.actor)
                    .map_err(|error| ApplicationError::Repository(error.to_string()))?,
            ),
            causation_id: context.causation_id,
            audit,
            response: serde_json::json!({ "accepted": true, "aggregate_id": aggregate_id }),
        })
        .await
}

impl<S, C> SupportService<S, C>
where
    S: SupportStore + 'static,
    C: Clock + 'static,
{
    pub fn new(store: Arc<S>, clock: Arc<C>, audit: Arc<dyn AuditSink>) -> Self {
        Self {
            store,
            clock,
            audit,
        }
    }

    pub async fn open_ticket(
        &self,
        customer_account_id: Uuid,
        severity: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<SupportTicket, ApplicationError> {
        let ticket = SupportTicket {
            id: Uuid::new_v4(),
            customer_account_id,
            status: erpx_domain::support::TicketStatus::New,
            severity: severity.into(),
            description: description.into(),
            recurrence_of: None,
            messages: Vec::new(),
            assignment: None,
            escalation: None,
            sla_deadline: None,
        };
        self.store.create_ticket(ticket.clone()).await?;
        Ok(ticket)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn request_session(
        &self,
        ticket_id: Uuid,
        customer_account_id: Uuid,
        agent_ref: &str,
        mode: erpx_domain::support::SupportMode,
        permissions: Vec<String>,
        scope: &str,
        module: &str,
        reason: &str,
        duration: Duration,
        represented_tenant_user: Option<String>,
    ) -> Result<SupportSession, ApplicationError> {
        let ticket = self
            .store
            .get_ticket(ticket_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        if ticket.customer_account_id != customer_account_id {
            return Err(ApplicationError::Authorization(
                "ticket/customer scope mismatch".into(),
            ));
        }
        let session = SupportSession::request_with_module(
            ticket_id,
            customer_account_id,
            agent_ref,
            mode,
            permissions,
            scope,
            module,
            reason,
            self.clock.now(),
            duration,
            represented_tenant_user,
        )?;
        self.store.create_session(session.clone()).await?;
        self.audit
            .record(AuditEvent {
                actor_ref: agent_ref.into(),
                customer_account_id: Some(customer_account_id),
                action: "support_session_requested".into(),
                purpose: reason.into(),
                correlation_id: Uuid::new_v4(),
            })
            .await?;
        Ok(session)
    }

    pub async fn approve_session(
        &self,
        session_id: Uuid,
        customer_approver: &str,
    ) -> Result<SupportSession, ApplicationError> {
        let mut session = self
            .store
            .get_session(session_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        session.approve(customer_approver, self.clock.now())?;
        self.store.save_session(session.clone()).await?;
        self.audit
            .record(AuditEvent {
                actor_ref: customer_approver.into(),
                customer_account_id: Some(session.customer_account_id),
                action: "support_session_approved".into(),
                purpose: "customer_approval".into(),
                correlation_id: Uuid::new_v4(),
            })
            .await?;
        Ok(session)
    }

    pub async fn authorize_action(
        &self,
        session_id: Uuid,
        permission: &str,
        scope: &str,
    ) -> Result<SupportSession, ApplicationError> {
        let session = self
            .store
            .get_session(session_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        session.authorize(self.clock.now(), permission, scope)?;
        Ok(session)
    }

    pub async fn revoke_session(
        &self,
        session_id: Uuid,
        actor_ref: &str,
    ) -> Result<(), ApplicationError> {
        let mut session = self
            .store
            .get_session(session_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        session.revoke(self.clock.now());
        self.store.save_session(session.clone()).await?;
        self.audit
            .record(AuditEvent {
                actor_ref: actor_ref.into(),
                customer_account_id: Some(session.customer_account_id),
                action: "support_session_revoked".into(),
                purpose: "revocation".into(),
                correlation_id: Uuid::new_v4(),
            })
            .await?;
        Ok(())
    }
}

pub struct RoleLifecycleService<R, C> {
    store: Arc<R>,
    clock: Arc<C>,
    audit: Arc<dyn AuditSink>,
}

pub struct AuthoritativeRoleLifecycleService<R, C, U> {
    store: Arc<R>,
    clock: Arc<C>,
    commands: Arc<U>,
}

impl<R, C, U> AuthoritativeRoleLifecycleService<R, C, U>
where
    R: RoleAssignmentStore + 'static,
    C: Clock + 'static,
    U: CommandUnitOfWork + 'static,
{
    pub fn new(store: Arc<R>, clock: Arc<C>, commands: Arc<U>) -> Self {
        Self {
            store,
            clock,
            commands,
        }
    }

    /// Build the review queue from authoritative role rows. The domain
    /// classifier is deterministic and the command path remains separate so
    /// a review read cannot grant or extend access by itself.
    pub async fn review_queue(
        &self,
        expiry_horizon: Duration,
    ) -> Result<RoleReviewQueue, ApplicationError> {
        let assignments = self.store.list(u32::MAX).await?;
        Ok(build_role_review_queue(
            &assignments,
            self.clock.now(),
            expiry_horizon,
        )?)
    }

    pub async fn renew(
        &self,
        context: &CommandContext,
        assignment_id: Uuid,
        valid_until: DateTime<Utc>,
        grantor: &str,
        approver: Option<String>,
    ) -> Result<RoleAssignment, ApplicationError> {
        let current = self
            .store
            .get(assignment_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        let parent = match current.parent_assignment_id {
            Some(parent_id) => Some(
                self.store
                    .get(parent_id)
                    .await?
                    .ok_or(ApplicationError::NotFound)?,
            ),
            None => None,
        };
        let renewed = current.renew(self.clock.now(), valid_until, grantor, approver)?;
        renewed.validate_for_grant(parent.as_ref())?;
        let aggregate_state = serde_json::to_value(&renewed)
            .map_err(|error| ApplicationError::Repository(error.to_string()))?;
        execute_generic_command(
            &self.commands,
            context,
            "role_assignment",
            renewed.id,
            1,
            "role_assignment.renewed",
            aggregate_state,
            None,
            "role_renewed",
            renewed.grant_reason.clone(),
        )
        .await?;
        Ok(renewed)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn renew_with_reviewer(
        &self,
        context: &CommandContext,
        assignment_id: Uuid,
        valid_until: DateTime<Utc>,
        grantor: &str,
        approver: Option<String>,
        renewal_reviewer: &str,
    ) -> Result<RoleAssignment, ApplicationError> {
        let current = self
            .store
            .get(assignment_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        let parent = match current.parent_assignment_id {
            Some(parent_id) => Some(
                self.store
                    .get(parent_id)
                    .await?
                    .ok_or(ApplicationError::NotFound)?,
            ),
            None => None,
        };
        let renewed = current.renew_with_reviewer(
            self.clock.now(),
            valid_until,
            grantor,
            approver,
            renewal_reviewer,
        )?;
        renewed.validate_for_grant(parent.as_ref())?;
        let aggregate_state = serde_json::to_value(&renewed)
            .map_err(|error| ApplicationError::Repository(error.to_string()))?;
        execute_generic_command(
            &self.commands,
            context,
            "role_assignment",
            renewed.id,
            1,
            "role_assignment.renewed",
            aggregate_state,
            None,
            "role_renewed_reviewed",
            renewed.grant_reason.clone(),
        )
        .await?;
        Ok(renewed)
    }

    /// Recover the final Platform Root or Holding Owner only when the
    /// authoritative write-side role set confirms no active target remains.
    /// The requester must be the authenticated command actor; materialization
    /// enforces finite validity and independent high-risk approval.
    pub async fn recover_final_role(
        &self,
        context: &CommandContext,
        mut request: RoleRecoveryRequest,
    ) -> Result<RoleAssignment, ApplicationError> {
        if request.requester_ref != context.actor.principal_ref {
            return Err(ApplicationError::Authorization(
                "recovery requester does not match command actor".into(),
            ));
        }
        // Recovery always begins at the authoritative write-side clock. The
        // transport does not get to choose or backdate the start instant.
        request.valid_from = self.clock.now();
        // The request ID is also the deterministic assignment ID. A command
        // can commit its aggregate/outbox transaction immediately before the
        // separate recovery-review receipt transaction; on retry, rehydrate
        // that exact assignment instead of treating the committed recovery as
        // a new target conflict. Validate against every other assignment so a
        // later conflicting recovery still fails closed.
        if let Some(existing) = self.store.get(request.request_id).await? {
            let other_assignments = self
                .store
                .list(u32::MAX)
                .await?
                .into_iter()
                .filter(|assignment| assignment.id != existing.id)
                .collect::<Vec<_>>();
            let expected = request.materialize(&other_assignments, request.valid_from)?;
            if expected != existing {
                return Err(ApplicationError::Conflict(
                    "recovery request ID is bound to a different assignment".into(),
                ));
            }
            return Ok(existing);
        }
        let assignments = self.store.list(u32::MAX).await?;
        let recovered = request.materialize(&assignments, request.valid_from)?;
        let aggregate_state = serde_json::to_value(&recovered)
            .map_err(|error| ApplicationError::Repository(error.to_string()))?;
        execute_generic_command(
            &self.commands,
            context,
            "role_assignment",
            recovered.id,
            1,
            "role_assignment.recovered",
            aggregate_state,
            None,
            "role_recovered",
            recovered.grant_reason.clone(),
        )
        .await?;
        Ok(recovered)
    }

    pub async fn grant(
        &self,
        context: &CommandContext,
        assignment: RoleAssignment,
        parent: Option<&RoleAssignment>,
    ) -> Result<RoleAssignment, ApplicationError> {
        assignment.validate_for_grant(parent)?;
        let aggregate_state = serde_json::to_value(&assignment)
            .map_err(|error| ApplicationError::Repository(error.to_string()))?;
        execute_generic_command(
            &self.commands,
            context,
            "role_assignment",
            assignment.id,
            1,
            "role_assignment.granted",
            aggregate_state,
            None,
            "role_granted",
            assignment.grant_reason.clone(),
        )
        .await?;
        Ok(assignment)
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_generic_command<U>(
    commands: &Arc<U>,
    context: &CommandContext,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    event_type: &str,
    aggregate_state: Value,
    customer_account_id: Option<Uuid>,
    action: &str,
    purpose: String,
) -> Result<CommandReceipt, ApplicationError>
where
    U: CommandUnitOfWork + 'static,
{
    let request_material = serde_json::json!({
        "command_id": context.command_id,
        "aggregate_type": aggregate_type,
        "aggregate_id": aggregate_id,
        "aggregate_version": aggregate_version,
        "event_type": event_type,
        "state": aggregate_state,
    });
    let mut digest = Sha256::new();
    digest.update(
        serde_json::to_vec(&request_material)
            .map_err(|error| ApplicationError::Repository(error.to_string()))?,
    );
    let audit = AuditEvent {
        actor_ref: context.actor.principal_ref.clone(),
        customer_account_id,
        action: action.into(),
        purpose,
        correlation_id: context.correlation_id,
    };
    let event = OutboxEvent {
        id: Uuid::new_v4(),
        aggregate_type: aggregate_type.into(),
        aggregate_id,
        aggregate_version,
        event_type: event_type.into(),
        payload: aggregate_state.clone(),
        occurred_at: Utc::now(),
    };
    commands
        .execute_command(CommandMutation {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            request_hash: hex::encode(digest.finalize()),
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected_aggregate_version: context.expected_aggregate_version,
            aggregate_state,
            event,
            event_version: 1,
            tenant_scope: Some(
                serde_json::to_value(&context.tenant_scope)
                    .map_err(|error| ApplicationError::Repository(error.to_string()))?,
            ),
            actor: Some(
                serde_json::to_value(&context.actor)
                    .map_err(|error| ApplicationError::Repository(error.to_string()))?,
            ),
            causation_id: context.causation_id,
            audit,
            response: serde_json::json!({ "accepted": true, "aggregate_id": aggregate_id }),
        })
        .await
}

impl<R, C> RoleLifecycleService<R, C>
where
    R: RoleAssignmentStore + 'static,
    C: Clock + 'static,
{
    pub fn new(store: Arc<R>, clock: Arc<C>, audit: Arc<dyn AuditSink>) -> Self {
        Self {
            store,
            clock,
            audit,
        }
    }

    pub async fn grant(
        &self,
        assignment: RoleAssignment,
        parent: Option<&RoleAssignment>,
    ) -> Result<RoleAssignment, ApplicationError> {
        assignment.validate_for_grant(parent)?;
        self.store.create(assignment.clone()).await?;
        self.audit
            .record(AuditEvent {
                actor_ref: assignment.grantor.clone(),
                customer_account_id: None,
                action: "role_granted".into(),
                purpose: assignment.grant_reason.clone(),
                correlation_id: Uuid::new_v4(),
            })
            .await?;
        Ok(assignment)
    }

    pub async fn renew(
        &self,
        assignment_id: Uuid,
        valid_until: DateTime<Utc>,
        grantor: &str,
        approver: Option<String>,
    ) -> Result<RoleAssignment, ApplicationError> {
        let current = self
            .store
            .get(assignment_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        let parent = match current.parent_assignment_id {
            Some(parent_id) => Some(
                self.store
                    .get(parent_id)
                    .await?
                    .ok_or(ApplicationError::NotFound)?,
            ),
            None => None,
        };
        let renewed = current.renew(self.clock.now(), valid_until, grantor, approver)?;
        renewed.validate_for_grant(parent.as_ref())?;
        self.store.create(renewed.clone()).await?;
        self.audit
            .record(AuditEvent {
                actor_ref: grantor.into(),
                customer_account_id: None,
                action: "role_renewed".into(),
                purpose: renewed.grant_reason.clone(),
                correlation_id: Uuid::new_v4(),
            })
            .await?;
        Ok(renewed)
    }

    pub async fn list_expiring(
        &self,
        within: Duration,
    ) -> Result<Vec<RoleAssignment>, ApplicationError> {
        let now = self.clock.now();
        let until = now + within;
        Ok(self
            .store
            .list(500)
            .await?
            .into_iter()
            .filter(|assignment| assignment.valid_until > now && assignment.valid_until <= until)
            .collect())
    }

    pub async fn review_queue(
        &self,
        expiry_horizon: Duration,
    ) -> Result<RoleReviewQueue, ApplicationError> {
        let assignments = self.store.list(10_000).await?;
        Ok(build_role_review_queue(
            &assignments,
            self.clock.now(),
            expiry_horizon,
        )?)
    }

    pub async fn list_expiring_in_scope(
        &self,
        within: Duration,
        requested_scope: &Scope,
    ) -> Result<Vec<RoleAssignment>, ApplicationError> {
        Ok(self
            .list_expiring(within)
            .await?
            .into_iter()
            .filter(|assignment| assignment.scope.contains(requested_scope))
            .collect())
    }
}

pub fn authorize_entitlement(
    billing: BillingState,
    entitlement: Option<&ModuleEntitlement>,
    rbac_allows: bool,
    at: DateTime<Utc>,
    is_mutation: bool,
) -> Result<(), ApplicationError> {
    match authorize_module(billing, entitlement, rbac_allows, at, is_mutation) {
        EntitlementDecision::Allowed => Ok(()),
        EntitlementDecision::Denied(reason) => Err(ApplicationError::Authorization(reason.into())),
    }
}

#[cfg(test)]
mod command_tests {
    use super::*;
    use erpx_domain::contracts::{ActorAttribution, ActorKind, TenantScope};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct RecordingUnitOfWork {
        received: Mutex<Vec<CommandMutation>>,
    }

    #[async_trait]
    impl CommandUnitOfWork for RecordingUnitOfWork {
        async fn execute_command(
            &self,
            mutation: CommandMutation,
        ) -> Result<CommandReceipt, ApplicationError> {
            let receipt = CommandReceipt {
                command_id: mutation.command_id,
                aggregate_type: mutation.aggregate_type.clone(),
                aggregate_id: mutation.aggregate_id,
                aggregate_version: mutation.event.aggregate_version,
                event_id: mutation.event.id,
                response: mutation.response.clone(),
                replayed: false,
            };
            self.received.lock().expect("recording lock").push(mutation);
            Ok(receipt)
        }
    }

    #[tokio::test]
    async fn command_service_forwards_the_complete_atomic_mutation() {
        let unit_of_work = Arc::new(RecordingUnitOfWork::default());
        let service = ApplicationCommandService::new(unit_of_work.clone());
        let command_id = Uuid::new_v4();
        let aggregate_id = Uuid::new_v4();
        let event_id = Uuid::new_v4();
        let mutation = CommandMutation {
            command_id,
            idempotency_key: "command-1".into(),
            request_hash: "hash-1".into(),
            aggregate_type: "customer".into(),
            aggregate_id,
            expected_aggregate_version: None,
            aggregate_state: serde_json::json!({"status": "active"}),
            event: OutboxEvent {
                id: event_id,
                aggregate_type: "customer".into(),
                aggregate_id,
                aggregate_version: 1,
                event_type: "customer.created".into(),
                payload: serde_json::json!({"status": "active"}),
                occurred_at: Utc::now(),
            },
            event_version: 1,
            tenant_scope: None,
            actor: None,
            causation_id: None,
            audit: AuditEvent {
                actor_ref: "test".into(),
                customer_account_id: None,
                action: "customer_created".into(),
                purpose: "test".into(),
                correlation_id: command_id,
            },
            response: serde_json::json!({"accepted": true}),
        };
        let receipt = service.execute(mutation).await.expect("command receipt");
        assert_eq!(receipt.command_id, command_id);
        assert_eq!(receipt.event_id, event_id);
        assert_eq!(
            unit_of_work.received.lock().expect("recording lock").len(),
            1
        );
    }

    #[derive(Debug, Default)]
    struct ReplayTicketStore {
        tickets: Mutex<HashMap<Uuid, SupportTicket>>,
    }

    #[async_trait]
    impl SupportStore for ReplayTicketStore {
        async fn create_ticket(&self, ticket: SupportTicket) -> Result<(), ApplicationError> {
            self.tickets
                .lock()
                .expect("ticket store lock")
                .insert(ticket.id, ticket);
            Ok(())
        }

        async fn get_ticket(&self, id: Uuid) -> Result<Option<SupportTicket>, ApplicationError> {
            Ok(self
                .tickets
                .lock()
                .expect("ticket store lock")
                .get(&id)
                .cloned())
        }

        async fn save_ticket(&self, ticket: SupportTicket) -> Result<(), ApplicationError> {
            self.create_ticket(ticket).await
        }

        async fn create_session(&self, _session: SupportSession) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn get_session(&self, _id: Uuid) -> Result<Option<SupportSession>, ApplicationError> {
            Ok(None)
        }

        async fn save_session(&self, _session: SupportSession) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    #[derive(Debug)]
    struct ReplayingSupportCommand {
        aggregate_id: Uuid,
    }

    #[async_trait]
    impl CommandUnitOfWork for ReplayingSupportCommand {
        async fn execute_command(
            &self,
            mutation: CommandMutation,
        ) -> Result<CommandReceipt, ApplicationError> {
            Ok(CommandReceipt {
                command_id: mutation.command_id,
                aggregate_type: mutation.aggregate_type,
                aggregate_id: self.aggregate_id,
                aggregate_version: 1,
                event_id: Uuid::new_v4(),
                response: serde_json::json!({"accepted": true, "aggregate_id": self.aggregate_id}),
                replayed: true,
            })
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct FixedSupportClock(DateTime<Utc>);

    impl Clock for FixedSupportClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    #[tokio::test]
    async fn recurrence_replay_rehydrates_receipt_aggregate_instead_of_returning_new_id() {
        let now = Utc::now();
        let customer_account_id = Uuid::new_v4();
        let holding_id = Uuid::new_v4();
        let closed = SupportTicket {
            id: Uuid::new_v4(),
            customer_account_id,
            status: erpx_domain::support::TicketStatus::Closed,
            severity: "sev2".into(),
            description: "original issue".into(),
            recurrence_of: None,
            messages: Vec::new(),
            assignment: None,
            escalation: None,
            sla_deadline: None,
        };
        let context = CommandContext::try_new(
            "support-recurrence-replay",
            ActorAttribution {
                principal_ref: "support-agent".into(),
                kind: ActorKind::PlatformUser,
                session_id: Some(Uuid::new_v4()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            TenantScope::Holding { holding_id },
            "reopen recurring support issue",
        )
        .expect("context");
        let recurrence_id =
            stable_support_ticket_id(&context, "recurrence", customer_account_id, Some(closed.id));
        let stored_recurrence = closed
            .recurrence(recurrence_id, "sev1", "rehydrated stored recurrence")
            .expect("recurrence");
        let store = Arc::new(ReplayTicketStore::default());
        store
            .create_ticket(closed.clone())
            .await
            .expect("closed ticket");
        store
            .create_ticket(stored_recurrence.clone())
            .await
            .expect("stored recurrence");
        let commands = Arc::new(ReplayingSupportCommand {
            aggregate_id: recurrence_id,
        });
        let service =
            AuthoritativeSupportService::new(store, Arc::new(FixedSupportClock(now)), commands);

        let returned = service
            .open_ticket_recurrence(
                &context,
                closed.id,
                "sev1",
                "request payload must not win over replayed state",
            )
            .await
            .expect("replayed recurrence");
        assert_eq!(returned.id, recurrence_id);
        assert_eq!(returned.description, "rehydrated stored recurrence");
    }
}

#[cfg(test)]
mod role_lifecycle_tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use erpx_domain::{
        access::{RiskTier, Scope},
        contracts::{ActorAttribution, ActorKind, TenantScope},
    };

    #[derive(Debug, Default)]
    struct RecordingRoleStore {
        assignments: Mutex<HashMap<Uuid, RoleAssignment>>,
    }

    #[async_trait]
    impl RoleAssignmentStore for RecordingRoleStore {
        async fn create(&self, assignment: RoleAssignment) -> Result<(), ApplicationError> {
            self.assignments
                .lock()
                .expect("role store lock")
                .insert(assignment.id, assignment);
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<RoleAssignment>, ApplicationError> {
            Ok(self
                .assignments
                .lock()
                .expect("role store lock")
                .get(&id)
                .cloned())
        }

        async fn list(&self, limit: u32) -> Result<Vec<RoleAssignment>, ApplicationError> {
            Ok(self
                .assignments
                .lock()
                .expect("role store lock")
                .values()
                .take(limit as usize)
                .cloned()
                .collect())
        }
    }

    #[derive(Debug, Default)]
    struct RecordingRoleCommands {
        received: Mutex<Vec<CommandMutation>>,
    }

    #[async_trait]
    impl CommandUnitOfWork for RecordingRoleCommands {
        async fn execute_command(
            &self,
            mutation: CommandMutation,
        ) -> Result<CommandReceipt, ApplicationError> {
            let receipt = CommandReceipt {
                command_id: mutation.command_id,
                aggregate_type: mutation.aggregate_type.clone(),
                aggregate_id: mutation.aggregate_id,
                aggregate_version: mutation.event.aggregate_version,
                event_id: mutation.event.id,
                response: mutation.response.clone(),
                replayed: false,
            };
            self.received
                .lock()
                .expect("role command lock")
                .push(mutation);
            Ok(receipt)
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct FixedClock(DateTime<Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    #[derive(Debug, Default)]
    struct NoopAudit;

    #[async_trait]
    impl AuditSink for NoopAudit {
        async fn record(&self, _event: AuditEvent) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    fn context() -> CommandContext {
        CommandContext::try_new(
            "role-lifecycle-test",
            ActorAttribution {
                principal_ref: "operator".into(),
                kind: ActorKind::TenantUser,
                session_id: Some(Uuid::new_v4()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            TenantScope::Platform,
            "role lifecycle test",
        )
        .expect("valid command context")
    }

    fn assignment(
        id: Uuid,
        from: DateTime<Utc>,
        until: DateTime<Utc>,
        parent_assignment_id: Option<Uuid>,
    ) -> RoleAssignment {
        RoleAssignment {
            id,
            principal_ref: "operator".into(),
            role_key: "finance_reader".into(),
            permissions: vec!["finance.read".into()],
            functional_domain: "finance".into(),
            authority_level: 2,
            approval_limit_minor: 0,
            scope: Scope::Platform,
            valid_from: from,
            valid_until: until,
            timezone: "UTC".into(),
            risk_tier: RiskTier::Standard,
            grant_reason: "lifecycle test".into(),
            ticket_reference: None,
            grantor: "operator".into(),
            approver: None,
            renewal_reviewer: None,
            parent_assignment_id,
            assignment_version: 1,
        }
    }

    #[tokio::test]
    async fn authoritative_renewal_rechecks_parent_and_emits_new_version() {
        let from = Utc::now();
        let parent_until = from + Duration::hours(2);
        let parent = assignment(Uuid::new_v4(), from, parent_until, None);
        let child = assignment(
            Uuid::new_v4(),
            from,
            from + Duration::hours(1),
            Some(parent.id),
        );
        let store = Arc::new(RecordingRoleStore::default());
        store.create(parent.clone()).await.expect("parent");
        store.create(child.clone()).await.expect("child");
        let commands = Arc::new(RecordingRoleCommands::default());
        let service = AuthoritativeRoleLifecycleService::new(
            store,
            Arc::new(FixedClock(from + Duration::minutes(15))),
            commands.clone(),
        );

        let renewed = service
            .renew(&context(), child.id, parent_until, "operator", None)
            .await
            .expect("parent-contained renewal");
        assert_ne!(renewed.id, child.id);
        assert_eq!(renewed.assignment_version, 2);
        assert_eq!(renewed.valid_until, parent_until);
        let mutations = commands.received.lock().expect("role command lock");
        assert_eq!(mutations.len(), 1);
        assert_eq!(mutations[0].event.event_type, "role_assignment.renewed");
        assert_eq!(mutations[0].event.aggregate_version, 1);
    }

    #[tokio::test]
    async fn authoritative_grant_rejects_self_approved_high_risk_assignment() {
        let from = Utc::now();
        let mut assignment = assignment(Uuid::new_v4(), from, from + Duration::hours(1), None);
        assignment.risk_tier = RiskTier::High;
        assignment.grantor = "operator".into();
        assignment.approver = Some("operator".into());
        let service = AuthoritativeRoleLifecycleService::new(
            Arc::new(RecordingRoleStore::default()),
            Arc::new(FixedClock(from)),
            Arc::new(RecordingRoleCommands::default()),
        );

        assert_eq!(
            service
                .grant(&context(), assignment, None)
                .await
                .expect_err("self-approval must be rejected")
                .to_string(),
            "high-risk assignments require an approver other than the grantor"
        );
    }

    #[tokio::test]
    async fn legacy_lifecycle_grant_rejects_expired_parent_scope() {
        let from = Utc::now();
        let parent = assignment(Uuid::new_v4(), from, from + Duration::minutes(5), None);
        let child = assignment(
            Uuid::new_v4(),
            from + Duration::minutes(6),
            from + Duration::minutes(7),
            Some(parent.id),
        );
        let store = Arc::new(RecordingRoleStore::default());
        let service =
            RoleLifecycleService::new(store, Arc::new(FixedClock(from)), Arc::new(NoopAudit));
        assert_eq!(
            service
                .grant(child, Some(&parent))
                .await
                .expect_err("child cannot start after parent expiry")
                .to_string(),
            "child assignment outlives its parent"
        );
    }

    #[tokio::test]
    async fn review_queue_is_authoritative_and_recovery_requires_command_actor() {
        let at = DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp");
        let store = Arc::new(RecordingRoleStore::default());
        store
            .create(assignment(
                Uuid::from_u128(90),
                at,
                at + Duration::hours(1),
                None,
            ))
            .await
            .expect("assignment");
        let audit = Arc::new(NoopAudit);
        let lifecycle = RoleLifecycleService::new(store.clone(), Arc::new(FixedClock(at)), audit);
        let queue = lifecycle
            .review_queue(Duration::hours(1))
            .await
            .expect("review queue");
        assert_eq!(queue.items.len(), 1);
        assert!(
            queue.items[0]
                .classifications
                .contains(&erpx_domain::access::RoleReviewClassification::ExpiringSoon)
        );

        let commands = Arc::new(RecordingRoleCommands::default());
        let authoritative = AuthoritativeRoleLifecycleService::new(
            store,
            Arc::new(FixedClock(at)),
            commands.clone(),
        );
        let request = RoleRecoveryRequest {
            request_id: Uuid::from_u128(91),
            target: erpx_domain::access::RoleRecoveryTarget::PlatformRoot,
            principal_ref: "recovered-root".into(),
            permissions: vec!["platform.read".into()],
            functional_domain: "platform".into(),
            authority_level: 255,
            valid_from: at,
            valid_until: at + Duration::days(1),
            timezone: "UTC".into(),
            grant_reason: "final root recovery".into(),
            ticket_reference: None,
            requester_ref: "not-operator".into(),
            approver_ref: "approver".into(),
            reviewer_ref: None,
        };
        assert_eq!(
            authoritative
                .recover_final_role(&context(), request.clone())
                .await
                .expect_err("actor mismatch")
                .to_string(),
            "authorization denied: recovery requester does not match command actor"
        );
        let mut authorized_request = request;
        authorized_request.requester_ref = "operator".into();
        let recovered = authoritative
            .recover_final_role(&context(), authorized_request)
            .await
            .expect("recovery");
        assert_eq!(recovered.role_key, "platform_root");
        assert_eq!(
            commands
                .received
                .lock()
                .expect("command lock")
                .last()
                .expect("recovery command")
                .event
                .event_type,
            "role_assignment.recovered"
        );
    }

    #[tokio::test]
    async fn recovery_retry_rehydrates_committed_assignment_without_second_command() {
        let at = DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp");
        let request = RoleRecoveryRequest {
            request_id: Uuid::from_u128(92),
            target: erpx_domain::access::RoleRecoveryTarget::PlatformRoot,
            principal_ref: "recovered-root".into(),
            permissions: vec!["platform.read".into()],
            functional_domain: "platform".into(),
            authority_level: 255,
            valid_from: at,
            valid_until: at + Duration::days(1),
            timezone: "UTC".into(),
            grant_reason: "repair recovery receipt after commit".into(),
            ticket_reference: Some("RECOVERY-RETRY".into()),
            requester_ref: "operator".into(),
            approver_ref: "approver".into(),
            reviewer_ref: None,
        };
        let committed = request.materialize(&[], at).expect("recovery assignment");
        let store = Arc::new(RecordingRoleStore::default());
        store
            .create(committed.clone())
            .await
            .expect("committed role");
        let commands = Arc::new(RecordingRoleCommands::default());
        let service = AuthoritativeRoleLifecycleService::new(
            store,
            Arc::new(FixedClock(at)),
            commands.clone(),
        );

        let returned = service
            .recover_final_role(&context(), request)
            .await
            .expect("replay existing recovery");
        assert_eq!(returned, committed);
        assert!(
            commands
                .received
                .lock()
                .expect("role command lock")
                .is_empty()
        );
    }
}

#[cfg(test)]
mod customer_audit_query_tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct RecordingAudit {
        events: Mutex<Vec<AuditEvent>>,
    }

    #[async_trait]
    impl AuditSink for RecordingAudit {
        async fn record(&self, event: AuditEvent) -> Result<(), ApplicationError> {
            self.events.lock().expect("audit lock").push(event);
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct EmptyAuditReader;

    #[async_trait]
    impl CustomerGlobalReadAuditDetailStore for EmptyAuditReader {
        async fn get_customer_global_read_audit_details(
            &self,
            query: &CustomerGlobalReadAuditDetailQuery,
        ) -> Result<CustomerReadView<CustomerGlobalReadAuditDetailPage>, ApplicationError> {
            Ok(CustomerReadView::absent(query.customer_id))
        }
    }

    fn query() -> CustomerGlobalReadAuditDetailQuery {
        CustomerGlobalReadAuditDetailQuery {
            customer_id: Uuid::from_u128(0x7001),
            security_admin_scope: CustomerSecurityAdminScope {
                customer_id: Uuid::from_u128(0x7001),
                platform: false,
                holding_id: Some(Uuid::from_u128(0x7101)),
                company_id: None,
                branch_id: None,
            },
            audit: CustomerGlobalReadAuditQueryContext {
                requester: GlobalReadActor {
                    principal_ref: "security-admin".into(),
                    session_ref: Some("security-session".into()),
                },
                purpose: "customer access review".into(),
                correlation_id: Uuid::from_u128(0x7201),
            },
            before: None,
            limit: 1_000,
        }
    }

    #[test]
    fn detail_query_requires_matching_customer_scope_and_bounds_page_size() {
        let query = query();
        assert!(query.validate().is_ok());
        assert_eq!(
            query.bounded_limit(),
            CUSTOMER_GLOBAL_READ_AUDIT_MAX_PAGE_SIZE
        );
        let mut mismatched = query;
        mismatched.security_admin_scope.customer_id = Uuid::from_u128(0x7002);
        assert!(mismatched.validate().is_err());
    }

    #[tokio::test]
    async fn detail_service_records_the_query_before_reading() {
        let audit = Arc::new(RecordingAudit::default());
        let service =
            CustomerGlobalReadAuditDetailService::new(Arc::new(EmptyAuditReader), audit.clone());
        let result = service.list(query()).await.expect("audited detail query");
        assert!(matches!(result, CustomerReadView::Absent { .. }));
        let events = audit.events.lock().expect("audit lock");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "customer_global_read_audit_detail");
        assert_eq!(events[0].customer_account_id, Some(Uuid::from_u128(0x7001)));
    }
}

#[cfg(test)]
mod marketing_content_write_contract_tests {
    use super::*;
    use erpx_domain::contracts::{ActorAttribution, ActorKind};
    use erpx_domain::marketing_content::{FormField, FormFieldKind, LocalizedText};

    fn scope() -> TenantScope {
        TenantScope::Holding {
            holding_id: Uuid::from_u128(0x5_001),
        }
    }

    fn context(idempotency_key: &str) -> CommandContext {
        CommandContext::try_new(
            idempotency_key,
            ActorAttribution {
                principal_ref: "marketing-operator".into(),
                kind: ActorKind::TenantUser,
                session_id: Some(Uuid::from_u128(0x5_002)),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope(),
            "marketing content contract test",
        )
        .expect("valid command context")
    }

    fn published_form() -> ContentForm {
        let field = FormField::new(
            "email",
            LocalizedText::new("Email", "อีเมล").expect("localized field label"),
            FormFieldKind::Email,
            true,
            320,
        )
        .expect("form field");
        let mut form = ContentForm::new(
            Uuid::from_u128(0x5_003),
            scope(),
            "contact-form",
            vec![field],
        )
        .expect("form");
        form.submit_for_review().expect("form review");
        form.publish().expect("form publish");
        form
    }

    #[test]
    fn commands_preserve_idempotency_and_correlation_context() {
        let form = published_form();
        let mut values = std::collections::BTreeMap::new();
        values.insert("email".into(), "customer@example.com".into());
        let submission_context = context("marketing-submit-1");
        let submission_correlation = submission_context.correlation_id;
        let submission = SubmitPublishedFormCommand::from_published_form(
            &form,
            Uuid::from_u128(0x5_004),
            values,
            Utc::now(),
            submission_context,
        )
        .expect("submission command");
        assert_eq!(submission.idempotency_key(), "marketing-submit-1");
        assert_eq!(submission.correlation_id(), submission_correlation);

        let template = ContentTemplateRef::new(Uuid::from_u128(0x5_005), 1, scope())
            .expect("template reference");
        let preview = PreviewGrant::new(
            Uuid::from_u128(0x5_006),
            template.clone(),
            Utc::now(),
            Utc::now() + Duration::hours(1),
        )
        .expect("preview grant");
        let preview_context = context("marketing-preview-1");
        let preview_correlation = preview_context.correlation_id;
        let preview_command = IssuePreviewCommand::new(
            preview,
            "A".repeat(MARKETING_CONTENT_HASH_BYTES),
            preview_context,
        )
        .expect("preview command");
        assert_eq!(preview_command.idempotency_key(), "marketing-preview-1");
        assert_eq!(preview_command.correlation_id(), preview_correlation);

        let domain = CustomDomain::new(Uuid::from_u128(0x5_007), scope(), "marketing.example.com")
            .expect("custom domain");
        let binding_context = context("marketing-bind-1");
        let binding_correlation = binding_context.correlation_id;
        let binding =
            BindDomainCommand::from_domain(&domain, template, Utc::now(), binding_context)
                .expect("binding command");
        assert_eq!(binding.idempotency_key(), "marketing-bind-1");
        assert_eq!(binding.correlation_id(), binding_correlation);
    }

    #[test]
    fn constructors_bound_inputs_and_reject_invalid_domain_requests() {
        let form = published_form();
        let form_ref = form.reference().expect("form reference");
        let mut too_many_values = std::collections::BTreeMap::new();
        for index in 0..=MARKETING_CONTENT_MAX_SUBMISSION_VALUES {
            too_many_values.insert(format!("field_{index}"), "value".into());
        }
        assert!(
            SubmitPublishedFormCommand::new(
                form_ref,
                Uuid::from_u128(0x5_008),
                too_many_values,
                Utc::now(),
                context("marketing-submit-too-many"),
            )
            .is_err()
        );

        let template = ContentTemplateRef::new(Uuid::from_u128(0x5_009), 1, scope())
            .expect("template reference");
        let preview = PreviewGrant::new(
            Uuid::from_u128(0x5_00a),
            template,
            Utc::now(),
            Utc::now() + Duration::hours(1),
        )
        .expect("preview grant");
        assert!(
            IssuePreviewCommand::new(
                preview,
                "not-a-sha256-digest",
                context("marketing-preview-invalid"),
            )
            .is_err()
        );

        assert!(
            RevokePreviewCommand::new(
                Uuid::from_u128(0x5_00b),
                TenantScope::Platform,
                Utc::now(),
                context("marketing-revoke-platform"),
            )
            .is_err()
        );

        let draft_form = ContentForm::new(
            Uuid::from_u128(0x5_00c),
            scope(),
            "draft-form",
            vec![
                FormField::new(
                    "name",
                    LocalizedText::new("Name", "ชื่อ").expect("localized label"),
                    FormFieldKind::Text,
                    true,
                    100,
                )
                .expect("form field"),
            ],
        )
        .expect("draft form");
        assert!(
            SubmitPublishedFormCommand::from_published_form(
                &draft_form,
                Uuid::from_u128(0x5_00d),
                std::collections::BTreeMap::from([(String::from("name"), String::from("Ada"))]),
                Utc::now(),
                context("marketing-draft-submit"),
            )
            .is_err()
        );
    }

    #[test]
    fn receipt_contains_only_transport_neutral_write_metadata() {
        let receipt = MarketingContentWriteReceipt {
            aggregate_type: "marketing_content_form_submission".into(),
            aggregate_id: Uuid::from_u128(0x5_00e),
            version: 1,
            idempotency_key: "marketing-receipt-1".into(),
            event_id: Uuid::from_u128(0x5_00f),
            replayed: true,
        };
        let encoded = serde_json::to_value(&receipt).expect("serializable receipt");
        assert_eq!(encoded["version"], 1);
        assert_eq!(encoded["replayed"], true);
        assert!(encoded.get("pool").is_none());
        assert!(encoded.get("transaction").is_none());
    }
}
