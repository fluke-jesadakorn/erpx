//! Target-neutral metadata for the Platform Operations routes.
//!
//! This module describes navigation and deep-link shape only. It deliberately contains no
//! holding data and does not evaluate the platform authorization requirement; those concerns stay
//! at the consuming application and server boundaries.

use std::{error::Error, fmt};

pub use erpx_domain::{
    contracts::HoldingId as PlatformHoldingId, data_plane::BusinessUnitId as HoldingWorkspaceModule,
};

use erpx_ui_core::{
    Locale, NavItem, ProjectionFreshness, RouteAuthRequirement, SafeUiText, TranslationKey,
    UiStateKind, normalize_deep_link, redact_deep_link,
};

/// Maximum bytes retained for a Platform Operations holding route identifier.
pub const MAX_PLATFORM_HOLDING_ID_BYTES: usize = 36;

/// The audience for a Platform Operations route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformOperationsAudience {
    /// ERPX platform operators and administrators.
    Platform,
}

/// Stable identifiers for the Platform Operations route allow-list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformOperationsRouteId {
    /// Holding list.
    Holdings,
    /// A single holding's platform operations view.
    Holding,
    /// A holding-bound module workspace entered by a platform administrator.
    HoldingWorkspace,
    /// Projection health and rebuild controls.
    Projections,
    /// Backup and restore history.
    Backups,
    /// Provider connector registry and connector detail.
    Providers,
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
    /// Canonical route template. `:id` is an opaque holding identifier segment.
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
        id: PlatformOperationsRouteId::Holdings,
        template: "/platform/holdings",
        label: TranslationKey::PlatformHoldings,
        audience: PlatformOperationsAudience::Platform,
        auth: RouteAuthRequirement::Authenticated,
        icon: "▦",
    },
    PlatformOperationsRouteDefinition {
        id: PlatformOperationsRouteId::Holding,
        template: "/platform/holdings/:id",
        label: TranslationKey::PlatformHolding,
        audience: PlatformOperationsAudience::Platform,
        auth: RouteAuthRequirement::Authenticated,
        icon: "▤",
    },
    PlatformOperationsRouteDefinition {
        id: PlatformOperationsRouteId::HoldingWorkspace,
        template: "/platform/holdings/:id/workspace/:module",
        label: TranslationKey::PlatformHolding,
        audience: PlatformOperationsAudience::Platform,
        auth: RouteAuthRequirement::Authenticated,
        icon: "▣",
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
        id: PlatformOperationsRouteId::Providers,
        template: "/platform/provider",
        label: TranslationKey::PlatformProviders,
        audience: PlatformOperationsAudience::Platform,
        auth: RouteAuthRequirement::Authenticated,
        icon: "⌁",
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

fn platform_holding_id(value: &str) -> Result<PlatformHoldingId, PlatformOperationsRouteError> {
    let value = uuid::Uuid::parse_str(value)
        .map_err(|_| PlatformOperationsRouteError::new("/".to_string()))?;
    PlatformHoldingId::try_from_uuid(value)
        .map_err(|_| PlatformOperationsRouteError::new("/".to_string()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldingWorkspaceViewerKind {
    HoldingUser,
    PlatformHoldingAdministrator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldingWorkspaceBillingMode {
    Writable,
    Grace,
    ReadOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoldingWorkspaceOrganizationScope {
    Headquarters {
        holding_id: SafeUiText,
    },
    Company {
        holding_id: SafeUiText,
        company_id: SafeUiText,
    },
    Branch {
        holding_id: SafeUiText,
        company_id: SafeUiText,
        branch_id: SafeUiText,
    },
}

/// Shared, display-safe module context. Authorization and consolidation stay
/// server-owned; the UI only carries the selected scope and opaque evidence.
#[derive(Clone, PartialEq, Eq)]
pub struct HoldingWorkspaceContext {
    pub holding_id: PlatformHoldingId,
    pub scope: HoldingWorkspaceOrganizationScope,
    pub viewer_kind: HoldingWorkspaceViewerKind,
    pub billing_mode: HoldingWorkspaceBillingMode,
    pub entitled_modules: Vec<HoldingWorkspaceModule>,
    /// Never rendered in UI diagnostics. Targets pass it only through the
    /// protected transport metadata boundary.
    administrative_context_id: Option<SafeUiText>,
}

impl fmt::Debug for HoldingWorkspaceContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HoldingWorkspaceContext")
            .field("holding_id", &self.holding_id)
            .field("scope", &self.scope)
            .field("viewer_kind", &self.viewer_kind)
            .field("billing_mode", &self.billing_mode)
            .field("entitled_modules", &self.entitled_modules)
            .field(
                "administrative_context",
                &self
                    .administrative_context_id
                    .as_ref()
                    .map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

impl HoldingWorkspaceContext {
    pub fn entered_platform(
        holding_id: PlatformHoldingId,
        scope: HoldingWorkspaceOrganizationScope,
        billing_mode: HoldingWorkspaceBillingMode,
        entitled_modules: Vec<HoldingWorkspaceModule>,
        administrative_context_id: impl AsRef<str>,
    ) -> Option<Self> {
        let administrative_context_id = administrative_context_id.as_ref().trim();
        if administrative_context_id.is_empty() {
            return None;
        }
        Some(Self {
            holding_id,
            scope,
            viewer_kind: HoldingWorkspaceViewerKind::PlatformHoldingAdministrator,
            billing_mode,
            entitled_modules,
            administrative_context_id: Some(SafeUiText::new(administrative_context_id)),
        })
    }

    pub fn administrative_context_metadata(&self) -> Option<&str> {
        self.administrative_context_id
            .as_ref()
            .map(SafeUiText::as_str)
    }

    pub fn permits_module(&self, module: HoldingWorkspaceModule) -> bool {
        self.entitled_modules.contains(&module)
    }

    pub const fn is_read_only(&self) -> bool {
        matches!(self.billing_mode, HoldingWorkspaceBillingMode::ReadOnly)
    }
}

/// Holding administration detail tabs. `EnterWorkspace` remains distinct from
/// read-only administrative projections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformHoldingAdministrationTab {
    Overview,
    Infrastructure,
    Identity,
    CorporateIdentity,
    BusinessUnits,
    RootAccess,
    ConfigurationHistory,
    Organization,
    Subscription,
    Modules,
    UsersAccess,
    Health,
    Audit,
    EnterWorkspace,
}

impl PlatformHoldingAdministrationTab {
    pub const ALL: [Self; 14] = [
        Self::Overview,
        Self::Infrastructure,
        Self::Identity,
        Self::CorporateIdentity,
        Self::BusinessUnits,
        Self::RootAccess,
        Self::ConfigurationHistory,
        Self::Organization,
        Self::Subscription,
        Self::Modules,
        Self::UsersAccess,
        Self::Health,
        Self::Audit,
        Self::EnterWorkspace,
    ];
}

/// Safe metadata accompanying an authorized Platform Operations read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformOperationsProjectionMetadata {
    /// Holding binding for a holding detail/read; absent for platform-wide lists.
    pub holding_id: Option<PlatformHoldingId>,
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
        holding_id: Option<PlatformHoldingId>,
        projection_updated_at: impl AsRef<str>,
    ) -> Self {
        Self::present(
            holding_id,
            ProjectionFreshness::Fresh,
            projection_updated_at,
        )
    }

    /// Construct metadata for a stale but displayable platform read.
    pub fn stale(
        holding_id: Option<PlatformHoldingId>,
        projection_updated_at: impl AsRef<str>,
    ) -> Self {
        Self::present(
            holding_id,
            ProjectionFreshness::Stale,
            projection_updated_at,
        )
    }

    /// Construct metadata with an explicit freshness classification.
    pub fn present(
        holding_id: Option<PlatformHoldingId>,
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
            holding_id,
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

    /// Whether this metadata is bound to the requested holding route.
    pub fn is_bound_to_holding(&self, holding_id: &PlatformHoldingId) -> bool {
        self.holding_id.as_ref() == Some(holding_id)
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

    /// Fail closed when a holding detail state carries another holding's metadata or payload.
    /// States with no payload are safe without a holding binding; a cached payload is not.
    pub fn is_scope_bound_to_holding(&self, holding_id: &PlatformHoldingId) -> bool {
        match self {
            Self::Ready { metadata, .. } | Self::Stale { metadata, .. } => {
                metadata.is_bound_to_holding(holding_id)
            }
            Self::Offline {
                data: Some(_),
                metadata: Some(metadata),
                ..
            } => metadata.is_bound_to_holding(holding_id),
            Self::Offline { data: Some(_), .. } => false,
            Self::Loading { metadata } | Self::Denied { metadata, .. } => metadata
                .as_ref()
                .is_none_or(|metadata| metadata.is_bound_to_holding(holding_id)),
            Self::Empty { metadata } => metadata
                .holding_id
                .as_ref()
                .is_none_or(|bound| bound == holding_id),
            Self::Offline {
                data: None,
                metadata,
                ..
            } => metadata
                .as_ref()
                .is_none_or(|metadata| metadata.is_bound_to_holding(holding_id)),
            Self::Error { .. } => true,
        }
    }
}

/// A typed Platform Operations route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformOperationsRoute {
    /// Holding list.
    Holdings,
    /// A single holding route with a validated opaque identifier.
    Holding { holding_id: PlatformHoldingId },
    /// Reused module UI inside one entered administrative context.
    HoldingWorkspace {
        holding_id: PlatformHoldingId,
        module: HoldingWorkspaceModule,
    },
    /// Projection health and rebuild controls.
    Projections,
    /// Backup and restore history.
    Backups,
    /// Provider connector registry.
    Providers,
    /// One provider connector identified by an opaque, validated segment.
    Provider { connector_id: PlatformHoldingId },
    /// Integration health and configuration metadata.
    Integrations,
    /// Platform audit records.
    Audit,
    /// Platform settings.
    Settings,
    /// Contextual corporate identity editor under platform settings.
    CorporateIdentity,
    /// Local-only access simulation under platform settings.
    AccessSimulation,
    /// Expiring-role review and recovery workflow.
    RoleReview,
}

impl PlatformOperationsRoute {
    /// Return the stable route identifier.
    pub const fn id(&self) -> PlatformOperationsRouteId {
        match self {
            Self::Holdings => PlatformOperationsRouteId::Holdings,
            Self::Holding { .. } => PlatformOperationsRouteId::Holding,
            Self::HoldingWorkspace { .. } => PlatformOperationsRouteId::HoldingWorkspace,
            Self::Projections => PlatformOperationsRouteId::Projections,
            Self::Backups => PlatformOperationsRouteId::Backups,
            Self::Providers | Self::Provider { .. } => PlatformOperationsRouteId::Providers,
            Self::Integrations => PlatformOperationsRouteId::Integrations,
            Self::Audit => PlatformOperationsRouteId::Audit,
            Self::Settings | Self::CorporateIdentity | Self::AccessSimulation => {
                PlatformOperationsRouteId::Settings
            }
            Self::RoleReview => PlatformOperationsRouteId::RoleReview,
        }
    }

    /// Return the canonical path without query or fragment state.
    pub fn path(&self) -> String {
        match self {
            Self::Holdings => "/platform/holdings".to_string(),
            Self::Holding { holding_id } => {
                format!("/platform/holdings/{holding_id}")
            }
            Self::HoldingWorkspace { holding_id, module } => format!(
                "/platform/holdings/{}/workspace/{}",
                holding_id,
                module.as_str()
            ),
            Self::Projections => "/platform/projections".to_string(),
            Self::Backups => "/platform/backups".to_string(),
            Self::Providers => "/platform/provider".to_string(),
            Self::Provider { connector_id } => {
                format!("/platform/provider/{connector_id}")
            }
            Self::Integrations => "/platform/integrations".to_string(),
            Self::Audit => "/platform/audit".to_string(),
            Self::Settings => "/platform/settings".to_string(),
            Self::CorporateIdentity => "/platform/settings/corporate-identity".to_string(),
            Self::AccessSimulation => "/platform/settings/access-simulation".to_string(),
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
            ["platform", "holdings"] => Ok(PlatformOperationsRoute::Holdings),
            ["platform", "holdings", holding_id] => platform_holding_id(holding_id)
                .map(|holding_id| PlatformOperationsRoute::Holding { holding_id })
                .map_err(|_| PlatformOperationsRouteError::new(normalized)),
            ["platform", "holdings", holding_id, "workspace", module] => {
                let holding_id = platform_holding_id(holding_id)
                    .map_err(|_| PlatformOperationsRouteError::new(normalized.clone()))?;
                let module = HoldingWorkspaceModule::parse(module)
                    .map_err(|_| PlatformOperationsRouteError::new(normalized.clone()))?;
                Ok(PlatformOperationsRoute::HoldingWorkspace { holding_id, module })
            }
            ["platform", "projections"] => Ok(PlatformOperationsRoute::Projections),
            ["platform", "backups"] => Ok(PlatformOperationsRoute::Backups),
            ["platform", "provider"] => Ok(PlatformOperationsRoute::Providers),
            ["platform", "provider", connector_id] => platform_holding_id(connector_id)
                .map(|connector_id| PlatformOperationsRoute::Provider { connector_id })
                .map_err(|_| PlatformOperationsRouteError::new(normalized)),
            ["platform", "integrations"] => Ok(PlatformOperationsRoute::Integrations),
            ["platform", "audit"] => Ok(PlatformOperationsRoute::Audit),
            ["platform", "settings"] => Ok(PlatformOperationsRoute::Settings),
            ["platform", "settings", "corporate-identity"] => {
                Ok(PlatformOperationsRoute::CorporateIdentity)
            }
            ["platform", "settings", "access-simulation"] => {
                Ok(PlatformOperationsRoute::AccessSimulation)
            }
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

#[cfg(test)]
mod tests {
    use super::{
        HoldingWorkspaceBillingMode, HoldingWorkspaceContext, HoldingWorkspaceModule,
        HoldingWorkspaceOrganizationScope, PLATFORM_OPERATIONS_ROUTES, PlatformOperationsAudience,
        PlatformOperationsPageState, PlatformOperationsProjectionMetadata, PlatformOperationsRoute,
        PlatformOperationsRouteId, PlatformOperationsRouteRegistry, platform_holding_id,
    };
    use erpx_ui_core::{
        Locale, ProjectionFreshness, RouteAuthRequirement, SafeUiText, UiStateKind,
    };

    #[test]
    fn registry_contains_exact_platform_operations_allow_list() {
        let expected = [
            (PlatformOperationsRouteId::Holdings, "/platform/holdings"),
            (PlatformOperationsRouteId::Holding, "/platform/holdings/:id"),
            (
                PlatformOperationsRouteId::HoldingWorkspace,
                "/platform/holdings/:id/workspace/:module",
            ),
            (
                PlatformOperationsRouteId::Projections,
                "/platform/projections",
            ),
            (PlatformOperationsRouteId::Backups, "/platform/backups"),
            (PlatformOperationsRouteId::Providers, "/platform/provider"),
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
            registry.resolve("/platform/holdings"),
            Ok(PlatformOperationsRoute::Holdings)
        ));
        assert!(matches!(
            registry.resolve("/platform/holdings/00000000-0000-4000-8000-000000000123"),
            Ok(PlatformOperationsRoute::Holding { .. })
        ));
        assert!(matches!(
            registry.resolve("/platform/projections"),
            Ok(PlatformOperationsRoute::Projections)
        ));
        assert!(matches!(
            registry.resolve("/platform/provider"),
            Ok(PlatformOperationsRoute::Providers)
        ));
        assert!(matches!(
            registry.resolve("/platform/provider/00000000-0000-4000-8000-000000000125"),
            Ok(PlatformOperationsRoute::Provider { .. })
        ));
        assert!(matches!(
            registry.resolve(
                "/platform/holdings/00000000-0000-4000-8000-000000000123/workspace/finance"
            ),
            Ok(PlatformOperationsRoute::HoldingWorkspace {
                module: HoldingWorkspaceModule::Finance,
                ..
            })
        ));
        assert!(matches!(
            registry.resolve("/platform/role-review"),
            Ok(PlatformOperationsRoute::RoleReview)
        ));
        assert!(matches!(
            registry.resolve("/platform/settings/corporate-identity"),
            Ok(PlatformOperationsRoute::CorporateIdentity)
        ));
        assert!(matches!(
            registry.resolve("/platform/settings/access-simulation"),
            Ok(PlatformOperationsRoute::AccessSimulation)
        ));
        for path in [
            "/platform/holdings-extra",
            "/platform/holdings/00000000-0000-4000-8000-000000000123/extra",
            "/platform/holdings/00000000-0000-4000-8000-000000000123/workspace/unknown",
            "/platform/holdings/00000000-0000-4000-8000-000000000123/workspace/finance/extra",
            "/platform/projections-extra",
            "/platform/backups-old",
            "/platform/provider/connector_123/extra",
            "/platform/settings/child",
            "/platform/settings/access-simulation/child",
            "/platform/role-review/child",
            "/platformx/holdings",
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
                .resolve("/#/platform//holdings///?tab=health#details")
                .expect("normalized holding route"),
            PlatformOperationsRoute::Holdings
        );
        assert_eq!(
            registry
                .resolve(
                    "/platform/holdings/00000000-0000-4000-8000-000000000123/?from=platform#holding"
                )
                .expect("normalized holding detail route")
                .path(),
            "/platform/holdings/00000000-0000-4000-8000-000000000123"
        );
        assert!(registry.resolve("/platform/../holdings").is_err());
    }

    #[test]
    fn credential_bearing_links_are_rejected_without_leaking_values() {
        let registry = PlatformOperationsRouteRegistry::new();
        for path in [
            "/platform/holdings?access_token=top-secret",
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
        let holding = platform_holding_id("00000000-0000-4000-8000-000000000123").expect("holding");
        assert!(platform_holding_id(&"a".repeat(129)).is_err());
        assert!(platform_holding_id("database_url=secret").is_err());
        let metadata =
            PlatformOperationsProjectionMetadata::fresh(Some(holding), "2026-07-29T00:00:00Z");
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
    fn holding_state_scope_checks_fail_closed_for_cached_cross_holding_payloads() {
        let holding = platform_holding_id("00000000-0000-4000-8000-000000000123").expect("holding");
        let other =
            platform_holding_id("00000000-0000-4000-8000-000000000124").expect("other holding");
        let bound =
            PlatformOperationsProjectionMetadata::fresh(Some(holding), "2026-07-29T00:00:00Z");
        let mismatched =
            PlatformOperationsProjectionMetadata::fresh(Some(other), "2026-07-29T00:00:00Z");
        assert!(bound.is_bound_to_holding(&holding));
        assert!(!mismatched.is_bound_to_holding(&holding));
        assert!(
            PlatformOperationsPageState::<u8>::Ready {
                data: 1,
                metadata: bound.clone(),
            }
            .is_scope_bound_to_holding(&holding)
        );
        assert!(
            !PlatformOperationsPageState::<u8>::Ready {
                data: 1,
                metadata: mismatched.clone(),
            }
            .is_scope_bound_to_holding(&holding)
        );
        assert!(
            !PlatformOperationsPageState::<u8>::Offline {
                data: Some(1),
                metadata: Some(mismatched),
                reason: SafeUiText::new("offline"),
            }
            .is_scope_bound_to_holding(&holding)
        );
        assert!(
            PlatformOperationsPageState::<u8>::Loading { metadata: None }
                .is_scope_bound_to_holding(&holding)
        );
    }

    #[test]
    fn entered_workspace_is_holding_bound_module_allow_listed_and_context_backed() {
        let registry = PlatformOperationsRouteRegistry::new();
        let route = registry
            .resolve("/platform/holdings/00000000-0000-4000-8000-000000000123/workspace/finance")
            .expect("entered workspace route");
        assert_eq!(
            route.path(),
            "/platform/holdings/00000000-0000-4000-8000-000000000123/workspace/finance"
        );
        let holding_id =
            platform_holding_id("00000000-0000-4000-8000-000000000123").expect("holding");
        let context = HoldingWorkspaceContext::entered_platform(
            holding_id,
            HoldingWorkspaceOrganizationScope::Headquarters {
                holding_id: SafeUiText::new("holding-123"),
            },
            HoldingWorkspaceBillingMode::ReadOnly,
            vec![HoldingWorkspaceModule::Finance],
            "opaque-admin-context",
        )
        .expect("administrative context");
        assert!(context.permits_module(HoldingWorkspaceModule::Finance));
        assert!(!context.permits_module(HoldingWorkspaceModule::Sales));
        assert!(context.is_read_only());
        assert_eq!(
            context.administrative_context_metadata(),
            Some("opaque-admin-context")
        );
        let debug = format!("{context:?}");
        assert!(!debug.contains("opaque-admin-context"));
        assert!(debug.contains("[REDACTED]"));
    }
}
