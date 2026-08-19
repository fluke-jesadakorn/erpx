//! Client-local support and incident presentation contracts.
//!
//! The types in this module are deliberately transport-neutral. An application converts an
//! authorized gRPC/HTTP response into these display-safe models at its boundary; this crate does
//! not carry capability secrets, access tokens, holding credentials, or internal support notes.

use std::{fmt, time::Duration};

use erpx_ui_core::{Locale, NavItem, ProjectionFreshness, TranslationKey, UiStateKind};

/// Maximum duration for each customer-approved support mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportMode {
    /// Read-only diagnostics and inspection.
    DiagnosticRead,
    /// Approved holding mutations performed by a support agent.
    HoldingMutation,
    /// Approved impersonation of a represented holding user.
    Impersonation,
}

/// Maximum diagnostic-read capability duration.
pub const DIAGNOSTIC_READ_MAX_SECONDS: u64 = 8 * 60 * 60;
/// Maximum holding-mutation capability duration.
pub const HOLDING_MUTATION_MAX_SECONDS: u64 = 2 * 60 * 60;
/// Maximum impersonation capability duration.
pub const IMPERSONATION_MAX_SECONDS: u64 = 2 * 60 * 60;

impl SupportMode {
    /// Stable mode identifier used by presentation adapters.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DiagnosticRead => "diagnostic_read",
            Self::HoldingMutation => "holding_mutation",
            Self::Impersonation => "impersonation",
        }
    }

    /// Maximum duration allowed by the support-access policy.
    pub const fn max_seconds(self) -> u64 {
        match self {
            Self::DiagnosticRead => DIAGNOSTIC_READ_MAX_SECONDS,
            Self::HoldingMutation => HOLDING_MUTATION_MAX_SECONDS,
            Self::Impersonation => IMPERSONATION_MAX_SECONDS,
        }
    }

    /// Maximum duration as a standard library duration.
    pub fn max_duration(self) -> Duration {
        Duration::from_secs(self.max_seconds())
    }

    /// Return the localized mode label.
    pub const fn label(self, locale: Locale) -> &'static str {
        self.translation_key().text(locale)
    }

    const fn translation_key(self) -> TranslationKey {
        match self {
            Self::DiagnosticRead => TranslationKey::SupportModeDiagnosticRead,
            Self::HoldingMutation => TranslationKey::SupportModeHoldingMutation,
            Self::Impersonation => TranslationKey::SupportModeImpersonation,
        }
    }

    /// Validate a requested duration against the mode limit.
    pub fn validate_duration(self, requested: Duration) -> Result<(), SupportDurationError> {
        if requested.is_zero() || requested > self.max_duration() {
            Err(SupportDurationError {
                mode: self,
                requested_seconds: requested.as_secs(),
                max_seconds: self.max_seconds(),
            })
        } else {
            Ok(())
        }
    }
}

/// A requested support duration is outside the policy boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportDurationError {
    /// Mode whose limit was exceeded.
    pub mode: SupportMode,
    /// Requested duration in whole seconds.
    pub requested_seconds: u64,
    /// Maximum permitted duration in whole seconds.
    pub max_seconds: u64,
}

/// Support-ticket lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportTicketStatus {
    /// Newly submitted ticket.
    New,
    /// Ticket has been classified and routed.
    Triaged,
    /// Waiting for a customer response.
    WaitingForCustomer,
    /// Waiting for customer-approved access.
    AccessRequested,
    /// Agent is actively working the ticket.
    InProgress,
    /// Customer approved the requested support access.
    Approved,
    /// Agent reports the issue resolved.
    Resolved,
    /// Customer confirmed the resolution.
    CustomerConfirmed,
    /// Terminally closed ticket.
    Closed,
}

impl SupportTicketStatus {
    /// Parse the stable status token emitted by the safe ticket transport DTO.
    pub fn from_transport(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "new" => Some(Self::New),
            "triaged" => Some(Self::Triaged),
            "waiting_for_customer" => Some(Self::WaitingForCustomer),
            "access_requested" => Some(Self::AccessRequested),
            "in_progress" => Some(Self::InProgress),
            "approved" => Some(Self::Approved),
            "resolved" => Some(Self::Resolved),
            "customer_confirmed" => Some(Self::CustomerConfirmed),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }

    /// Return the localized lifecycle label.
    pub const fn label(self, locale: Locale) -> &'static str {
        self.translation_key().text(locale)
    }

    const fn translation_key(self) -> TranslationKey {
        match self {
            Self::New => TranslationKey::SupportStatusNew,
            Self::Triaged => TranslationKey::SupportStatusTriaged,
            Self::WaitingForCustomer => TranslationKey::SupportStatusWaitingForCustomer,
            Self::AccessRequested => TranslationKey::SupportStatusAccessRequested,
            Self::InProgress => TranslationKey::SupportStatusInProgress,
            Self::Approved => TranslationKey::SupportStatusApproved,
            Self::Resolved => TranslationKey::SupportStatusResolved,
            Self::CustomerConfirmed => TranslationKey::SupportStatusCustomerConfirmed,
            Self::Closed => TranslationKey::SupportStatusClosed,
        }
    }
}

/// Incident lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncidentStatus {
    /// Incident response is investigating the symptoms.
    Investigating,
    /// Impact and affected services are identified.
    Identified,
    /// Mitigation is being monitored.
    Monitoring,
    /// Incident impact is resolved.
    Resolved,
    /// Corrective actions and timeline are complete.
    PostmortemComplete,
}

impl IncidentStatus {
    /// Parse the stable status token emitted by the safe incident transport DTO.
    pub fn from_transport(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "investigating" => Some(Self::Investigating),
            "identified" => Some(Self::Identified),
            "monitoring" => Some(Self::Monitoring),
            "resolved" => Some(Self::Resolved),
            "postmortem_complete" => Some(Self::PostmortemComplete),
            _ => None,
        }
    }

    /// Return the stable transport token for this status.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Investigating => "investigating",
            Self::Identified => "identified",
            Self::Monitoring => "monitoring",
            Self::Resolved => "resolved",
            Self::PostmortemComplete => "postmortem_complete",
        }
    }

    /// Return the localized lifecycle label.
    pub const fn label(self, locale: Locale) -> &'static str {
        self.translation_key().text(locale)
    }

    const fn translation_key(self) -> TranslationKey {
        match self {
            Self::Investigating => TranslationKey::IncidentStatusInvestigating,
            Self::Identified => TranslationKey::IncidentStatusIdentified,
            Self::Monitoring => TranslationKey::IncidentStatusMonitoring,
            Self::Resolved => TranslationKey::IncidentStatusResolved,
            Self::PostmortemComplete => TranslationKey::IncidentStatusPostmortemComplete,
        }
    }
}

/// Customer-approval state shown on support access request and session surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportApprovalState {
    /// The selected mode does not need customer approval.
    NotRequired,
    /// The request is waiting for an eligible customer approver.
    Pending,
    /// An eligible customer approver approved the exact request.
    Approved,
    /// The request was rejected.
    Rejected,
    /// The approval window ended before use.
    Expired,
    /// The approval or capability was revoked.
    Revoked,
}

impl SupportApprovalState {
    /// Return the localized approval label.
    pub const fn label(self, locale: Locale) -> &'static str {
        self.translation_key().text(locale)
    }

    const fn translation_key(self) -> TranslationKey {
        match self {
            Self::NotRequired => TranslationKey::SupportApprovalNotRequired,
            Self::Pending => TranslationKey::SupportApprovalPending,
            Self::Approved => TranslationKey::SupportApprovalApproved,
            Self::Rejected => TranslationKey::SupportApprovalRejected,
            Self::Expired => TranslationKey::SupportApprovalExpired,
            Self::Revoked => TranslationKey::SupportApprovalRevoked,
        }
    }
}

/// Safe represented-user context for impersonation banners.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupportImpersonationState {
    /// No represented user is active.
    NotActive,
    /// An impersonation request is awaiting approval.
    PendingApproval,
    /// An approved represented-user session is active.
    Active {
        /// Display-only represented-user label.
        represented_label: String,
        /// Customer-visible reason, never a token or credential.
        reason: String,
    },
    /// The represented-user session ended or was revoked.
    Ended,
}

impl SupportImpersonationState {
    /// Whether this context is actively impersonating a represented user.
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    /// Return the represented-user label when impersonation is active.
    pub fn represented_label(&self) -> Option<&str> {
        match self {
            Self::Active {
                represented_label, ..
            } => Some(represented_label),
            Self::NotActive | Self::PendingApproval | Self::Ended => None,
        }
    }
}

/// Safe support-session context shown by banners and approval pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportContextMetadata {
    /// Requested support mode.
    pub mode: SupportMode,
    /// Exact customer-approval state for this request.
    pub approval: SupportApprovalState,
    /// Represented-user state; no impersonation token is carried.
    pub impersonation: SupportImpersonationState,
    /// Whether the current context permits no mutations.
    pub read_only: bool,
    /// Display-only expiry timestamp supplied by the authorized boundary.
    pub expires_at: Option<String>,
}

impl SupportContextMetadata {
    /// Create a context with explicit approval and impersonation metadata.
    pub fn new(
        mode: SupportMode,
        approval: SupportApprovalState,
        impersonation: SupportImpersonationState,
        read_only: bool,
        expires_at: Option<impl Into<String>>,
    ) -> Self {
        Self {
            mode,
            approval,
            impersonation,
            read_only,
            expires_at: expires_at.map(|value| redact_value(&value.into())),
        }
    }

    /// Whether customer approval is currently required and not yet satisfied.
    pub const fn approval_pending(&self) -> bool {
        matches!(self.approval, SupportApprovalState::Pending)
    }

    /// Whether this context is actively impersonating a represented user.
    pub const fn is_impersonating(&self) -> bool {
        self.impersonation.is_active()
    }

    /// Whether this context must be rendered as non-mutating.
    pub const fn is_read_only(&self) -> bool {
        self.read_only
            || self.is_impersonating()
            || matches!(
                self.approval,
                SupportApprovalState::Pending
                    | SupportApprovalState::Rejected
                    | SupportApprovalState::Expired
                    | SupportApprovalState::Revoked
            )
    }
}

/// Stable support and incident route identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportRoute {
    /// Platform support queue.
    PlatformSupport,
    /// Platform support intake.
    PlatformSupportNew,
    /// Platform support ticket detail.
    PlatformSupportDetail,
    /// Platform support access request.
    PlatformSupportAccess,
    /// Platform support diagnostics.
    PlatformSupportDiagnostics,
    /// Active support sessions.
    PlatformSupportSessions,
    /// Platform incident queue.
    PlatformIncidents,
    /// Platform incident detail.
    PlatformIncidentDetail,
    /// Holding support queue.
    HoldingSupport,
    /// Holding support intake.
    HoldingSupportNew,
    /// Holding support ticket detail.
    HoldingSupportDetail,
    /// Holding customer-approval surface.
    HoldingSupportAccessApproval,
    /// Holding platform-access surface.
    HoldingPlatformAccess,
}

/// Audience for a support route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportRouteAudience {
    /// Platform support staff.
    Platform,
    /// Customer holding users.
    Holding,
}

/// Safe route metadata for the Dioxus navigation shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportRouteDefinition {
    /// Stable route identifier.
    pub route: SupportRoute,
    /// Canonical path template.
    pub template: &'static str,
    /// Platform or holding audience.
    pub audience: SupportRouteAudience,
    /// Whether an authenticated session is required.
    pub authenticated: bool,
}

/// Canonical support and incident routes required by platform and holding clients.
pub const SUPPORT_ROUTES: &[SupportRouteDefinition] = &[
    SupportRouteDefinition {
        route: SupportRoute::PlatformSupport,
        template: "/platform/support",
        audience: SupportRouteAudience::Platform,
        authenticated: true,
    },
    SupportRouteDefinition {
        route: SupportRoute::PlatformSupportNew,
        template: "/platform/support/new",
        audience: SupportRouteAudience::Platform,
        authenticated: true,
    },
    SupportRouteDefinition {
        route: SupportRoute::PlatformSupportDetail,
        template: "/platform/support/:id",
        audience: SupportRouteAudience::Platform,
        authenticated: true,
    },
    SupportRouteDefinition {
        route: SupportRoute::PlatformSupportAccess,
        template: "/platform/support/:id/access",
        audience: SupportRouteAudience::Platform,
        authenticated: true,
    },
    SupportRouteDefinition {
        route: SupportRoute::PlatformSupportDiagnostics,
        template: "/platform/support/:id/diagnostics",
        audience: SupportRouteAudience::Platform,
        authenticated: true,
    },
    SupportRouteDefinition {
        route: SupportRoute::PlatformSupportSessions,
        template: "/platform/support/sessions",
        audience: SupportRouteAudience::Platform,
        authenticated: true,
    },
    SupportRouteDefinition {
        route: SupportRoute::PlatformIncidents,
        template: "/platform/incidents",
        audience: SupportRouteAudience::Platform,
        authenticated: true,
    },
    SupportRouteDefinition {
        route: SupportRoute::PlatformIncidentDetail,
        template: "/platform/incidents/:id",
        audience: SupportRouteAudience::Platform,
        authenticated: true,
    },
    SupportRouteDefinition {
        route: SupportRoute::HoldingSupport,
        template: "/support",
        audience: SupportRouteAudience::Holding,
        authenticated: true,
    },
    SupportRouteDefinition {
        route: SupportRoute::HoldingSupportNew,
        template: "/support/new",
        audience: SupportRouteAudience::Holding,
        authenticated: true,
    },
    SupportRouteDefinition {
        route: SupportRoute::HoldingSupportDetail,
        template: "/support/:id",
        audience: SupportRouteAudience::Holding,
        authenticated: true,
    },
    SupportRouteDefinition {
        route: SupportRoute::HoldingSupportAccessApproval,
        template: "/support/:id/access-approval",
        audience: SupportRouteAudience::Holding,
        authenticated: true,
    },
    SupportRouteDefinition {
        route: SupportRoute::HoldingPlatformAccess,
        template: "/support/platform-access",
        audience: SupportRouteAudience::Holding,
        authenticated: true,
    },
];

/// The only scope exposed by the platform incident presentation surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncidentScope {
    /// Platform-wide incident coordination scope.
    Platform,
}

impl IncidentScope {
    /// Return the localized scope label used by the navigation shell.
    pub const fn label(self, locale: Locale) -> &'static str {
        match self {
            Self::Platform => SupportRoute::PlatformIncidents.label(locale),
        }
    }

    /// Whether this is the platform-wide scope.
    pub const fn is_platform(self) -> bool {
        matches!(self, Self::Platform)
    }
}

/// Projection metadata for a platform incident read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentProjectionMetadata {
    /// Explicit audience/scope for the incident read.
    pub scope: IncidentScope,
    /// Server-provided display timestamp.
    pub projection_updated_at: Option<String>,
    /// Freshness classification supplied by the authorized read boundary.
    pub freshness: ProjectionFreshness,
    /// Incident presentation is non-mutating unless a consuming app separately authorizes a
    /// command; this shared slice never enables commands.
    pub read_only: bool,
}

impl IncidentProjectionMetadata {
    /// Build fresh platform-scope metadata.
    pub fn fresh(projection_updated_at: impl Into<String>) -> Self {
        Self::present(projection_updated_at, ProjectionFreshness::Fresh)
    }

    /// Build stale but displayable platform-scope metadata.
    pub fn stale(projection_updated_at: impl Into<String>) -> Self {
        Self::present(projection_updated_at, ProjectionFreshness::Stale)
    }

    /// Build metadata when no projection timestamp is available.
    pub const fn unknown() -> Self {
        Self {
            scope: IncidentScope::Platform,
            projection_updated_at: None,
            freshness: ProjectionFreshness::Unknown,
            read_only: true,
        }
    }

    /// Build platform-scope metadata with an explicit freshness classification.
    pub fn present(
        projection_updated_at: impl Into<String>,
        freshness: ProjectionFreshness,
    ) -> Self {
        let projection_updated_at = safe_optional_value(&projection_updated_at.into());
        Self {
            scope: IncidentScope::Platform,
            freshness: if projection_updated_at.is_some() {
                freshness
            } else {
                ProjectionFreshness::Unknown
            },
            projection_updated_at,
            read_only: true,
        }
    }

    /// Set the explicit read-only flag without changing scope or freshness.
    pub const fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Whether this projection is explicitly read-only.
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Whether the projection is outside its freshness budget.
    pub const fn is_stale(&self) -> bool {
        matches!(self.freshness, ProjectionFreshness::Stale)
    }

    /// Whether the projection is within its freshness budget.
    pub const fn is_fresh(&self) -> bool {
        matches!(self.freshness, ProjectionFreshness::Fresh)
    }
}

/// A safe, transport-neutral copy of the incident fields exposed by the platform DTO.
///
/// This adapter intentionally contains no timeline, broadcast, mitigation, root-cause,
/// corrective-action, evidence, credential, or secret fields.
#[derive(Clone)]
pub struct IncidentDtoInput {
    id: String,
    status: String,
    commander_ref: String,
    leads: Vec<String>,
    affected_services: Vec<String>,
    affected_customer_accounts: Vec<String>,
    version: u64,
}

impl fmt::Debug for IncidentDtoInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IncidentDtoInput")
            .field("id", &"REDACTED")
            .field("status", &self.status)
            .field("commander_ref", &"REDACTED")
            .field("leads", &format_args!("{} values", self.leads.len()))
            .field(
                "affected_services",
                &format_args!("{} values", self.affected_services.len()),
            )
            .field(
                "affected_customer_accounts",
                &format_args!("{} values", self.affected_customer_accounts.len()),
            )
            .field("version", &self.version)
            .finish()
    }
}

impl IncidentDtoInput {
    /// Copy fields from a generated-proto-like DTO without importing a transport crate.
    pub fn new(
        id: impl AsRef<str>,
        status: impl AsRef<str>,
        commander_ref: impl AsRef<str>,
        leads: impl IntoIterator<Item = impl AsRef<str>>,
        affected_services: impl IntoIterator<Item = impl AsRef<str>>,
        affected_customer_accounts: impl IntoIterator<Item = impl AsRef<str>>,
        version: u64,
    ) -> Self {
        Self {
            id: id.as_ref().to_string(),
            status: status.as_ref().to_string(),
            commander_ref: commander_ref.as_ref().to_string(),
            leads: leads
                .into_iter()
                .map(|value| value.as_ref().to_string())
                .collect(),
            affected_services: affected_services
                .into_iter()
                .map(|value| value.as_ref().to_string())
                .collect(),
            affected_customer_accounts: affected_customer_accounts
                .into_iter()
                .map(|value| value.as_ref().to_string())
                .collect(),
            version,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SafeIncidentFields {
    id: String,
    status: IncidentStatus,
    commander_ref: Option<String>,
    leads: Vec<String>,
    affected_services: Vec<String>,
    affected_customer_accounts: Vec<String>,
    version: u64,
}

impl SafeIncidentFields {
    fn from_input(input: IncidentDtoInput) -> Option<Self> {
        let id = safe_required_value(&input.id)?;
        let status = IncidentStatus::from_transport(&input.status)?;
        let commander_ref = safe_optional_value(&input.commander_ref);
        Some(Self {
            id,
            status,
            commander_ref,
            leads: safe_collection(input.leads),
            affected_services: safe_collection(input.affected_services),
            affected_customer_accounts: safe_collection(input.affected_customer_accounts),
            version: input.version,
        })
    }
}

fn safe_required_value(value: &str) -> Option<String> {
    safe_optional_value(value)
}

fn safe_collection(values: impl IntoIterator<Item = String>) -> Vec<String> {
    values
        .into_iter()
        .take(MAX_SAFE_SUPPORT_COLLECTION_ITEMS)
        .filter_map(|value| safe_optional_value(&value))
        .collect()
}

/// One safe item shown in the platform incident list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentListItem {
    id: String,
    status: IncidentStatus,
    commander_ref: Option<String>,
    leads: Vec<String>,
    affected_services: Vec<String>,
    affected_customer_accounts: Vec<String>,
    version: u64,
}

impl IncidentListItem {
    fn from_safe_fields(fields: SafeIncidentFields) -> Self {
        Self {
            id: fields.id,
            status: fields.status,
            commander_ref: fields.commander_ref,
            leads: fields.leads,
            affected_services: fields.affected_services,
            affected_customer_accounts: fields.affected_customer_accounts,
            version: fields.version,
        }
    }

    /// Opaque incident identifier safe for route construction by the consuming app.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Typed lifecycle status.
    pub const fn status(&self) -> IncidentStatus {
        self.status
    }

    /// Localized lifecycle label.
    pub const fn status_label(&self, locale: Locale) -> &'static str {
        self.status.label(locale)
    }

    /// Display-safe commander reference, if one was supplied.
    pub fn commander_ref(&self) -> Option<&str> {
        self.commander_ref.as_deref()
    }

    /// Display-safe incident leads.
    pub fn leads(&self) -> &[String] {
        &self.leads
    }

    /// Display-safe affected services.
    pub fn affected_services(&self) -> &[String] {
        &self.affected_services
    }

    /// Display-safe affected customer-account references.
    pub fn affected_customer_accounts(&self) -> &[String] {
        &self.affected_customer_accounts
    }

    /// Aggregate version used by an authorized command boundary.
    pub const fn version(&self) -> u64 {
        self.version
    }
}

/// Safe display model for `/platform/incidents`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentListDisplayModel {
    /// Existing route-registry entry consumed by the navigation shell.
    pub route: SupportRoute,
    /// Explicit platform scope and projection freshness metadata.
    pub metadata: IncidentProjectionMetadata,
    items: Vec<IncidentListItem>,
    dropped_record_count: usize,
}

impl IncidentListDisplayModel {
    /// Convert safe incident DTO fields into a platform-scoped list model.
    pub fn from_safe_dtos(
        metadata: IncidentProjectionMetadata,
        inputs: impl IntoIterator<Item = IncidentDtoInput>,
    ) -> Self {
        let mut items = Vec::new();
        let mut dropped_record_count = 0;
        for input in inputs {
            match SafeIncidentFields::from_input(input) {
                Some(fields) => items.push(IncidentListItem::from_safe_fields(fields)),
                None => dropped_record_count += 1,
            }
        }
        Self {
            route: SupportRoute::PlatformIncidents,
            metadata,
            items,
            dropped_record_count,
        }
    }

    /// Return safe list items only.
    pub fn items(&self) -> &[IncidentListItem] {
        &self.items
    }

    /// Number of records rejected because their required DTO fields were invalid.
    pub const fn dropped_record_count(&self) -> usize {
        self.dropped_record_count
    }

    /// Localized route label.
    pub const fn route_label(&self, locale: Locale) -> &'static str {
        self.route.label(locale)
    }
}

/// Safe display model for `/platform/incidents/:id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentDisplayModel {
    /// Existing route-registry entry consumed by the navigation shell.
    pub route: SupportRoute,
    /// Explicit platform scope and projection freshness metadata.
    pub metadata: IncidentProjectionMetadata,
    id: String,
    status: IncidentStatus,
    commander_ref: Option<String>,
    leads: Vec<String>,
    affected_services: Vec<String>,
    affected_customer_accounts: Vec<String>,
    version: u64,
}

impl IncidentDisplayModel {
    /// Convert safe incident DTO fields into a detail model.
    ///
    /// A detail model can only be bound to the existing platform incident-detail route.
    pub fn from_safe_dto(
        metadata: IncidentProjectionMetadata,
        input: IncidentDtoInput,
    ) -> Option<Self> {
        let fields = SafeIncidentFields::from_input(input)?;
        Some(Self {
            route: SupportRoute::PlatformIncidentDetail,
            metadata,
            id: fields.id,
            status: fields.status,
            commander_ref: fields.commander_ref,
            leads: fields.leads,
            affected_services: fields.affected_services,
            affected_customer_accounts: fields.affected_customer_accounts,
            version: fields.version,
        })
    }

    /// Opaque incident identifier safe for route construction by the consuming app.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Typed lifecycle status.
    pub const fn status(&self) -> IncidentStatus {
        self.status
    }

    /// Localized lifecycle label.
    pub const fn status_label(&self, locale: Locale) -> &'static str {
        self.status.label(locale)
    }

    /// Display-safe commander reference, if one was supplied.
    pub fn commander_ref(&self) -> Option<&str> {
        self.commander_ref.as_deref()
    }

    /// Display-safe incident leads.
    pub fn leads(&self) -> &[String] {
        &self.leads
    }

    /// Display-safe affected services.
    pub fn affected_services(&self) -> &[String] {
        &self.affected_services
    }

    /// Display-safe affected customer-account references.
    pub fn affected_customer_accounts(&self) -> &[String] {
        &self.affected_customer_accounts
    }

    /// Aggregate version used by an authorized command boundary.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Localized route label.
    pub const fn route_label(&self, locale: Locale) -> &'static str {
        self.route.label(locale)
    }
}

/// Explicit state machine for the incident list and detail presentation surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncidentViewState<T> {
    /// A read is in progress.
    Loading {
        /// Previously visible safe metadata, when available.
        metadata: Option<IncidentProjectionMetadata>,
    },
    /// The read completed with no records/content.
    Empty {
        /// Metadata accompanying the empty result.
        metadata: IncidentProjectionMetadata,
    },
    /// Current display data is available.
    Ready {
        /// Safe display data.
        data: T,
        /// Projection metadata.
        metadata: IncidentProjectionMetadata,
    },
    /// The principal cannot view the incident surface.
    Denied {
        /// Metadata may be absent when denial occurs before the read.
        metadata: Option<IncidentProjectionMetadata>,
        /// Safe policy/message text only.
        reason: String,
    },
    /// Cached data is outside the freshness budget.
    Stale {
        /// Last safe display data.
        data: T,
        /// Stale projection metadata.
        metadata: IncidentProjectionMetadata,
        /// Age supplied by the read boundary.
        age_seconds: u64,
        /// Safe freshness explanation.
        reason: String,
    },
    /// Data is available but mutations are disabled.
    ReadOnly {
        /// Last safe display data.
        data: T,
        /// Projection metadata.
        metadata: IncidentProjectionMetadata,
        /// Safe read-only explanation.
        reason: String,
    },
    /// A write lost an optimistic-concurrency race.
    Conflict {
        /// Last safe display data, when available.
        data: Option<T>,
        /// Projection metadata, when available.
        metadata: Option<IncidentProjectionMetadata>,
        /// Safe conflict explanation.
        reason: String,
    },
    /// The backend is unavailable; cached display data may remain visible.
    Offline {
        /// Cached safe display data, when available.
        data: Option<T>,
        /// Cached projection metadata, when available.
        metadata: Option<IncidentProjectionMetadata>,
        /// Safe connectivity explanation.
        reason: String,
    },
    /// An unexpected request failure occurred.
    Error {
        /// Safe error explanation.
        reason: String,
    },
}

impl<T> IncidentViewState<T> {
    /// Return the shared explicit UI state kind.
    pub const fn kind(&self) -> Option<UiStateKind> {
        match self {
            Self::Loading { .. } => Some(UiStateKind::Loading),
            Self::Empty { .. } => Some(UiStateKind::Empty),
            Self::Ready { .. } => None,
            Self::Denied { .. } => Some(UiStateKind::Denied),
            Self::Stale { .. } => Some(UiStateKind::Stale),
            Self::ReadOnly { .. } => Some(UiStateKind::ReadOnly),
            Self::Conflict { .. } => Some(UiStateKind::Conflict),
            Self::Offline { .. } => Some(UiStateKind::Offline),
            Self::Error { .. } => Some(UiStateKind::Error),
        }
    }

    /// Return the localized label for a non-ready state.
    pub const fn state_label(&self, locale: Locale) -> Option<&'static str> {
        let key = match self.kind() {
            Some(UiStateKind::Loading) => TranslationKey::Loading,
            Some(UiStateKind::Empty) => TranslationKey::Empty,
            Some(UiStateKind::Denied) => TranslationKey::AccessDenied,
            Some(UiStateKind::Stale) => TranslationKey::Stale,
            Some(UiStateKind::ReadOnly) => TranslationKey::ReadOnly,
            Some(UiStateKind::Conflict) => TranslationKey::Conflict,
            Some(UiStateKind::Offline) => TranslationKey::Offline,
            Some(UiStateKind::Error) => TranslationKey::Error,
            Some(_) => TranslationKey::Error,
            None => return None,
        };
        Some(key.text(locale))
    }

    /// Return safe cached/current display data, if this state has one.
    pub fn data(&self) -> Option<&T> {
        match self {
            Self::Ready { data, .. } | Self::Stale { data, .. } | Self::ReadOnly { data, .. } => {
                Some(data)
            }
            Self::Conflict { data, .. } | Self::Offline { data, .. } => data.as_ref(),
            Self::Loading { .. }
            | Self::Empty { .. }
            | Self::Denied { .. }
            | Self::Error { .. } => None,
        }
    }

    /// Return projection metadata, if available.
    pub fn metadata(&self) -> Option<&IncidentProjectionMetadata> {
        match self {
            Self::Loading { metadata }
            | Self::Denied { metadata, .. }
            | Self::Conflict { metadata, .. }
            | Self::Offline { metadata, .. } => metadata.as_ref(),
            Self::Empty { metadata }
            | Self::Ready { metadata, .. }
            | Self::Stale { metadata, .. }
            | Self::ReadOnly { metadata, .. } => Some(metadata),
            Self::Error { .. } => None,
        }
    }

    /// Whether this state or its metadata disables writes.
    pub fn is_read_only(&self) -> bool {
        matches!(self.kind(), Some(UiStateKind::ReadOnly))
            || self
                .metadata()
                .is_some_and(IncidentProjectionMetadata::is_read_only)
    }
}

/// State for the platform incident list route.
pub type IncidentListState = IncidentViewState<IncidentListDisplayModel>;

/// State for the platform incident detail route.
pub type IncidentDetailState = IncidentViewState<IncidentDisplayModel>;

impl SupportRoute {
    /// Return every support and incident route in stable navigation order.
    pub const fn all() -> &'static [SupportRouteDefinition] {
        SUPPORT_ROUTES
    }

    /// Return the canonical route template.
    pub const fn template(self) -> &'static str {
        match self {
            Self::PlatformSupport => "/platform/support",
            Self::PlatformSupportNew => "/platform/support/new",
            Self::PlatformSupportDetail => "/platform/support/:id",
            Self::PlatformSupportAccess => "/platform/support/:id/access",
            Self::PlatformSupportDiagnostics => "/platform/support/:id/diagnostics",
            Self::PlatformSupportSessions => "/platform/support/sessions",
            Self::PlatformIncidents => "/platform/incidents",
            Self::PlatformIncidentDetail => "/platform/incidents/:id",
            Self::HoldingSupport => "/support",
            Self::HoldingSupportNew => "/support/new",
            Self::HoldingSupportDetail => "/support/:id",
            Self::HoldingSupportAccessApproval => "/support/:id/access-approval",
            Self::HoldingPlatformAccess => "/support/platform-access",
        }
    }

    /// Return the audience for this route.
    pub const fn audience(self) -> SupportRouteAudience {
        match self {
            Self::PlatformSupport
            | Self::PlatformSupportNew
            | Self::PlatformSupportDetail
            | Self::PlatformSupportAccess
            | Self::PlatformSupportDiagnostics
            | Self::PlatformSupportSessions
            | Self::PlatformIncidents
            | Self::PlatformIncidentDetail => SupportRouteAudience::Platform,
            Self::HoldingSupport
            | Self::HoldingSupportNew
            | Self::HoldingSupportDetail
            | Self::HoldingSupportAccessApproval
            | Self::HoldingPlatformAccess => SupportRouteAudience::Holding,
        }
    }

    /// Return the localized route label.
    pub const fn label(self, locale: Locale) -> &'static str {
        self.translation_key().text(locale)
    }

    /// Build a route-agnostic navigation item.
    pub fn nav_item(self, locale: Locale) -> NavItem {
        NavItem::new(self.label(locale), self.template(), "◈")
    }

    const fn translation_key(self) -> TranslationKey {
        match self {
            Self::PlatformSupport | Self::HoldingSupport => TranslationKey::Support,
            Self::PlatformSupportNew | Self::HoldingSupportNew => TranslationKey::SupportNew,
            Self::PlatformSupportDetail | Self::HoldingSupportDetail => TranslationKey::Support,
            Self::PlatformSupportAccess => TranslationKey::SupportAccess,
            Self::PlatformSupportDiagnostics => TranslationKey::SupportDiagnostics,
            Self::PlatformSupportSessions => TranslationKey::SupportSessions,
            Self::PlatformIncidents | Self::PlatformIncidentDetail => TranslationKey::Incidents,
            Self::HoldingSupportAccessApproval => TranslationKey::SupportApproval,
            Self::HoldingPlatformAccess => TranslationKey::SupportPlatformAccess,
        }
    }
}

/// One untrusted field handed to the support display boundary.
pub struct SupportFieldInput<'a> {
    key: &'a str,
    value: &'a str,
}

impl<'a> SupportFieldInput<'a> {
    /// Create a field candidate from an authorized support projection DTO.
    pub const fn new(key: &'a str, value: &'a str) -> Self {
        Self { key, value }
    }
}

impl fmt::Debug for SupportFieldInput<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SupportFieldInput")
            .field("key", &self.key)
            .field("value", &"REDACTED")
            .finish()
    }
}

/// A safe support field for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportDisplayField {
    key: String,
    value: String,
}

impl SupportDisplayField {
    /// Copy a field only when its key is allow-listed and its value is not secret material.
    pub fn from_untrusted(input: SupportFieldInput<'_>) -> Option<Self> {
        let key = normalize_key(input.key);
        if !is_allowed_display_key(&key) {
            return None;
        }
        let value = safe_optional_value(input.value)?;
        Some(Self { key, value })
    }

    /// Stable display key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Safe display value.
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Safe, route-oriented support display model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportDisplayModel {
    /// Route that consumes this model.
    pub route: SupportRoute,
    /// Projection freshness and read-only metadata.
    pub metadata: SupportProjectionMetadata,
    /// Customer-approval, mode, and impersonation context.
    pub context: SupportContextMetadata,
    fields: Vec<SupportDisplayField>,
    redacted_field_count: usize,
}

impl SupportDisplayModel {
    /// Create an empty display model for a support route.
    pub fn new(
        route: SupportRoute,
        metadata: SupportProjectionMetadata,
        context: SupportContextMetadata,
    ) -> Self {
        Self {
            route,
            metadata,
            context,
            fields: Vec::new(),
            redacted_field_count: 0,
        }
    }

    /// Create a display model from untrusted projection field candidates.
    pub fn from_untrusted_fields<'a>(
        route: SupportRoute,
        metadata: SupportProjectionMetadata,
        context: SupportContextMetadata,
        inputs: impl IntoIterator<Item = SupportFieldInput<'a>>,
    ) -> Self {
        let mut model = Self::new(route, metadata, context);
        for input in inputs {
            model.push_untrusted(input);
        }
        model
    }

    /// Try to append one field; returns `true` only when it is retained.
    pub fn push_untrusted(&mut self, input: SupportFieldInput<'_>) -> bool {
        match SupportDisplayField::from_untrusted(input) {
            Some(field) => {
                self.fields.push(field);
                true
            }
            None => {
                self.redacted_field_count += 1;
                false
            }
        }
    }

    /// Return safe display fields only.
    pub fn fields(&self) -> &[SupportDisplayField] {
        &self.fields
    }

    /// Number of input fields omitted at the redaction boundary.
    pub const fn redacted_field_count(&self) -> usize {
        self.redacted_field_count
    }

    /// Localized route label.
    pub const fn route_label(&self, locale: Locale) -> &'static str {
        self.route.label(locale)
    }
}

/// Freshness and mutation metadata for a support projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportProjectionMetadata {
    /// Server-provided display timestamp.
    pub projection_updated_at: Option<String>,
    /// Freshness classification supplied by the read boundary.
    pub freshness: ProjectionFreshness,
    /// Whether the source context is explicitly non-mutating.
    pub read_only: bool,
    scope: Option<SupportScope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SupportScope {
    holding_id: String,
}

impl SupportProjectionMetadata {
    /// Build fresh projection metadata.
    pub fn fresh(projection_updated_at: impl Into<String>) -> Self {
        Self::present(projection_updated_at, ProjectionFreshness::Fresh)
    }

    /// Build stale but displayable projection metadata.
    pub fn stale(projection_updated_at: impl Into<String>) -> Self {
        Self::present(projection_updated_at, ProjectionFreshness::Stale)
    }

    /// Build metadata with no known projection timestamp.
    pub fn unknown() -> Self {
        Self {
            projection_updated_at: None,
            freshness: ProjectionFreshness::Unknown,
            read_only: true,
            scope: None,
        }
    }

    /// Build metadata with an explicit freshness classification.
    pub fn present(
        projection_updated_at: impl Into<String>,
        freshness: ProjectionFreshness,
    ) -> Self {
        let projection_updated_at = safe_optional_value(&projection_updated_at.into());
        Self {
            freshness: if projection_updated_at.is_some() {
                freshness
            } else {
                ProjectionFreshness::Unknown
            },
            projection_updated_at,
            read_only: true,
            scope: None,
        }
    }

    /// Build metadata explicitly bound to one holding and holding.
    pub fn fresh_for_scope(
        holding_id: impl AsRef<str>,
        projection_updated_at: impl Into<String>,
    ) -> Self {
        let mut metadata = Self::fresh(projection_updated_at);
        metadata.scope = safe_support_scope(holding_id.as_ref());
        if metadata.scope.is_none() {
            metadata.freshness = ProjectionFreshness::Unknown;
        }
        metadata
    }

    /// Whether this metadata has an explicit holding/holding binding.
    pub const fn has_scope(&self) -> bool {
        self.scope.is_some()
    }

    /// Whether this metadata matches the requested holding and holding.
    pub fn is_bound_to_scope(&self, holding_id: &str) -> bool {
        self.scope
            .as_ref()
            .is_some_and(|scope| scope.holding_id == holding_id.trim())
    }

    /// Whether two projection states carry the same explicit holding/holding scope.
    pub fn has_same_scope_as(&self, other: &Self) -> bool {
        self.scope.is_some() && self.scope == other.scope
    }

    /// Set the explicit read-only flag.
    pub const fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Whether the projection is read-only.
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Whether the projection is outside its freshness budget.
    pub const fn is_stale(&self) -> bool {
        matches!(self.freshness, ProjectionFreshness::Stale)
    }
}

/// Safe, transport-neutral ticket fields accepted by the platform support list.
///
/// Description and reply bodies are intentionally absent: they may contain customer or
/// internal support content and require a separate disclosure policy.
#[derive(Clone)]
pub struct SupportTicketDtoInput {
    id: String,
    customer_id: String,
    status: String,
    severity: String,
}

impl fmt::Debug for SupportTicketDtoInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SupportTicketDtoInput")
            .field("id", &"REDACTED")
            .field("customer_id", &"REDACTED")
            .field("status", &self.status)
            .field("severity", &self.severity)
            .finish()
    }
}

impl SupportTicketDtoInput {
    /// Copy only the bounded ticket metadata exposed by the transport boundary.
    pub fn new(
        id: impl AsRef<str>,
        customer_id: impl AsRef<str>,
        status: impl AsRef<str>,
        severity: impl AsRef<str>,
    ) -> Self {
        Self {
            id: id.as_ref().to_string(),
            customer_id: customer_id.as_ref().to_string(),
            status: status.as_ref().to_string(),
            severity: severity.as_ref().to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SafeTicketFields {
    id: String,
    customer_id: String,
    status: SupportTicketStatus,
    severity: String,
}

impl SafeTicketFields {
    fn from_input(input: SupportTicketDtoInput) -> Option<Self> {
        let id = safe_required_value(&input.id)?;
        let customer_id = safe_required_value(&input.customer_id)?;
        let status = SupportTicketStatus::from_transport(&input.status)?;
        let severity = safe_required_value(&input.severity)?;
        if severity.len() > 64 {
            return None;
        }
        Some(Self {
            id,
            customer_id,
            status,
            severity,
        })
    }
}

/// One safe item shown by the platform support ticket list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportTicketListItem {
    id: String,
    customer_id: String,
    status: SupportTicketStatus,
    severity: String,
}

impl SupportTicketListItem {
    fn from_safe_fields(fields: SafeTicketFields) -> Self {
        Self {
            id: fields.id,
            customer_id: fields.customer_id,
            status: fields.status,
            severity: fields.severity,
        }
    }

    /// Opaque ticket identifier safe for a detail route.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Opaque customer reference used only for the authorized platform list.
    pub fn customer_id(&self) -> &str {
        &self.customer_id
    }

    /// Typed ticket lifecycle status.
    pub const fn status(&self) -> SupportTicketStatus {
        self.status
    }

    /// Bounded severity label.
    pub fn severity(&self) -> &str {
        &self.severity
    }
}

/// Safe display model for the platform support ticket list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportTicketListDisplayModel {
    /// Existing platform support route consumed by the navigation shell.
    pub route: SupportRoute,
    /// Explicit projection freshness and read-only metadata.
    pub metadata: SupportProjectionMetadata,
    items: Vec<SupportTicketListItem>,
    dropped_record_count: usize,
}

impl SupportTicketListDisplayModel {
    const MAX_PAGE_SIZE: usize = 512;

    /// Convert safe ticket DTO fields into a read-only list model.
    pub fn from_safe_dtos(
        metadata: SupportProjectionMetadata,
        inputs: impl IntoIterator<Item = SupportTicketDtoInput>,
    ) -> Self {
        let mut items = Vec::new();
        let mut dropped_record_count = 0;
        for (index, input) in inputs.into_iter().enumerate() {
            if index >= Self::MAX_PAGE_SIZE {
                dropped_record_count += 1;
                continue;
            }
            match SafeTicketFields::from_input(input) {
                Some(fields) => items.push(SupportTicketListItem::from_safe_fields(fields)),
                None => dropped_record_count += 1,
            }
        }
        Self {
            route: SupportRoute::PlatformSupport,
            metadata,
            items,
            dropped_record_count,
        }
    }

    /// Return safe ticket items only.
    pub fn items(&self) -> &[SupportTicketListItem] {
        &self.items
    }

    /// Number of transport rows rejected at the display boundary.
    pub const fn dropped_record_count(&self) -> usize {
        self.dropped_record_count
    }
}

/// Explicit support/incident state machine for list, detail, diagnostics, and access pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupportViewState<T> {
    /// A read is in progress.
    Loading {
        /// Previously visible safe metadata, when available.
        metadata: Option<SupportProjectionMetadata>,
    },
    /// The read completed with no records/content.
    Empty {
        /// Metadata accompanying the empty result.
        metadata: SupportProjectionMetadata,
    },
    /// Current display data is available.
    Ready {
        /// Safe display data.
        data: T,
        /// Projection metadata.
        metadata: SupportProjectionMetadata,
    },
    /// The principal cannot view the support surface.
    Denied {
        /// Metadata may be absent when denial occurs before the read.
        metadata: Option<SupportProjectionMetadata>,
        /// Safe policy/message text only.
        reason: String,
    },
    /// Cached data is outside the freshness budget.
    Stale {
        /// Last safe display data.
        data: T,
        /// Stale projection metadata.
        metadata: SupportProjectionMetadata,
        /// Age supplied by the read boundary.
        age_seconds: u64,
        /// Safe freshness explanation.
        reason: String,
    },
    /// Data is available but mutations are disabled.
    ReadOnly {
        /// Last safe display data.
        data: T,
        /// Projection metadata.
        metadata: SupportProjectionMetadata,
        /// Safe read-only explanation.
        reason: String,
    },
    /// A write lost an optimistic-concurrency race.
    Conflict {
        /// Last safe display data, when available.
        data: Option<T>,
        /// Projection metadata, when available.
        metadata: Option<SupportProjectionMetadata>,
        /// Safe conflict explanation.
        reason: String,
    },
    /// The backend is unavailable; cached display data may remain visible.
    Offline {
        /// Cached safe display data, when available.
        data: Option<T>,
        /// Cached projection metadata, when available.
        metadata: Option<SupportProjectionMetadata>,
        /// Safe connectivity explanation.
        reason: String,
    },
    /// An unexpected request failure occurred.
    Error {
        /// Safe error explanation.
        reason: String,
    },
}

impl<T> SupportViewState<T> {
    /// Return the shared explicit state kind.
    pub const fn kind(&self) -> Option<UiStateKind> {
        match self {
            Self::Loading { .. } => Some(UiStateKind::Loading),
            Self::Empty { .. } => Some(UiStateKind::Empty),
            Self::Ready { .. } => None,
            Self::Denied { .. } => Some(UiStateKind::Denied),
            Self::Stale { .. } => Some(UiStateKind::Stale),
            Self::ReadOnly { .. } => Some(UiStateKind::ReadOnly),
            Self::Conflict { .. } => Some(UiStateKind::Conflict),
            Self::Offline { .. } => Some(UiStateKind::Offline),
            Self::Error { .. } => Some(UiStateKind::Error),
        }
    }

    /// Return the localized label for a non-ready state.
    pub const fn state_label(&self, locale: Locale) -> Option<&'static str> {
        let key = match self.kind() {
            Some(UiStateKind::Loading) => TranslationKey::Loading,
            Some(UiStateKind::Empty) => TranslationKey::Empty,
            Some(UiStateKind::Denied) => TranslationKey::AccessDenied,
            Some(UiStateKind::Stale) => TranslationKey::Stale,
            Some(UiStateKind::ReadOnly) => TranslationKey::ReadOnly,
            Some(UiStateKind::Conflict) => TranslationKey::Conflict,
            Some(UiStateKind::Offline) => TranslationKey::Offline,
            Some(UiStateKind::Error) => TranslationKey::Error,
            Some(_) => TranslationKey::Error,
            None => return None,
        };
        Some(key.text(locale))
    }

    /// Return safe cached/current display data, if this state has one.
    pub fn data(&self) -> Option<&T> {
        match self {
            Self::Ready { data, .. } | Self::Stale { data, .. } | Self::ReadOnly { data, .. } => {
                Some(data)
            }
            Self::Conflict { data, .. } | Self::Offline { data, .. } => data.as_ref(),
            Self::Loading { .. }
            | Self::Empty { .. }
            | Self::Denied { .. }
            | Self::Error { .. } => None,
        }
    }

    /// Return projection metadata, if available.
    pub fn metadata(&self) -> Option<&SupportProjectionMetadata> {
        match self {
            Self::Loading { metadata }
            | Self::Denied { metadata, .. }
            | Self::Conflict { metadata, .. }
            | Self::Offline { metadata, .. } => metadata.as_ref(),
            Self::Empty { metadata }
            | Self::Ready { metadata, .. }
            | Self::Stale { metadata, .. }
            | Self::ReadOnly { metadata, .. } => Some(metadata),
            Self::Error { .. } => None,
        }
    }

    /// Whether this state or its metadata disables writes.
    pub fn is_read_only(&self) -> bool {
        matches!(self.kind(), Some(UiStateKind::ReadOnly))
            || self
                .metadata()
                .is_some_and(SupportProjectionMetadata::is_read_only)
    }
}

/// Concrete support state for safe display models.
pub type SupportState = SupportViewState<SupportDisplayModel>;

/// Concrete support ticket-list state for safe display models.
pub type SupportTicketListState = SupportViewState<SupportTicketListDisplayModel>;

fn normalize_key(key: &str) -> String {
    key.trim().to_ascii_lowercase().replace('-', "_")
}

fn safe_support_scope(holding_id: &str) -> Option<SupportScope> {
    let holding_id = safe_scope_id(holding_id)?;
    Some(SupportScope { holding_id })
}

fn safe_scope_id(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || value.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '/' | '\\' | '?' | '#')
        })
        || contains_secret_marker(value)
    {
        None
    } else {
        Some(value.to_owned())
    }
}

fn is_allowed_display_key(key: &str) -> bool {
    matches!(
        key,
        "ticket_id"
            | "incident_id"
            | "subject"
            | "status"
            | "severity"
            | "category"
            | "affected_user_count"
            | "environment"
            | "scope_label"
            | "requester_label"
            | "assignee_label"
            | "commander_label"
            | "created_at"
            | "updated_at"
            | "sla_due_at"
            | "resolution_summary"
            | "affected_service"
            | "timeline_entry"
            | "broadcast_status"
            | "mitigation_summary"
            | "corrective_action"
            | "diagnostic_version"
            | "projection_lag_seconds"
            | "worker_health"
            | "backup_health"
            | "integration_error_count"
            | "diagnostic_schema_version"
            | "worker_health_age_seconds"
            | "backup_verified"
            | "failed_job_count"
            | "redacted_configuration_count"
            | "truncated_item_count"
            | "correlation_id"
    )
}

const MAX_SAFE_SUPPORT_VALUE_BYTES: usize = 512;
const MAX_SAFE_SUPPORT_COLLECTION_ITEMS: usize = 128;

fn safe_optional_value(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_SAFE_SUPPORT_VALUE_BYTES
        || value.chars().any(char::is_control)
        || contains_secret_marker(value)
    {
        None
    } else {
        Some(value.to_owned())
    }
}

fn redact_value(value: &str) -> String {
    safe_optional_value(value).unwrap_or_else(|| "[redacted]".to_string())
}

fn contains_secret_marker(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "access_token=",
        "refresh_token=",
        "client_secret=",
        "password=",
        "authorization: bearer ",
        "bearer ",
        "-----begin ",
        "_secret",
        "api_key=",
        "private_key=",
        "signed_url=",
        "presigned_url=",
        "sk_live_",
        "rk_live_",
        "capability_secret",
        "holding_credential",
    ]
    .iter()
    .any(|marker| value.contains(marker))
        || value.trim() == "token"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(mode: SupportMode) -> SupportContextMetadata {
        SupportContextMetadata::new(
            mode,
            SupportApprovalState::Approved,
            SupportImpersonationState::NotActive,
            false,
            Some("2026-07-24T12:00:00Z"),
        )
    }

    fn metadata() -> SupportProjectionMetadata {
        SupportProjectionMetadata::fresh("2026-07-24T00:00:00Z")
    }

    #[test]
    fn required_routes_are_complete_and_bilingual() {
        let expected = [
            "/platform/support",
            "/platform/support/new",
            "/platform/support/:id",
            "/platform/support/:id/access",
            "/platform/support/:id/diagnostics",
            "/platform/support/sessions",
            "/platform/incidents",
            "/platform/incidents/:id",
            "/support",
            "/support/new",
            "/support/:id",
            "/support/:id/access-approval",
            "/support/platform-access",
        ];

        assert_eq!(SUPPORT_ROUTES.len(), expected.len());
        for (definition, expected_template) in SUPPORT_ROUTES.iter().zip(expected) {
            assert_eq!(definition.template, expected_template);
            assert!(definition.authenticated);
            assert!(!definition.route.label(Locale::En).is_empty());
            assert!(!definition.route.label(Locale::Th).is_empty());
            assert_ne!(
                definition.route.label(Locale::En),
                definition.route.label(Locale::Th)
            );
            assert_eq!(
                definition.route.nav_item(Locale::En).path,
                expected_template
            );
        }
        assert_eq!(
            SupportRoute::PlatformSupport.audience(),
            SupportRouteAudience::Platform
        );
        assert_eq!(
            SupportRoute::HoldingSupport.audience(),
            SupportRouteAudience::Holding
        );
    }

    #[test]
    fn support_mode_limits_match_the_approval_policy() {
        assert_eq!(
            SupportMode::DiagnosticRead.max_duration(),
            Duration::from_secs(8 * 3600)
        );
        assert_eq!(
            SupportMode::HoldingMutation.max_duration(),
            Duration::from_secs(2 * 3600)
        );
        assert_eq!(
            SupportMode::Impersonation.max_duration(),
            Duration::from_secs(2 * 3600)
        );

        for mode in [
            SupportMode::DiagnosticRead,
            SupportMode::HoldingMutation,
            SupportMode::Impersonation,
        ] {
            assert!(mode.validate_duration(mode.max_duration()).is_ok());
            assert!(mode.validate_duration(Duration::ZERO).is_err());
            assert!(
                mode.validate_duration(mode.max_duration() + Duration::from_secs(1))
                    .is_err()
            );
        }
    }

    #[test]
    fn lifecycle_and_mode_labels_are_localized() {
        assert_eq!(
            SupportTicketStatus::WaitingForCustomer.label(Locale::En),
            "Waiting for customer"
        );
        assert_eq!(
            IncidentStatus::PostmortemComplete.label(Locale::Th),
            "เสร็จสิ้นการทบทวนเหตุการณ์"
        );
        assert_ne!(
            SupportMode::Impersonation.label(Locale::En),
            SupportMode::Impersonation.label(Locale::Th)
        );
        assert_eq!(
            SupportApprovalState::Pending.label(Locale::En),
            "Pending customer approval"
        );
    }

    #[test]
    fn context_distinguishes_approval_impersonation_and_read_only() {
        let pending = SupportContextMetadata::new(
            SupportMode::HoldingMutation,
            SupportApprovalState::Pending,
            SupportImpersonationState::PendingApproval,
            true,
            None::<String>,
        );
        assert!(pending.approval_pending());
        assert!(!pending.is_impersonating());
        assert!(pending.is_read_only());

        let active = SupportContextMetadata::new(
            SupportMode::Impersonation,
            SupportApprovalState::Approved,
            SupportImpersonationState::Active {
                represented_label: "Branch operator".into(),
                reason: "Reproduce a reported workflow issue".into(),
            },
            false,
            Some("2026-07-24T12:00:00Z"),
        );
        assert!(!active.approval_pending());
        assert!(active.is_impersonating());
        assert!(active.is_read_only());
        assert_eq!(
            active.impersonation.represented_label(),
            Some("Branch operator")
        );
    }

    #[test]
    fn ended_or_unapproved_contexts_force_read_only_presentation() {
        for approval in [
            SupportApprovalState::Pending,
            SupportApprovalState::Rejected,
            SupportApprovalState::Expired,
            SupportApprovalState::Revoked,
        ] {
            let context = SupportContextMetadata::new(
                SupportMode::HoldingMutation,
                approval,
                SupportImpersonationState::NotActive,
                false,
                Some("2026-07-24T12:00:00Z"),
            );
            assert!(
                context.is_read_only(),
                "approval state must be fail-closed: {approval:?}"
            );
        }
    }

    #[test]
    fn display_fields_allow_list_and_redact_secrets_and_internal_notes() {
        let safe = SupportDisplayField::from_untrusted(SupportFieldInput::new(
            "affected-service",
            "Checkout API",
        ))
        .expect("safe field should display");
        assert_eq!(safe.key(), "affected_service");
        assert_eq!(safe.value(), "Checkout API");

        for (key, value) in [
            ("internal_note", "agent-only text"),
            ("private_notes", "agent-only text"),
            ("capability_secret", "one-time-secret"),
            ("holding_credentials", "postgres://holding"),
            ("diagnostic_version", "access_token=secret"),
            ("correlation_id", "Bearer secret"),
            ("attachment_url", "https://example.test/signed_url=secret"),
        ] {
            assert!(
                SupportDisplayField::from_untrusted(SupportFieldInput::new(key, value)).is_none(),
                "unsafe field must not reach display: {key}"
            );
        }

        let model = SupportDisplayModel::from_untrusted_fields(
            SupportRoute::PlatformSupportDiagnostics,
            metadata(),
            context(SupportMode::DiagnosticRead),
            [
                SupportFieldInput::new("diagnostic_version", "1.0.0"),
                SupportFieldInput::new("internal_notes", "agent-only"),
                SupportFieldInput::new("capability_secret", "secret"),
            ],
        );
        assert_eq!(model.fields().len(), 1);
        assert_eq!(model.redacted_field_count(), 2);
        assert!(!format!("{model:?}").contains("agent-only"));
        assert!(!format!("{model:?}").contains("secret"));
    }

    #[test]
    fn display_values_reject_exact_tokens_controls_and_oversized_values() {
        assert!(
            SupportDisplayField::from_untrusted(SupportFieldInput::new("subject", "token"))
                .is_none()
        );
        assert!(
            SupportDisplayField::from_untrusted(SupportFieldInput::new("subject", "safe\nvalue"))
                .is_none()
        );

        let oversized = "x".repeat(MAX_SAFE_SUPPORT_VALUE_BYTES + 1);
        assert!(
            SupportDisplayField::from_untrusted(SupportFieldInput::new("subject", &oversized))
                .is_none()
        );
        assert!(
            IncidentProjectionMetadata::present("safe\nvalue", ProjectionFreshness::Fresh)
                .projection_updated_at
                .is_none()
        );
    }

    #[test]
    fn holding_projection_metadata_requires_matching_holding_and_holding_scope() {
        let metadata =
            SupportProjectionMetadata::fresh_for_scope("holding-1", "2026-07-24T00:00:00Z");
        assert!(metadata.has_scope());
        assert!(metadata.is_bound_to_scope("holding-1"));
        assert!(!metadata.is_bound_to_scope("holding-2"));
        assert!(
            !SupportProjectionMetadata::fresh_for_scope("holding/unsafe", "2026-07-24T00:00:00Z",)
                .has_scope()
        );
    }

    #[test]
    fn ticket_display_boundary_caps_rows_without_fabricating_more_data() {
        let model = SupportTicketListDisplayModel::from_safe_dtos(
            SupportProjectionMetadata::unknown(),
            (0..=SupportTicketListDisplayModel::MAX_PAGE_SIZE).map(|index| {
                SupportTicketDtoInput::new(
                    format!("ticket-{index}"),
                    "customer-1",
                    "in_progress",
                    "high",
                )
            }),
        );
        assert_eq!(
            model.items().len(),
            SupportTicketListDisplayModel::MAX_PAGE_SIZE
        );
        assert_eq!(model.dropped_record_count(), 1);
    }

    #[test]
    fn all_view_states_remain_explicit() {
        let model = SupportDisplayModel::new(
            SupportRoute::HoldingSupport,
            metadata(),
            context(SupportMode::DiagnosticRead),
        );
        let states = [
            SupportState::Loading {
                metadata: Some(metadata()),
            }
            .kind(),
            SupportState::Empty {
                metadata: metadata(),
            }
            .kind(),
            SupportState::Denied {
                metadata: None,
                reason: "policy".into(),
            }
            .kind(),
            SupportState::Stale {
                data: model.clone(),
                metadata: SupportProjectionMetadata::stale("2026-07-23T00:00:00Z"),
                age_seconds: 90,
                reason: "projection lag".into(),
            }
            .kind(),
            SupportState::ReadOnly {
                data: model.clone(),
                metadata: metadata().with_read_only(true),
                reason: "billing".into(),
            }
            .kind(),
            SupportState::Conflict {
                data: Some(model.clone()),
                metadata: Some(metadata()),
                reason: "version".into(),
            }
            .kind(),
            SupportState::Offline {
                data: Some(model),
                metadata: None,
                reason: "network".into(),
            }
            .kind(),
            SupportViewState::<SupportDisplayModel>::Error {
                reason: "backend".into(),
            }
            .kind(),
        ];
        assert_eq!(
            states,
            [
                Some(UiStateKind::Loading),
                Some(UiStateKind::Empty),
                Some(UiStateKind::Denied),
                Some(UiStateKind::Stale),
                Some(UiStateKind::ReadOnly),
                Some(UiStateKind::Conflict),
                Some(UiStateKind::Offline),
                Some(UiStateKind::Error),
            ]
        );
    }

    fn incident_metadata() -> IncidentProjectionMetadata {
        IncidentProjectionMetadata::fresh("2026-07-24T00:00:00Z")
    }

    fn incident_input() -> IncidentDtoInput {
        IncidentDtoInput::new(
            "incident-123",
            "investigating",
            "commander-platform",
            ["lead-primary", "lead-secondary"],
            ["checkout-api", "billing-worker"],
            ["customer-001", "customer-002"],
            7,
        )
    }

    #[test]
    fn incident_lifecycle_labels_cover_every_transport_status_in_both_locales() {
        let statuses = [
            ("investigating", IncidentStatus::Investigating),
            ("identified", IncidentStatus::Identified),
            ("monitoring", IncidentStatus::Monitoring),
            ("resolved", IncidentStatus::Resolved),
            ("postmortem_complete", IncidentStatus::PostmortemComplete),
        ];

        for (token, status) in statuses {
            assert_eq!(IncidentStatus::from_transport(token), Some(status));
            assert_eq!(status.as_str(), token);
            assert!(!status.label(Locale::En).is_empty());
            assert!(!status.label(Locale::Th).is_empty());
            assert_ne!(status.label(Locale::En), status.label(Locale::Th));
        }
        assert_eq!(IncidentStatus::from_transport("unknown"), None);
    }

    #[test]
    fn incident_routes_and_metadata_are_platform_scoped() {
        assert_eq!(
            SupportRoute::PlatformIncidents.template(),
            "/platform/incidents"
        );
        assert_eq!(
            SupportRoute::PlatformIncidentDetail.template(),
            "/platform/incidents/:id"
        );
        assert_eq!(
            SupportRoute::PlatformIncidents.audience(),
            SupportRouteAudience::Platform
        );
        assert!(IncidentScope::Platform.is_platform());
        assert_ne!(
            IncidentScope::Platform.label(Locale::En),
            IncidentScope::Platform.label(Locale::Th)
        );

        let metadata = incident_metadata();
        assert_eq!(metadata.scope, IncidentScope::Platform);
        assert!(metadata.is_fresh());
        assert!(metadata.is_read_only());
        assert_eq!(
            IncidentProjectionMetadata::unknown().freshness,
            ProjectionFreshness::Unknown
        );
    }

    #[test]
    fn incident_dto_mapping_keeps_only_safe_fields_for_list_and_detail() {
        let input = IncidentDtoInput::new(
            "incident-safe",
            "postmortem_complete",
            "commander-safe",
            ["lead-safe", "Bearer should-not-display"],
            ["checkout-api", "private_key=not-a-service"],
            ["customer-safe", "holding_credential=not-an-account"],
            12,
        );
        let detail = IncidentDisplayModel::from_safe_dto(incident_metadata(), input.clone())
            .expect("valid incident DTO should map");
        assert_eq!(detail.route, SupportRoute::PlatformIncidentDetail);
        assert_eq!(detail.id(), "incident-safe");
        assert_eq!(detail.status(), IncidentStatus::PostmortemComplete);
        assert_eq!(detail.commander_ref(), Some("commander-safe"));
        assert_eq!(detail.leads(), &["lead-safe"]);
        assert_eq!(detail.affected_services(), &["checkout-api"]);
        assert_eq!(detail.affected_customer_accounts(), &["customer-safe"]);
        assert_eq!(detail.version(), 12);
        assert!(!format!("{detail:?}").contains("timeline"));
        assert!(!format!("{detail:?}").contains("corrective_action"));

        let list = IncidentListDisplayModel::from_safe_dtos(incident_metadata(), [input]);
        assert_eq!(list.route, SupportRoute::PlatformIncidents);
        assert_eq!(list.items().len(), 1);
        assert_eq!(list.dropped_record_count(), 0);
        assert_eq!(list.items()[0].affected_services(), &["checkout-api"]);

        let invalid = IncidentDtoInput::new(
            "",
            "not-a-status",
            "commander",
            ["lead"],
            ["service"],
            ["customer"],
            1,
        );
        let list = IncidentListDisplayModel::from_safe_dtos(incident_metadata(), [invalid]);
        assert!(list.items().is_empty());
        assert_eq!(list.dropped_record_count(), 1);
        assert!(
            IncidentDisplayModel::from_safe_dto(incident_metadata(), incident_input()).is_some()
        );
    }

    #[test]
    fn incident_list_and_detail_states_keep_all_explicit_distinctions() {
        let detail = IncidentDisplayModel::from_safe_dto(incident_metadata(), incident_input())
            .expect("detail fixture should map");
        let list =
            IncidentListDisplayModel::from_safe_dtos(incident_metadata(), [incident_input()]);
        let states = [
            IncidentDetailState::Loading {
                metadata: Some(incident_metadata()),
            }
            .kind(),
            IncidentListState::Empty {
                metadata: incident_metadata(),
            }
            .kind(),
            IncidentDetailState::Denied {
                metadata: None,
                reason: "policy".into(),
            }
            .kind(),
            IncidentDetailState::Ready {
                data: detail.clone(),
                metadata: incident_metadata(),
            }
            .kind(),
            IncidentDetailState::Stale {
                data: detail.clone(),
                metadata: IncidentProjectionMetadata::stale("2026-07-23T00:00:00Z"),
                age_seconds: 90,
                reason: "projection lag".into(),
            }
            .kind(),
            IncidentDetailState::ReadOnly {
                data: detail.clone(),
                metadata: incident_metadata(),
                reason: "platform presentation".into(),
            }
            .kind(),
            IncidentListState::Conflict {
                data: Some(list.clone()),
                metadata: Some(incident_metadata()),
                reason: "version".into(),
            }
            .kind(),
            IncidentListState::Offline {
                data: Some(list),
                metadata: None,
                reason: "network".into(),
            }
            .kind(),
            IncidentDetailState::Error {
                reason: "backend".into(),
            }
            .kind(),
        ];
        assert_eq!(
            states,
            [
                Some(UiStateKind::Loading),
                Some(UiStateKind::Empty),
                Some(UiStateKind::Denied),
                None,
                Some(UiStateKind::Stale),
                Some(UiStateKind::ReadOnly),
                Some(UiStateKind::Conflict),
                Some(UiStateKind::Offline),
                Some(UiStateKind::Error),
            ]
        );
        let ready = IncidentDetailState::Ready {
            data: detail,
            metadata: incident_metadata(),
        };
        assert!(ready.data().is_some());
        assert!(ready.metadata().is_some());
        assert!(ready.is_read_only());
        assert_eq!(
            IncidentDetailState::Error {
                reason: "backend".into(),
            }
            .state_label(Locale::Th),
            Some("ข้อผิดพลาด")
        );
    }

    #[test]
    fn support_ticket_list_boundary_keeps_only_typed_metadata() {
        let model = SupportTicketListDisplayModel::from_safe_dtos(
            SupportProjectionMetadata::fresh("2026-07-27T00:00:00Z"),
            [SupportTicketDtoInput::new(
                "ticket-safe",
                "customer-safe",
                "in_progress",
                "high",
            )],
        );
        assert_eq!(model.items().len(), 1);
        assert_eq!(model.items()[0].id(), "ticket-safe");
        assert_eq!(model.items()[0].customer_id(), "customer-safe");
        assert_eq!(model.items()[0].status(), SupportTicketStatus::InProgress);
        assert_eq!(model.items()[0].severity(), "high");
        assert!(!format!("{model:?}").contains("description"));

        let invalid = SupportTicketListDisplayModel::from_safe_dtos(
            SupportProjectionMetadata::unknown(),
            [SupportTicketDtoInput::new(
                "",
                "customer-safe",
                "unknown",
                "high",
            )],
        );
        assert!(invalid.items().is_empty());
        assert_eq!(invalid.dropped_record_count(), 1);
    }
}
