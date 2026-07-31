//! Target-neutral metadata for the Platform Operations routes.
//!
//! This module describes navigation and deep-link shape only. It deliberately contains no
//! tenant data and does not evaluate the platform authorization requirement; those concerns stay
//! at the consuming application and server boundaries.

use std::{error::Error, fmt};

use crate::{
    Locale, NavItem, ProjectionFreshness, RouteAuthRequirement, SafeUiText, TranslationKey,
    UiStateKind, normalize_deep_link, redact_deep_link,
};

/// Maximum bytes retained for a Platform Operations tenant route identifier.
pub const MAX_PLATFORM_TENANT_ID_BYTES: usize = 128;

/// The audience for a Platform Operations route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformOperationsAudience {
    /// ERPX platform operators and administrators.
    Platform,
}

/// Stable identifiers for the Platform Operations route allow-list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformOperationsRouteId {
    /// Tenant list.
    Tenants,
    /// A single tenant's platform operations view.
    Tenant,
    /// Projection health and rebuild controls.
    Projections,
    /// Backup and restore history.
    Backups,
    /// Integration health and configuration metadata.
    Integrations,
    /// Platform audit records.
    Audit,
    /// Platform settings.
    Settings,
    /// Expiring-role review and recovery workflow.
    RoleReview,
}

/// A route definition for a Platform Operations page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformOperationsRouteDefinition {
    /// Stable route identifier.
    pub id: PlatformOperationsRouteId,
    /// Canonical route template. `:id` is an opaque tenant identifier segment.
    pub template: &'static str,
    /// Existing localization key for the route label.
    pub label: TranslationKey,
    /// Audience metadata mirrored for navigation and shell policy.
    pub audience: PlatformOperationsAudience,
    /// Authentication metadata mirrored for navigation UX. This is not authorization.
    pub auth: RouteAuthRequirement,
    /// Target-neutral icon token interpreted by the consuming client theme.
    pub icon: &'static str,
}

impl PlatformOperationsRouteDefinition {
    /// Return the localized allow-listed label.
    pub const fn label_for(self, locale: Locale) -> &'static str {
        self.label.text(locale)
    }

    /// Build a target-neutral navigation item without data or authorization state.
    pub fn nav_item(self, locale: Locale) -> NavItem {
        NavItem::new(self.label_for(locale), self.template, self.icon)
    }
}

/// Canonical Platform Operations route allow-list in stable display order.
pub const PLATFORM_OPERATIONS_ROUTES: &[PlatformOperationsRouteDefinition] = &[
    PlatformOperationsRouteDefinition {
        id: PlatformOperationsRouteId::Tenants,
        template: "/platform/tenants",
        label: TranslationKey::PlatformTenants,
        audience: PlatformOperationsAudience::Platform,
        auth: RouteAuthRequirement::Authenticated,
        icon: "▦",
    },
    PlatformOperationsRouteDefinition {
        id: PlatformOperationsRouteId::Tenant,
        template: "/platform/tenants/:id",
        label: TranslationKey::PlatformTenant,
        audience: PlatformOperationsAudience::Platform,
        auth: RouteAuthRequirement::Authenticated,
        icon: "▤",
    },
    PlatformOperationsRouteDefinition {
        id: PlatformOperationsRouteId::Projections,
        template: "/platform/projections",
        label: TranslationKey::PlatformProjections,
        audience: PlatformOperationsAudience::Platform,
        auth: RouteAuthRequirement::Authenticated,
        icon: "◒",
    },
    PlatformOperationsRouteDefinition {
        id: PlatformOperationsRouteId::Backups,
        template: "/platform/backups",
        label: TranslationKey::PlatformBackups,
        audience: PlatformOperationsAudience::Platform,
        auth: RouteAuthRequirement::Authenticated,
        icon: "⬡",
    },
    PlatformOperationsRouteDefinition {
        id: PlatformOperationsRouteId::Integrations,
        template: "/platform/integrations",
        label: TranslationKey::PlatformIntegrations,
        audience: PlatformOperationsAudience::Platform,
        auth: RouteAuthRequirement::Authenticated,
        icon: "⌘",
    },
    PlatformOperationsRouteDefinition {
        id: PlatformOperationsRouteId::Audit,
        template: "/platform/audit",
        label: TranslationKey::PlatformAudit,
        audience: PlatformOperationsAudience::Platform,
        auth: RouteAuthRequirement::Authenticated,
        icon: "◷",
    },
    PlatformOperationsRouteDefinition {
        id: PlatformOperationsRouteId::Settings,
        template: "/platform/settings",
        label: TranslationKey::PlatformSettings,
        audience: PlatformOperationsAudience::Platform,
        auth: RouteAuthRequirement::Authenticated,
        icon: "⚙",
    },
    PlatformOperationsRouteDefinition {
        id: PlatformOperationsRouteId::RoleReview,
        template: "/platform/role-review",
        label: TranslationKey::PlatformRoleReview,
        audience: PlatformOperationsAudience::Platform,
        auth: RouteAuthRequirement::Authenticated,
        icon: "⌛",
    },
];

/// A validated opaque tenant identifier used only as a route segment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlatformTenantId(String);

impl PlatformTenantId {
    /// Validate an opaque tenant identifier without interpreting its contents.
    pub fn new(value: impl Into<String>) -> Result<Self, PlatformOperationsRouteError> {
        let value = value.into();
        if value.len() <= MAX_PLATFORM_TENANT_ID_BYTES
            && is_safe_segment(&value)
            && !contains_sensitive_marker(&value)
        {
            Ok(Self(value))
        } else {
            Err(PlatformOperationsRouteError::new("/".to_string()))
        }
    }

    /// Borrow the validated identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Safe metadata accompanying an authorized Platform Operations read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformOperationsProjectionMetadata {
    /// Tenant binding for a tenant detail/read; absent for platform-wide lists.
    pub tenant_id: Option<PlatformTenantId>,
    /// Freshness classification supplied by the server read boundary.
    pub freshness: ProjectionFreshness,
    /// Platform Operations pages never authorize mutations locally.
    pub read_only: bool,
    /// Bounded server-provided display timestamp.
    pub projection_updated_at: Option<SafeUiText>,
}

impl PlatformOperationsProjectionMetadata {
    /// Construct metadata for a fresh platform read.
    pub fn fresh(
        tenant_id: Option<PlatformTenantId>,
        projection_updated_at: impl AsRef<str>,
    ) -> Self {
        Self::present(tenant_id, ProjectionFreshness::Fresh, projection_updated_at)
    }

    /// Construct metadata for a stale but displayable platform read.
    pub fn stale(
        tenant_id: Option<PlatformTenantId>,
        projection_updated_at: impl AsRef<str>,
    ) -> Self {
        Self::present(tenant_id, ProjectionFreshness::Stale, projection_updated_at)
    }

    /// Construct metadata with an explicit freshness classification.
    pub fn present(
        tenant_id: Option<PlatformTenantId>,
        freshness: ProjectionFreshness,
        projection_updated_at: impl AsRef<str>,
    ) -> Self {
        let projection_updated_at = projection_updated_at.as_ref().trim();
        let projection_updated_at = if projection_updated_at.is_empty() {
            None
        } else {
            Some(SafeUiText::new(projection_updated_at))
        };
        Self {
            tenant_id,
            freshness: if projection_updated_at.is_some() {
                freshness
            } else {
                ProjectionFreshness::Unknown
            },
            read_only: true,
            projection_updated_at,
        }
    }

    /// Whether the read is outside the freshness budget.
    pub const fn is_stale(&self) -> bool {
        matches!(self.freshness, ProjectionFreshness::Stale)
    }

    /// Whether this metadata is bound to the requested tenant route.
    pub fn is_bound_to_tenant(&self, tenant_id: &PlatformTenantId) -> bool {
        self.tenant_id.as_ref() == Some(tenant_id)
    }
}

/// Explicit Platform Operations page states for target adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformOperationsPageState<T = ()> {
    /// An authoritative read is in progress.
    Loading {
        metadata: Option<PlatformOperationsProjectionMetadata>,
    },
    /// The authoritative read returned no rows.
    Empty {
        metadata: PlatformOperationsProjectionMetadata,
    },
    /// Safe data is available for display.
    Ready {
        data: T,
        metadata: PlatformOperationsProjectionMetadata,
    },
    /// The server denied the platform read.
    Denied {
        metadata: Option<PlatformOperationsProjectionMetadata>,
        reason: SafeUiText,
    },
    /// Cached data is stale and remains read-only.
    Stale {
        data: T,
        metadata: PlatformOperationsProjectionMetadata,
        reason: SafeUiText,
    },
    /// The backend is unavailable; optional cached data remains read-only.
    Offline {
        data: Option<T>,
        metadata: Option<PlatformOperationsProjectionMetadata>,
        reason: SafeUiText,
    },
    /// An unexpected read error occurred.
    Error { reason: SafeUiText },
}

impl<T> PlatformOperationsPageState<T> {
    /// Return the shared state marker used by target accessibility renderers.
    pub const fn kind(&self) -> Option<UiStateKind> {
        match self {
            Self::Loading { .. } => Some(UiStateKind::Loading),
            Self::Empty { .. } => Some(UiStateKind::Empty),
            Self::Ready { .. } => None,
            Self::Denied { .. } => Some(UiStateKind::Denied),
            Self::Stale { .. } => Some(UiStateKind::Stale),
            Self::Offline { .. } => Some(UiStateKind::Offline),
            Self::Error { .. } => Some(UiStateKind::Error),
        }
    }

    /// Return safe current or cached data, when present.
    pub fn data(&self) -> Option<&T> {
        match self {
            Self::Ready { data, .. } | Self::Stale { data, .. } => Some(data),
            Self::Offline { data, .. } => data.as_ref(),
            Self::Loading { .. }
            | Self::Empty { .. }
            | Self::Denied { .. }
            | Self::Error { .. } => None,
        }
    }

    /// Return metadata, when present.
    pub fn metadata(&self) -> Option<&PlatformOperationsProjectionMetadata> {
        match self {
            Self::Loading { metadata }
            | Self::Denied { metadata, .. }
            | Self::Offline { metadata, .. } => metadata.as_ref(),
            Self::Empty { metadata }
            | Self::Ready { metadata, .. }
            | Self::Stale { metadata, .. } => Some(metadata),
            Self::Error { .. } => None,
        }
    }

    /// Platform state never authorizes or enables a mutation locally.
    pub const fn is_read_only(&self) -> bool {
        true
    }

    /// Fail closed when a tenant detail state carries another tenant's metadata or payload.
    /// States with no payload are safe without a tenant binding; a cached payload is not.
    pub fn is_scope_bound_to_tenant(&self, tenant_id: &PlatformTenantId) -> bool {
        match self {
            Self::Ready { metadata, .. } | Self::Stale { metadata, .. } => {
                metadata.is_bound_to_tenant(tenant_id)
            }
            Self::Offline {
                data: Some(_),
                metadata: Some(metadata),
                ..
            } => metadata.is_bound_to_tenant(tenant_id),
            Self::Offline { data: Some(_), .. } => false,
            Self::Loading { metadata } | Self::Denied { metadata, .. } => metadata
                .as_ref()
                .is_none_or(|metadata| metadata.is_bound_to_tenant(tenant_id)),
            Self::Empty { metadata } => metadata
                .tenant_id
                .as_ref()
                .is_none_or(|bound| bound == tenant_id),
            Self::Offline {
                data: None,
                metadata,
                ..
            } => metadata
                .as_ref()
                .is_none_or(|metadata| metadata.is_bound_to_tenant(tenant_id)),
            Self::Error { .. } => true,
        }
    }
}

/// A typed Platform Operations route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformOperationsRoute {
    /// Tenant list.
    Tenants,
    /// A single tenant route with a validated opaque identifier.
    Tenant { tenant_id: PlatformTenantId },
    /// Projection health and rebuild controls.
    Projections,
    /// Backup and restore history.
    Backups,
    /// Integration health and configuration metadata.
    Integrations,
    /// Platform audit records.
    Audit,
    /// Platform settings.
    Settings,
    /// Expiring-role review and recovery workflow.
    RoleReview,
}

impl PlatformOperationsRoute {
    /// Return the stable route identifier.
    pub const fn id(&self) -> PlatformOperationsRouteId {
        match self {
            Self::Tenants => PlatformOperationsRouteId::Tenants,
            Self::Tenant { .. } => PlatformOperationsRouteId::Tenant,
            Self::Projections => PlatformOperationsRouteId::Projections,
            Self::Backups => PlatformOperationsRouteId::Backups,
            Self::Integrations => PlatformOperationsRouteId::Integrations,
            Self::Audit => PlatformOperationsRouteId::Audit,
            Self::Settings => PlatformOperationsRouteId::Settings,
            Self::RoleReview => PlatformOperationsRouteId::RoleReview,
        }
    }

    /// Return the canonical path without query or fragment state.
    pub fn path(&self) -> String {
        match self {
            Self::Tenants => "/platform/tenants".to_string(),
            Self::Tenant { tenant_id } => {
                format!("/platform/tenants/{}", tenant_id.as_str())
            }
            Self::Projections => "/platform/projections".to_string(),
            Self::Backups => "/platform/backups".to_string(),
            Self::Integrations => "/platform/integrations".to_string(),
            Self::Audit => "/platform/audit".to_string(),
            Self::Settings => "/platform/settings".to_string(),
            Self::RoleReview => "/platform/role-review".to_string(),
        }
    }
}

impl fmt::Display for PlatformOperationsRoute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.path())
    }
}

/// A cheap, target-neutral handle to the Platform Operations route registry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlatformOperationsRouteRegistry;

impl PlatformOperationsRouteRegistry {
    /// Return the canonical registry.
    pub const fn new() -> Self {
        Self
    }

    /// Return every allow-listed definition in canonical order.
    pub const fn definitions(self) -> &'static [PlatformOperationsRouteDefinition] {
        PLATFORM_OPERATIONS_ROUTES
    }

    /// Resolve a normalized, credential-free deep link against the exact segment allow-list.
    pub fn resolve(
        self,
        path: &str,
    ) -> Result<PlatformOperationsRoute, PlatformOperationsRouteError> {
        let normalized = normalize_deep_link(&redact_deep_link(path));
        let segments: Vec<&str> = normalized
            .trim_matches('/')
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect();

        match segments.as_slice() {
            ["platform", "tenants"] => Ok(PlatformOperationsRoute::Tenants),
            ["platform", "tenants", tenant_id] => PlatformTenantId::new(*tenant_id)
                .map(|tenant_id| PlatformOperationsRoute::Tenant { tenant_id })
                .map_err(|_| PlatformOperationsRouteError::new(normalized)),
            ["platform", "projections"] => Ok(PlatformOperationsRoute::Projections),
            ["platform", "backups"] => Ok(PlatformOperationsRoute::Backups),
            ["platform", "integrations"] => Ok(PlatformOperationsRoute::Integrations),
            ["platform", "audit"] => Ok(PlatformOperationsRoute::Audit),
            ["platform", "settings"] => Ok(PlatformOperationsRoute::Settings),
            ["platform", "role-review"] => Ok(PlatformOperationsRoute::RoleReview),
            _ => Err(PlatformOperationsRouteError::new(normalized)),
        }
    }

    /// Resolve only the stable identifier for a deep link.
    pub fn resolve_id(
        self,
        path: &str,
    ) -> Result<PlatformOperationsRouteId, PlatformOperationsRouteError> {
        self.resolve(path).map(|route| route.id())
    }

    /// Return the allow-listed definition for a stable route identifier.
    pub fn definition(self, id: PlatformOperationsRouteId) -> PlatformOperationsRouteDefinition {
        self.definitions()
            .iter()
            .find(|definition| definition.id == id)
            .copied()
            .expect("every Platform Operations route ID has a definition")
    }
}

/// Error returned when a deep link is outside the Platform Operations allow-list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformOperationsRouteError {
    /// Safe normalized path retained for diagnostics; credentials are never retained.
    pub normalized_path: String,
}

impl PlatformOperationsRouteError {
    fn new(normalized_path: String) -> Self {
        Self { normalized_path }
    }
}

impl fmt::Display for PlatformOperationsRouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "no ERPX Platform Operations route matches {}",
            self.normalized_path
        )
    }
}

impl Error for PlatformOperationsRouteError {}

fn is_safe_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|character| !character.is_control() && !character.is_whitespace())
        && !value.contains(['/', '\\', '?', '#'])
}

fn contains_sensitive_marker(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "access_token=",
        "refresh_token=",
        "password=",
        "authorization:",
        "bearer ",
        "database_url=",
        "client_secret=",
        "api_key=",
    ]
    .iter()
    .any(|marker| value.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::{
        PLATFORM_OPERATIONS_ROUTES, PlatformOperationsAudience, PlatformOperationsPageState,
        PlatformOperationsProjectionMetadata, PlatformOperationsRoute, PlatformOperationsRouteId,
        PlatformOperationsRouteRegistry, PlatformTenantId,
    };
    use crate::{Locale, ProjectionFreshness, RouteAuthRequirement, SafeUiText, UiStateKind};

    #[test]
    fn registry_contains_exact_platform_operations_allow_list() {
        let expected = [
            (PlatformOperationsRouteId::Tenants, "/platform/tenants"),
            (PlatformOperationsRouteId::Tenant, "/platform/tenants/:id"),
            (
                PlatformOperationsRouteId::Projections,
                "/platform/projections",
            ),
            (PlatformOperationsRouteId::Backups, "/platform/backups"),
            (
                PlatformOperationsRouteId::Integrations,
                "/platform/integrations",
            ),
            (PlatformOperationsRouteId::Audit, "/platform/audit"),
            (PlatformOperationsRouteId::Settings, "/platform/settings"),
            (
                PlatformOperationsRouteId::RoleReview,
                "/platform/role-review",
            ),
        ];
        let registry = PlatformOperationsRouteRegistry::new();

        assert_eq!(registry.definitions().len(), expected.len());
        assert_eq!(PLATFORM_OPERATIONS_ROUTES, registry.definitions());
        for ((expected_id, expected_template), definition) in
            expected.into_iter().zip(registry.definitions())
        {
            assert_eq!(definition.id, expected_id);
            assert_eq!(definition.template, expected_template);
            assert_eq!(definition.audience, PlatformOperationsAudience::Platform);
            assert_eq!(definition.auth, RouteAuthRequirement::Authenticated);
        }
    }

    #[test]
    fn matching_is_exact_and_segment_aware() {
        let registry = PlatformOperationsRouteRegistry::new();

        assert!(matches!(
            registry.resolve("/platform/tenants"),
            Ok(PlatformOperationsRoute::Tenants)
        ));
        assert!(matches!(
            registry.resolve("/platform/tenants/tenant_123"),
            Ok(PlatformOperationsRoute::Tenant { .. })
        ));
        assert!(matches!(
            registry.resolve("/platform/projections"),
            Ok(PlatformOperationsRoute::Projections)
        ));
        assert!(matches!(
            registry.resolve("/platform/role-review"),
            Ok(PlatformOperationsRoute::RoleReview)
        ));
        for path in [
            "/platform/tenants-extra",
            "/platform/tenants/tenant_123/extra",
            "/platform/projections-extra",
            "/platform/backups-old",
            "/platform/settings/child",
            "/platform/role-review/child",
            "/platformx/tenants",
        ] {
            assert!(
                registry.resolve(path).is_err(),
                "unexpected match for {path}"
            );
        }
    }

    #[test]
    fn labels_and_navigation_are_bilingual() {
        let registry = PlatformOperationsRouteRegistry::new();
        for definition in registry.definitions() {
            let english = definition.label_for(Locale::En);
            let thai = definition.label_for(Locale::Th);
            assert!(!english.is_empty());
            assert!(!thai.is_empty());
            assert_ne!(english, thai);

            let item = definition.nav_item(Locale::Th);
            assert_eq!(item.path, definition.template);
            assert_eq!(item.label, thai);
            assert!(!item.icon.is_empty());
        }
    }

    #[test]
    fn deep_links_normalize_query_hash_and_repeated_segments() {
        let registry = PlatformOperationsRouteRegistry::new();

        assert_eq!(
            registry
                .resolve("/#/platform//tenants///?tab=health#details")
                .expect("normalized tenant route"),
            PlatformOperationsRoute::Tenants
        );
        assert_eq!(
            registry
                .resolve("/platform/tenants/tenant_123/?from=platform#tenant")
                .expect("normalized tenant detail route")
                .path(),
            "/platform/tenants/tenant_123"
        );
        assert!(registry.resolve("/platform/../tenants").is_err());
    }

    #[test]
    fn credential_bearing_links_are_rejected_without_leaking_values() {
        let registry = PlatformOperationsRouteRegistry::new();
        for path in [
            "/platform/tenants?access_token=top-secret",
            "/platform/projections#refresh_token=top-secret",
            "/platform/backups?authorization=Bearer%20top-secret",
            "/platform/settings?client_secret=top-secret",
        ] {
            let error = registry
                .resolve(path)
                .expect_err("credential link must be rejected");
            assert_eq!(error.normalized_path, "/");
            assert!(!error.to_string().contains("top-secret"));
        }
    }

    #[test]
    fn platform_state_is_typed_bounded_and_never_authorizes_locally() {
        let tenant = PlatformTenantId::new("tenant_123").expect("tenant");
        assert!(PlatformTenantId::new("a".repeat(129)).is_err());
        assert!(PlatformTenantId::new("database_url=secret").is_err());
        let metadata =
            PlatformOperationsProjectionMetadata::fresh(Some(tenant), "2026-07-29T00:00:00Z");
        assert_eq!(metadata.freshness, ProjectionFreshness::Fresh);
        let state = PlatformOperationsPageState::<u8>::Ready { data: 1, metadata };
        assert_eq!(state.kind(), None);
        assert_eq!(state.data(), Some(&1));
        assert!(state.is_read_only());

        let denied = PlatformOperationsPageState::<u8>::Denied {
            metadata: None,
            reason: SafeUiText::new("platform authorization denied"),
        };
        assert_eq!(denied.kind(), Some(UiStateKind::Denied));
        assert!(denied.data().is_none());
        assert!(denied.is_read_only());
    }

    #[test]
    fn tenant_state_scope_checks_fail_closed_for_cached_cross_tenant_payloads() {
        let tenant = PlatformTenantId::new("tenant_123").expect("tenant");
        let other = PlatformTenantId::new("tenant_other").expect("other tenant");
        let bound = PlatformOperationsProjectionMetadata::fresh(
            Some(tenant.clone()),
            "2026-07-29T00:00:00Z",
        );
        let mismatched =
            PlatformOperationsProjectionMetadata::fresh(Some(other), "2026-07-29T00:00:00Z");
        assert!(bound.is_bound_to_tenant(&tenant));
        assert!(!mismatched.is_bound_to_tenant(&tenant));
        assert!(
            PlatformOperationsPageState::<u8>::Ready {
                data: 1,
                metadata: bound.clone(),
            }
            .is_scope_bound_to_tenant(&tenant)
        );
        assert!(
            !PlatformOperationsPageState::<u8>::Ready {
                data: 1,
                metadata: mismatched.clone(),
            }
            .is_scope_bound_to_tenant(&tenant)
        );
        assert!(
            !PlatformOperationsPageState::<u8>::Offline {
                data: Some(1),
                metadata: Some(mismatched),
                reason: SafeUiText::new("offline"),
            }
            .is_scope_bound_to_tenant(&tenant)
        );
        assert!(
            PlatformOperationsPageState::<u8>::Loading { metadata: None }
                .is_scope_bound_to_tenant(&tenant)
        );
    }
}
