//! G12 product-surface contracts for the ERPX console.
//!
//! This module is deliberately transport- and target-neutral. It accepts only bounded,
//! already-authorized display facts at the application boundary; it does not contain a
//! repository, protobuf client, mutation callback, credential, or authorization decision.
//! The coordinator can register it from `main.rs` after the corresponding server/read-model
//! packets are integrated.

use dioxus::dioxus_core::current_scope_id;
use dioxus::prelude::*;
use erpx_domain::data_plane::BusinessUnitId;
use erpx_ui_core::{
    ClientTarget, Locale, LocaleContext, LocalizedText, SecureSessionSupport,
    UiAccessibilityContract, UiStateKind, redact_deep_link,
};
use std::fmt;

fn product_locale() -> Locale {
    try_consume_context::<LocaleContext>()
        .map(LocaleContext::current)
        .unwrap_or_default()
}

fn product_copy(en: &'static str, th: &'static str) -> &'static str {
    LocalizedText::new(en, th).text(product_locale())
}

/// The revocation budget required by the ERPX security contract.
pub const FIVE_MINUTE_REVOCATION_SECONDS: u32 = 300;

/// The freshness budget used by the existing platform projection-status route.
///
/// This mirrors the server-side projection-status contract. The client never derives
/// freshness from a row count; it accepts the authoritative response classification and
/// validates any supplied age against this bound before rendering a model.
pub const MAX_PROJECTION_FRESHNESS_SECONDS: u64 = 300;

/// The maximum full-import size supported by a directory connector contract.
pub const MAX_FULL_IMPORT_USERS: u64 = 1_000_000;

/// Authority marker attached to directory UI nodes supplied by the coordinator's read boundary.
pub const DIRECTORY_READ_MODEL_AUTHORITY: &str = "authoritative-server-read-model";

const MAX_OPAQUE_ID_BYTES: usize = 128;
const MAX_SAFE_TEXT_BYTES: usize = 256;
/// Product-context pages are bounded before they reach a Dioxus surface.
pub const MAX_PRODUCT_PAGE_SIZE: u32 = 1_000;
const MAX_PRODUCT_PAGE_CURSOR_BYTES: usize = 256;

/// A stable, non-secret identity supplied by an authoritative boundary.
///
/// IDs are intentionally restricted to opaque URL-safe tokens. Directory DNs, URLs, bearer
/// material, and provider payloads never enter this type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OpaqueIdentity(String);

impl OpaqueIdentity {
    /// Validate an opaque identifier for display and route correlation.
    pub fn new(value: impl Into<String>) -> Result<Self, ProductContractError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= MAX_OPAQUE_ID_BYTES
            && !contains_sensitive_marker(&value)
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
        if valid {
            Ok(Self(value))
        } else {
            Err(ProductContractError::InvalidOpaqueIdentity)
        }
    }

    /// Borrow the safe identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Tenant and holding binding for a product-context read.
///
/// Product contexts are never platform-wide. Platform Operations has a separate model and
/// adapter set; a product-row response must carry both identities so a holding response cannot
/// be rendered as a platform response or silently reused for another holding.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProductContextScope {
    tenant_id: OpaqueIdentity,
    holding_id: OpaqueIdentity,
}

impl ProductContextScope {
    /// Construct an explicit holding-bound product scope.
    pub const fn new(tenant_id: OpaqueIdentity, holding_id: OpaqueIdentity) -> Self {
        Self {
            tenant_id,
            holding_id,
        }
    }

    /// Tenant identity attached to the authoritative response.
    pub fn tenant_id(&self) -> &OpaqueIdentity {
        &self.tenant_id
    }

    /// Holding identity attached to the authoritative response.
    pub fn holding_id(&self) -> &OpaqueIdentity {
        &self.holding_id
    }

    /// Safe display label for evidence and QA selectors.
    pub fn label(&self) -> String {
        format!(
            "Tenant {} · Holding {}",
            self.tenant_id.as_str(),
            self.holding_id.as_str()
        )
    }
}

/// Opaque continuation token for a product-context page.
///
/// The cursor is only a transport handoff. It is deliberately not readable after construction,
/// and its debug representation is redacted so a capability-like token cannot enter logs or
/// rendered diagnostics.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ProductPageCursor(String);

impl ProductPageCursor {
    /// Validate a server-supplied continuation token.
    pub fn new(value: impl Into<String>) -> Result<Self, ProductContractError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= MAX_PRODUCT_PAGE_CURSOR_BYTES
            && !contains_sensitive_marker(&value)
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
        if valid {
            Ok(Self(value))
        } else {
            Err(ProductContractError::InvalidProductPageCursor)
        }
    }
}

impl fmt::Debug for ProductPageCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ProductPageCursor")
            .field(&"[redacted]")
            .finish()
    }
}

/// Validated product-context pagination metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductPage {
    returned_count: u32,
    page_size: u32,
    next_cursor: Option<ProductPageCursor>,
}

impl ProductPage {
    /// Validate a page returned by an authoritative product read model.
    pub fn new(
        returned_count: u32,
        page_size: u32,
        next_cursor: Option<ProductPageCursor>,
    ) -> Result<Self, ProductContractError> {
        if page_size == 0 || page_size > MAX_PRODUCT_PAGE_SIZE {
            return Err(ProductContractError::InvalidProductPageSize);
        }
        if returned_count > page_size {
            return Err(ProductContractError::ProductPageCountExceedsSize);
        }
        if returned_count == 0 && next_cursor.is_some() {
            return Err(ProductContractError::EmptyProductPageHasCursor);
        }
        Ok(Self {
            returned_count,
            page_size,
            next_cursor,
        })
    }

    /// Number of rows in this authoritative page.
    pub const fn returned_count(&self) -> u32 {
        self.returned_count
    }

    /// Requested server page size.
    pub const fn page_size(&self) -> u32 {
        self.page_size
    }

    /// Whether the server supplied another page.
    pub const fn has_next_page(&self) -> bool {
        self.next_cursor.is_some()
    }
}

/// Bounded display text. Sensitive-looking input is replaced at construction time.
#[derive(Clone, PartialEq, Eq)]
pub struct SafeDisplayText(String);

impl SafeDisplayText {
    /// Construct display text from an untrusted boundary value.
    pub fn new(value: impl AsRef<str>) -> Self {
        let value = value.as_ref().trim();
        if value.is_empty()
            || value.len() > MAX_SAFE_TEXT_BYTES
            || value.chars().any(char::is_control)
            || contains_sensitive_marker(value)
        {
            return Self("[redacted]".into());
        }
        Self(value.into())
    }

    /// Borrow the sanitized value.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Return whether the value is the client-side redaction sentinel.
    pub fn is_redacted(&self) -> bool {
        self.0 == "[redacted]"
    }
}

impl fmt::Debug for SafeDisplayText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("SafeDisplayText")
            .field(&self.0)
            .finish()
    }
}

impl fmt::Display for SafeDisplayText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn contains_sensitive_marker(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "secret",
        "password",
        "credential",
        "token",
        "access_token",
        "refresh_token",
        "client_secret",
        "api_key",
        "api-key",
        "apikey",
        "api key",
        "authorization",
        "bearer ",
        "cookie",
        "private_key",
        "database_url",
        "postgres://",
        "mysql://",
        "redis://",
        "amqp://",
        "ssh://",
        "ldapi://",
        "ldap://",
        "ldaps://",
        "grpc://",
        "http://",
        "https://",
        "session_id",
        "mfa_code",
        "otp=",
        "one_time_code",
    ]
    .iter()
    .any(|marker| value.contains(marker))
}

/// Directory products supported by the connector contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryVendor {
    /// Generic LDAP with RFC 4533 content synchronization.
    GenericLdap,
    /// On-premises Microsoft Active Directory Domain Services with DirSync.
    ActiveDirectoryDomainServices,
    /// Univention Corporate Server with its Provisioning Service.
    UniventionCorporateServer,
}

impl DirectoryVendor {
    /// Return the required native change feed for this vendor.
    pub const fn native_change_feed(self) -> NativeChangeFeed {
        match self {
            Self::GenericLdap => NativeChangeFeed::Rfc4533,
            Self::ActiveDirectoryDomainServices => NativeChangeFeed::AdDirSync,
            Self::UniventionCorporateServer => NativeChangeFeed::UcsProvisioningService,
        }
    }

    /// Return a stable safe label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::GenericLdap => "Generic LDAP",
            Self::ActiveDirectoryDomainServices => "Microsoft Active Directory Domain Services",
            Self::UniventionCorporateServer => "Univention Corporate Server",
        }
    }
}

/// Native directory change feeds. Polling is intentionally absent from the production contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeChangeFeed {
    /// RFC 4533 content synchronization for generic LDAP.
    Rfc4533,
    /// Active Directory DirSync control.
    AdDirSync,
    /// UCS Provisioning Service feed.
    UcsProvisioningService,
}

impl NativeChangeFeed {
    /// Return a stable transport label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rfc4533 => "RFC 4533",
            Self::AdDirSync => "AD DirSync",
            Self::UcsProvisioningService => "UCS Provisioning Service",
        }
    }
}

/// Connector ownership scope. A holding connector is never implicitly platform-wide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectorScope {
    /// Platform-owned connector used only by platform-authorized operations.
    Platform,
    /// Connector owned by one explicitly identified holding.
    Holding { holding_id: OpaqueIdentity },
}

impl ConnectorScope {
    /// Return a safe scope label for a status surface.
    pub fn label(&self) -> String {
        match self {
            Self::Platform => "Platform".into(),
            Self::Holding { holding_id } => format!("Holding {}", holding_id.as_str()),
        }
    }

    /// Whether this scope is platform-owned.
    pub const fn is_platform(&self) -> bool {
        matches!(self, Self::Platform)
    }
}

/// Full-import capacity supported by a connector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullImportCapability {
    /// The connector is contractually bounded for a full import of one million users.
    UpToOneMillionUsers,
}

impl FullImportCapability {
    /// Return the maximum supported full-import size.
    pub const fn max_users(self) -> u64 {
        MAX_FULL_IMPORT_USERS
    }
}

/// Directory role-mapping ownership mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleMappingMode {
    /// Directory groups are the authoritative role-assignment input.
    DirectoryManaged,
    /// Local-only mapping, which is not acceptable for a directory connector.
    LocalOnly,
}

/// Identity-linking policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityLinkingMode {
    /// External subject to internal principal links cannot be rewritten in place.
    Immutable,
    /// Mutable links are rejected by the ERPX connector contract.
    Mutable,
}

/// Role-revocation persistence policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleRevocationMode {
    /// Revocations are append-only facts and cannot be silently removed.
    AppendOnly,
    /// Replaceable role state is not acceptable for the security surface.
    Replaceable,
}

/// Local fallback policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalFallbackMode {
    /// Local access is limited to an explicitly audited break-glass path.
    BreakGlassOnly,
    /// General local fallback is forbidden by the approved design.
    GeneralLocalFallback,
}

/// High-risk authentication policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighRiskMfaPolicy {
    /// A fresh MFA assertion is required for high-risk access.
    Required { max_age_seconds: u32 },
    /// No MFA gate, which is not acceptable for high-risk access.
    NotRequired,
}

impl HighRiskMfaPolicy {
    /// Whether the policy requires MFA and stays within the five-minute freshness budget.
    pub const fn satisfies_high_risk_requirement(self) -> bool {
        matches!(
            self,
            Self::Required {
                max_age_seconds: 1..=FIVE_MINUTE_REVOCATION_SECONDS
            }
        )
    }

    /// Return the configured freshness budget.
    pub const fn max_age_seconds(self) -> Option<u32> {
        match self {
            Self::Required { max_age_seconds } => Some(max_age_seconds),
            Self::NotRequired => None,
        }
    }
}

/// Revocation timing policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevocationPolicy {
    /// Access and mapped roles must be revocable within five minutes.
    FiveMinutes,
    /// A non-compliant custom budget, retained only so boundary validation can reject it.
    Custom { max_seconds: u32 },
}

impl RevocationPolicy {
    /// Return the maximum revocation age represented by the policy.
    pub const fn max_seconds(self) -> u32 {
        match self {
            Self::FiveMinutes => FIVE_MINUTE_REVOCATION_SECONDS,
            Self::Custom { max_seconds } => max_seconds,
        }
    }
}

/// Security and authorization posture rendered by directory administration surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessSecurityPolicy {
    /// All connector communication must use TLS.
    pub tls_only: bool,
    /// High-risk access gate.
    pub high_risk_mfa: HighRiskMfaPolicy,
    /// Revocation budget.
    pub revocation: RevocationPolicy,
    /// Local fallback policy.
    pub local_fallback: LocalFallbackMode,
}

impl AccessSecurityPolicy {
    /// The approved fail-closed security posture.
    pub const fn strict() -> Self {
        Self {
            tls_only: true,
            high_risk_mfa: HighRiskMfaPolicy::Required {
                max_age_seconds: FIVE_MINUTE_REVOCATION_SECONDS,
            },
            revocation: RevocationPolicy::FiveMinutes,
            local_fallback: LocalFallbackMode::BreakGlassOnly,
        }
    }
}

/// Connector policy presented after the authoritative server boundary validates it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryConnectorPolicy {
    /// Vendor-native change feed.
    pub change_feed: NativeChangeFeed,
    /// Full-import bound.
    pub full_import: FullImportCapability,
    /// Role mapping ownership.
    pub role_mapping: RoleMappingMode,
    /// Identity-linking ownership.
    pub identity_linking: IdentityLinkingMode,
    /// Append-only revocation policy.
    pub role_revocation: RoleRevocationMode,
    /// Transport and access posture.
    pub security: AccessSecurityPolicy,
}

impl DirectoryConnectorPolicy {
    /// Build the approved policy for a vendor.
    pub const fn strict(vendor: DirectoryVendor) -> Self {
        Self {
            change_feed: vendor.native_change_feed(),
            full_import: FullImportCapability::UpToOneMillionUsers,
            role_mapping: RoleMappingMode::DirectoryManaged,
            identity_linking: IdentityLinkingMode::Immutable,
            role_revocation: RoleRevocationMode::AppendOnly,
            security: AccessSecurityPolicy::strict(),
        }
    }
}

/// A validated directory connector contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryConnectorContract {
    connector_id: OpaqueIdentity,
    vendor: DirectoryVendor,
    scope: ConnectorScope,
    policy: DirectoryConnectorPolicy,
    lifecycle: SafeDisplayText,
    readiness: SafeDisplayText,
}

impl DirectoryConnectorContract {
    /// Validate all user-import and security invariants before a connector reaches the UI.
    pub fn new(
        connector_id: OpaqueIdentity,
        vendor: DirectoryVendor,
        scope: ConnectorScope,
        policy: DirectoryConnectorPolicy,
    ) -> Result<Self, ProductContractError> {
        if policy.change_feed != vendor.native_change_feed() {
            return Err(ProductContractError::ConnectorMustUseNativeFeed);
        }
        if policy.full_import.max_users() < MAX_FULL_IMPORT_USERS {
            return Err(ProductContractError::FullImportBelowLimit);
        }
        if policy.role_mapping != RoleMappingMode::DirectoryManaged {
            return Err(ProductContractError::DirectoryRoleMappingRequired);
        }
        if policy.identity_linking != IdentityLinkingMode::Immutable {
            return Err(ProductContractError::ImmutableIdentityLinkRequired);
        }
        if policy.role_revocation != RoleRevocationMode::AppendOnly {
            return Err(ProductContractError::AppendOnlyRevocationRequired);
        }
        if !policy.security.tls_only {
            return Err(ProductContractError::TlsOnlyRequired);
        }
        if policy.security.revocation.max_seconds() != FIVE_MINUTE_REVOCATION_SECONDS {
            return Err(ProductContractError::FiveMinuteRevocationRequired);
        }
        if !policy
            .security
            .high_risk_mfa
            .satisfies_high_risk_requirement()
        {
            return Err(ProductContractError::HighRiskMfaRequired);
        }
        if policy.security.local_fallback != LocalFallbackMode::BreakGlassOnly {
            return Err(ProductContractError::BreakGlassOnlyFallbackRequired);
        }
        Ok(Self {
            connector_id,
            vendor,
            scope,
            policy,
            lifecycle: SafeDisplayText::new("unconfigured"),
            readiness: SafeDisplayText::new("unconfigured"),
        })
    }

    pub fn with_operational_state(
        mut self,
        lifecycle: &str,
        readiness: &str,
    ) -> Result<Self, ProductContractError> {
        if !matches!(
            lifecycle,
            "draft" | "verified" | "importing" | "ready" | "active" | "suspended" | "failed"
        ) || !matches!(readiness, "ready" | "not_ready" | "unconfigured")
        {
            return Err(ProductContractError::InvalidDirectoryLifecycle);
        }
        self.lifecycle = SafeDisplayText::new(lifecycle);
        self.readiness = SafeDisplayText::new(readiness);
        Ok(self)
    }

    /// Connector identifier.
    pub fn connector_id(&self) -> &OpaqueIdentity {
        &self.connector_id
    }

    /// Directory vendor.
    pub const fn vendor(&self) -> DirectoryVendor {
        self.vendor
    }

    /// Connector ownership scope.
    pub fn scope(&self) -> &ConnectorScope {
        &self.scope
    }

    /// Validated policy.
    pub const fn policy(&self) -> &DirectoryConnectorPolicy {
        &self.policy
    }

    pub fn lifecycle(&self) -> &SafeDisplayText {
        &self.lifecycle
    }

    pub fn readiness(&self) -> &SafeDisplayText {
        &self.readiness
    }
}

/// A directory group to ERPX role mapping with append-only revocation semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryRoleMapping {
    directory_group: SafeDisplayText,
    erpx_role: SafeDisplayText,
    scope: ConnectorScope,
    revocation: RoleRevocationMode,
}

impl DirectoryRoleMapping {
    /// Construct a mapping only when its revocation history is append-only.
    pub fn new(
        directory_group: impl AsRef<str>,
        erpx_role: impl AsRef<str>,
        scope: ConnectorScope,
        revocation: RoleRevocationMode,
    ) -> Result<Self, ProductContractError> {
        if revocation != RoleRevocationMode::AppendOnly {
            return Err(ProductContractError::AppendOnlyRevocationRequired);
        }
        Ok(Self {
            directory_group: SafeDisplayText::new(directory_group),
            erpx_role: SafeDisplayText::new(erpx_role),
            scope,
            revocation,
        })
    }

    /// Redacted directory-group label.
    pub fn directory_group(&self) -> &SafeDisplayText {
        &self.directory_group
    }

    /// Redacted ERPX role label.
    pub fn erpx_role(&self) -> &SafeDisplayText {
        &self.erpx_role
    }

    /// Mapping scope.
    pub fn scope(&self) -> &ConnectorScope {
        &self.scope
    }

    /// Revocation mode.
    pub const fn revocation(&self) -> RoleRevocationMode {
        self.revocation
    }
}

/// An immutable external-subject to ERPX-principal link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImmutableIdentityLink {
    external_subject: SafeDisplayText,
    erpx_principal_id: OpaqueIdentity,
}

impl ImmutableIdentityLink {
    /// Create a link whose identity relationship is immutable by construction.
    pub fn new(external_subject: impl AsRef<str>, erpx_principal_id: OpaqueIdentity) -> Self {
        Self {
            external_subject: SafeDisplayText::new(external_subject),
            erpx_principal_id,
        }
    }

    /// Redacted external subject label.
    pub fn external_subject(&self) -> &SafeDisplayText {
        &self.external_subject
    }

    /// Internal principal identifier.
    pub fn erpx_principal_id(&self) -> &OpaqueIdentity {
        &self.erpx_principal_id
    }

    /// This link type has no mutable operation by design.
    pub const fn is_immutable(&self) -> bool {
        true
    }
}

/// Bounded, secret-free full-import progress supplied by the authoritative
/// directory write-side checkpoint. Opaque source cursors and failure text
/// are intentionally not represented here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryImportStatusDisplay {
    connector_id: OpaqueIdentity,
    import_id: OpaqueIdentity,
    state: SafeDisplayText,
    cursor_kind: SafeDisplayText,
    max_users: u64,
    users_seen: u64,
    pages_applied: u64,
    page_count: u64,
    started_at: SafeDisplayText,
    finalized_at: Option<SafeDisplayText>,
    updated_at: SafeDisplayText,
    failure_category: Option<SafeDisplayText>,
}

impl DirectoryImportStatusDisplay {
    // The wire contract is intentionally explicit: each bounded, display-safe checkpoint field
    // is validated at this one boundary instead of being accepted as an untyped blob.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        connector_id: OpaqueIdentity,
        import_id: OpaqueIdentity,
        state: impl AsRef<str>,
        cursor_kind: impl AsRef<str>,
        max_users: u64,
        users_seen: u64,
        pages_applied: u64,
        page_count: u64,
        started_at: impl AsRef<str>,
        finalized_at: Option<impl AsRef<str>>,
        updated_at: impl AsRef<str>,
        failure_category: Option<impl AsRef<str>>,
    ) -> Result<Self, ProductContractError> {
        if !matches!(
            state.as_ref(),
            "running" | "finalizing" | "completed" | "failed" | "rolled_back"
        ) || max_users == 0
            || max_users > MAX_FULL_IMPORT_USERS
            || users_seen > max_users
            || page_count == 0
            || pages_applied > page_count
            || cursor_kind.as_ref().trim().is_empty()
        {
            return Err(ProductContractError::InvalidDirectoryImportStatus);
        }
        Ok(Self {
            connector_id,
            import_id,
            state: SafeDisplayText::new(state),
            cursor_kind: SafeDisplayText::new(cursor_kind),
            max_users,
            users_seen,
            pages_applied,
            page_count,
            started_at: SafeDisplayText::new(started_at),
            finalized_at: finalized_at.map(|value| SafeDisplayText::new(value)),
            updated_at: SafeDisplayText::new(updated_at),
            failure_category: failure_category.map(|value| SafeDisplayText::new(value)),
        })
    }

    pub fn connector_id(&self) -> &OpaqueIdentity {
        &self.connector_id
    }
    pub fn import_id(&self) -> &OpaqueIdentity {
        &self.import_id
    }
    pub fn state(&self) -> &SafeDisplayText {
        &self.state
    }
    pub fn cursor_kind(&self) -> &SafeDisplayText {
        &self.cursor_kind
    }
    pub const fn max_users(&self) -> u64 {
        self.max_users
    }
    pub const fn users_seen(&self) -> u64 {
        self.users_seen
    }
    pub const fn pages_applied(&self) -> u64 {
        self.pages_applied
    }
    pub const fn page_count(&self) -> u64 {
        self.page_count
    }
    pub fn started_at(&self) -> &SafeDisplayText {
        &self.started_at
    }
    pub fn finalized_at(&self) -> Option<&SafeDisplayText> {
        self.finalized_at.as_ref()
    }
    pub fn updated_at(&self) -> &SafeDisplayText {
        &self.updated_at
    }
    pub fn failure_category(&self) -> Option<&SafeDisplayText> {
        self.failure_category.as_ref()
    }
}

/// Directory administration read model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryAdministrationModel {
    connectors: Vec<DirectoryConnectorContract>,
    role_mappings: Vec<DirectoryRoleMapping>,
    identity_links: Vec<ImmutableIdentityLink>,
    identity_links_truncated: bool,
    import_statuses: Vec<DirectoryImportStatusDisplay>,
    runtime_state: SafeDisplayText,
    evidence: PlatformOperationsEvidence,
}

impl DirectoryAdministrationModel {
    /// Build a typed display model from validated connector and identity facts.
    pub fn new(
        connectors: Vec<DirectoryConnectorContract>,
        role_mappings: Vec<DirectoryRoleMapping>,
        identity_links: Vec<ImmutableIdentityLink>,
    ) -> Self {
        Self {
            connectors,
            role_mappings,
            identity_links,
            identity_links_truncated: false,
            import_statuses: Vec::new(),
            runtime_state: SafeDisplayText::new("enabled"),
            evidence: PlatformOperationsEvidence::authoritative(
                ReconciliationEvidence::NotReported,
            ),
        }
    }

    /// Build a directory model from an authorized persisted read boundary with explicit
    /// reconciliation evidence. Pending reconciliation remains visible and read-only; drift
    /// is rejected by the final render boundary.
    pub fn from_authoritative_read_model(
        connectors: Vec<DirectoryConnectorContract>,
        role_mappings: Vec<DirectoryRoleMapping>,
        identity_links: Vec<ImmutableIdentityLink>,
        reconciliation: ReconciliationEvidence,
    ) -> Self {
        Self {
            connectors,
            role_mappings,
            identity_links,
            identity_links_truncated: false,
            import_statuses: Vec::new(),
            runtime_state: SafeDisplayText::new("enabled"),
            evidence: PlatformOperationsEvidence::authoritative(reconciliation),
        }
    }

    /// Build the bounded first page returned by the authoritative identity
    /// link read RPC. The UI must expose that the page is incomplete instead
    /// of presenting a million-user connector as a complete list.
    pub fn with_identity_link_page(
        connectors: Vec<DirectoryConnectorContract>,
        role_mappings: Vec<DirectoryRoleMapping>,
        identity_links: Vec<ImmutableIdentityLink>,
        identity_links_truncated: bool,
    ) -> Self {
        Self {
            connectors,
            role_mappings,
            identity_links,
            identity_links_truncated,
            import_statuses: Vec::new(),
            runtime_state: SafeDisplayText::new("enabled"),
            evidence: PlatformOperationsEvidence::authoritative(
                ReconciliationEvidence::NotReported,
            ),
        }
    }

    /// Validated connector summaries.
    pub fn connectors(&self) -> &[DirectoryConnectorContract] {
        &self.connectors
    }

    /// Directory-managed role mappings.
    pub fn role_mappings(&self) -> &[DirectoryRoleMapping] {
        &self.role_mappings
    }

    /// Immutable identity links.
    pub fn identity_links(&self) -> &[ImmutableIdentityLink] {
        &self.identity_links
    }

    /// Authoritative import checkpoints aligned to visible connector IDs.
    pub fn with_import_statuses(
        mut self,
        import_statuses: Vec<DirectoryImportStatusDisplay>,
    ) -> Self {
        self.import_statuses = import_statuses;
        self
    }

    pub fn import_statuses(&self) -> &[DirectoryImportStatusDisplay] {
        &self.import_statuses
    }

    pub fn with_runtime_state(mut self, runtime_state: &str) -> Self {
        self.runtime_state = SafeDisplayText::new(runtime_state);
        self
    }

    pub fn runtime_state(&self) -> &SafeDisplayText {
        &self.runtime_state
    }

    /// Whether the server returned more immutable identity links than the
    /// bounded page rendered by this surface.
    pub const fn identity_links_truncated(&self) -> bool {
        self.identity_links_truncated
    }

    /// Return whether at least one platform-owned connector is present.
    pub fn has_platform_connector(&self) -> bool {
        self.connectors
            .iter()
            .any(|connector| connector.scope().is_platform())
    }

    /// Count connectors explicitly owned by a holding.
    pub fn holding_connector_count(&self) -> usize {
        self.connectors
            .iter()
            .filter(|connector| matches!(connector.scope(), ConnectorScope::Holding { .. }))
            .count()
    }

    /// Persistence evidence for the displayed directory model.
    pub const fn persistence(&self) -> PersistenceEvidence {
        self.evidence.persistence()
    }

    /// Reconciliation evidence supplied by the directory read boundary.
    pub const fn reconciliation(&self) -> ReconciliationEvidence {
        self.evidence.reconciliation()
    }

    /// Authorization evidence supplied by the directory read boundary.
    pub const fn authorization(&self) -> AuthorizationEvidence {
        self.evidence.authorization()
    }

    /// Return whether every displayed fact satisfies the directory security contract.
    ///
    /// The constructor for each child fact already validates its invariants. This aggregate
    /// check also requires a platform connector and a connector scope for every role mapping;
    /// otherwise an empty or holding-only snapshot could be mistaken for a complete platform
    /// directory configuration by the UI.
    pub fn is_contract_compliant(&self) -> bool {
        if self.runtime_state.as_str() == "disabled" {
            return self.connectors.is_empty()
                && self.role_mappings.is_empty()
                && self.identity_links.is_empty()
                && self.import_statuses.is_empty();
        }
        !self.connectors.is_empty()
            && self.has_platform_connector()
            && self.connectors.iter().all(|connector| {
                let policy = connector.policy();
                policy.change_feed == connector.vendor().native_change_feed()
                    && policy.full_import.max_users() >= MAX_FULL_IMPORT_USERS
                    && policy.role_mapping == RoleMappingMode::DirectoryManaged
                    && policy.identity_linking == IdentityLinkingMode::Immutable
                    && policy.role_revocation == RoleRevocationMode::AppendOnly
                    && policy.security.tls_only
                    && policy.security.revocation.max_seconds() == FIVE_MINUTE_REVOCATION_SECONDS
                    && policy
                        .security
                        .high_risk_mfa
                        .satisfies_high_risk_requirement()
                    && policy.security.local_fallback == LocalFallbackMode::BreakGlassOnly
            })
            && self.role_mappings.iter().all(|mapping| {
                mapping.revocation() == RoleRevocationMode::AppendOnly
                    && self
                        .connectors
                        .iter()
                        .any(|connector| connector.scope() == mapping.scope())
            })
            && self
                .identity_links
                .iter()
                .all(ImmutableIdentityLink::is_immutable)
    }

    /// Directory surfaces never expose commands; all displayed facts are read-only.
    pub const fn is_read_only(&self) -> bool {
        true
    }
}

/// The directory surface never exposes a client rollback command. Reconciliation rollback and
/// replay remain server/worker-owned, preserving immutable identity links and append-only role
/// revocation facts.
pub const DIRECTORY_ROLLBACK_BOUNDARY: &str =
    "Server/worker-owned rollback and replay; no client action is available.";

/// Platform Operations areas represented by the read-only console surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformOperationsArea {
    /// Tenant registry and safe tenant status.
    Tenants,
    /// Projection freshness and bounded retry status.
    Projections,
    /// Backup history metadata.
    Backups,
    /// Provider connector registry.
    Providers,
    /// Categorical integration health.
    Integrations,
    /// Customer-visible platform audit.
    Audit,
    /// Architecture/settings metadata.
    Settings,
    /// Expiring-role review and recovery workflow.
    RoleReview,
}

/// Stable Platform Operations area order.
pub const PLATFORM_OPERATIONS_AREAS: &[PlatformOperationsArea] = &[
    PlatformOperationsArea::Tenants,
    PlatformOperationsArea::Projections,
    PlatformOperationsArea::Backups,
    PlatformOperationsArea::Providers,
    PlatformOperationsArea::Integrations,
    PlatformOperationsArea::Audit,
    PlatformOperationsArea::Settings,
    PlatformOperationsArea::RoleReview,
];

impl PlatformOperationsArea {
    /// Return the canonical Platform Operations area order.
    pub const fn all() -> &'static [Self] {
        PLATFORM_OPERATIONS_AREAS
    }

    /// Return the safe display label for this authenticated read area.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Tenants => "Tenants",
            Self::Projections => "Projections",
            Self::Backups => "Backups",
            Self::Providers => "Providers",
            Self::Integrations => "Integrations",
            Self::Audit => "Audit",
            Self::Settings => "Settings",
            Self::RoleReview => "Role review",
        }
    }

    /// Locale-aware area label while preserving the stable English API.
    pub const fn label_for(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Tenants, Locale::Th) => "ผู้เช่า",
            (Self::Projections, Locale::Th) => "โพรเจกชัน",
            (Self::Backups, Locale::Th) => "ข้อมูลสำรอง",
            (Self::Providers, Locale::Th) => "ผู้ให้บริการ",
            (Self::Integrations, Locale::Th) => "การเชื่อมต่อระบบ",
            (Self::Audit, Locale::Th) => "การตรวจสอบ",
            (Self::Settings, Locale::Th) => "การตั้งค่า",
            (Self::RoleReview, Locale::Th) => "การทบทวนบทบาท",
            (_, Locale::En) => self.label(),
        }
    }

    /// Canonical read path.
    pub const fn path(self) -> &'static str {
        match self {
            Self::Tenants => "/platform/tenants",
            Self::Projections => "/platform/projections",
            Self::Backups => "/platform/backups",
            Self::Providers => "/platform/provider",
            Self::Integrations => "/platform/integrations",
            Self::Audit => "/platform/audit",
            Self::Settings => "/platform/settings",
            Self::RoleReview => "/platform/role-review",
        }
    }
}

/// The coordinator-owned authenticated adapter behind a Platform Operations area.
///
/// This is an allow-list, not a user-provided string. Keeping the adapter identity typed makes
/// it impossible for a generic status row to claim that it came from an authenticated route
/// without naming the real client boundary that supplied it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformOperationsAdapter {
    /// `PlatformOperationsService::ListPlatformTenants`.
    ListPlatformTenants,
    /// `PlatformOperationsService::ListProjectionStatuses`.
    ListProjectionStatuses,
    /// `PlatformOperationsService::ListTenantBackupJobs`.
    ListTenantBackupJobs,
    /// `PlatformAdministrationQueryService::ListConnectors`.
    ListPlatformConnectors,
    /// `PlatformOperationsService::ListPlatformIntegrations`.
    ListPlatformIntegrations,
    /// `PlatformOperationsService::ListPlatformAudit`.
    ListPlatformAudit,
    /// `PlatformOperationsService::GetPlatformSettings`.
    GetPlatformSettings,
    /// `RoleLifecycleService::ListRoleReview`.
    ListRoleReview,
}

impl PlatformOperationsAdapter {
    /// Return the stable adapter identifier used by QA selectors and audit evidence.
    pub const fn id(self) -> &'static str {
        match self {
            Self::ListPlatformTenants => "platform.list_tenants",
            Self::ListProjectionStatuses => "platform.list_projection_statuses",
            Self::ListTenantBackupJobs => "platform.list_tenant_backup_jobs",
            Self::ListPlatformConnectors => "platform.list_connectors",
            Self::ListPlatformIntegrations => "platform.list_integrations",
            Self::ListPlatformAudit => "platform.list_audit",
            Self::GetPlatformSettings => "platform.get_settings",
            Self::ListRoleReview => "role.list_review",
        }
    }

    /// Return the adapter that must provide the area read model.
    pub const fn for_area(area: PlatformOperationsArea) -> Self {
        match area {
            PlatformOperationsArea::Tenants => Self::ListPlatformTenants,
            PlatformOperationsArea::Projections => Self::ListProjectionStatuses,
            PlatformOperationsArea::Backups => Self::ListTenantBackupJobs,
            PlatformOperationsArea::Providers => Self::ListPlatformConnectors,
            PlatformOperationsArea::Integrations => Self::ListPlatformIntegrations,
            PlatformOperationsArea::Audit => Self::ListPlatformAudit,
            PlatformOperationsArea::Settings => Self::GetPlatformSettings,
            PlatformOperationsArea::RoleReview => Self::ListRoleReview,
        }
    }
}

/// Per-area evidence supplied by a real authenticated Platform Operations adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformOperationsAreaEvidence {
    area: PlatformOperationsArea,
    adapter: PlatformOperationsAdapter,
    persistence: PersistenceEvidence,
    reconciliation: ReconciliationEvidence,
    authorization: AuthorizationEvidence,
}

impl PlatformOperationsAreaEvidence {
    /// Build evidence for a server-authorized persisted read from the required adapter.
    pub const fn authoritative(
        area: PlatformOperationsArea,
        reconciliation: ReconciliationEvidence,
    ) -> Self {
        Self {
            area,
            adapter: PlatformOperationsAdapter::for_area(area),
            persistence: PersistenceEvidence::AuthoritativeReadModel,
            reconciliation,
            authorization: AuthorizationEvidence::ServerAuthorized,
        }
    }

    /// Area covered by this evidence.
    pub const fn area(self) -> PlatformOperationsArea {
        self.area
    }

    /// Real coordinator adapter represented by this evidence.
    pub const fn adapter(self) -> PlatformOperationsAdapter {
        self.adapter
    }

    /// Persistence evidence.
    pub const fn persistence(self) -> PersistenceEvidence {
        self.persistence
    }

    /// Reconciliation evidence.
    pub const fn reconciliation(self) -> ReconciliationEvidence {
        self.reconciliation
    }

    /// Authorization evidence.
    pub const fn authorization(self) -> AuthorizationEvidence {
        self.authorization
    }
}

/// Platform Operations read model. It contains status metadata only, never provider secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformOperationsModel {
    areas: Vec<PlatformOperationsArea>,
    connector_scopes: Vec<ConnectorScope>,
    evidence: PlatformOperationsEvidence,
    area_evidence: Vec<PlatformOperationsAreaEvidence>,
}

/// Typed evidence attached to a Platform Operations read model.
///
/// The default constructor is deliberately conservative: area metadata alone is not proof that
/// a server-authorized persisted read was available. The coordinator must use
/// `from_authoritative_read_model` after receiving the corresponding boundary evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformOperationsEvidence {
    persistence: PersistenceEvidence,
    reconciliation: ReconciliationEvidence,
    authorization: AuthorizationEvidence,
}

impl PlatformOperationsEvidence {
    /// Evidence for a read model supplied by the authorized persisted boundary.
    pub const fn authoritative(reconciliation: ReconciliationEvidence) -> Self {
        Self {
            persistence: PersistenceEvidence::AuthoritativeReadModel,
            reconciliation,
            authorization: AuthorizationEvidence::ServerAuthorized,
        }
    }

    /// Conservative evidence when only local route/area metadata is available.
    pub const fn unavailable() -> Self {
        Self {
            persistence: PersistenceEvidence::Unavailable,
            reconciliation: ReconciliationEvidence::NotReported,
            authorization: AuthorizationEvidence::Unavailable,
        }
    }

    /// Persistence evidence.
    pub const fn persistence(self) -> PersistenceEvidence {
        self.persistence
    }

    /// Reconciliation evidence.
    pub const fn reconciliation(self) -> ReconciliationEvidence {
        self.reconciliation
    }

    /// Authorization evidence.
    pub const fn authorization(self) -> AuthorizationEvidence {
        self.authorization
    }
}

impl PlatformOperationsModel {
    /// Construct route metadata without claiming authoritative data evidence.
    pub fn new(areas: Vec<PlatformOperationsArea>, connector_scopes: Vec<ConnectorScope>) -> Self {
        Self {
            areas,
            connector_scopes,
            evidence: PlatformOperationsEvidence::unavailable(),
            area_evidence: Vec::new(),
        }
    }

    /// Construct a Platform Operations model from an authorized persisted read boundary.
    pub fn from_authoritative_read_model(
        areas: Vec<PlatformOperationsArea>,
        connector_scopes: Vec<ConnectorScope>,
        reconciliation: ReconciliationEvidence,
    ) -> Self {
        let area_evidence = areas
            .iter()
            .copied()
            .map(|area| PlatformOperationsAreaEvidence::authoritative(area, reconciliation))
            .collect();
        Self {
            areas,
            connector_scopes,
            evidence: PlatformOperationsEvidence::authoritative(reconciliation),
            area_evidence,
        }
    }

    /// Construct a model only when every listed area has exactly one real adapter attestation.
    pub fn from_authoritative_area_reads(
        areas: Vec<PlatformOperationsArea>,
        connector_scopes: Vec<ConnectorScope>,
        area_evidence: Vec<PlatformOperationsAreaEvidence>,
    ) -> Result<Self, ProductContractError> {
        if areas.is_empty() || area_evidence.len() != areas.len() {
            return Err(ProductContractError::PlatformAdapterCoverageIncomplete);
        }

        for (index, area) in areas.iter().enumerate() {
            let Some(attestation) = area_evidence.get(index) else {
                return Err(ProductContractError::PlatformAdapterCoverageIncomplete);
            };
            if attestation.area() != *area
                || attestation.adapter() != PlatformOperationsAdapter::for_area(*area)
                || attestation.persistence() != PersistenceEvidence::AuthoritativeReadModel
                || attestation.authorization() != AuthorizationEvidence::ServerAuthorized
            {
                return Err(ProductContractError::PlatformAdapterEvidenceMismatch);
            }
        }

        let reconciliation = area_evidence[0].reconciliation();
        if area_evidence
            .iter()
            .any(|attestation| attestation.reconciliation() != reconciliation)
        {
            return Err(ProductContractError::PlatformAdapterEvidenceMismatch);
        }

        Ok(Self {
            areas,
            connector_scopes,
            evidence: PlatformOperationsEvidence::authoritative(reconciliation),
            area_evidence,
        })
    }

    /// Areas with server-authorized read adapters.
    pub fn areas(&self) -> &[PlatformOperationsArea] {
        &self.areas
    }

    /// Connector ownership scopes visible to the platform surface.
    pub fn connector_scopes(&self) -> &[ConnectorScope] {
        &self.connector_scopes
    }

    /// Per-area adapter attestations. An empty slice means the model is route metadata only.
    pub fn area_evidence(&self) -> &[PlatformOperationsAreaEvidence] {
        &self.area_evidence
    }

    /// Whether every displayed area is backed by its expected authenticated adapter.
    pub fn has_complete_adapter_coverage(&self) -> bool {
        self.area_evidence.len() == self.areas.len()
            && self.areas.iter().enumerate().all(|(index, area)| {
                self.area_evidence.get(index).is_some_and(|attestation| {
                    attestation.area() == *area
                        && attestation.adapter() == PlatformOperationsAdapter::for_area(*area)
                        && attestation.persistence() == PersistenceEvidence::AuthoritativeReadModel
                        && attestation.authorization() == AuthorizationEvidence::ServerAuthorized
                })
            })
    }

    /// Evidence for the displayed Platform Operations model.
    pub const fn evidence(&self) -> PlatformOperationsEvidence {
        self.evidence
    }

    /// Persistence evidence for the displayed Platform Operations model.
    pub const fn persistence(&self) -> PersistenceEvidence {
        self.evidence.persistence()
    }

    /// Reconciliation evidence for the displayed Platform Operations model.
    pub const fn reconciliation(&self) -> ReconciliationEvidence {
        self.evidence.reconciliation()
    }

    /// Authorization evidence for the displayed Platform Operations model.
    pub const fn authorization(&self) -> AuthorizationEvidence {
        self.evidence.authorization()
    }
}

/// Product contexts represented by the target-neutral read surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductContext {
    /// Cross-domain coordination and exception projections.
    Operations,
    /// Journals, subledgers, cash, banking, and close projections.
    Finance,
    /// Sales pipeline and order-to-cash context.
    Sales,
    /// Procurement and purchase-to-pay context.
    Procurement,
    /// Inventory, catalog, and stock context.
    Inventory,
    /// Asset lifecycle context.
    Assets,
    /// Maintenance and capacity context.
    Maintenance,
    /// Quality inspections and release disposition context.
    Quality,
    /// Marketing and CRM context.
    MarketingCrm,
    /// HR, leave, expense, and payroll context.
    HrPayroll,
    /// POS sales, payments, and shift-close context.
    Pos,
    /// Legal contracts, documents, and retention context.
    Legal,
}

/// All ERPX product contexts in stable QA order.
pub const PRODUCT_CONTEXTS: &[ProductContext] = &[
    ProductContext::Operations,
    ProductContext::Finance,
    ProductContext::Sales,
    ProductContext::Procurement,
    ProductContext::Inventory,
    ProductContext::Assets,
    ProductContext::Maintenance,
    ProductContext::Quality,
    ProductContext::MarketingCrm,
    ProductContext::HrPayroll,
    ProductContext::Pos,
    ProductContext::Legal,
];

impl ProductContext {
    pub const fn business_unit(self) -> BusinessUnitId {
        match self {
            Self::Operations => BusinessUnitId::Operations,
            Self::Finance => BusinessUnitId::Finance,
            Self::Sales => BusinessUnitId::Sales,
            Self::Procurement => BusinessUnitId::Procurement,
            Self::Inventory => BusinessUnitId::Inventory,
            Self::Assets => BusinessUnitId::Assets,
            Self::Maintenance => BusinessUnitId::Maintenance,
            Self::Quality => BusinessUnitId::Quality,
            Self::MarketingCrm => BusinessUnitId::MarketingCrm,
            Self::HrPayroll => BusinessUnitId::HrPayroll,
            Self::Pos => BusinessUnitId::PointOfSale,
            Self::Legal => BusinessUnitId::Legal,
        }
    }

    /// Canonical product path prefix.
    pub const fn path(self) -> &'static str {
        match self {
            Self::Operations => "/operations",
            Self::Finance => "/finance",
            Self::Sales => "/sales",
            Self::Procurement => "/procurement",
            Self::Inventory => "/inventory",
            Self::Assets => "/asset",
            Self::Maintenance => "/maintenance",
            Self::Quality => "/quality",
            Self::MarketingCrm => "/marketing",
            Self::HrPayroll => "/people",
            Self::Pos => "/pos",
            Self::Legal => "/legal",
        }
    }

    /// Stable display label.
    pub const fn label(self) -> &'static str {
        self.business_unit().route().label_en
    }

    /// Locale-aware product label without changing the stable English API.
    pub const fn label_for(self, locale: Locale) -> &'static str {
        match locale {
            Locale::En => self.business_unit().route().label_en,
            Locale::Th => self.business_unit().route().label_th,
        }
    }
}

/// Projection freshness for context surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionFreshness {
    /// The read model is within the declared freshness budget.
    Fresh,
    /// The read model is beyond the declared freshness budget.
    Stale,
    /// Freshness was not supplied by the read boundary.
    Unknown,
}

impl ProjectionFreshness {
    /// Parse the bounded freshness value returned by the authoritative projection read.
    ///
    /// Unknown server values stay unknown and therefore cannot be rendered as fresh.
    pub fn from_authoritative_label(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "fresh" => Self::Fresh,
            "stale" => Self::Stale,
            _ => Self::Unknown,
        }
    }

    /// Stable selector value for QA and telemetry-safe DOM inspection.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        }
    }

    /// Safe human-readable value for the evidence card.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fresh => "Fresh projection",
            Self::Stale => "Stale projection",
            Self::Unknown => "Projection freshness unavailable",
        }
    }

    pub const fn label_for(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Fresh, Locale::Th) => "โพรเจกชันเป็นปัจจุบัน",
            (Self::Stale, Locale::Th) => "โพรเจกชันล้าสมัย",
            (Self::Unknown, Locale::Th) => "ไม่มีข้อมูลความใหม่ของโพรเจกชัน",
            (_, Locale::En) => self.label(),
        }
    }
}

/// Evidence that a context surface contains product rows rather than only projection status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductRowEvidence {
    /// Product rows were supplied by an authoritative product read model.
    Authoritative,
    /// The route has no product-row evidence to render.
    Unavailable,
    /// The surface is not a product-context row surface.
    NotApplicable,
}

impl ProductRowEvidence {
    /// Stable selector value for QA and telemetry-safe DOM inspection.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Authoritative => "authoritative",
            Self::Unavailable => "unavailable",
            Self::NotApplicable => "not-applicable",
        }
    }
}

/// Persistence evidence supplied by the authorized read boundary.
///
/// The console may render this fact, but it never infers persistence health from a local
/// cache. `AuthoritativeReadModel` means the server has supplied a persisted projection read;
/// `Unavailable` is rendered explicitly when that evidence is not present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceEvidence {
    /// The displayed snapshot came from an authoritative persisted read model.
    AuthoritativeReadModel,
    /// No persisted read-model evidence is available to render.
    Unavailable,
}

impl PersistenceEvidence {
    /// Stable selector value for QA and telemetry-safe DOM inspection.
    pub const fn key(self) -> &'static str {
        match self {
            Self::AuthoritativeReadModel => "authoritative-read-model",
            Self::Unavailable => "unavailable",
        }
    }

    /// Safe human-readable value for the evidence card.
    pub const fn label(self) -> &'static str {
        match self {
            Self::AuthoritativeReadModel => "Authoritative persisted read model",
            Self::Unavailable => "Persistence evidence unavailable",
        }
    }

    pub const fn label_for(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::AuthoritativeReadModel, Locale::Th) => "ข้อมูลอ่านที่จัดเก็บและเป็นข้อยุติ",
            (Self::Unavailable, Locale::Th) => "ไม่มีหลักฐานการจัดเก็บ",
            (_, Locale::En) => self.label(),
        }
    }
}

/// Reconciliation evidence for a context projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciliationEvidence {
    /// Source and target facts have been reconciled by the authoritative worker/read boundary.
    Reconciled,
    /// Reconciliation is still running; the surface must remain read-only.
    Pending,
    /// Reconciliation found drift; the surface must not imply completeness.
    DriftDetected,
    /// The read boundary did not provide reconciliation evidence.
    NotReported,
}

impl ReconciliationEvidence {
    /// Stable selector value for QA and telemetry-safe DOM inspection.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Reconciled => "reconciled",
            Self::Pending => "pending",
            Self::DriftDetected => "drift-detected",
            Self::NotReported => "not-reported",
        }
    }

    /// Safe human-readable value for the evidence card.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Reconciled => "Reconciled",
            Self::Pending => "Reconciliation pending",
            Self::DriftDetected => "Reconciliation drift detected",
            Self::NotReported => "Reconciliation evidence unavailable",
        }
    }

    pub const fn label_for(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Reconciled, Locale::Th) => "กระทบยอดแล้ว",
            (Self::Pending, Locale::Th) => "รอกระทบยอด",
            (Self::DriftDetected, Locale::Th) => "พบความคลาดเคลื่อนจากการกระทบยอด",
            (Self::NotReported, Locale::Th) => "ไม่มีหลักฐานการกระทบยอด",
            (_, Locale::En) => self.label(),
        }
    }
}

/// Authorization evidence for the product read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationEvidence {
    /// The server authorized the displayed read model.
    ServerAuthorized,
    /// The server denied the read.
    Denied,
    /// A policy or dependency blocks the read from being presented as available.
    Blocked,
    /// Authorization has not completed yet.
    Unavailable,
}

impl AuthorizationEvidence {
    /// Stable selector value for QA and telemetry-safe DOM inspection.
    pub const fn key(self) -> &'static str {
        match self {
            Self::ServerAuthorized => "server-authorized",
            Self::Denied => "denied",
            Self::Blocked => "blocked",
            Self::Unavailable => "unavailable",
        }
    }

    /// Safe human-readable value for the evidence card.
    pub const fn label(self) -> &'static str {
        match self {
            Self::ServerAuthorized => "Server-authorized read",
            Self::Denied => "Read denied by server",
            Self::Blocked => "Read blocked by policy or dependency",
            Self::Unavailable => "Authorization evidence unavailable",
        }
    }

    pub const fn label_for(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::ServerAuthorized, Locale::Th) => "ข้อมูลอ่านที่เซิร์ฟเวอร์อนุญาต",
            (Self::Denied, Locale::Th) => "เซิร์ฟเวอร์ปฏิเสธการอ่าน",
            (Self::Blocked, Locale::Th) => "นโยบายหรือส่วนประกอบที่เกี่ยวข้องบล็อกการอ่าน",
            (Self::Unavailable, Locale::Th) => "ไม่มีหลักฐานการอนุญาต",
            (_, Locale::En) => self.label(),
        }
    }
}

/// Typed context snapshot shared by Operations, Finance, and product contexts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSurfaceModel {
    context: ProductContext,
    projection: ProjectionFreshness,
    /// Product-row evidence is absent for the projection-status route.
    visible_row_count: Option<u64>,
    /// Product-row responses must be bound to one tenant and holding. Projection-status-only
    /// responses intentionally leave this absent because they do not contain product rows.
    scope: Option<ProductContextScope>,
    /// Product-row responses must carry validated pagination metadata.
    pagination: Option<ProductPage>,
    /// Number of authoritative projection-status rows returned for this route.
    projection_status_count: u64,
    redacted_field_count: u32,
    persistence: PersistenceEvidence,
    reconciliation: ReconciliationEvidence,
    authorization: AuthorizationEvidence,
    read_only: bool,
}

impl ContextSurfaceModel {
    /// Construct a read-only context route from projection-status metadata.
    ///
    /// The coordinator currently supplies the number of matching projection-status records in
    /// these positional arguments. Those records are authoritative status evidence, not product
    /// rows, so the values are deliberately not promoted into `visible_row_count`.
    pub const fn new(
        context: ProductContext,
        projection: ProjectionFreshness,
        projection_status_count: u64,
        _redacted_field_count: u32,
    ) -> Self {
        Self {
            context,
            projection,
            visible_row_count: None,
            scope: None,
            pagination: None,
            projection_status_count,
            redacted_field_count: 0,
            persistence: PersistenceEvidence::AuthoritativeReadModel,
            reconciliation: ReconciliationEvidence::NotReported,
            authorization: AuthorizationEvidence::ServerAuthorized,
            read_only: true,
        }
    }

    /// Construct a context snapshot from an authoritative projection-status read model.
    ///
    /// Projection status is persisted and server-authorized, but it does not contain product
    /// rows. Product rows remain unavailable until a product-specific read model supplies them.
    pub const fn from_projection_status_read_model(
        context: ProductContext,
        projection: ProjectionFreshness,
    ) -> Self {
        Self {
            context,
            projection,
            visible_row_count: None,
            scope: None,
            pagination: None,
            projection_status_count: 0,
            redacted_field_count: 0,
            persistence: PersistenceEvidence::AuthoritativeReadModel,
            reconciliation: ReconciliationEvidence::NotReported,
            authorization: AuthorizationEvidence::ServerAuthorized,
            read_only: true,
        }
    }

    /// Construct a context snapshot from the server-authorized persisted projection boundary.
    ///
    /// Reconciliation is deliberately supplied by the boundary rather than inferred from row
    /// counts. This prevents a populated projection from being presented as complete when its
    /// source/target reconciliation is pending or has detected drift. This compatibility form
    /// intentionally lacks product scope and page metadata; the authoritative wrapper and final
    /// render guard reject it as product-row data.
    pub const fn from_authoritative_read_model(
        context: ProductContext,
        projection: ProjectionFreshness,
        visible_row_count: u64,
        redacted_field_count: u32,
        reconciliation: ReconciliationEvidence,
    ) -> Self {
        Self {
            context,
            projection,
            visible_row_count: Some(visible_row_count),
            scope: None,
            pagination: None,
            projection_status_count: 0,
            redacted_field_count,
            persistence: PersistenceEvidence::AuthoritativeReadModel,
            reconciliation,
            authorization: AuthorizationEvidence::ServerAuthorized,
            read_only: true,
        }
    }

    /// Construct a product-row snapshot only from an explicit holding-bound page.
    ///
    /// The older unscoped constructor remains available for projection-status compatibility, but
    /// its product rows are rejected by the authoritative wrapper and final render guard.
    pub fn from_authoritative_scoped_read_model(
        context: ProductContext,
        projection: ProjectionFreshness,
        scope: ProductContextScope,
        page: ProductPage,
        redacted_field_count: u32,
        reconciliation: ReconciliationEvidence,
    ) -> Self {
        Self {
            context,
            projection,
            visible_row_count: Some(u64::from(page.returned_count())),
            scope: Some(scope),
            pagination: Some(page),
            projection_status_count: 0,
            redacted_field_count,
            persistence: PersistenceEvidence::AuthoritativeReadModel,
            reconciliation,
            authorization: AuthorizationEvidence::ServerAuthorized,
            read_only: true,
        }
    }

    /// Product context.
    pub const fn context(&self) -> ProductContext {
        self.context
    }

    /// Projection freshness.
    pub const fn projection(&self) -> ProjectionFreshness {
        self.projection
    }

    /// Number of product rows allowed into the display model, when supplied by a product read.
    pub const fn visible_row_count(&self) -> Option<u64> {
        self.visible_row_count
    }

    /// Holding/tenant binding for product rows, when this is a product read rather than a
    /// projection-status-only response.
    pub fn scope(&self) -> Option<&ProductContextScope> {
        self.scope.as_ref()
    }

    /// Validated page metadata for product rows, when present.
    pub fn pagination(&self) -> Option<&ProductPage> {
        self.pagination.as_ref()
    }

    /// Number of authoritative projection-status rows returned for this context route.
    pub const fn projection_status_count(&self) -> u64 {
        self.projection_status_count
    }

    /// Stable QA value for whether product rows, rather than only projection status, are present.
    pub const fn product_row_evidence(&self) -> ProductRowEvidence {
        if self.visible_row_count.is_some() {
            ProductRowEvidence::Authoritative
        } else {
            ProductRowEvidence::Unavailable
        }
    }

    /// Number of fields removed by the redaction boundary.
    pub const fn redacted_field_count(&self) -> u32 {
        self.redacted_field_count
    }

    /// Persistence evidence for the displayed data.
    pub const fn persistence(&self) -> PersistenceEvidence {
        self.persistence
    }

    /// Reconciliation evidence supplied by the authoritative boundary.
    pub const fn reconciliation(&self) -> ReconciliationEvidence {
        self.reconciliation
    }

    /// Authorization evidence supplied by the authoritative boundary.
    pub const fn authorization(&self) -> AuthorizationEvidence {
        self.authorization
    }

    /// Context surfaces are read-only until authoritative commands are integrated.
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }
}

/// The complete set of typed ERPX product-surface models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductSurfaceModel {
    /// Directory administration model.
    DirectoryAdministration(DirectoryAdministrationModel),
    /// Platform Operations model.
    PlatformOperations(PlatformOperationsModel),
    /// Operations/Finance/product-context model.
    Context(ContextSurfaceModel),
}

impl ProductSurfaceModel {
    /// Return the surface represented by this model.
    pub const fn surface(&self) -> ProductSurface {
        match self {
            Self::DirectoryAdministration(_) => ProductSurface::DirectoryAdministration,
            Self::PlatformOperations(_) => ProductSurface::PlatformOperations,
            Self::Context(model) => ProductSurface::Context(model.context()),
        }
    }

    /// Return a bounded, non-secret summary for a status card.
    pub fn summary(&self) -> String {
        match self {
            Self::DirectoryAdministration(model) => format!(
                "{} connector(s), {} role mapping(s), {} immutable identity link(s)",
                model.connectors().len(),
                model.role_mappings().len(),
                model.identity_links().len()
            ),
            Self::PlatformOperations(model) => {
                format!(
                    "{} read area(s), {} connector scope(s)",
                    model.areas().len(),
                    model.connector_scopes().len()
                )
            }
            Self::Context(model) => match model.visible_row_count() {
                Some(visible_row_count) => format!(
                    "{} visible row(s), {} redacted field(s)",
                    visible_row_count,
                    model.redacted_field_count()
                ),
                None if model.projection_status_count() > 0 => format!(
                    "{} authoritative projection status row(s); product rows unavailable",
                    model.projection_status_count()
                ),
                None => "Projection status available; product rows unavailable".into(),
            },
        }
    }

    pub fn summary_for(&self, locale: Locale) -> String {
        if locale == Locale::En {
            return self.summary();
        }
        let number = |value: usize| locale.formatter().format_integer(value as i64);
        match self {
            Self::DirectoryAdministration(model) => format!(
                "คอนเนกเตอร์ {} รายการ, การจับคู่บทบาท {} รายการ, การเชื่อมโยงตัวตนที่แก้ไขไม่ได้ {} รายการ",
                number(model.connectors().len()),
                number(model.role_mappings().len()),
                number(model.identity_links().len())
            ),
            Self::PlatformOperations(model) => format!(
                "พื้นที่ข้อมูลอ่าน {} รายการ, ขอบเขตคอนเนกเตอร์ {} รายการ",
                number(model.areas().len()),
                number(model.connector_scopes().len())
            ),
            Self::Context(model) => match model.visible_row_count() {
                Some(visible) => format!(
                    "แถวที่มองเห็น {} แถว, ฟิลด์ที่ปกปิด {} ฟิลด์",
                    locale.formatter().format_integer(visible as i64),
                    locale
                        .formatter()
                        .format_integer(i64::from(model.redacted_field_count()))
                ),
                None if model.projection_status_count() > 0 => format!(
                    "สถานะโพรเจกชันที่เป็นข้อยุติ {} แถว; ไม่มีแถวข้อมูลผลิตภัณฑ์",
                    locale
                        .formatter()
                        .format_integer(model.projection_status_count() as i64)
                ),
                None => "มีสถานะโพรเจกชัน; ไม่มีแถวข้อมูลผลิตภัณฑ์".into(),
            },
        }
    }

    /// Whether this model is explicitly read-only.
    pub const fn is_read_only(&self) -> bool {
        match self {
            Self::DirectoryAdministration(_) | Self::PlatformOperations(_) => true,
            Self::Context(model) => model.is_read_only(),
        }
    }

    /// Return projection freshness when the model is a product-context projection.
    pub const fn projection_freshness(&self) -> ProjectionFreshness {
        match self {
            Self::Context(model) => model.projection(),
            Self::DirectoryAdministration(_) | Self::PlatformOperations(_) => {
                ProjectionFreshness::Unknown
            }
        }
    }

    /// Stable QA value for product-row evidence on this model.
    pub const fn product_row_evidence(&self) -> ProductRowEvidence {
        match self {
            Self::Context(model) => model.product_row_evidence(),
            Self::DirectoryAdministration(_) | Self::PlatformOperations(_) => {
                ProductRowEvidence::NotApplicable
            }
        }
    }

    /// Holding/tenant binding for a product-context row model.
    pub fn product_scope(&self) -> Option<&ProductContextScope> {
        match self {
            Self::Context(model) => model.scope(),
            Self::DirectoryAdministration(_) | Self::PlatformOperations(_) => None,
        }
    }

    /// Pagination metadata for a product-context row model.
    pub fn product_pagination(&self) -> Option<&ProductPage> {
        match self {
            Self::Context(model) => model.pagination(),
            Self::DirectoryAdministration(_) | Self::PlatformOperations(_) => None,
        }
    }

    /// Persistence evidence for any model accepted by the product route adapter.
    pub const fn persistence(&self) -> PersistenceEvidence {
        match self {
            Self::DirectoryAdministration(model) => model.persistence(),
            Self::PlatformOperations(model) => model.persistence(),
            Self::Context(model) => model.persistence(),
        }
    }

    /// Reconciliation evidence for any model accepted by the product route adapter.
    pub const fn reconciliation(&self) -> ReconciliationEvidence {
        match self {
            Self::DirectoryAdministration(model) => model.reconciliation(),
            Self::PlatformOperations(model) => model.reconciliation(),
            Self::Context(model) => model.reconciliation(),
        }
    }

    /// Authorization evidence for any model accepted by the product route adapter.
    pub const fn authorization(&self) -> AuthorizationEvidence {
        match self {
            Self::DirectoryAdministration(model) => model.authorization(),
            Self::PlatformOperations(model) => model.authorization(),
            Self::Context(model) => model.authorization(),
        }
    }
}

/// A route/surface rendered by the G12 product completion component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductSurface {
    /// Directory connector, role mapping, and identity-link administration.
    DirectoryAdministration,
    /// Platform tenant/projection/backup/integration/audit/settings reads.
    PlatformOperations,
    /// One of the ERPX bounded product contexts.
    Context(ProductContext),
}

impl ProductSurface {
    /// Return a stable surface key for DOM and QA selectors.
    pub const fn key(self) -> &'static str {
        match self {
            Self::DirectoryAdministration => "directory-administration",
            Self::PlatformOperations => "platform-operations",
            Self::Context(ProductContext::Operations) => "operations",
            Self::Context(ProductContext::Finance) => "finance",
            Self::Context(ProductContext::Sales) => "sales",
            Self::Context(ProductContext::Procurement) => "procurement",
            Self::Context(ProductContext::Inventory) => "inventory",
            Self::Context(ProductContext::Maintenance) => "maintenance",
            Self::Context(ProductContext::Quality) => "quality",
            Self::Context(ProductContext::Assets) => "assets",
            Self::Context(ProductContext::MarketingCrm) => "marketing-crm",
            Self::Context(ProductContext::HrPayroll) => "hr-payroll",
            Self::Context(ProductContext::Pos) => "pos",
            Self::Context(ProductContext::Legal) => "legal",
        }
    }

    /// Return the canonical route prefix.
    pub const fn path(self) -> &'static str {
        match self {
            Self::DirectoryAdministration => "/platform/directories",
            Self::PlatformOperations => "/platform",
            Self::Context(context) => context.path(),
        }
    }

    /// Return a stable title.
    pub const fn label(self) -> &'static str {
        match self {
            Self::DirectoryAdministration => "Directory administration",
            Self::PlatformOperations => "Platform Operations",
            Self::Context(context) => context.label(),
        }
    }

    pub const fn label_for(self, locale: Locale) -> &'static str {
        match self {
            Self::DirectoryAdministration if matches!(locale, Locale::Th) => "การจัดการไดเรกทอรี",
            Self::PlatformOperations if matches!(locale, Locale::Th) => "การปฏิบัติการแพลตฟอร์ม",
            Self::Context(context) => context.label_for(locale),
            _ => self.label(),
        }
    }
}

/// Explicit state for a product-surface read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductSurfaceState {
    /// The server-authorized read is in flight.
    Loading { surface: ProductSurface },
    /// The read completed successfully with no visible rows.
    Empty {
        surface: ProductSurface,
        message: SafeDisplayText,
    },
    /// The route is known, but no authoritative response is currently available to render.
    Unavailable {
        surface: ProductSurface,
        reason: SafeDisplayText,
    },
    /// A safe read model is available.
    Ready { model: ProductSurfaceModel },
    /// The authoritative boundary denied this read.
    Denied {
        surface: ProductSurface,
        reason: SafeDisplayText,
    },
    /// A policy or dependency blocks a read before an authorized model can be shown.
    Blocked {
        surface: ProductSurface,
        reason: SafeDisplayText,
    },
    /// A previously authorized read is outside its freshness budget.
    Stale {
        model: ProductSurfaceModel,
        age_seconds: u64,
        reason: SafeDisplayText,
    },
    /// The authoritative response could not be presented because its version/identity facts
    /// conflict. Conflicting data is never rendered as a usable model.
    Conflict {
        surface: ProductSurface,
        reason: SafeDisplayText,
    },
    /// A read-only model is available and mutation is disabled.
    ReadOnly {
        model: ProductSurfaceModel,
        reason: SafeDisplayText,
    },
    /// The backend returned a safe, categorical error.
    Error {
        surface: ProductSurface,
        reason: SafeDisplayText,
    },
    /// The target cannot currently reach the server.
    Offline {
        surface: ProductSurface,
        reason: SafeDisplayText,
    },
}

impl ProductSurfaceState {
    /// Return the route/surface represented by this state.
    pub const fn surface(&self) -> ProductSurface {
        match self {
            Self::Loading { surface }
            | Self::Empty { surface, .. }
            | Self::Unavailable { surface, .. }
            | Self::Denied { surface, .. }
            | Self::Blocked { surface, .. }
            | Self::Conflict { surface, .. }
            | Self::Error { surface, .. }
            | Self::Offline { surface, .. } => *surface,
            Self::Ready { model } | Self::ReadOnly { model, .. } | Self::Stale { model, .. } => {
                model.surface()
            }
        }
    }

    /// Map this state to the shared UI state taxonomy.
    pub const fn kind(&self) -> Option<UiStateKind> {
        match self {
            Self::Loading { .. } => Some(UiStateKind::Loading),
            Self::Empty { .. } => Some(UiStateKind::Empty),
            Self::Ready { .. } => None,
            Self::Unavailable { .. } => Some(UiStateKind::Offline),
            Self::Denied { .. } => Some(UiStateKind::Denied),
            Self::Blocked { .. } => Some(UiStateKind::Denied),
            Self::Stale { .. } => Some(UiStateKind::Stale),
            Self::ReadOnly { .. } => Some(UiStateKind::ReadOnly),
            Self::Conflict { .. } => Some(UiStateKind::Conflict),
            Self::Error { .. } => Some(UiStateKind::Error),
            Self::Offline { .. } => Some(UiStateKind::Offline),
        }
    }

    /// Return the target-neutral accessibility contract for this rendered state.
    ///
    /// A `Ready` model has no non-ready `UiStateKind`, but product models are still read-only at
    /// this boundary, so it uses the shared read-only announcement contract. This keeps denied,
    /// conflict, and error states assertive instead of treating every product surface as polite
    /// status text.
    pub const fn accessibility_contract(&self) -> UiAccessibilityContract {
        match self.kind() {
            Some(kind) => kind.accessibility(),
            None => UiStateKind::ReadOnly.accessibility(),
        }
    }

    /// Whether this state contains an already-authorized model.
    pub const fn has_model(&self) -> bool {
        matches!(
            self,
            Self::Ready { .. } | Self::Stale { .. } | Self::ReadOnly { .. }
        )
    }

    /// Return projection freshness supplied by an authorized model, or unknown when no model
    /// is available for this state.
    pub const fn projection_freshness(&self) -> ProjectionFreshness {
        match self {
            Self::Ready { model } | Self::Stale { model, .. } | Self::ReadOnly { model, .. } => {
                model.projection_freshness()
            }
            Self::Loading { .. }
            | Self::Empty { .. }
            | Self::Unavailable { .. }
            | Self::Denied { .. }
            | Self::Blocked { .. }
            | Self::Conflict { .. }
            | Self::Error { .. }
            | Self::Offline { .. } => ProjectionFreshness::Unknown,
        }
    }

    /// Return the authoritative age only for a stale state.
    pub const fn stale_age_seconds(&self) -> Option<u64> {
        match self {
            Self::Stale { age_seconds, .. } => Some(*age_seconds),
            Self::Loading { .. }
            | Self::Empty { .. }
            | Self::Unavailable { .. }
            | Self::Ready { .. }
            | Self::Denied { .. }
            | Self::Blocked { .. }
            | Self::Conflict { .. }
            | Self::ReadOnly { .. }
            | Self::Error { .. }
            | Self::Offline { .. } => None,
        }
    }

    /// Every product-completion state is mutation-disabled at this UI boundary.
    pub const fn mutations_enabled(&self) -> bool {
        false
    }

    /// Return a stable state selector.
    pub const fn state_key(&self) -> &'static str {
        match self {
            Self::Loading { .. } => "loading",
            Self::Empty { .. } => "empty",
            Self::Unavailable { .. } => "unavailable",
            Self::Ready { .. } => "ready",
            Self::Denied { .. } => "denied",
            Self::Blocked { .. } => "blocked",
            Self::Stale { .. } => "stale",
            Self::ReadOnly { .. } => "read-only",
            Self::Conflict { .. } => "conflict",
            Self::Error { .. } => "error",
            Self::Offline { .. } => "offline",
        }
    }

    pub const fn state_label(&self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Loading { .. }, Locale::Th) => "กำลังโหลด",
            (Self::Empty { .. }, Locale::Th) => "ไม่มีรายการ",
            (Self::Unavailable { .. }, Locale::Th) => "ไม่พร้อมใช้งาน",
            (Self::Ready { .. }, Locale::Th) => "พร้อมใช้งาน",
            (Self::Denied { .. }, Locale::Th) => "ปฏิเสธการเข้าถึง",
            (Self::Blocked { .. }, Locale::Th) => "ถูกบล็อก",
            (Self::Stale { .. }, Locale::Th) => "ข้อมูลล้าสมัย",
            (Self::ReadOnly { .. }, Locale::Th) => "อ่านอย่างเดียว",
            (Self::Conflict { .. }, Locale::Th) => "ข้อมูลขัดแย้ง",
            (Self::Error { .. }, Locale::Th) => "ข้อผิดพลาด",
            (Self::Offline { .. }, Locale::Th) => "ออฟไลน์",
            (_, Locale::En) => match self {
                Self::Loading { .. } => "Loading",
                Self::Empty { .. } => "Empty",
                Self::Unavailable { .. } => "Unavailable",
                Self::Ready { .. } => "Ready",
                Self::Denied { .. } => "Access denied",
                Self::Blocked { .. } => "Blocked",
                Self::Stale { .. } => "Stale",
                Self::ReadOnly { .. } => "Read-only",
                Self::Conflict { .. } => "Conflict",
                Self::Error { .. } => "Error",
                Self::Offline { .. } => "Offline",
            },
        }
    }

    /// Return safe copy for the state panel.
    pub fn message(&self) -> SafeDisplayText {
        match self {
            Self::Loading { .. } => SafeDisplayText::new("Loading authorized data…"),
            Self::Empty { message, .. }
            | Self::Unavailable {
                reason: message, ..
            }
            | Self::Denied {
                reason: message, ..
            }
            | Self::Blocked {
                reason: message, ..
            }
            | Self::Error {
                reason: message, ..
            }
            | Self::Offline {
                reason: message, ..
            }
            | Self::Stale {
                reason: message, ..
            }
            | Self::Conflict {
                reason: message, ..
            }
            | Self::ReadOnly {
                reason: message, ..
            } => message.clone(),
            Self::Ready { .. } => SafeDisplayText::new("Authorized read model available."),
        }
    }

    /// Borrow a safe model when one is present.
    pub fn model(&self) -> Option<&ProductSurfaceModel> {
        match self {
            Self::Ready { model } | Self::Stale { model, .. } | Self::ReadOnly { model, .. } => {
                Some(model)
            }
            _ => None,
        }
    }

    /// Enforce read-model invariants at the final render boundary.
    ///
    /// Most routes use [`ProductRouteStateAdapter`], but an existing console route may still
    /// construct a state directly from a typed server response. This guard prevents that path
    /// from presenting a stale model as `ready`/`read-only`, presenting a cache-only model as
    /// authoritative, or presenting a malformed stale age as usable data. Invalid models are
    /// converted to an explicit blocked/conflict state without inventing a freshness age.
    pub fn fail_closed_for_render(self) -> Self {
        match self {
            Self::Ready { model } => {
                if let Some(state) = model_render_guard(&model, false, None) {
                    state
                } else {
                    Self::Ready { model }
                }
            }
            Self::ReadOnly { model, reason } => {
                if let Some(state) = model_render_guard(&model, false, None) {
                    state
                } else {
                    Self::ReadOnly { model, reason }
                }
            }
            Self::Stale {
                model,
                age_seconds,
                reason,
            } => {
                if let Some(state) = model_render_guard(&model, true, Some(age_seconds)) {
                    state
                } else {
                    Self::Stale {
                        model,
                        age_seconds,
                        reason,
                    }
                }
            }
            state => state,
        }
    }

    /// Persistence evidence rendered by the state boundary.
    pub const fn persistence(&self) -> PersistenceEvidence {
        match self {
            Self::Ready { model } | Self::Stale { model, .. } | Self::ReadOnly { model, .. } => {
                match model {
                    ProductSurfaceModel::Context(context) => context.persistence(),
                    ProductSurfaceModel::DirectoryAdministration(directory) => {
                        directory.persistence()
                    }
                    ProductSurfaceModel::PlatformOperations(platform) => platform.persistence(),
                }
            }
            Self::Loading { .. }
            | Self::Empty { .. }
            | Self::Unavailable { .. }
            | Self::Denied { .. }
            | Self::Blocked { .. }
            | Self::Conflict { .. }
            | Self::Error { .. }
            | Self::Offline { .. } => PersistenceEvidence::Unavailable,
        }
    }

    /// Reconciliation evidence rendered by the state boundary.
    pub const fn reconciliation(&self) -> ReconciliationEvidence {
        match self {
            Self::Ready { model } | Self::Stale { model, .. } | Self::ReadOnly { model, .. } => {
                match model {
                    ProductSurfaceModel::Context(context) => context.reconciliation(),
                    ProductSurfaceModel::DirectoryAdministration(directory) => {
                        directory.reconciliation()
                    }
                    ProductSurfaceModel::PlatformOperations(platform) => platform.reconciliation(),
                }
            }
            Self::Loading { .. }
            | Self::Empty { .. }
            | Self::Unavailable { .. }
            | Self::Denied { .. }
            | Self::Blocked { .. }
            | Self::Conflict { .. }
            | Self::Error { .. }
            | Self::Offline { .. } => ReconciliationEvidence::NotReported,
        }
    }

    /// Authorization evidence rendered by the state boundary.
    pub const fn authorization(&self) -> AuthorizationEvidence {
        match self {
            Self::Ready { model } | Self::Stale { model, .. } | Self::ReadOnly { model, .. } => {
                match model {
                    ProductSurfaceModel::DirectoryAdministration(directory) => {
                        directory.authorization()
                    }
                    ProductSurfaceModel::PlatformOperations(platform) => platform.authorization(),
                    ProductSurfaceModel::Context(context) => context.authorization(),
                }
            }
            Self::Denied { .. } => AuthorizationEvidence::Denied,
            Self::Blocked { .. } => AuthorizationEvidence::Blocked,
            Self::Conflict { .. } => AuthorizationEvidence::Blocked,
            Self::Loading { .. }
            | Self::Empty { .. }
            | Self::Unavailable { .. }
            | Self::Error { .. }
            | Self::Offline { .. } => AuthorizationEvidence::Unavailable,
        }
    }

    /// Stable QA value for product-row evidence rendered by this state.
    pub const fn product_row_evidence(&self) -> ProductRowEvidence {
        match self {
            Self::Ready { model } | Self::Stale { model, .. } | Self::ReadOnly { model, .. } => {
                model.product_row_evidence()
            }
            Self::Loading { .. }
            | Self::Empty { .. }
            | Self::Unavailable { .. }
            | Self::Denied { .. }
            | Self::Blocked { .. }
            | Self::Conflict { .. }
            | Self::Error { .. }
            | Self::Offline { .. } => ProductRowEvidence::Unavailable,
        }
    }
}

fn model_render_guard(
    model: &ProductSurfaceModel,
    stale_state: bool,
    stale_age_seconds: Option<u64>,
) -> Option<ProductSurfaceState> {
    let surface = model.surface();
    if model.persistence() != PersistenceEvidence::AuthoritativeReadModel
        || model.authorization() != AuthorizationEvidence::ServerAuthorized
        || !model.is_read_only()
    {
        return Some(ProductSurfaceState::Blocked {
            surface,
            reason: SafeDisplayText::new(
                "This read cannot be rendered until authoritative persistence and server authorization are available.",
            ),
        });
    }

    if let ProductSurfaceModel::DirectoryAdministration(directory) = model {
        if !directory.is_contract_compliant() {
            return Some(ProductSurfaceState::Conflict {
                surface,
                reason: SafeDisplayText::new(
                    "The directory read model does not satisfy its authoritative security contract.",
                ),
            });
        }
        if directory.reconciliation() == ReconciliationEvidence::DriftDetected {
            return Some(ProductSurfaceState::Conflict {
                surface,
                reason: SafeDisplayText::new(
                    "Directory reconciliation detected drift; the read remains blocked until the authoritative worker resolves it.",
                ),
            });
        }
    }

    // Reconciliation is a cross-surface authority boundary. A product, platform, or directory
    // snapshot with detected drift must never be presented as a usable read model; otherwise a
    // populated projection could be mistaken for complete authoritative state. Pending
    // reconciliation intentionally remains visible and read-only so operators can observe the
    // worker-owned repair boundary without receiving a client action.
    if model.reconciliation() == ReconciliationEvidence::DriftDetected {
        return Some(ProductSurfaceState::Conflict {
            surface,
            reason: SafeDisplayText::new(
                "Authoritative reconciliation detected drift; this read remains blocked until the worker resolves it.",
            ),
        });
    }

    if let ProductSurfaceModel::Context(context) = model {
        // A product-row response is holding-bound and paginated. Projection-status-only routes
        // intentionally have neither field because they contain no product rows.
        if context.visible_row_count().is_some()
            && (context.scope().is_none() || context.pagination().is_none())
        {
            return Some(ProductSurfaceState::Conflict {
                surface,
                reason: SafeDisplayText::new(
                    "Product rows require an explicit tenant/holding binding and validated pagination metadata.",
                ),
            });
        }
        if let (Some(visible_row_count), Some(page)) =
            (context.visible_row_count(), context.pagination())
            && visible_row_count != u64::from(page.returned_count())
        {
            return Some(ProductSurfaceState::Conflict {
                surface,
                reason: SafeDisplayText::new(
                    "Product row count does not match the authoritative page metadata.",
                ),
            });
        }
    }

    match (model.projection_freshness(), stale_state, stale_age_seconds) {
        (ProjectionFreshness::Stale, false, _) => Some(ProductSurfaceState::Conflict {
            surface,
            reason: SafeDisplayText::new(
                "A stale projection must be rendered with an authoritative stale age.",
            ),
        }),
        (ProjectionFreshness::Stale, true, Some(age)) if age > MAX_PROJECTION_FRESHNESS_SECONDS => {
            None
        }
        (ProjectionFreshness::Stale, true, _) | (_, true, _) => {
            Some(ProductSurfaceState::Conflict {
                surface,
                reason: SafeDisplayText::new(
                    "The authoritative projection freshness and stale age are inconsistent.",
                ),
            })
        }
        (_, false, _) => None,
    }
}

/// A validated, server-authorized read model handed from an ERPX route response to the UI.
///
/// This wrapper is the boundary that was missing from the earlier typed evidence helpers. It
/// refuses local/cache-only models, non-read-only models, and contradictory freshness claims.
/// It carries no transport credentials or provider payloads and is usable by every Dioxus target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoritativeProductReadModel {
    model: ProductSurfaceModel,
    age_seconds: Option<u64>,
}

impl AuthoritativeProductReadModel {
    /// Validate a model and its server-supplied freshness age before UI adaptation.
    pub fn new(
        model: ProductSurfaceModel,
        age_seconds: Option<u64>,
    ) -> Result<Self, ProductContractError> {
        if model.persistence() != PersistenceEvidence::AuthoritativeReadModel {
            return Err(ProductContractError::PersistedReadModelRequired);
        }
        if model.authorization() != AuthorizationEvidence::ServerAuthorized {
            return Err(ProductContractError::ServerAuthorizationRequired);
        }
        if !model.is_read_only() {
            return Err(ProductContractError::ReadOnlySurfaceRequired);
        }

        if let ProductSurfaceModel::Context(context) = &model
            && context.visible_row_count().is_some()
        {
            if context.scope().is_none() {
                return Err(ProductContractError::ProductContextScopeRequired);
            }
            let Some(page) = context.pagination() else {
                return Err(ProductContractError::ProductPageMetadataRequired);
            };
            if context.visible_row_count() != Some(u64::from(page.returned_count())) {
                return Err(ProductContractError::ProductPageCountConflict);
            }
        }

        match model.projection_freshness() {
            ProjectionFreshness::Fresh
                if age_seconds.is_some_and(|age| age > MAX_PROJECTION_FRESHNESS_SECONDS) =>
            {
                return Err(ProductContractError::FreshnessAgeConflict);
            }
            ProjectionFreshness::Stale
                if age_seconds.is_none_or(|age| age <= MAX_PROJECTION_FRESHNESS_SECONDS) =>
            {
                return Err(ProductContractError::StaleFreshnessAgeRequired);
            }
            ProjectionFreshness::Unknown if age_seconds.is_some() => {
                return Err(ProductContractError::FreshnessAgeConflict);
            }
            _ => {}
        }

        Ok(Self { model, age_seconds })
    }

    /// Construct the read model used by the existing `/operations` projection-status route.
    pub fn from_projection_status(
        context: ProductContext,
        projection_status_count: u64,
        freshness: ProjectionFreshness,
        age_seconds: Option<u64>,
    ) -> Result<Self, ProductContractError> {
        let model = ProductSurfaceModel::Context(ContextSurfaceModel::new(
            context,
            freshness,
            projection_status_count,
            0,
        ));
        Self::new(model, age_seconds)
    }

    /// Borrow the validated model.
    pub fn model(&self) -> &ProductSurfaceModel {
        &self.model
    }

    /// Return the server-supplied age, when the response included one.
    pub const fn age_seconds(&self) -> Option<u64> {
        self.age_seconds
    }

    /// Return the route represented by the validated model.
    pub const fn surface(&self) -> ProductSurface {
        self.model.surface()
    }
}

/// Explicit response states produced by an authoritative ERPX route adapter.
///
/// The variants mirror the shared UI taxonomy while retaining a separate `Unavailable` state
/// for an absent read response and a `Conflict` state for contradictory server facts. Only
/// `ReadOnly` and `Stale` carry a validated model; all other states render no business data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductRouteResponse {
    /// The authoritative request is in flight.
    Loading { surface: ProductSurface },
    /// The route exists, but its authoritative read response is unavailable.
    Unavailable {
        surface: ProductSurface,
        reason: SafeDisplayText,
    },
    /// The authoritative read returned no rows/content.
    Empty {
        surface: ProductSurface,
        message: SafeDisplayText,
    },
    /// A fresh or unknown-freshness read-only model is available.
    ReadOnly {
        read_model: AuthoritativeProductReadModel,
        reason: SafeDisplayText,
    },
    /// A stale but otherwise authoritative read-only model is available for inspection only.
    Stale {
        read_model: AuthoritativeProductReadModel,
        reason: SafeDisplayText,
    },
    /// The authoritative boundary denied this read.
    Denied {
        surface: ProductSurface,
        reason: SafeDisplayText,
    },
    /// The response contained conflicting identity/version/freshness facts.
    Conflict {
        surface: ProductSurface,
        reason: SafeDisplayText,
    },
    /// The authoritative backend returned a categorical error.
    Error {
        surface: ProductSurface,
        reason: SafeDisplayText,
    },
    /// The current target cannot reach the authoritative backend.
    Offline {
        surface: ProductSurface,
        reason: SafeDisplayText,
    },
}

impl ProductRouteResponse {
    /// Create a loading response for a known route.
    pub const fn loading(surface: ProductSurface) -> Self {
        Self::Loading { surface }
    }

    /// Create an unavailable response with safe display text.
    pub fn unavailable(surface: ProductSurface, reason: impl AsRef<str>) -> Self {
        Self::Unavailable {
            surface,
            reason: SafeDisplayText::new(reason),
        }
    }

    /// Create an empty response with safe display text.
    pub fn empty(surface: ProductSurface, message: impl AsRef<str>) -> Self {
        Self::Empty {
            surface,
            message: SafeDisplayText::new(message),
        }
    }

    /// Create a read-only response from a validated authoritative model.
    pub fn read_only(
        read_model: AuthoritativeProductReadModel,
        reason: impl AsRef<str>,
    ) -> Result<Self, ProductContractError> {
        if read_model.model().projection_freshness() == ProjectionFreshness::Stale {
            return Err(ProductContractError::StaleResponseRequiresStaleState);
        }
        Ok(Self::ReadOnly {
            read_model,
            reason: SafeDisplayText::new(reason),
        })
    }

    /// Create a stale response from a validated stale authoritative model.
    pub fn stale(
        read_model: AuthoritativeProductReadModel,
        reason: impl AsRef<str>,
    ) -> Result<Self, ProductContractError> {
        if read_model.model().projection_freshness() != ProjectionFreshness::Stale {
            return Err(ProductContractError::FreshResponseCannotBeStale);
        }
        Ok(Self::Stale {
            read_model,
            reason: SafeDisplayText::new(reason),
        })
    }

    /// Create an authorization-denied response with safe display text.
    pub fn denied(surface: ProductSurface, reason: impl AsRef<str>) -> Self {
        Self::Denied {
            surface,
            reason: SafeDisplayText::new(reason),
        }
    }

    /// Create a conflict response; conflicting payloads never cross into a model.
    pub fn conflict(surface: ProductSurface, reason: impl AsRef<str>) -> Self {
        Self::Conflict {
            surface,
            reason: SafeDisplayText::new(reason),
        }
    }

    /// Create an error response with safe display text.
    pub fn error(surface: ProductSurface, reason: impl AsRef<str>) -> Self {
        Self::Error {
            surface,
            reason: SafeDisplayText::new(reason),
        }
    }

    /// Create an offline response with safe display text.
    pub fn offline(surface: ProductSurface, reason: impl AsRef<str>) -> Self {
        Self::Offline {
            surface,
            reason: SafeDisplayText::new(reason),
        }
    }
}

/// Target-neutral adapter from an authoritative route response to the Dioxus UI state.
///
/// This adapter is intentionally synchronous and side-effect-free. The coordinator-owned
/// transport supplies `ProductRouteResponse`; the same state contract is then rendered by web,
/// desktop, Android, and iOS without target-specific assumptions or client-side authorization.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProductRouteStateAdapter;

impl ProductRouteStateAdapter {
    /// Adapt an authoritative response into the state rendered by `ProductCompletionSurface`.
    pub fn adapt(&self, response: ProductRouteResponse) -> ProductSurfaceState {
        match response {
            ProductRouteResponse::Loading { surface } => ProductSurfaceState::Loading { surface },
            ProductRouteResponse::Unavailable { surface, reason } => {
                ProductSurfaceState::Unavailable { surface, reason }
            }
            ProductRouteResponse::Empty { surface, message } => {
                ProductSurfaceState::Empty { surface, message }
            }
            ProductRouteResponse::ReadOnly { read_model, reason } => {
                ProductSurfaceState::ReadOnly {
                    model: read_model.model,
                    reason,
                }
            }
            ProductRouteResponse::Stale { read_model, reason } => {
                let Some(age_seconds) = read_model.age_seconds() else {
                    return ProductSurfaceState::Conflict {
                        surface: read_model.surface(),
                        reason: SafeDisplayText::new(
                            "Authoritative stale response omitted its freshness age.",
                        ),
                    };
                };
                ProductSurfaceState::Stale {
                    model: read_model.model,
                    age_seconds,
                    reason,
                }
            }
            ProductRouteResponse::Denied { surface, reason } => {
                ProductSurfaceState::Denied { surface, reason }
            }
            ProductRouteResponse::Conflict { surface, reason } => {
                ProductSurfaceState::Conflict { surface, reason }
            }
            ProductRouteResponse::Error { surface, reason } => {
                ProductSurfaceState::Error { surface, reason }
            }
            ProductRouteResponse::Offline { surface, reason } => {
                ProductSurfaceState::Offline { surface, reason }
            }
        }
    }

    /// Adapt an authoritative product-context projection-status response after its transport
    /// layer has reduced server rows to the route facts used by the UI.
    ///
    /// Projection status is not product-row data. The resulting model therefore remains an
    /// explicit status-only read and never fabricates a tenant/holding page. A stale response
    /// without an authoritative age is rejected as a conflict by the same validation boundary
    /// used by every product route.
    pub fn adapt_projection_status(
        &self,
        context: ProductContext,
        projection_status_count: u64,
        freshness_label: &str,
        age_seconds: Option<u64>,
    ) -> ProductSurfaceState {
        let surface = ProductSurface::Context(context);
        if projection_status_count == 0 {
            return self.adapt(ProductRouteResponse::empty(
                surface,
                format!(
                    "No authorized projection status is available for {}.",
                    context.label()
                ),
            ));
        }

        let freshness = ProjectionFreshness::from_authoritative_label(freshness_label);
        let read_model = match AuthoritativeProductReadModel::from_projection_status(
            context,
            projection_status_count,
            freshness,
            age_seconds,
        ) {
            Ok(read_model) => read_model,
            Err(_) => {
                return self.adapt(ProductRouteResponse::conflict(
                    surface,
                    format!(
                        "The authoritative {} projection response was inconsistent.",
                        context.label()
                    ),
                ));
            }
        };

        match freshness {
            ProjectionFreshness::Stale => self.adapt(
                ProductRouteResponse::stale(
                    read_model,
                    format!(
                        "The {} projection is stale; this route remains read-only.",
                        context.label()
                    ),
                )
                .expect("validated stale response must preserve stale freshness"),
            ),
            ProjectionFreshness::Fresh => self.adapt(
                ProductRouteResponse::read_only(
                    read_model,
                    format!(
                        "The {} projection is fresh and server-authorized; mutations remain server-owned.",
                        context.label()
                    ),
                )
                .expect("validated fresh response must preserve fresh freshness"),
            ),
            ProjectionFreshness::Unknown => self.adapt(
                ProductRouteResponse::read_only(
                    read_model,
                    format!(
                        "{} projection freshness is unavailable; the route remains read-only.",
                        context.label()
                    ),
                )
                .expect("validated unknown freshness response must remain read-only"),
            ),
        }
    }

    /// Adapt the real `/operations` projection-status response shape after its transport layer
    /// has reduced server rows to the authoritative route facts used by the UI.
    pub fn adapt_operations_projection_status(
        &self,
        projection_status_count: u64,
        freshness_label: &str,
        age_seconds: Option<u64>,
    ) -> ProductSurfaceState {
        self.adapt_projection_status(
            ProductContext::Operations,
            projection_status_count,
            freshness_label,
            age_seconds,
        )
    }
}

/// Target-neutral handoff status for mobile clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobileTargetHandoffStatus {
    /// The surface contract can be handed to a target adapter.
    TargetNeutralSurfaceReady,
    /// Device execution and vendor certification remain required.
    NativeCertificationRequired,
}

/// Handoff metadata for Android and iOS without claiming device certification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MobileTargetHandoff {
    target: ClientTarget,
    secure_session: SecureSessionSupport,
    status: MobileTargetHandoffStatus,
}

impl MobileTargetHandoff {
    /// Create a handoff descriptor from compile-time target metadata.
    pub const fn new(target: ClientTarget, secure_session: SecureSessionSupport) -> Self {
        let status = match target {
            ClientTarget::Android | ClientTarget::Ios => {
                MobileTargetHandoffStatus::NativeCertificationRequired
            }
            _ => MobileTargetHandoffStatus::TargetNeutralSurfaceReady,
        };
        Self {
            target,
            secure_session,
            status,
        }
    }

    /// Target identity.
    pub const fn target(self) -> ClientTarget {
        self.target
    }

    /// Target-owned secure-session capability, never a credential.
    pub const fn secure_session(self) -> SecureSessionSupport {
        self.secure_session
    }

    /// Handoff status.
    pub const fn status(self) -> MobileTargetHandoffStatus {
        self.status
    }

    /// This module never provides device or external-vendor certification evidence.
    pub const fn claims_certification(self) -> bool {
        false
    }
}

/// Redact a return path before it can become client display state.
pub fn safe_return_path(path: &str) -> SafeDisplayText {
    SafeDisplayText::new(redact_deep_link(path))
}

fn directory_bool_status(value: bool) -> &'static str {
    if value {
        product_copy("Compliant", "ปฏิบัติตาม")
    } else {
        product_copy("Not compliant", "ไม่ปฏิบัติตาม")
    }
}

fn directory_known_status(value: &str) -> &str {
    match (value, product_locale()) {
        ("draft", Locale::Th) => "ฉบับร่าง",
        ("verified", Locale::Th) => "ตรวจสอบแล้ว",
        ("importing", Locale::Th) => "กำลังนำเข้า",
        ("ready", Locale::Th) => "พร้อมใช้งาน",
        ("active", Locale::Th) => "ใช้งานอยู่",
        ("suspended", Locale::Th) => "ระงับการใช้งาน",
        ("failed", Locale::Th) => "ล้มเหลว",
        ("not_ready", Locale::Th) => "ยังไม่พร้อม",
        ("unconfigured", Locale::Th) => "ยังไม่ได้ตั้งค่า",
        ("running", Locale::Th) => "กำลังทำงาน",
        ("finalizing", Locale::Th) => "กำลังดำเนินการขั้นสุดท้าย",
        ("completed", Locale::Th) => "เสร็จสมบูรณ์",
        ("rolled_back", Locale::Th) => "ย้อนกลับแล้ว",
        ("disabled", Locale::Th) => "ปิดใช้งาน",
        ("enabled", Locale::Th) => "เปิดใช้งาน",
        _ => value,
    }
}

fn directory_scope_label(value: &str) -> &str {
    match (value, product_locale()) {
        ("Platform", Locale::Th) => "แพลตฟอร์ม",
        _ => value,
    }
}

/// Localize only ERPX-authored state messages. Unknown text may be supplied by
/// an authoritative read boundary and is intentionally presented unchanged.
fn product_state_message(message: &SafeDisplayText, locale: Locale) -> String {
    if locale == Locale::En {
        return message.to_string();
    }
    match message.as_str() {
        "Directory facts are server-authorized and read-only." => {
            "ข้อมูลไดเรกทอรีได้รับอนุญาตจากเซิร์ฟเวอร์และเป็นแบบอ่านอย่างเดียว".into()
        }
        "Every Platform Operations area requires a matching authenticated read adapter." => {
            "ทุกพื้นที่การปฏิบัติการแพลตฟอร์มต้องมีอะแดปเตอร์อ่านที่ยืนยันตัวตนตรงกัน".into()
        }
        "Platform Operations reconciliation detected drift; the read remains blocked until the authoritative worker resolves it." => {
            "การกระทบยอดการปฏิบัติการแพลตฟอร์มพบความคลาดเคลื่อน การอ่านจะถูกบล็อกจนกว่าเวิร์กเกอร์ที่เป็นข้อยุติจะแก้ไข"
                .into()
        }
        "Authoritative stale response omitted its freshness age." => {
            "การตอบกลับที่ล้าสมัยจากแหล่งข้อมูลที่เป็นข้อยุติไม่มีอายุความใหม่".into()
        }
        "This read cannot be rendered until authoritative persistence and server authorization are available." => {
            "ไม่สามารถแสดงข้อมูลอ่านนี้จนกว่าจะมีการจัดเก็บที่เป็นข้อยุติและการอนุญาตจากเซิร์ฟเวอร์".into()
        }
        "The directory read model does not satisfy its authoritative security contract." => {
            "โมเดลอ่านไดเรกทอรีไม่เป็นไปตามสัญญาความปลอดภัยที่เป็นข้อยุติ".into()
        }
        "Directory reconciliation detected drift; the read remains blocked until the authoritative worker resolves it." => {
            "การกระทบยอดไดเรกทอรีพบความคลาดเคลื่อน การอ่านจะถูกบล็อกจนกว่าเวิร์กเกอร์ที่เป็นข้อยุติจะแก้ไข".into()
        }
        "Authoritative reconciliation detected drift; this read remains blocked until the worker resolves it." => {
            "การกระทบยอดที่เป็นข้อยุติพบความคลาดเคลื่อน การอ่านนี้จะถูกบล็อกจนกว่าเวิร์กเกอร์จะแก้ไข".into()
        }
        "Product rows require an explicit tenant/holding binding and validated pagination metadata." => {
            "แถวผลิตภัณฑ์ต้องระบุการเชื่อมโยงผู้เช่า/โฮลดิ้งและข้อมูลกำกับการแบ่งหน้าที่ตรวจสอบแล้ว".into()
        }
        "Product row count does not match the authoritative page metadata." => {
            "จำนวนแถวผลิตภัณฑ์ไม่ตรงกับข้อมูลกำกับหน้าที่เป็นข้อยุติ".into()
        }
        "A stale projection must be rendered with an authoritative stale age." => {
            "โพรเจกชันที่ล้าสมัยต้องแสดงพร้อมอายุความล้าสมัยจากแหล่งข้อมูลที่เป็นข้อยุติ".into()
        }
        "The authoritative projection freshness and stale age are inconsistent." => {
            "ความใหม่และอายุความล้าสมัยของโพรเจกชันที่เป็นข้อยุติไม่สอดคล้องกัน".into()
        }
        _ => message.to_string(),
    }
}

fn directory_role_mapping_status(mode: RoleMappingMode) -> &'static str {
    match mode {
        RoleMappingMode::DirectoryManaged => product_copy("Directory-managed", "จัดการโดยไดเรกทอรี"),
        RoleMappingMode::LocalOnly => {
            product_copy("Local-only (not allowed)", "เฉพาะบัญชีภายใน (ไม่อนุญาต)")
        }
    }
}

fn directory_identity_linking_status(mode: IdentityLinkingMode) -> &'static str {
    match mode {
        IdentityLinkingMode::Immutable => product_copy("Immutable", "แก้ไขไม่ได้"),
        IdentityLinkingMode::Mutable => product_copy("Mutable (not allowed)", "แก้ไขได้ (ไม่อนุญาต)"),
    }
}

fn directory_role_revocation_status(mode: RoleRevocationMode) -> &'static str {
    match mode {
        RoleRevocationMode::AppendOnly => product_copy("Append-only", "เพิ่มข้อมูลเท่านั้น"),
        RoleRevocationMode::Replaceable => {
            product_copy("Replaceable (not allowed)", "แทนที่ได้ (ไม่อนุญาต)")
        }
    }
}

fn directory_fallback_status(mode: LocalFallbackMode) -> &'static str {
    match mode {
        LocalFallbackMode::BreakGlassOnly => product_copy("Break-glass only", "เฉพาะกรณีฉุกเฉิน"),
        LocalFallbackMode::GeneralLocalFallback => product_copy(
            "General local fallback (not allowed)",
            "ใช้บัญชีภายในทั่วไปเป็นทางเลือก (ไม่อนุญาต)",
        ),
    }
}

fn directory_mfa_status(policy: HighRiskMfaPolicy) -> String {
    match policy {
        HighRiskMfaPolicy::Required { max_age_seconds } => match product_locale() {
            Locale::En => format!("Required; assertion age ≤ {max_age_seconds} seconds"),
            Locale::Th => format!("จำเป็น; คำยืนยันมีอายุ ≤ {max_age_seconds} วินาที"),
        },
        HighRiskMfaPolicy::NotRequired => {
            product_copy("Not required (not allowed)", "ไม่จำเป็น (ไม่อนุญาต)").into()
        }
    }
}

/// Render a target-neutral authoritative route response through the shared state component.
///
/// Transport code supplies only `ProductRouteResponse`; this adapter component owns no fetch,
/// mutation, credential, or permission behavior. It makes the response-state contract usable by
/// an existing ERPX route without requiring web/desktop/mobile-specific branching.
#[component]
pub fn ProductRouteSurface(response: ProductRouteResponse) -> Element {
    let state = ProductRouteStateAdapter.adapt(response);
    rsx! { ProductCompletionSurface { state } }
}

/// Render the authorized directory administration read model.
///
/// This component is intentionally a pure projection: it has no event handlers, forms,
/// credentials, provider endpoints, or mutation affordances. The coordinator supplies the
/// `DirectoryAdministrationModel` only after the server/read-model authorization boundary.
#[component]
pub fn DirectoryAdministrationSurface(model: DirectoryAdministrationModel) -> Element {
    // This component is also callable directly by the coordinator shell, so it must enforce the
    // same final render guard as `ProductCompletionSurface`; otherwise an invalid hand-built
    // snapshot could still leak provider-derived fields before the shared state adapter runs.
    let state = ProductSurfaceState::ReadOnly {
        model: ProductSurfaceModel::DirectoryAdministration(model.clone()),
        reason: SafeDisplayText::new("Directory facts are server-authorized and read-only."),
    }
    .fail_closed_for_render();
    if !state.has_model() {
        return rsx! { ProductCompletionSurface { state } };
    }

    let connector_count = model.connectors().len();
    let role_mapping_count = model.role_mappings().len();
    let identity_link_count = model.identity_links().len();
    let holding_connector_count = model.holding_connector_count();
    let contract_status = directory_bool_status(model.is_contract_compliant());
    let persistence = model.persistence();
    let reconciliation = model.reconciliation();
    let authorization = model.authorization();
    let runtime_state = model.runtime_state().as_str();

    rsx! {
        section {
            class: "erpx-directory-administration",
            "data-surface": ProductSurface::DirectoryAdministration.key(),
            "data-authority": DIRECTORY_READ_MODEL_AUTHORITY,
            "data-read-only": "true",
            "data-mutations": "disabled",
            "data-runtime-state": runtime_state,
            "data-contract": if model.is_contract_compliant() { "compliant" } else { "not-compliant" },
            "data-persistence": persistence.key(),
            "data-reconciliation": reconciliation.key(),
            "data-authorization": authorization.key(),
            aria_label: product_copy("Directory administration", "การจัดการไดเรกทอรี"),
            header {
                class: "erpx-directory-header",
                span { class: "ui-eyebrow", {product_copy("ERPX DIRECTORY ADMINISTRATION", "การจัดการไดเรกทอรี ERPX")} }
                h2 { {product_copy("Directory administration", "การจัดการไดเรกทอรี")} }
                p { {product_copy("Authoritative connector and identity facts. This surface is read-only; directory commands remain server-authorized.", "เชื่อมต่อและข้อมูลประจำตัวที่เป็นที่ยอมรับ. หน้าจอนี้มีเพียงการอ่าน;คำสั่งในรายการไดเรกทอรียังคงถูกอนุญาตโดยเซิร์ฟเวอร์")} }
            }
            if runtime_state == "disabled" {
                p {
                    class: "status-banner warn",
                    {product_copy("Disabled · LDAP, AD DS, and UCS connectors are compiled but no connector or directory worker is active in this development profile.", "ปิดใช้งาน · คอมไพล์คอนเนกเตอร์ LDAP, AD DS และ UCS แล้ว แต่ไม่มีคอนเนกเตอร์หรือเวิร์กเกอร์ไดเรกทอรีทำงานในโพรไฟล์การพัฒนานี้")}
                }
            }

            section {
                class: "erpx-directory-contract-summary",
                aria_label: product_copy("Directory contract summary", "สรุปสัญญา ไดเรกทอรี"),
                h3 { {product_copy("Connector contract", "ข้อตกลงของคอนเนกเตอร์")} }
                div {
                    class: "erpx-directory-metric-grid",
                    div { class: "erpx-directory-metric", "data-metric": "connectors",
                        strong { "{connector_count}" }
                        span { {product_copy("connector(s)", "คอนเนกเตอร์")} }
                    }
                    div { class: "erpx-directory-metric", "data-metric": "holding-connectors",
                        strong { "{holding_connector_count}" }
                        span { {product_copy("holding connector(s)", "คอนเนกเตอร์ของโฮลดิ้ง")} }
                    }
                    div { class: "erpx-directory-metric", "data-metric": "role-mappings",
                        strong { "{role_mapping_count}" }
                        span { {product_copy("directory role mapping(s)", "การจับคู่-บทบาทในรายการ")} }
                    }
                    div { class: "erpx-directory-metric", "data-metric": "identity-links",
                        strong { "{identity_link_count}" }
                        span { if model.identity_links_truncated() { {product_copy("visible immutable identity link(s)", "ลิงก์ประจำตัวที่ไม่สามารถแก้ไขได้")} } else { {product_copy("immutable identity link(s)", "ลิงก์ความเป็นเจ้าของที่ไม่สามารถเปลี่ยนแปลงได้")} } }
                    }
                    div {
                        class: "erpx-directory-metric",
                        "data-metric": "contract-status",
                        "data-status": if model.is_contract_compliant() { "compliant" } else { "not-compliant" },
                        strong { "{contract_status}" }
                        span { {product_copy("security contract", "สัญญาความปลอดภัย")} }
                    }
                }
            }

            section {
                class: "erpx-directory-connectors",
                aria_label: product_copy("Directory connectors", "คอนเนกเตอร์ไดเรกทอรี"),
                h3 { {product_copy("Connectors", "คอนเนกเตอร์")} }
                if model.connectors().is_empty() {
                    p { class: "erpx-directory-empty", {product_copy("No authorized directory connectors are visible.", "ไม่มีคอนเนกเตอร์ไดเรกทอรีที่ได้รับอนุญาตให้แสดง")} }
                } else {
                    for connector in model.connectors().iter() {
                        article {
                            class: "erpx-directory-connector",
                            "data-connector-id": connector.connector_id().as_str(),
                            "data-vendor": connector.vendor().label(),
                            "data-scope": connector.scope().label(),
                            "data-native-feed": connector.policy().change_feed.label(),
                            aria_label: product_copy("Directory connector", "คอนเนกเตอร์ไดเรกทอรี"),
                            header {
                                class: "erpx-directory-connector-header",
                                h4 { "{connector.vendor().label()}" }
                                span { class: "erpx-directory-connector-id", "{product_copy(\"Connector\", \"คอนเนกเตอร์\")} {connector.connector_id().as_str()}" }
                            }
                            dl {
                                class: "erpx-directory-fact-grid",
                                div { dt { {product_copy("Scope", "ขอบเขต")} } dd { "{directory_scope_label(connector.scope().label().as_str())}" } }
                                div { dt { {product_copy("Lifecycle", "วงจรชีวิต")} } dd { "{directory_known_status(connector.lifecycle().as_str())}" } }
                                div { dt { {product_copy("Readiness", "ความพร้อม")} } dd { "{directory_known_status(connector.readiness().as_str())}" } }
                                div { dt { {product_copy("Native change feed", "การเปลี่ยนแปลงแบบท้องถิ่น")} } dd { "{connector.policy().change_feed.label()}" } }
                                div { dt { {product_copy("Full import", "การนำเข้าแบบเต็ม")} } dd { {match product_locale() {
                                    Locale::En => format!("Up to {} users", product_locale().formatter().format_integer(connector.policy().full_import.max_users() as i64)),
                                    Locale::Th => format!("ผู้ใช้สูงสุด {} ราย", product_locale().formatter().format_integer(connector.policy().full_import.max_users() as i64)),
                                }} } }
                                div { dt { {product_copy("Communication", "การสื่อสาร")} } dd { "{directory_bool_status(connector.policy().security.tls_only)} · " {product_copy("TLS-only", "TLS เท่านั้น")} } }
                                div { dt { {product_copy("Role mapping", "การจับคู่-บทบาท")} } dd { "{directory_role_mapping_status(connector.policy().role_mapping)}" } }
                                div { dt { {product_copy("Identity linking", "การเชื่อมโยงตัวตน")} } dd { "{directory_identity_linking_status(connector.policy().identity_linking)}" } }
                                div { dt { {product_copy("Role revocation", "การเพิกถอนบทบาท")} } dd { "{directory_role_revocation_status(connector.policy().role_revocation)}" } }
                                div { dt { {product_copy("Revocation budget", "กรอบเวลาการเพิกถอน")} } dd { {match product_locale() {
                                    Locale::En => format!("Within {} seconds", connector.policy().security.revocation.max_seconds()),
                                    Locale::Th => format!("ภายใน {} วินาที", connector.policy().security.revocation.max_seconds()),
                                }} } }
                                div { dt { {product_copy("High-risk MFA", "MFA สำหรับความเสี่ยงสูง")} } dd { "{directory_mfa_status(connector.policy().security.high_risk_mfa)}" } }
                                div { dt { {product_copy("Local fallback", "การใช้บัญชีภายในสำรอง")} } dd { "{directory_fallback_status(connector.policy().security.local_fallback)}" } }
                            }
                        }
                    }
                }
            }

            section {
                class: "erpx-directory-role-mappings",
                aria_label: product_copy("Directory-managed role mappings", "การจับคู่ บทบาท ไดเรกทอรีที่จัดการโดย ไดเรกทอรี"),
                h3 { {product_copy("Directory-managed role mappings", "การจับคู่ บทบาท ไดเรกทอรีที่จัดการโดย ไดเรกทอรี")} }
                if model.role_mappings().is_empty() {
                    p { class: "erpx-directory-empty", {product_copy("No authorized role mappings are visible.", "ไม่มีการจับคู่บทบาทที่ได้รับอนุญาตให้แสดง")} }
                } else {
                    for mapping in model.role_mappings().iter() {
                        article {
                            class: "erpx-directory-role-mapping",
                            "data-scope": mapping.scope().label(),
                            "data-revocation": directory_role_revocation_status(mapping.revocation()),
                            aria_label: product_copy("Directory role mapping", "การจับคู่ บทบาท ไดเรกทอรี"),
                            dl {
                                class: "erpx-directory-fact-grid",
                                div { dt { {product_copy("Directory group", "กลุ่ม ไดเรกทอรี")} } dd { "{mapping.directory_group()}" } }
                                div { dt { {product_copy("ERPX role", "บทบาท ERPX")} } dd { "{mapping.erpx_role()}" } }
                                div { dt { {product_copy("Scope", "ขอบเขต")} } dd { "{mapping.scope().label()}" } }
                                div { dt { {product_copy("Revocation history", "ประวัติการยกเลิก")} } dd { "{directory_role_revocation_status(mapping.revocation())}" } }
                            }
                        }
                    }
                }
            }

            section {
                class: "erpx-directory-import-statuses",
                aria_label: product_copy("Directory full import status", "สถานะการนำเข้าเต็ม ไดเรกทอรี"),
                h3 { {product_copy("Full import status", "สถานะการนำเข้าแบบเต็ม")} }
                if model.import_statuses().is_empty() {
                    p { class: "erpx-directory-empty", {product_copy("No authorized full-import checkpoint is visible for these connectors.", "ไม่มีจุดตรวจสอบการนำเข้าแบบเต็มที่ได้รับอนุญาตให้แสดงสำหรับคอนเนกเตอร์เหล่านี้")} }
                } else {
                    for status in model.import_statuses().iter() {
                        article {
                            class: "erpx-directory-import-status",
                            "data-connector-id": status.connector_id().as_str(),
                            "data-import-id": status.import_id().as_str(),
                            "data-state": status.state().as_str(),
                            aria_label: product_copy("Directory full import checkpoint", "จุดตรวจสอบเต็ม ไดเรกทอรี"),
                            header {
                                class: "erpx-directory-import-status-header",
                                h4 { "{product_copy(\"Import\", \"การนำเข้า\")} {status.import_id().as_str()}" }
                                span { "{directory_known_status(status.state().as_str())}" }
                            }
                            dl {
                                class: "erpx-directory-fact-grid",
                                div { dt { {product_copy("Connector", "คอนเนกเตอร์")} } dd { "{status.connector_id().as_str()}" } }
                                div { dt { {product_copy("Cursor protocol", "โปรโตคอลเคอร์เซอร์")} } dd { "{status.cursor_kind()}" } }
                                div { dt { {product_copy("Users", "ผู้ใช้")} } dd { "{product_locale().formatter().format_integer(status.users_seen() as i64)} / {product_locale().formatter().format_integer(status.max_users() as i64)}" } }
                                div { dt { {product_copy("Pages", "หน้า")} } dd { "{product_locale().formatter().format_integer(status.pages_applied() as i64)} {product_copy(\"of\", \"จาก\")} {product_locale().formatter().format_integer(status.page_count() as i64)}" } }
                                div { dt { {product_copy("Started", "เริ่มเมื่อ")} } dd { "{product_locale().formatter().format_timestamp(status.started_at().as_str())}" } }
                                div { dt { {product_copy("Updated", "อัปเดต")} } dd { "{product_locale().formatter().format_timestamp(status.updated_at().as_str())}" } }
                                if let Some(finalized_at) = status.finalized_at() {
                                    div { dt { {product_copy("Finalized", "เสร็จสิ้นเมื่อ")} } dd { "{product_locale().formatter().format_timestamp(finalized_at.as_str())}" } }
                                }
                                if let Some(failure_category) = status.failure_category() {
                                    div { dt { {product_copy("Failure category", "หมวดหมู่ความล้มเหลว")} } dd { "{failure_category}" } }
                                }
                            }
                            small { {product_copy("Checkpoint is authoritative and secret-free; source cursors, external IDs, and failure details remain server-side.", "จุดตรวจสอบเป็นที่ยอมรับและไม่มีความลับ; ข้อมูลต้นทาง, ID ภายนอก และรายละเอียดการผิดพลาดยังคงอยู่บนเซิร์ฟเวอร์")} }
                        }
                    }
                }
            }

            section {
                class: "erpx-directory-identity-links",
                aria_label: product_copy("Immutable identity links", "การเชื่อมโยงตัวตนที่แก้ไขไม่ได้"),
                h3 { {product_copy("Immutable identity links", "การเชื่อมโยงตัวตนที่แก้ไขไม่ได้")} }
                if model.identity_links().is_empty() {
                    p { class: "erpx-directory-empty", {product_copy("No authorized identity links are visible.", "ไม่มีลิงก์ข้อมูลประจำตัวที่ได้รับอนุญาตให้แสดง")} }
                } else {
                    for link in model.identity_links().iter() {
                        article {
                            class: "erpx-directory-identity-link",
                            "data-immutable": if link.is_immutable() { "true" } else { "false" },
                            aria_label: product_copy("Immutable identity link", "การเชื่อมโยงตัวตนที่แก้ไขไม่ได้"),
                            dl {
                                class: "erpx-directory-fact-grid",
                                div { dt { {product_copy("External subject", "ตัวตนภายนอก")} } dd { "{link.external_subject()}" } }
                                div { dt { {product_copy("ERPX principal", "ผู้มีสิทธิ์หลัก ERPX")} } dd { "{link.erpx_principal_id().as_str()}" } }
                                div { dt { {product_copy("Link policy", "นโยบายการเชื่อมโยง")} } dd { if link.is_immutable() { {product_copy("Immutable", "แก้ไขไม่ได้")} } else { {product_copy("Mutable (not allowed)", "แก้ไขได้ (ไม่อนุญาต)")} } } }
                            }
                        }
                    }
                }
                if model.identity_links_truncated() {
                    small { {product_copy("This is a bounded first page. Additional immutable identity links remain server-side and require the read cursor; the visible count is not the connector total.", "นี่คือหน้าแรกแบบจำกัด การเชื่อมโยงตัวตนเพิ่มเติมยังอยู่ที่เซิร์ฟเวอร์และต้องใช้เคอร์เซอร์อ่าน จำนวนที่เห็นไม่ใช่จำนวนทั้งหมดของคอนเนกเตอร์")} }
                }
            }

            section {
                class: "erpx-directory-read-evidence",
                aria_label: product_copy("Directory read evidence", "หลักฐานการอ่าน ไดเรกทอรี"),
                h3 { {product_copy("Read evidence", "หลักฐานข้อมูลอ่าน")} }
                dl {
                    class: "erpx-directory-fact-grid",
                    div { dt { {product_copy("Data authority", "แหล่งข้อมูลที่เป็นข้อยุติ")} } dd { "{persistence.label_for(product_locale())}" } }
                    div { dt { {product_copy("Reconciliation", "การกระทบยอด")} } dd { "{reconciliation.label_for(product_locale())}" } }
                    div { dt { {product_copy("Authorization", "การอนุญาต")} } dd { "{authorization.label_for(product_locale())}" } }
                    div { dt { {product_copy("Rollback", "การย้อนกลับ")} } dd { "{DIRECTORY_ROLLBACK_BOUNDARY}" } }
                }
                small { {product_copy("Pending reconciliation remains read-only; detected drift blocks this surface. No local rollback or fallback action is exposed.", "การกระทบยอดยังคงเป็นอ่าน-only; การตรวจพบความไม่สอดคล้องหยุดการทำงานพื้นที่นี้ ไม่มีการกลับเข้าถิ่นที่มีการปิดบัญชีหรือการฟื้นฟูอย่างเปิดเผย")} }
            }

            footer {
                class: "erpx-directory-read-only-note",
                "data-mutations": "disabled",
                {product_copy("Read-only presentation. Secrets, credentials, provider endpoints, and local fallback commands are never rendered.", "การแสดงผลเป็นแบบอ่านอย่างเดียว และจะไม่แสดงความลับ ข้อมูลประจำตัว ปลายทางของผู้ให้บริการ หรือคำสั่งสำรองภายใน")}
            }
        }
    }
}

/// Render the per-area evidence for authenticated Platform Operations reads.
///
/// The coordinator may use this component only after the corresponding live adapters have
/// supplied every attestation. Route metadata, cached rows, and an incomplete adapter set render
/// as a blocked read-only state; this component never manufactures an authorized area.
#[component]
pub fn PlatformOperationsEvidenceSurface(model: PlatformOperationsModel) -> Element {
    if !model.has_complete_adapter_coverage()
        || model.persistence() != PersistenceEvidence::AuthoritativeReadModel
        || model.authorization() != AuthorizationEvidence::ServerAuthorized
    {
        let state = ProductSurfaceState::Blocked {
            surface: ProductSurface::PlatformOperations,
            reason: SafeDisplayText::new(
                "Every Platform Operations area requires a matching authenticated read adapter.",
            ),
        };
        return rsx! { ProductCompletionSurface { state } };
    }

    if model.reconciliation() == ReconciliationEvidence::DriftDetected {
        let state = ProductSurfaceState::Conflict {
            surface: ProductSurface::PlatformOperations,
            reason: SafeDisplayText::new(
                "Platform Operations reconciliation detected drift; the read remains blocked until the authoritative worker resolves it.",
            ),
        };
        return rsx! { ProductCompletionSurface { state } };
    }

    rsx! {
        section {
            class: "erpx-platform-operations-evidence",
            "data-surface": ProductSurface::PlatformOperations.key(),
            "data-route": ProductSurface::PlatformOperations.path(),
            "data-state": "read-only",
            "data-read-only": "true",
            "data-mutations": "disabled",
            "data-persistence": model.persistence().key(),
            "data-reconciliation": model.reconciliation().key(),
            "data-authorization": model.authorization().key(),
            aria_label: product_copy("Platform Operations authenticated read evidence", "หลักฐานการอ่านที่ได้รับการยืนยันของ Platform Operations"),
            header {
                class: "erpx-platform-operations-evidence-header",
                span { class: "ui-eyebrow", {product_copy("ERPX PLATFORM OPERATIONS", "การปฏิบัติการแพลตฟอร์ม ERPX")} }
                h2 { {product_copy("Authenticated read evidence", "หลักฐานข้อมูลอ่านที่ยืนยันตัวตนแล้ว")} }
                p {
                    {product_copy("Each area below is bound to its coordinator-owned adapter. This surface is read-only and exposes no operational command or provider material.", "แต่ละพื้นที่ด้านล่างผูกกับอะแดปเตอร์ที่ผู้ประสานงานเป็นเจ้าของ หน้านี้อ่านอย่างเดียวและไม่แสดงคำสั่งปฏิบัติการหรือข้อมูลของผู้ให้บริการ")}
                }
            }
            div {
                class: "erpx-platform-operations-evidence-grid",
                for attestation in model.area_evidence().iter().copied() {
                    article {
                        class: "erpx-platform-operations-evidence-card",
                        "data-area": attestation.area().path(),
                        "data-adapter": attestation.adapter().id(),
                        "data-persistence": attestation.persistence().key(),
                        "data-reconciliation": attestation.reconciliation().key(),
                        "data-authorization": attestation.authorization().key(),
                        "data-read-only": "true",
                        h3 { "{attestation.area().label_for(product_locale())}" }
                        p { "{attestation.area().path()}" }
                        dl {
                            div { dt { {product_copy("Authenticated adapter", "อะแดปเตอร์ที่ยืนยันตัวตนแล้ว")} } dd { "{attestation.adapter().id()}" } }
                            div { dt { {product_copy("Data authority", "แหล่งข้อมูลที่เป็นข้อยุติ")} } dd { "{attestation.persistence().label_for(product_locale())}" } }
                            div { dt { {product_copy("Reconciliation", "การกระทบยอด")} } dd { "{attestation.reconciliation().label_for(product_locale())}" } }
                            div { dt { {product_copy("Authorization", "การอนุญาต")} } dd { "{attestation.authorization().label_for(product_locale())}" } }
                        }
                    }
                }
            }
            footer {
                class: "erpx-platform-operations-evidence-footer",
                "data-mutations": "disabled",
                {product_copy("Read-only evidence only. Commands, credentials, endpoints, and raw provider payloads remain outside the client boundary.", "แสดงเฉพาะหลักฐานแบบอ่านอย่างเดียว คำสั่ง ข้อมูลประจำตัว ปลายทาง และเพย์โหลดดิบของผู้ให้บริการอยู่นอกขอบเขตไคลเอนต์")}
            }
        }
    }
}

/// Product-surface component used by the coordinator's shell integration hook.
///
/// It intentionally renders only bounded state metadata and a read-only summary. There are no
/// command callbacks, secrets, provider endpoints, or client-side permission grants here.
#[component]
pub fn ProductCompletionSurface(state: ProductSurfaceState) -> Element {
    let locale = product_locale();
    let state = state.fail_closed_for_render();
    let accessibility = state.accessibility_contract();
    let surface = state.surface();
    let state_key = state.state_key();
    let title = surface.label_for(locale);
    let state_label = state.state_label(locale);
    let message = match &state {
        ProductSurfaceState::Loading { .. } => {
            product_copy("Loading authorized data…", "กำลังโหลดข้อมูลที่ได้รับอนุญาต…").to_string()
        }
        ProductSurfaceState::Ready { .. } => product_copy(
            "Authorized read model available.",
            "ข้อมูลอ่านที่ได้รับอนุญาตพร้อมใช้งาน",
        )
        .to_string(),
        _ => product_state_message(&state.message(), locale),
    };
    let summary = state.model().map(|model| model.summary_for(locale));
    let has_model = state.has_model();
    // This component never renders commands. A missing model is therefore still a
    // read-only/blocked boundary, rather than an editable empty state.
    let read_only = state.model().is_none_or(ProductSurfaceModel::is_read_only);
    let persistence = state.persistence();
    let reconciliation = state.reconciliation();
    let authorization = state.authorization();
    let projection = state.projection_freshness();
    let product_row_evidence = state.product_row_evidence().key();
    let stale_age_seconds = state
        .stale_age_seconds()
        .map_or_else(|| "none".to_string(), |age| age.to_string());
    let product_scope = state
        .model()
        .and_then(ProductSurfaceModel::product_scope)
        .map_or_else(|| "unbound".to_string(), ProductContextScope::label);
    let product_page_size = state
        .model()
        .and_then(ProductSurfaceModel::product_pagination)
        .map_or_else(|| "none".to_string(), |page| page.page_size().to_string());
    let product_has_next_page = state
        .model()
        .and_then(ProductSurfaceModel::product_pagination)
        .map_or("not-applicable", |page| {
            if page.has_next_page() {
                "true"
            } else {
                "false"
            }
        });
    let scope_id = current_scope_id().0;
    let surface_title_id = format!("erpx-product-title-{scope_id}");
    let state_title_id = format!("erpx-product-state-title-{scope_id}");
    let state_message_id = format!("erpx-product-state-message-{scope_id}");

    rsx! {
        section {
            class: "erpx-product-completion erpx-state-{state_key}",
            "data-surface": surface.key(),
            "data-route": surface.path(),
            "data-state": state_key,
            "data-mutations": if state.mutations_enabled() { "enabled" } else { "disabled" },
            "data-read-only": if read_only { "true" } else { "false" },
            "data-persistence": persistence.key(),
            "data-reconciliation": reconciliation.key(),
            "data-authorization": authorization.key(),
            "data-projection-freshness": projection.key(),
            "data-product-row-evidence": product_row_evidence,
            "data-product-scope": product_scope,
            "data-product-page-size": product_page_size,
            "data-product-has-next-page": product_has_next_page,
            "data-stale-age-seconds": stale_age_seconds,
            role: accessibility.role,
            aria_live: accessibility.aria_live,
            aria_atomic: accessibility.aria_atomic.to_string(),
            aria_busy: accessibility.aria_busy.to_string(),
            aria_labelledby: "{surface_title_id}",
            aria_describedby: "{state_message_id}",
            header {
                span { class: "ui-eyebrow", {product_copy("ERPX PRODUCT SURFACE", "หน้าจอผลิตภัณฑ์ ERPX")} }
                h2 { id: "{surface_title_id}", "{title}" }
            }
            div {
                class: "erpx-product-state",
                role: accessibility.role,
                aria_live: accessibility.aria_live,
                aria_atomic: accessibility.aria_atomic.to_string(),
                aria_busy: accessibility.aria_busy.to_string(),
                aria_labelledby: "{state_title_id}",
                aria_describedby: "{state_message_id}",
                "data-has-model": if has_model { "true" } else { "false" },
                strong { id: "{state_title_id}", "{state_label}" }
                p { id: "{state_message_id}", "{message}" }
            }
            if let Some(summary) = summary {
                div {
                    class: "erpx-product-summary",
                    aria_label: product_copy("Authorized read model summary", "สรุปโมเดลอ่านที่ได้รับอนุมัติ"),
                    p { "{summary}" }
                    small { {product_copy("Read-only presentation; server authorization remains authoritative.", "การแสดงผลอ่านอย่างเดียว โดยการอนุญาตจากเซิร์ฟเวอร์ยังเป็นข้อยุติ")} }
                }
            }
            section {
                class: "erpx-product-evidence",
                aria_label: product_copy("Persistence reconciliation and authorization evidence", "ความยืนยันและการอนุมัติของ reconciliation และหลักฐานของ authorization"),
                h3 { {product_copy("Read evidence", "หลักฐานข้อมูลอ่าน")} }
                dl {
                    div {
                        class: "erpx-product-evidence-item",
                        "data-evidence": "persistence",
                        dt { {product_copy("Data authority", "แหล่งข้อมูลที่เป็นข้อยุติ")} }
                        dd { "{persistence.label_for(locale)}" }
                    }
                    div {
                        class: "erpx-product-evidence-item",
                        "data-evidence": "reconciliation",
                        dt { {product_copy("Reconciliation", "การกระทบยอด")} }
                        dd { "{reconciliation.label_for(locale)}" }
                    }
                    div {
                        class: "erpx-product-evidence-item",
                        "data-evidence": "authorization",
                        dt { {product_copy("Authorization", "การอนุญาต")} }
                        dd { "{authorization.label_for(locale)}" }
                    }
                    div {
                        class: "erpx-product-evidence-item",
                        "data-evidence": "projection-freshness",
                        dt { {product_copy("Projection freshness", "ความใหม่ของโพรเจกชัน")} }
                        dd { "{projection.label_for(locale)}" }
                    }
                    div {
                        class: "erpx-product-evidence-item",
                        "data-evidence": "stale-age",
                        dt { {product_copy("Stale age", "อายุข้อมูลล้าสมัย")} }
                        dd { if let Some(age) = state.stale_age_seconds() {
                            {match locale {
                                Locale::En => format!("{} seconds", locale.formatter().format_integer(age as i64)),
                                Locale::Th => format!("{} วินาที", locale.formatter().format_integer(age as i64)),
                            }}
                        } else { {product_copy("Not stale", "ไม่ล้าสมัย")} } }
                    }
                }
                small {
                    {product_copy("Evidence is display-only. Missing, pending, drifted, denied, or blocked facts never enable commands.", "หลักฐานใช้เพื่อแสดงผลเท่านั้น ข้อมูลที่ขาดหาย รอดำเนินการ คลาดเคลื่อน ถูกปฏิเสธ หรือถูกบล็อกจะไม่เปิดใช้คำสั่ง")}
                }
            }
        }
    }
}

/// Errors returned when a display contract cannot be safely formed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductContractError {
    /// Identifier is empty, oversized, or contains unsafe route characters.
    InvalidOpaqueIdentity,
    /// A continuation token is empty, oversized, unsafe, or contains provider material.
    InvalidProductPageCursor,
    /// A product page size is zero or exceeds the UI adapter bound.
    InvalidProductPageSize,
    /// A product page reports more rows than its requested size.
    ProductPageCountExceedsSize,
    /// An empty product page cannot claim a continuation cursor.
    EmptyProductPageHasCursor,
    /// Vendor and native feed do not match.
    ConnectorMustUseNativeFeed,
    /// Connector cannot represent the one-million-user full-import bound.
    FullImportBelowLimit,
    /// A directory import checkpoint is outside the bounded display contract.
    InvalidDirectoryImportStatus,
    /// Directory lifecycle/readiness values must use the closed server vocabulary.
    InvalidDirectoryLifecycle,
    /// Directory-managed role mapping is required.
    DirectoryRoleMappingRequired,
    /// Identity links must be immutable.
    ImmutableIdentityLinkRequired,
    /// Role revocation must be append-only.
    AppendOnlyRevocationRequired,
    /// Connector transport must be TLS-only.
    TlsOnlyRequired,
    /// Revocation must use the five-minute budget.
    FiveMinuteRevocationRequired,
    /// High-risk access must require fresh MFA.
    HighRiskMfaRequired,
    /// Local fallback must remain break-glass-only.
    BreakGlassOnlyFallbackRequired,
    /// A display model must come from an authoritative persisted read boundary.
    PersistedReadModelRequired,
    /// A display model must carry server authorization evidence.
    ServerAuthorizationRequired,
    /// Product route surfaces are read-only until commands have an authoritative owner.
    ReadOnlySurfaceRequired,
    /// The authoritative freshness label and age contradict each other.
    FreshnessAgeConflict,
    /// A stale authoritative response must include an age beyond the freshness budget.
    StaleFreshnessAgeRequired,
    /// A response marked read-only cannot carry a stale model.
    StaleResponseRequiresStaleState,
    /// A response marked stale cannot carry a fresh or unknown-freshness model.
    FreshResponseCannotBeStale,
    /// A Platform Operations model must attest every displayed area.
    PlatformAdapterCoverageIncomplete,
    /// A Platform Operations attestation must match its area and real adapter.
    PlatformAdapterEvidenceMismatch,
    /// Product rows must be bound to an explicit tenant and holding.
    ProductContextScopeRequired,
    /// Product rows must carry validated pagination metadata.
    ProductPageMetadataRequired,
    /// Product row count must match the authoritative page metadata.
    ProductPageCountConflict,
}

impl fmt::Display for ProductContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidOpaqueIdentity => "invalid opaque identity",
            Self::InvalidProductPageCursor => "invalid product page cursor",
            Self::InvalidProductPageSize => "invalid product page size",
            Self::ProductPageCountExceedsSize => "product page row count exceeds page size",
            Self::EmptyProductPageHasCursor => "an empty product page cannot have a cursor",
            Self::ConnectorMustUseNativeFeed => "connector must use the vendor-native change feed",
            Self::FullImportBelowLimit => {
                "connector full-import capacity is below one million users"
            }
            Self::InvalidDirectoryImportStatus => "invalid directory import status",
            Self::InvalidDirectoryLifecycle => "invalid directory lifecycle or readiness",
            Self::DirectoryRoleMappingRequired => "directory-managed role mapping is required",
            Self::ImmutableIdentityLinkRequired => "immutable identity linking is required",
            Self::AppendOnlyRevocationRequired => "append-only role revocation is required",
            Self::TlsOnlyRequired => "TLS-only communication is required",
            Self::FiveMinuteRevocationRequired => "five-minute revocation is required",
            Self::HighRiskMfaRequired => "fresh MFA is required for high-risk access",
            Self::BreakGlassOnlyFallbackRequired => "local fallback must be break-glass-only",
            Self::PersistedReadModelRequired => "an authoritative persisted read model is required",
            Self::ServerAuthorizationRequired => "server authorization evidence is required",
            Self::ReadOnlySurfaceRequired => "the product display surface must be read-only",
            Self::FreshnessAgeConflict => "authoritative freshness and age conflict",
            Self::StaleFreshnessAgeRequired => {
                "a stale authoritative response must include an age beyond its budget"
            }
            Self::StaleResponseRequiresStaleState => {
                "a read-only response cannot carry a stale model"
            }
            Self::FreshResponseCannotBeStale => "a stale response must carry a stale model",
            Self::PlatformAdapterCoverageIncomplete => {
                "every Platform Operations area requires an authenticated adapter attestation"
            }
            Self::PlatformAdapterEvidenceMismatch => product_copy(
                "Platform Operations adapter evidence does not match its area",
                "เครื่องมือ Platform Operations ไม่ตรงกับพื้นที่ของมัน",
            ),
            Self::ProductContextScopeRequired => product_copy(
                "product rows require an explicit tenant and holding scope",
                "แถวผลิตภัณฑ์ต้องระบุชัดเจน tenant และขอบเขตการถือครอง",
            ),
            Self::ProductPageMetadataRequired => product_copy(
                "product rows require validated pagination metadata",
                "แถวผลิตภัณฑ์ต้องมีข้อมูล pagination ที่ได้รับการยืนยัน",
            ),
            Self::ProductPageCountConflict => product_copy(
                "product row count conflicts with authoritative page metadata",
                "จำนวนแถวของผลิตภัณฑ์ขัดแย้งกับข้อมูลที่มาจากแหล่งน่าเชื่อถือ",
            ),
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProductContractError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque(value: &str) -> OpaqueIdentity {
        OpaqueIdentity::new(value).expect("test identity must be valid")
    }

    fn product_scope() -> ProductContextScope {
        ProductContextScope::new(opaque("tenant-001"), opaque("holding-001"))
    }

    fn product_page(returned_count: u32) -> ProductPage {
        ProductPage::new(returned_count, returned_count.max(1), None)
            .expect("test page must satisfy pagination contract")
    }

    fn scoped_product_model(
        context: ProductContext,
        projection: ProjectionFreshness,
        returned_count: u32,
        redacted_field_count: u32,
        reconciliation: ReconciliationEvidence,
    ) -> ContextSurfaceModel {
        ContextSurfaceModel::from_authoritative_scoped_read_model(
            context,
            projection,
            product_scope(),
            product_page(returned_count),
            redacted_field_count,
            reconciliation,
        )
    }

    fn directory_surface() -> ProductSurfaceModel {
        let connector = DirectoryConnectorContract::new(
            opaque("connector-001"),
            DirectoryVendor::GenericLdap,
            ConnectorScope::Platform,
            DirectoryConnectorPolicy::strict(DirectoryVendor::GenericLdap),
        )
        .expect("strict connector must cross the display boundary");
        let mapping = DirectoryRoleMapping::new(
            "ERPX-Operators",
            "operations.viewer",
            ConnectorScope::Platform,
            RoleRevocationMode::AppendOnly,
        )
        .expect("append-only mapping must cross the display boundary");
        let link = ImmutableIdentityLink::new("external-subject-001", opaque("principal-001"));
        ProductSurfaceModel::DirectoryAdministration(DirectoryAdministrationModel::new(
            vec![connector],
            vec![mapping],
            vec![link],
        ))
    }

    #[test]
    fn opaque_identity_is_bounded_and_route_safe_at_the_boundary() {
        let at_limit = "a".repeat(MAX_OPAQUE_ID_BYTES);
        assert_eq!(opaque(&at_limit).as_str(), at_limit);
        assert_eq!(
            OpaqueIdentity::new(format!("{at_limit}a")),
            Err(ProductContractError::InvalidOpaqueIdentity)
        );

        for unsafe_value in [
            "",
            "dn=CN=Operators,DC=example,DC=internal",
            "principal/001",
            "https://directory.example.internal/users/1",
            "bearer-token-001",
            "秘密",
        ] {
            assert_eq!(
                OpaqueIdentity::new(unsafe_value),
                Err(ProductContractError::InvalidOpaqueIdentity),
                "unsafe identity must be rejected: {unsafe_value}"
            );
        }
    }

    #[test]
    fn safe_display_text_redacts_provider_material_and_bounds_output() {
        for sensitive_value in [
            "password=directory-secret",
            "client_secret=provider-secret",
            "Authorization: Bearer abc123",
            "ldaps://directory.example.internal",
            "database_url=postgres://user:password@host/db",
            "session_id=opaque-session",
        ] {
            let display = SafeDisplayText::new(sensitive_value);
            assert!(
                display.is_redacted(),
                "value must be redacted: {sensitive_value}"
            );
            assert_eq!(display.as_str(), "[redacted]");
        }

        assert_eq!(
            SafeDisplayText::new("  Projection is fresh.  ").as_str(),
            "Projection is fresh."
        );
        assert!(SafeDisplayText::new("x".repeat(MAX_SAFE_TEXT_BYTES + 1)).is_redacted());
        assert!(SafeDisplayText::new("line\nbreak").is_redacted());
    }

    #[test]
    fn directory_surface_keeps_untrusted_identity_fields_redacted_and_read_only() {
        let ProductSurfaceModel::DirectoryAdministration(compliant_model) = directory_surface()
        else {
            panic!("directory fixture must construct the directory surface");
        };
        assert!(compliant_model.has_platform_connector());
        assert_eq!(compliant_model.holding_connector_count(), 0);
        assert!(compliant_model.is_contract_compliant());

        let mapping = DirectoryRoleMapping::new(
            "cn=Operators,dc=example,dc=internal password=secret",
            "operations.viewer",
            ConnectorScope::Holding {
                holding_id: opaque("holding-001"),
            },
            RoleRevocationMode::AppendOnly,
        )
        .expect("revocation mode is authoritative and valid");
        let link = ImmutableIdentityLink::new(
            "external-subject-001 client_secret=secret",
            opaque("principal-001"),
        );
        assert!(mapping.directory_group().is_redacted());
        assert!(link.external_subject().is_redacted());
        assert!(mapping.scope().label().contains("holding-001"));
        assert!(link.is_immutable());

        let model = ProductSurfaceModel::DirectoryAdministration(
            DirectoryAdministrationModel::new(Vec::new(), vec![mapping], vec![link]),
        );
        assert_eq!(model.surface(), ProductSurface::DirectoryAdministration);
        assert_eq!(
            model.summary(),
            "0 connector(s), 1 role mapping(s), 1 immutable identity link(s)"
        );
        assert!(model.is_read_only());

        let state = ProductSurfaceState::ReadOnly {
            model,
            reason: SafeDisplayText::new("Directory facts are server-authorized."),
        };
        assert_eq!(state.surface().path(), "/platform/directories");
        assert_eq!(state.state_key(), "read-only");
        assert!(!state.mutations_enabled());
        assert!(!format!("{state:?}").contains("client_secret=secret"));
    }

    #[test]
    fn all_g9_through_g11_contexts_have_bounded_read_only_surfaces() {
        let expected = [
            (ProductContext::Sales, "/sales", "sales"),
            (ProductContext::Procurement, "/procurement", "procurement"),
            (ProductContext::Inventory, "/inventory", "inventory"),
            (ProductContext::Maintenance, "/maintenance", "maintenance"),
            (ProductContext::Quality, "/quality", "quality"),
            (ProductContext::MarketingCrm, "/marketing", "marketing-crm"),
            (ProductContext::HrPayroll, "/people", "hr-payroll"),
            (ProductContext::Pos, "/pos", "pos"),
            (ProductContext::Legal, "/legal", "legal"),
        ];

        for (context, path, key) in expected {
            assert!(PRODUCT_CONTEXTS.contains(&context));
            let model = ProductSurfaceModel::Context(ContextSurfaceModel::new(
                context,
                ProjectionFreshness::Fresh,
                7,
                2,
            ));
            let state = ProductSurfaceState::Ready { model };
            assert_eq!(state.surface().path(), path);
            assert_eq!(state.surface().key(), key);
            assert!(state.model().is_some_and(ProductSurfaceModel::is_read_only));
            assert!(!state.mutations_enabled());
            assert_eq!(state.message().as_str(), "Authorized read model available.");
        }
    }

    #[test]
    fn projection_status_context_does_not_fabricate_product_rows() {
        let status_model = ContextSurfaceModel::new(
            ProductContext::Operations,
            ProjectionFreshness::Fresh,
            17,
            9,
        );
        assert_eq!(status_model.visible_row_count(), None);
        assert_eq!(
            status_model.product_row_evidence(),
            ProductRowEvidence::Unavailable
        );
        assert_eq!(
            status_model.persistence(),
            PersistenceEvidence::AuthoritativeReadModel
        );
        assert_eq!(
            status_model.reconciliation(),
            ReconciliationEvidence::NotReported
        );
        assert_eq!(
            status_model.authorization(),
            AuthorizationEvidence::ServerAuthorized
        );

        let status_surface = ProductSurfaceModel::Context(status_model);
        assert_eq!(
            status_surface.summary(),
            "17 authoritative projection status row(s); product rows unavailable"
        );
        assert_eq!(
            status_surface.product_row_evidence(),
            ProductRowEvidence::Unavailable
        );

        let product_model = scoped_product_model(
            ProductContext::Operations,
            ProjectionFreshness::Fresh,
            17,
            9,
            ReconciliationEvidence::Reconciled,
        );
        assert_eq!(product_model.visible_row_count(), Some(17));
        assert_eq!(
            product_model.product_row_evidence(),
            ProductRowEvidence::Authoritative
        );
        assert_eq!(
            ProductSurfaceModel::Context(product_model).summary(),
            "17 visible row(s), 9 redacted field(s)"
        );
    }

    #[test]
    fn projection_status_adapter_is_context_typed_and_fails_closed_without_stale_age() {
        let adapter = ProductRouteStateAdapter;

        let fresh = adapter.adapt_projection_status(ProductContext::Sales, 2, "fresh", None);
        assert_eq!(
            fresh.surface(),
            ProductSurface::Context(ProductContext::Sales)
        );
        assert_eq!(fresh.state_key(), "read-only");
        assert!(fresh.has_model());
        assert_eq!(
            fresh.product_row_evidence(),
            ProductRowEvidence::Unavailable
        );

        let unknown = adapter.adapt_projection_status(
            ProductContext::MarketingCrm,
            1,
            "unrecognized-server-label",
            None,
        );
        assert_eq!(
            unknown.surface(),
            ProductSurface::Context(ProductContext::MarketingCrm)
        );
        assert_eq!(unknown.state_key(), "read-only");
        assert_eq!(unknown.projection_freshness(), ProjectionFreshness::Unknown);

        let empty = adapter.adapt_projection_status(ProductContext::Quality, 0, "fresh", None);
        assert_eq!(empty.state_key(), "empty");
        assert!(!empty.has_model());

        let stale_without_age =
            adapter.adapt_projection_status(ProductContext::Legal, 3, "stale", None);
        assert_eq!(stale_without_age.state_key(), "conflict");
        assert!(!stale_without_age.has_model());
        assert_eq!(
            stale_without_age.authorization(),
            AuthorizationEvidence::Blocked
        );
    }

    #[test]
    fn product_surface_accessibility_contract_follows_explicit_state() {
        let surface = ProductSurface::Context(ProductContext::Sales);
        let cases = [
            (
                ProductSurfaceState::Loading { surface },
                UiStateKind::Loading,
            ),
            (
                ProductSurfaceState::Denied {
                    surface,
                    reason: SafeDisplayText::new("denied"),
                },
                UiStateKind::Denied,
            ),
            (
                ProductSurfaceState::Conflict {
                    surface,
                    reason: SafeDisplayText::new("conflict"),
                },
                UiStateKind::Conflict,
            ),
            (
                ProductSurfaceState::Offline {
                    surface,
                    reason: SafeDisplayText::new("offline"),
                },
                UiStateKind::Offline,
            ),
            (
                ProductSurfaceState::Error {
                    surface,
                    reason: SafeDisplayText::new("error"),
                },
                UiStateKind::Error,
            ),
        ];

        for (state, kind) in cases {
            assert_eq!(state.accessibility_contract(), kind.accessibility());
        }

        let ready = ProductSurfaceState::Ready {
            model: ProductSurfaceModel::Context(ContextSurfaceModel::new(
                ProductContext::Sales,
                ProjectionFreshness::Fresh,
                1,
                0,
            )),
        };
        assert_eq!(
            ready.accessibility_contract(),
            UiStateKind::ReadOnly.accessibility()
        );
    }

    #[test]
    fn product_rows_require_holding_scope_and_validated_page_metadata() {
        let scoped = scoped_product_model(
            ProductContext::Sales,
            ProjectionFreshness::Fresh,
            25,
            2,
            ReconciliationEvidence::Reconciled,
        );
        assert_eq!(
            scoped
                .scope()
                .expect("scope must be attached")
                .tenant_id()
                .as_str(),
            "tenant-001"
        );
        assert_eq!(
            scoped
                .scope()
                .expect("scope must be attached")
                .holding_id()
                .as_str(),
            "holding-001"
        );
        assert_eq!(
            scoped
                .pagination()
                .expect("page must be attached")
                .returned_count(),
            25
        );
        assert_eq!(
            scoped
                .pagination()
                .expect("page must be attached")
                .page_size(),
            25
        );

        let unscoped =
            ProductSurfaceModel::Context(ContextSurfaceModel::from_authoritative_read_model(
                ProductContext::Sales,
                ProjectionFreshness::Fresh,
                25,
                2,
                ReconciliationEvidence::Reconciled,
            ));
        assert_eq!(
            AuthoritativeProductReadModel::new(unscoped.clone(), None),
            Err(ProductContractError::ProductContextScopeRequired)
        );
        let blocked = ProductSurfaceState::ReadOnly {
            model: unscoped,
            reason: SafeDisplayText::new("unscoped product response"),
        }
        .fail_closed_for_render();
        assert_eq!(blocked.state_key(), "conflict");
        assert!(!blocked.has_model());
        assert_eq!(blocked.authorization(), AuthorizationEvidence::Blocked);
    }

    #[test]
    fn product_pagination_is_bounded_and_cursor_debug_is_redacted() {
        let cursor = ProductPageCursor::new("cursor-001").expect("safe cursor must validate");
        let page = ProductPage::new(25, 100, Some(cursor)).expect("page must validate");
        assert_eq!(page.returned_count(), 25);
        assert_eq!(page.page_size(), 100);
        assert!(page.has_next_page());
        assert!(!format!("{page:?}").contains("cursor-001"));

        assert_eq!(
            ProductPage::new(1, 0, None),
            Err(ProductContractError::InvalidProductPageSize)
        );
        assert_eq!(
            ProductPage::new(MAX_PRODUCT_PAGE_SIZE + 1, MAX_PRODUCT_PAGE_SIZE + 1, None),
            Err(ProductContractError::InvalidProductPageSize)
        );
        assert_eq!(
            ProductPage::new(101, 100, None),
            Err(ProductContractError::ProductPageCountExceedsSize)
        );
        assert_eq!(
            ProductPage::new(
                0,
                100,
                Some(ProductPageCursor::new("cursor-001").expect("cursor must validate"))
            ),
            Err(ProductContractError::EmptyProductPageHasCursor)
        );
        assert_eq!(
            ProductPageCursor::new("access_token-secret"),
            Err(ProductContractError::InvalidProductPageCursor)
        );
    }

    #[test]
    fn render_boundary_does_not_hide_stale_projection_in_read_only_state() {
        let model = ProductSurfaceModel::Context(ContextSurfaceModel::new(
            ProductContext::Operations,
            ProjectionFreshness::Stale,
            3,
            0,
        ));
        let state = ProductSurfaceState::ReadOnly {
            model,
            reason: SafeDisplayText::new("Projection status returned."),
        }
        .fail_closed_for_render();

        assert_eq!(state.state_key(), "conflict");
        assert!(!state.has_model());
        assert_eq!(state.authorization(), AuthorizationEvidence::Blocked);
        assert_eq!(
            state.message().as_str(),
            "A stale projection must be rendered with an authoritative stale age."
        );
    }

    #[test]
    fn render_boundary_blocks_non_authoritative_models_and_invalid_stale_ages() {
        let non_authoritative = ProductSurfaceState::ReadOnly {
            model: ProductSurfaceModel::PlatformOperations(PlatformOperationsModel::new(
                vec![PlatformOperationsArea::Projections],
                Vec::new(),
            )),
            reason: SafeDisplayText::new("Platform status returned."),
        }
        .fail_closed_for_render();
        assert_eq!(non_authoritative.state_key(), "blocked");
        assert!(!non_authoritative.has_model());
        assert_eq!(
            non_authoritative.persistence(),
            PersistenceEvidence::Unavailable
        );

        let fresh_model = ProductSurfaceModel::Context(ContextSurfaceModel::new(
            ProductContext::Finance,
            ProjectionFreshness::Fresh,
            2,
            0,
        ));
        let invalid_stale = ProductSurfaceState::Stale {
            model: fresh_model,
            age_seconds: MAX_PROJECTION_FRESHNESS_SECONDS + 1,
            reason: SafeDisplayText::new("Stale projection."),
        }
        .fail_closed_for_render();
        assert_eq!(invalid_stale.state_key(), "conflict");
        assert!(!invalid_stale.has_model());

        let valid_stale = ProductSurfaceState::Stale {
            model: ProductSurfaceModel::Context(ContextSurfaceModel::new(
                ProductContext::Finance,
                ProjectionFreshness::Stale,
                2,
                0,
            )),
            age_seconds: MAX_PROJECTION_FRESHNESS_SECONDS + 1,
            reason: SafeDisplayText::new("Stale projection."),
        }
        .fail_closed_for_render();
        assert_eq!(valid_stale.state_key(), "stale");
        assert_eq!(valid_stale.stale_age_seconds(), Some(301));
    }

    #[test]
    fn context_surface_exposes_persistence_reconciliation_and_authorization_evidence() {
        let model = scoped_product_model(
            ProductContext::Finance,
            ProjectionFreshness::Fresh,
            42,
            3,
            ReconciliationEvidence::Reconciled,
        );

        assert_eq!(
            model.persistence(),
            PersistenceEvidence::AuthoritativeReadModel
        );
        assert_eq!(model.reconciliation(), ReconciliationEvidence::Reconciled);
        assert_eq!(
            model.authorization(),
            AuthorizationEvidence::ServerAuthorized
        );
        assert_eq!(model.persistence().key(), "authoritative-read-model");
        assert_eq!(model.reconciliation().label(), "Reconciled");
        assert_eq!(model.authorization().key(), "server-authorized");

        let state = ProductSurfaceState::ReadOnly {
            model: ProductSurfaceModel::Context(model),
            reason: SafeDisplayText::new("Server-authorized persisted projection."),
        };
        assert_eq!(
            state.persistence(),
            PersistenceEvidence::AuthoritativeReadModel
        );
        assert_eq!(state.reconciliation(), ReconciliationEvidence::Reconciled);
        assert_eq!(
            state.authorization(),
            AuthorizationEvidence::ServerAuthorized
        );
        assert!(!state.mutations_enabled());
    }

    #[test]
    fn pending_or_drifted_reconciliation_remains_visible_and_blocked() {
        for reconciliation in [
            ReconciliationEvidence::Pending,
            ReconciliationEvidence::DriftDetected,
        ] {
            let model = scoped_product_model(
                ProductContext::Operations,
                ProjectionFreshness::Fresh,
                10,
                0,
                reconciliation,
            );
            assert_eq!(model.reconciliation(), reconciliation);
            assert!(!reconciliation.label().is_empty());
        }

        let state = ProductSurfaceState::Blocked {
            surface: ProductSurface::Context(ProductContext::Operations),
            reason: SafeDisplayText::new("Reconciliation drift blocks this read."),
        };
        assert_eq!(state.state_key(), "blocked");
        assert_eq!(state.kind(), Some(UiStateKind::Denied));
        assert_eq!(state.authorization(), AuthorizationEvidence::Blocked);
        assert_eq!(state.persistence(), PersistenceEvidence::Unavailable);
        assert_eq!(state.reconciliation(), ReconciliationEvidence::NotReported);
        assert!(!state.has_model());
        assert!(!state.mutations_enabled());
    }

    #[test]
    fn drifted_product_read_models_fail_closed_at_the_final_render_boundary() {
        let model = ProductSurfaceModel::Context(scoped_product_model(
            ProductContext::Finance,
            ProjectionFreshness::Fresh,
            12,
            1,
            ReconciliationEvidence::DriftDetected,
        ));
        let state = ProductSurfaceState::ReadOnly {
            model,
            reason: SafeDisplayText::new("Finance read model returned."),
        }
        .fail_closed_for_render();

        assert_eq!(state.state_key(), "conflict");
        assert_eq!(state.kind(), Some(UiStateKind::Conflict));
        assert!(!state.has_model());
        assert_eq!(state.authorization(), AuthorizationEvidence::Blocked);
        assert_eq!(state.reconciliation(), ReconciliationEvidence::NotReported);
        assert!(!state.mutations_enabled());
        assert!(
            state
                .message()
                .as_str()
                .contains("reconciliation detected drift")
        );
    }

    #[test]
    fn unavailable_states_expose_missing_evidence_instead_of_fabricating_readiness() {
        let state = ProductSurfaceState::Offline {
            surface: ProductSurface::Context(ProductContext::Sales),
            reason: SafeDisplayText::new("The authorized read model is unavailable."),
        };
        assert_eq!(state.persistence(), PersistenceEvidence::Unavailable);
        assert_eq!(state.reconciliation(), ReconciliationEvidence::NotReported);
        assert_eq!(state.authorization(), AuthorizationEvidence::Unavailable);
        assert_eq!(state.state_key(), "offline");
        assert!(!state.has_model());
    }

    #[test]
    fn strict_connector_policy_is_complete_for_each_supported_vendor() {
        for vendor in [
            DirectoryVendor::GenericLdap,
            DirectoryVendor::ActiveDirectoryDomainServices,
            DirectoryVendor::UniventionCorporateServer,
        ] {
            let contract = DirectoryConnectorContract::new(
                opaque("connector-001"),
                vendor,
                ConnectorScope::Platform,
                DirectoryConnectorPolicy::strict(vendor),
            )
            .expect("strict vendor policy must cross the display boundary");
            let policy = contract.policy();
            assert_eq!(policy.change_feed, vendor.native_change_feed());
            assert_eq!(policy.full_import.max_users(), MAX_FULL_IMPORT_USERS);
            assert_eq!(policy.role_mapping, RoleMappingMode::DirectoryManaged);
            assert_eq!(policy.identity_linking, IdentityLinkingMode::Immutable);
            assert_eq!(policy.role_revocation, RoleRevocationMode::AppendOnly);
            assert!(policy.security.tls_only);
            assert!(
                policy
                    .security
                    .high_risk_mfa
                    .satisfies_high_risk_requirement()
            );
            assert_eq!(policy.security.revocation, RevocationPolicy::FiveMinutes);
            assert_eq!(
                policy.security.local_fallback,
                LocalFallbackMode::BreakGlassOnly
            );
        }
    }

    #[test]
    fn directory_import_display_status_is_bounded_and_secret_free() {
        let status = DirectoryImportStatusDisplay::new(
            opaque("connector-001"),
            opaque("import-001"),
            "running",
            "rfc4533",
            MAX_FULL_IMPORT_USERS,
            250_000,
            250,
            1_000,
            "2026-07-30T00:00:00Z",
            None::<&str>,
            "2026-07-30T00:05:00Z",
            None::<&str>,
        )
        .expect("bounded import status must cross the display boundary");
        assert_eq!(status.max_users(), MAX_FULL_IMPORT_USERS);
        assert_eq!(status.users_seen(), 250_000);
        assert_eq!(status.cursor_kind().as_str(), "rfc4533");

        assert!(
            DirectoryImportStatusDisplay::new(
                opaque("connector-001"),
                opaque("import-002"),
                "running",
                "rfc4533",
                MAX_FULL_IMPORT_USERS + 1,
                0,
                0,
                1,
                "started",
                None::<&str>,
                "updated",
                None::<&str>,
            )
            .is_err()
        );
        assert!(
            DirectoryImportStatusDisplay::new(
                opaque("connector-001"),
                opaque("import-003"),
                "running",
                "rfc4533",
                MAX_FULL_IMPORT_USERS,
                MAX_FULL_IMPORT_USERS + 1,
                0,
                1,
                "started",
                None::<&str>,
                "updated",
                None::<&str>,
            )
            .is_err()
        );
    }
}
