use std::{error::Error, fmt};

use crate::{Locale, NavItem, SafeUiText, TranslationKey, UiStateKind};

/// The authorization boundary required by a route definition. The UI never evaluates this
/// requirement; the authenticated application boundary supplies `RouteState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteAuthRequirement {
    /// The route requires an active authenticated session.
    Authenticated,
}

/// Stable identifiers for the existing platform Customer 360 route set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteId {
    /// Platform landing page.
    PlatformHome,
    /// Customer account list.
    CustomerList,
    /// Customer 360 overview.
    CustomerOverview,
    /// Customer organization projection.
    CustomerOrganization,
    /// Customer contacts projection.
    CustomerContacts,
    /// Customer commercial projection.
    CustomerCommercial,
    /// Customer entitlements projection.
    CustomerEntitlements,
    /// Customer onboarding projection.
    CustomerOnboarding,
    /// Customer health projection.
    CustomerHealth,
    /// Customer support projection.
    CustomerSupport,
    /// Customer global-read access audit.
    CustomerAccessAudit,
    /// Customer notes projection.
    CustomerNotes,
    /// Marketing landing page.
    MarketingOverview,
    /// Marketable profiles.
    MarketingProfiles,
    /// Consent and suppression.
    MarketingConsent,
    /// Audience definitions and membership.
    MarketingAudiences,
    /// Campaign lifecycle.
    MarketingCampaigns,
    /// Campaign and content calendar.
    MarketingCalendar,
    /// Durable marketing journeys.
    MarketingJourneys,
    /// Marketing content assets.
    MarketingContent,
    /// Versioned marketing templates.
    MarketingTemplates,
    /// Dioxus landing pages.
    MarketingLandingPages,
    /// Marketing forms.
    MarketingForms,
    /// Marketing leads.
    MarketingLeads,
    /// Lead scoring.
    MarketingLeadScoring,
    /// Marketing budget.
    MarketingBudget,
    /// Revenue attribution.
    MarketingAttribution,
    /// Marketing analytics.
    MarketingAnalytics,
    /// Marketing delivery channels.
    MarketingChannels,
    /// Marketing custom domains.
    MarketingDomains,
    /// CRM accounts.
    CrmAccounts,
    /// CRM contacts.
    CrmContacts,
    /// CRM leads.
    CrmLeads,
    /// CRM opportunities.
    CrmOpportunities,
    /// CRM activities.
    CrmActivities,
}

/// A route registry entry. Templates are display metadata only; parsing is performed by
/// `PlatformRoute` so customer identifiers remain typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteDefinition {
    /// Stable route identifier.
    pub id: RouteId,
    /// Canonical path template.
    pub template: &'static str,
    /// Localizable display key/label seed.
    pub label: &'static str,
    /// Server-side authentication requirement mirrored for navigation UX.
    pub auth: RouteAuthRequirement,
}

/// Canonical registry for the platform and Customer 360 routes already defined by ERPX.
pub const PLATFORM_CUSTOMER_360_ROUTES: &[RouteDefinition] = &[
    RouteDefinition {
        id: RouteId::PlatformHome,
        template: "/platform",
        label: "Platform",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CustomerList,
        template: "/platform/customers",
        label: "Customers",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CustomerOverview,
        template: "/platform/customers/:customer_id",
        label: "Customer 360",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CustomerOrganization,
        template: "/platform/customers/:customer_id/organization",
        label: "Organization",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CustomerContacts,
        template: "/platform/customers/:customer_id/contacts",
        label: "Contacts",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CustomerCommercial,
        template: "/platform/customers/:customer_id/commercial",
        label: "Commercial",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CustomerEntitlements,
        template: "/platform/customers/:customer_id/entitlements",
        label: "Entitlements",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CustomerOnboarding,
        template: "/platform/customers/:customer_id/onboarding",
        label: "Onboarding",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CustomerHealth,
        template: "/platform/customers/:customer_id/health",
        label: "Health",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CustomerSupport,
        template: "/platform/customers/:customer_id/support",
        label: "Support",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CustomerAccessAudit,
        template: "/platform/customers/:customer_id/access-audit",
        label: "Access audit",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CustomerNotes,
        template: "/platform/customers/:customer_id/notes",
        label: "Notes",
        auth: RouteAuthRequirement::Authenticated,
    },
];

/// Canonical Marketing and CRM route registry for target-neutral clients.
pub const MARKETING_CRM_ROUTES: &[RouteDefinition] = &[
    RouteDefinition {
        id: RouteId::MarketingOverview,
        template: "/marketing",
        label: "Marketing",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingProfiles,
        template: "/marketing/profiles",
        label: "Profiles",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingConsent,
        template: "/marketing/consent",
        label: "Consent",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingAudiences,
        template: "/marketing/audiences",
        label: "Audiences",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingCampaigns,
        template: "/marketing/campaigns",
        label: "Campaigns",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingCalendar,
        template: "/marketing/calendar",
        label: "Calendar",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingJourneys,
        template: "/marketing/journeys",
        label: "Journeys",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingContent,
        template: "/marketing/content",
        label: "Content",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingTemplates,
        template: "/marketing/templates",
        label: "Templates",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingLandingPages,
        template: "/marketing/landing-pages",
        label: "Landing pages",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingForms,
        template: "/marketing/forms",
        label: "Forms",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingLeads,
        template: "/marketing/leads",
        label: "Leads",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingLeadScoring,
        template: "/marketing/lead-scoring",
        label: "Lead scoring",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingBudget,
        template: "/marketing/budget",
        label: "Budget",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingAttribution,
        template: "/marketing/attribution",
        label: "Attribution",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingAnalytics,
        template: "/marketing/analytics",
        label: "Analytics",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingChannels,
        template: "/marketing/channels",
        label: "Channels",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::MarketingDomains,
        template: "/marketing/domains",
        label: "Domains",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CrmAccounts,
        template: "/crm/accounts",
        label: "Accounts",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CrmContacts,
        template: "/crm/contacts",
        label: "Contacts",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CrmLeads,
        template: "/crm/leads",
        label: "Leads",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CrmOpportunities,
        template: "/crm/opportunities",
        label: "Opportunities",
        auth: RouteAuthRequirement::Authenticated,
    },
    RouteDefinition {
        id: RouteId::CrmActivities,
        template: "/crm/activities",
        label: "Activities",
        auth: RouteAuthRequirement::Authenticated,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RouteRegistryKind {
    Platform,
    MarketingCrm,
}

/// A cheap, copyable handle to the platform route registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteRegistry {
    kind: RouteRegistryKind,
}

impl Default for RouteRegistry {
    fn default() -> Self {
        Self::platform()
    }
}

impl RouteRegistry {
    /// Return the canonical platform registry.
    pub const fn platform() -> Self {
        Self {
            kind: RouteRegistryKind::Platform,
        }
    }

    /// Return the canonical Marketing and CRM registry.
    pub const fn marketing_crm() -> Self {
        Self {
            kind: RouteRegistryKind::MarketingCrm,
        }
    }

    /// Return every route definition in canonical order.
    pub const fn definitions(self) -> &'static [RouteDefinition] {
        match self.kind {
            RouteRegistryKind::Platform => PLATFORM_CUSTOMER_360_ROUTES,
            RouteRegistryKind::MarketingCrm => MARKETING_CRM_ROUTES,
        }
    }

    /// Parse a deep link against the typed registry.
    pub fn resolve(self, path: &str) -> Result<PlatformRoute, RouteParseError> {
        if self.kind != RouteRegistryKind::Platform {
            return Err(RouteParseError::new(redact_deep_link(path)));
        }
        let route = PlatformRoute::parse(path)?;
        Ok(route)
    }

    /// Resolve a registered route to its stable, typed identifier.
    pub fn resolve_id(self, path: &str) -> Result<RouteId, RouteParseError> {
        match self.kind {
            RouteRegistryKind::Platform => self.resolve(path).map(|route| route.id()),
            RouteRegistryKind::MarketingCrm => {
                let normalized = redact_deep_link(path);
                MARKETING_CRM_ROUTES
                    .iter()
                    .find(|definition| definition.template == normalized)
                    .map(|definition| definition.id)
                    .ok_or_else(|| RouteParseError::new(normalized))
            }
        }
    }
}

impl RouteDefinition {
    /// Return a bilingual, allow-listed display label for this route.
    pub const fn label_for(self, locale: Locale) -> &'static str {
        match route_translation_key(self.id) {
            Some(key) => key.text(locale),
            None => self.label,
        }
    }

    /// Return a static icon token interpreted by the consuming target theme.
    pub const fn icon(self) -> &'static str {
        route_icon(self.id)
    }

    /// Build a target-neutral navigation item without data or authorization state.
    pub fn nav_item(self, locale: Locale) -> NavItem {
        NavItem::new(self.label_for(locale), self.template, self.icon())
    }
}

const fn route_translation_key(id: RouteId) -> Option<TranslationKey> {
    Some(match id {
        RouteId::MarketingOverview => TranslationKey::Marketing,
        RouteId::MarketingProfiles => TranslationKey::MarketingProfiles,
        RouteId::MarketingConsent => TranslationKey::MarketingConsent,
        RouteId::MarketingAudiences => TranslationKey::MarketingAudiences,
        RouteId::MarketingCampaigns => TranslationKey::MarketingCampaigns,
        RouteId::MarketingCalendar => TranslationKey::MarketingCalendar,
        RouteId::MarketingJourneys => TranslationKey::MarketingJourneys,
        RouteId::MarketingContent => TranslationKey::MarketingContent,
        RouteId::MarketingTemplates => TranslationKey::MarketingTemplates,
        RouteId::MarketingLandingPages => TranslationKey::MarketingLandingPages,
        RouteId::MarketingForms => TranslationKey::MarketingForms,
        RouteId::MarketingLeads => TranslationKey::MarketingLeads,
        RouteId::MarketingLeadScoring => TranslationKey::MarketingLeadScoring,
        RouteId::MarketingBudget => TranslationKey::MarketingBudget,
        RouteId::MarketingAttribution => TranslationKey::MarketingAttribution,
        RouteId::MarketingAnalytics => TranslationKey::MarketingAnalytics,
        RouteId::MarketingChannels => TranslationKey::MarketingChannels,
        RouteId::MarketingDomains => TranslationKey::MarketingDomains,
        RouteId::CrmAccounts => TranslationKey::CrmAccounts,
        RouteId::CrmContacts => TranslationKey::CrmContacts,
        RouteId::CrmLeads => TranslationKey::CrmLeads,
        RouteId::CrmOpportunities => TranslationKey::CrmOpportunities,
        RouteId::CrmActivities => TranslationKey::CrmActivities,
        _ => return None,
    })
}

const fn route_icon(id: RouteId) -> &'static str {
    match id {
        RouteId::MarketingOverview => "◇",
        RouteId::MarketingProfiles => "◎",
        RouteId::MarketingConsent => "✓",
        RouteId::MarketingAudiences => "◌",
        RouteId::MarketingCampaigns => "▣",
        RouteId::MarketingCalendar => "□",
        RouteId::MarketingJourneys => "↝",
        RouteId::MarketingContent => "▤",
        RouteId::MarketingTemplates => "▧",
        RouteId::MarketingLandingPages => "⌂",
        RouteId::MarketingForms => "▱",
        RouteId::MarketingLeads => "↗",
        RouteId::MarketingLeadScoring => "★",
        RouteId::MarketingBudget => "¤",
        RouteId::MarketingAttribution => "⌁",
        RouteId::MarketingAnalytics => "◒",
        RouteId::MarketingChannels => "⌘",
        RouteId::MarketingDomains => "@",
        RouteId::CrmAccounts => "▦",
        RouteId::CrmContacts => "♙",
        RouteId::CrmLeads => "↗",
        RouteId::CrmOpportunities => "◇",
        RouteId::CrmActivities => "◷",
        _ => "•",
    }
}

/// A validated opaque customer identifier. It may be displayed or sent to an application route,
/// but it is not a credential and contains no bearer/refresh token material.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CustomerId(String);

/// Maximum bytes retained for a customer route identifier.
pub const MAX_CUSTOMER_ID_BYTES: usize = 128;

impl CustomerId {
    /// Validate and create an opaque route identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidCustomerId> {
        let value = value.into();
        if value.len() <= MAX_CUSTOMER_ID_BYTES && is_safe_segment(&value) {
            Ok(Self(value))
        } else {
            Err(InvalidCustomerId)
        }
    }

    /// Borrow the validated identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Error returned for an unsafe or empty customer identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidCustomerId;

impl fmt::Display for InvalidCustomerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("customer route identifier is empty or contains an unsafe path character")
    }
}

impl Error for InvalidCustomerId {}

impl TryFrom<&str> for CustomerId {
    type Error = InvalidCustomerId;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Customer 360 sub-page represented by a typed route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomerView {
    /// Base overview page.
    Overview,
    /// Organization page.
    Organization,
    /// Contacts page.
    Contacts,
    /// Commercial page.
    Commercial,
    /// Entitlements page.
    Entitlements,
    /// Onboarding page.
    Onboarding,
    /// Health page.
    Health,
    /// Support page.
    Support,
    /// Global-read access audit page.
    AccessAudit,
    /// Notes page.
    Notes,
}

/// Typed platform route registry output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformRoute {
    /// Platform landing route.
    PlatformHome,
    /// Customer list route.
    CustomerList,
    /// A typed Customer 360 sub-route.
    Customer {
        /// Opaque customer identifier.
        customer_id: CustomerId,
        /// Customer 360 view.
        view: CustomerView,
    },
}

impl PlatformRoute {
    /// Return the canonical registry.
    pub const fn registry() -> RouteRegistry {
        RouteRegistry::platform()
    }

    /// Parse and normalize a route/deep link.
    pub fn parse(path: &str) -> Result<Self, RouteParseError> {
        let normalized = redact_deep_link(path);
        let segments: Vec<&str> = normalized
            .trim_matches('/')
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect();

        match segments.as_slice() {
            [platform] if *platform == "platform" => Ok(Self::PlatformHome),
            [platform, customers] if *platform == "platform" && *customers == "customers" => {
                Ok(Self::CustomerList)
            }
            [platform, customers, customer_id]
                if *platform == "platform" && *customers == "customers" =>
            {
                Self::customer_route(customer_id, CustomerView::Overview, normalized.clone())
            }
            [platform, customers, customer_id, view]
                if *platform == "platform" && *customers == "customers" =>
            {
                let view = match *view {
                    "organization" => CustomerView::Organization,
                    "contacts" => CustomerView::Contacts,
                    "commercial" => CustomerView::Commercial,
                    "entitlements" => CustomerView::Entitlements,
                    "onboarding" => CustomerView::Onboarding,
                    "health" => CustomerView::Health,
                    "support" => CustomerView::Support,
                    "access-audit" => CustomerView::AccessAudit,
                    "notes" => CustomerView::Notes,
                    _ => return Err(RouteParseError::new(normalized)),
                };
                Self::customer_route(customer_id, view, normalized.clone())
            }
            _ => Err(RouteParseError::new(normalized)),
        }
    }

    /// Return a canonical deep-link path.
    pub fn path(&self) -> String {
        match self {
            Self::PlatformHome => "/platform".to_string(),
            Self::CustomerList => "/platform/customers".to_string(),
            Self::Customer { customer_id, view } => {
                let suffix = match view {
                    CustomerView::Overview => String::new(),
                    CustomerView::Organization => "/organization".to_string(),
                    CustomerView::Contacts => "/contacts".to_string(),
                    CustomerView::Commercial => "/commercial".to_string(),
                    CustomerView::Entitlements => "/entitlements".to_string(),
                    CustomerView::Onboarding => "/onboarding".to_string(),
                    CustomerView::Health => "/health".to_string(),
                    CustomerView::Support => "/support".to_string(),
                    CustomerView::AccessAudit => "/access-audit".to_string(),
                    CustomerView::Notes => "/notes".to_string(),
                };
                format!("/platform/customers/{}{suffix}", customer_id.as_str())
            }
        }
    }

    /// Return the registry identifier for this route.
    pub const fn id(&self) -> RouteId {
        match self {
            Self::PlatformHome => RouteId::PlatformHome,
            Self::CustomerList => RouteId::CustomerList,
            Self::Customer {
                view: CustomerView::Overview,
                ..
            } => RouteId::CustomerOverview,
            Self::Customer {
                view: CustomerView::Organization,
                ..
            } => RouteId::CustomerOrganization,
            Self::Customer {
                view: CustomerView::Contacts,
                ..
            } => RouteId::CustomerContacts,
            Self::Customer {
                view: CustomerView::Commercial,
                ..
            } => RouteId::CustomerCommercial,
            Self::Customer {
                view: CustomerView::Entitlements,
                ..
            } => RouteId::CustomerEntitlements,
            Self::Customer {
                view: CustomerView::Onboarding,
                ..
            } => RouteId::CustomerOnboarding,
            Self::Customer {
                view: CustomerView::Health,
                ..
            } => RouteId::CustomerHealth,
            Self::Customer {
                view: CustomerView::Support,
                ..
            } => RouteId::CustomerSupport,
            Self::Customer {
                view: CustomerView::AccessAudit,
                ..
            } => RouteId::CustomerAccessAudit,
            Self::Customer {
                view: CustomerView::Notes,
                ..
            } => RouteId::CustomerNotes,
        }
    }

    fn customer_route(
        customer_id: &str,
        view: CustomerView,
        normalized: String,
    ) -> Result<Self, RouteParseError> {
        CustomerId::new(customer_id.to_string())
            .map(|customer_id| Self::Customer { customer_id, view })
            .map_err(|_| RouteParseError::new(normalized))
    }
}

impl fmt::Display for PlatformRoute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.path())
    }
}

/// Error returned for a route outside the typed registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteParseError {
    /// The normalized path that failed to resolve.
    pub normalized_path: String,
}

impl RouteParseError {
    fn new(normalized_path: String) -> Self {
        Self { normalized_path }
    }
}

impl fmt::Display for RouteParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no ERPX platform route matches {}", self.normalized_path)
    }
}

impl Error for RouteParseError {}

/// Maximum raw bytes accepted from a browser/native deep-link handoff.
///
/// This bounds work and retained input before credential scanning, normalization, or route
/// parsing. Oversized values fail closed to the application home path rather than being
/// truncated into a different route.
const MAX_DEEP_LINK_BYTES: usize = 4096;

/// Normalize browser hashes, query strings, repeated separators, and trailing slashes into a
/// safe internal route path. Dot segments are rejected to avoid client-side path traversal.
pub fn normalize_deep_link(input: &str) -> String {
    if input.len() > MAX_DEEP_LINK_BYTES {
        return "/".to_string();
    }

    let mut raw = input.trim();
    if let Some(hash_route) = raw.strip_prefix("/#") {
        raw = hash_route;
    } else if let Some(hash_route) = raw.strip_prefix('#') {
        raw = hash_route;
    }

    // A native deep-link bridge must never reinterpret an external URI as an
    // application-relative path.  Reject URI schemes and protocol-relative
    // authorities before splitting the route into segments.
    if has_external_origin(raw) {
        return "/".to_string();
    }

    let raw = raw.split(['?', '#']).next().unwrap_or_default();
    let mut normalized = String::new();
    for segment in raw.split('/') {
        if segment.is_empty() {
            continue;
        }
        if segment == "."
            || segment == ".."
            || segment
                .chars()
                .any(|character| character.is_control() || matches!(character, '\\' | '%'))
        {
            return "/".to_string();
        }
        normalized.push('/');
        normalized.push_str(segment);
    }
    if normalized.is_empty() {
        "/".to_string()
    } else {
        normalized
    }
}

/// Remove credential-bearing deep links before they enter client session state or UI error
/// messages. Query and fragment values are never retained; links containing authentication
/// material are discarded completely rather than normalized into a potentially unsafe route.
pub fn redact_deep_link(input: &str) -> String {
    if input.len() > MAX_DEEP_LINK_BYTES {
        return "/".to_string();
    }

    let lower = input.to_ascii_lowercase();
    const SENSITIVE_MARKERS: &[&str] = &[
        "access_token",
        "refresh_token",
        "id_token",
        "authorization",
        "client_secret",
        "password",
        "secret",
    ];
    if SENSITIVE_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
    {
        "/".to_string()
    } else {
        normalize_deep_link(input)
    }
}

fn has_external_origin(value: &str) -> bool {
    if value.starts_with("//") || value.starts_with("\\\\") {
        return true;
    }

    let separator = value.find(['/', '\\']);
    let colon = value.find(':');
    match (colon, separator) {
        (Some(colon), Some(separator)) => colon <= separator,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

fn is_safe_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CUSTOMER_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && !value.contains("..")
}

/// Safe, non-secret metadata supplied by an authenticated application boundary.
///
/// This intentionally has no token, cookie, password, private key, or bearer/refresh-token
/// fields. It is appropriate for rendering persistent context banners only.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SafeSessionMetadata {
    /// Display-only principal name or label.
    pub principal_label: Option<String>,
    /// Display-only active role label.
    pub role_label: Option<String>,
    /// Display-only session expiry timestamp.
    pub session_expires_at: Option<String>,
    /// Display-only selected scope label.
    pub scope_label: Option<String>,
    /// Optional approved support context.
    pub support: Option<SupportAccessMetadata>,
    /// Optional represented-user context.
    pub impersonation: Option<ImpersonationMetadata>,
    /// Optional billing/policy read-only context.
    pub read_only: Option<ReadOnlyMetadata>,
}

impl fmt::Debug for SafeSessionMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SafeSessionMetadata")
            .field(
                "principal_label",
                &self.principal_label.as_deref().map(SafeUiText::new),
            )
            .field(
                "role_label",
                &self.role_label.as_deref().map(SafeUiText::new),
            )
            .field(
                "session_expires_at",
                &self.session_expires_at.as_deref().map(SafeUiText::new),
            )
            .field(
                "scope_label",
                &self.scope_label.as_deref().map(SafeUiText::new),
            )
            .field("support", &self.support)
            .field("impersonation", &self.impersonation)
            .field("read_only", &self.read_only)
            .finish()
    }
}

/// Display-only metadata for an approved support context.
#[derive(Clone, PartialEq, Eq)]
pub struct SupportAccessMetadata {
    /// Customer-visible ticket label.
    pub ticket_label: String,
    /// Display-only capability expiry.
    pub expires_at: String,
    /// Display-only access mode label.
    pub mode_label: String,
}

impl fmt::Debug for SupportAccessMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SupportAccessMetadata")
            .field("ticket_label", &SafeUiText::new(&self.ticket_label))
            .field("expires_at", &SafeUiText::new(&self.expires_at))
            .field("mode_label", &SafeUiText::new(&self.mode_label))
            .finish()
    }
}

/// Display-only metadata for an approved impersonation context.
#[derive(Clone, PartialEq, Eq)]
pub struct ImpersonationMetadata {
    /// Represented user's display label.
    pub represented_label: String,
    /// Customer-visible reason label.
    pub reason: String,
}

impl fmt::Debug for ImpersonationMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImpersonationMetadata")
            .field(
                "represented_label",
                &SafeUiText::new(&self.represented_label),
            )
            .field("reason", &SafeUiText::new(&self.reason))
            .finish()
    }
}

/// Display-only metadata for a read-only route/session context.
#[derive(Clone, PartialEq, Eq)]
pub struct ReadOnlyMetadata {
    /// Customer-visible reason for mutation blocking.
    pub reason: String,
}

impl fmt::Debug for ReadOnlyMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReadOnlyMetadata")
            .field("reason", &SafeUiText::new(&self.reason))
            .finish()
    }
}

/// Explicit route state supplied by the authenticated application boundary.
#[derive(Clone, PartialEq, Eq)]
pub enum RouteState {
    /// An active authenticated session may render the route.
    Authenticated { session: SafeSessionMetadata },
    /// No authenticated session exists; the return path is non-secret navigation metadata.
    Unauthenticated { return_to: String },
    /// The session expired and the user must reauthenticate.
    SessionExpired { return_to: String },
    /// The session was revoked and the user must reauthenticate.
    SessionRevoked { return_to: String },
    /// The session is authenticated but mutations are disabled.
    ReadOnly {
        session: SafeSessionMetadata,
        reason: String,
    },
    /// The route cannot reach its backend; cached content may still be shown by the app.
    Offline {
        cached_view_available: bool,
        reason: String,
    },
    /// The authenticated principal is not allowed to view the route.
    Denied { reason: String },
    /// A projection is available but outside its freshness budget.
    Stale { age_seconds: u64, reason: String },
    /// An optimistic-concurrency conflict requires review.
    Conflict { reason: String },
}

impl fmt::Debug for RouteState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authenticated { session } => formatter
                .debug_struct("RouteState::Authenticated")
                .field("session", session)
                .finish(),
            Self::Unauthenticated { return_to } => formatter
                .debug_struct("RouteState::Unauthenticated")
                .field("return_to", &debug_route_target(return_to))
                .finish(),
            Self::SessionExpired { return_to } => formatter
                .debug_struct("RouteState::SessionExpired")
                .field("return_to", &debug_route_target(return_to))
                .finish(),
            Self::SessionRevoked { return_to } => formatter
                .debug_struct("RouteState::SessionRevoked")
                .field("return_to", &debug_route_target(return_to))
                .finish(),
            Self::ReadOnly { session, reason } => formatter
                .debug_struct("RouteState::ReadOnly")
                .field("session", session)
                .field("reason", &SafeUiText::new(reason))
                .finish(),
            Self::Offline {
                cached_view_available,
                reason,
            } => formatter
                .debug_struct("RouteState::Offline")
                .field("cached_view_available", cached_view_available)
                .field("reason", &SafeUiText::new(reason))
                .finish(),
            Self::Stale {
                age_seconds,
                reason,
            } => formatter
                .debug_struct("RouteState::Stale")
                .field("age_seconds", age_seconds)
                .field("reason", &SafeUiText::new(reason))
                .finish(),
            Self::Denied { reason } => formatter
                .debug_struct("RouteState::Denied")
                .field("reason", &SafeUiText::new(reason))
                .finish(),
            Self::Conflict { reason } => formatter
                .debug_struct("RouteState::Conflict")
                .field("reason", &SafeUiText::new(reason))
                .finish(),
        }
    }
}

fn debug_route_target(value: &str) -> SafeUiText {
    SafeUiText::new(redact_deep_link(value))
}

impl RouteState {
    /// Return the UI state kind for non-ready route states.
    pub const fn ui_state(&self) -> Option<UiStateKind> {
        match self {
            Self::Authenticated { .. } => None,
            Self::Unauthenticated { .. }
            | Self::SessionExpired { .. }
            | Self::SessionRevoked { .. }
            | Self::Denied { .. } => Some(UiStateKind::Denied),
            Self::ReadOnly { .. } => Some(UiStateKind::ReadOnly),
            Self::Offline { .. } => Some(UiStateKind::Offline),
            Self::Stale { .. } => Some(UiStateKind::Stale),
            Self::Conflict { .. } => Some(UiStateKind::Conflict),
        }
    }

    /// Return safe metadata for persistent banners, when available.
    pub fn session_metadata(&self) -> Option<&SafeSessionMetadata> {
        match self {
            Self::Authenticated { session } | Self::ReadOnly { session, .. } => Some(session),
            _ => None,
        }
    }

    /// Return a normalized non-secret navigation target for reauthentication states.
    pub fn return_to(&self) -> Option<String> {
        match self {
            Self::Unauthenticated { return_to }
            | Self::SessionExpired { return_to }
            | Self::SessionRevoked { return_to } => Some(safe_return_to(return_to)),
            _ => None,
        }
    }
}

/// Return a bounded, app-relative path suitable for a browser or native deep-link handoff.
///
/// Route states can be supplied directly by a consuming target, so they must not rely on the
/// authentication controller having already applied its stricter return-path policy. Falling
/// back to the home route keeps oversized or target-ambiguous values from reaching a router.
fn safe_return_to(value: &str) -> String {
    let normalized = redact_deep_link(value.trim());
    if normalized == "/"
        || (normalized.len() <= SafeUiText::MAX_BYTES
            && normalized.starts_with('/')
            && normalized.chars().all(|character| {
                !character.is_control()
                    && !character.is_whitespace()
                    && !matches!(character, '\\' | '%' | ':')
            }))
    {
        normalized
    } else {
        "/".to_owned()
    }
}

/// Scope kinds shown in the client selector. Scope containment and authorization remain server
/// responsibilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// Platform-wide scope.
    Platform,
    /// Holding scope.
    Holding,
    /// Legal-company scope.
    Company,
    /// Branch scope.
    Branch,
}

/// Safe scope option metadata for the selector; it contains no credential or tenant secret.
#[derive(Clone, PartialEq, Eq)]
pub struct ScopeOption {
    /// Opaque display/navigation key supplied by the authenticated boundary.
    pub id: String,
    /// Localized display label.
    pub label: String,
    /// Scope level used for display.
    pub kind: ScopeKind,
}

impl fmt::Debug for ScopeOption {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScopeOption")
            .field("id", &SafeUiText::new(&self.id))
            .field("label", &SafeUiText::new(&self.label))
            .field("kind", &self.kind)
            .finish()
    }
}

impl ScopeOption {
    /// Construct a safe selector option.
    pub fn new(id: impl Into<String>, label: impl Into<String>, kind: ScopeKind) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CustomerView, MAX_DEEP_LINK_BYTES, PlatformRoute, RouteAuthRequirement, RouteId,
        RouteRegistry, RouteState, SafeSessionMetadata, ScopeKind, ScopeOption,
        normalize_deep_link,
    };

    #[test]
    fn registry_covers_every_customer_360_route() {
        let registry = RouteRegistry::platform();
        assert_eq!(registry.definitions().len(), 12);
        assert!(
            registry
                .definitions()
                .iter()
                .all(|definition| definition.auth == super::RouteAuthRequirement::Authenticated)
        );

        let route = registry
            .resolve("#/platform/customers/cus_123/health?tab=projection")
            .expect("health route should resolve");
        assert_eq!(route.id(), RouteId::CustomerHealth);
        assert_eq!(route.path(), "/platform/customers/cus_123/health");
    }

    #[test]
    fn marketing_and_crm_registry_covers_exact_allow_list() {
        let registry = RouteRegistry::marketing_crm();
        let expected = [
            "/marketing",
            "/marketing/profiles",
            "/marketing/consent",
            "/marketing/audiences",
            "/marketing/campaigns",
            "/marketing/calendar",
            "/marketing/journeys",
            "/marketing/content",
            "/marketing/templates",
            "/marketing/landing-pages",
            "/marketing/forms",
            "/marketing/leads",
            "/marketing/lead-scoring",
            "/marketing/budget",
            "/marketing/attribution",
            "/marketing/analytics",
            "/marketing/channels",
            "/marketing/domains",
            "/crm/accounts",
            "/crm/contacts",
            "/crm/leads",
            "/crm/opportunities",
            "/crm/activities",
        ];

        assert_eq!(registry.definitions().len(), expected.len());
        for (definition, expected_path) in registry.definitions().iter().zip(expected) {
            assert_eq!(definition.template, expected_path);
            assert_eq!(definition.auth, RouteAuthRequirement::Authenticated);
            let route_id = registry
                .resolve_id(&format!("{expected_path}/"))
                .expect("allow-listed route should resolve");
            assert_eq!(route_id, definition.id);
        }

        assert!(RouteRegistry::platform().resolve_id("/marketing").is_err());
        assert!(registry.resolve_id("/marketingx").is_err());
        assert!(registry.resolve_id("/marketing/profiles/extra").is_err());
    }

    #[test]
    fn marketing_and_crm_metadata_is_bilingual_and_static() {
        let registry = RouteRegistry::marketing_crm();
        for definition in registry.definitions() {
            let english = definition.label_for(super::Locale::En);
            let thai = definition.label_for(super::Locale::Th);
            assert!(!english.is_empty());
            assert!(!thai.is_empty());
            assert_ne!(english, thai);

            let item = definition.nav_item(super::Locale::Th);
            assert_eq!(item.path, definition.template);
            assert_eq!(item.label, thai);
            assert!(!item.icon.is_empty());
        }
    }

    #[test]
    fn deep_links_are_normalized_and_dot_segments_are_rejected() {
        assert_eq!(
            normalize_deep_link("/#/platform//customers/cus_123///?tab=health"),
            "/platform/customers/cus_123"
        );
        assert_eq!(normalize_deep_link("   "), "/");
        assert_eq!(normalize_deep_link("/platform/../customers"), "/");
        assert!(PlatformRoute::parse("/platform/customers/cus_123/unknown").is_err());
    }

    #[test]
    fn credential_bearing_deep_links_are_redacted_before_display() {
        assert_eq!(
            super::redact_deep_link("/#/platform/customers?access_token=secret"),
            "/"
        );
        assert_eq!(
            super::redact_deep_link("/platform/customers#refresh_token=secret"),
            "/"
        );
        assert_eq!(
            super::redact_deep_link("/platform/customers?tab=health"),
            "/platform/customers"
        );
        assert_eq!(
            super::redact_deep_link("/marketing/campaigns?client_secret=secret"),
            "/"
        );
        assert!(PlatformRoute::parse("/crm/accounts?access_token=secret").is_err());
        assert!(
            RouteRegistry::marketing_crm()
                .resolve_id("/marketing/consent#refresh_token=secret")
                .is_err()
        );
    }

    #[test]
    fn deep_link_boundary_rejects_external_and_encoded_targets() {
        for input in [
            "https://evil.example/platform",
            "//evil.example/platform",
            "\\\\evil.example\\platform",
            "/platform/%2Fcustomers",
            "/platform\\customers",
        ] {
            assert_eq!(
                super::redact_deep_link(input),
                "/",
                "unsafe deep link must fail closed: {input}"
            );
        }
        assert!(PlatformRoute::parse("https://evil.example/platform").is_err());
        for ambiguous in [
            "/platform/customers/customer:opaque",
            "/platform/customers/customer with spaces",
            &format!("/platform/customers/{}", "customer-".repeat(40)),
        ] {
            assert!(
                PlatformRoute::parse(ambiguous).is_err(),
                "ambiguous or oversized route identifiers must fail closed: {ambiguous}"
            );
        }
    }

    #[test]
    fn deep_link_boundary_rejects_oversized_input_without_truncating_a_route() {
        let route_prefix = "/platform?tab=";
        let oversized = format!(
            "{route_prefix}{}",
            "x".repeat(MAX_DEEP_LINK_BYTES - route_prefix.len() + 1)
        );
        assert_eq!(oversized.len(), MAX_DEEP_LINK_BYTES + 1);
        assert_eq!(normalize_deep_link(&oversized), "/");
        assert_eq!(super::redact_deep_link(&oversized), "/");
        assert!(PlatformRoute::parse(&oversized).is_err());

        let route_prefix = "/platform?tab=";
        let at_limit = format!(
            "{route_prefix}{}",
            "x".repeat(MAX_DEEP_LINK_BYTES - route_prefix.len())
        );
        assert_eq!(at_limit.len(), MAX_DEEP_LINK_BYTES);
        assert_eq!(normalize_deep_link(&at_limit), "/platform");
        assert_eq!(
            PlatformRoute::parse(&at_limit).expect("bounded route should parse"),
            PlatformRoute::PlatformHome
        );
    }

    #[test]
    fn typed_customer_views_round_trip() {
        let routes = [
            (CustomerView::Overview, "/platform/customers/cus_123"),
            (
                CustomerView::Organization,
                "/platform/customers/cus_123/organization",
            ),
            (
                CustomerView::AccessAudit,
                "/platform/customers/cus_123/access-audit",
            ),
            (CustomerView::Notes, "/platform/customers/cus_123/notes"),
        ];
        for (view, path) in routes {
            let parsed = PlatformRoute::parse(path).expect("route should parse");
            assert_eq!(parsed.path(), path);
            assert!(
                matches!(parsed, PlatformRoute::Customer { view: actual, .. } if actual == view)
            );
        }
    }

    #[test]
    fn route_states_keep_auth_session_and_failure_states_distinct() {
        let session = SafeSessionMetadata {
            principal_label: Some("Platform Administrator".into()),
            ..SafeSessionMetadata::default()
        };
        assert_eq!(
            RouteState::Authenticated {
                session: session.clone()
            }
            .ui_state(),
            None
        );
        assert_eq!(
            RouteState::Unauthenticated {
                return_to: "/platform/customers".into()
            }
            .ui_state(),
            Some(super::UiStateKind::Denied)
        );
        assert_eq!(
            RouteState::SessionExpired {
                return_to: "/platform/customers/".into()
            }
            .return_to(),
            Some("/platform/customers".into())
        );
        assert_eq!(
            RouteState::ReadOnly {
                session,
                reason: "billing grace".into()
            }
            .session_metadata()
            .and_then(|value| value.principal_label.as_deref()),
            Some("Platform Administrator")
        );
        assert_eq!(
            RouteState::Conflict {
                reason: "version changed".into()
            }
            .ui_state(),
            Some(super::UiStateKind::Conflict)
        );

        let option = ScopeOption::new("holding-1", "Holding A", ScopeKind::Holding);
        assert_eq!(option.id, "holding-1");
    }
}
