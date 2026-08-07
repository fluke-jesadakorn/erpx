use super::super::*;

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
