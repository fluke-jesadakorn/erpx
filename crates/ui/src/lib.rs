//! Shared, route-agnostic Dioxus UI foundations for ERPX clients.
//!
//! This crate owns presentation primitives and client-local interaction state. It does not
//! know about authentication tokens, ERPX protobuf messages, holding data, or server-side
//! authorization. Applications provide those concerns at their boundary.

use std::{collections::BTreeSet, fmt, sync::Arc};

use dioxus::prelude::*;
pub use erpx_console_transport_core::{
    FeatureTransportError, FeatureTransportPrimitive, OpaqueCredentialProvider,
};

mod auth;
mod enterprise;
mod hosted_pricing;
mod i18n;
mod navigation;
mod routes;
mod session_target;
mod state;
mod theme;
mod work;
mod workflows;

pub use auth::{
    AccessToken, ActiveRoleMetadata, AuthTransportError, ExpiryTimestamp, IssuedSession,
    LoginRequest, RefreshToken, SecretInput, SecureSessionBundleStore, SecureSessionError,
    SecureSessionFuture, SecureSessionStore, SessionCommandError, SessionController, SessionFuture,
    SessionId, SessionMaterialError, SessionSnapshot, SessionState, SessionTransport,
    StoredAccountSummary, StoredSession, StoredSessionBundle,
};
pub use enterprise::{
    AdaptiveDensity, CanonicalAppRoute, CanonicalRouteError, ClientFormFactor, EnterpriseHubRoute,
    EnterpriseState, EnterpriseStateKind, MutationMode, REFERENCE_ITEM_TEMPLATE,
    canonical_workspace_navigation, workspace_label, workspace_supports_channel,
};
pub use hosted_pricing::{HostedPlanView, HostedQuoteView, HostedSeatStepView};
pub use i18n::{
    Locale, LocaleContext, LocaleFormatter, LocalizedText, TranslationKey, provide_locale,
    use_locale,
};
pub use navigation::{
    NavAccess, NavGroup, NavItem, NavigationState, localized_navigation_label,
    localized_navigation_requirement, localized_role_label, normalize_path,
};
pub use routes::{
    CustomerId, CustomerView, HoldingId, ImpersonationMetadata, InvalidCustomerId,
    InvalidHoldingId, MAX_CUSTOMER_ID_BYTES, MAX_HOLDING_ID_BYTES, PLATFORM_HOLDING_ROUTES,
    PlatformHoldingView, PlatformRoute, ReadOnlyMetadata, RouteAuthRequirement, RouteDefinition,
    RouteId, RouteParseError, RouteRegistry, RouteState, SafeSessionMetadata, ScopeKind,
    ScopeOption, SupportAccessMetadata, normalize_deep_link, redact_deep_link,
};
pub use session_target::{
    ClientTarget, SecureSessionSupport, TargetSecureSessionAdapter, TargetSessionDescriptor,
};
pub use state::{
    FormState, ProjectionFreshness, SafeUiText, UiAccessibilityContract, UiState, UiStateKind,
    ValidationError, validate_email, validate_required,
};
pub use theme::{
    CorporateIdentityContext, ThemeContext, ThemeMode, corporate_identity_style,
    provide_corporate_identity, provide_theme, use_corporate_identity, use_theme,
};
pub use work::{
    MAX_WORK_ACTION_CAPABILITIES, MAX_WORK_ITEMS, MAX_WORK_PAGE_SIZE, MAX_WORK_REFERENCE_BYTES,
    MAX_WORK_SOURCE_CONTRIBUTIONS, MAX_WORK_TEXT_BYTES, ReadModelAvailability, ReadModelFreshness,
    WorkDomain, WorkItemKind, WorkItemReadModel, WorkItemSeverity, WorkItemStage, WorkItemTarget,
    WorkQueueCounters, WorkQueueMeta, WorkQueueReadModel, WorkQueueView, WorkRoute, WorkSlaState,
    WorkSourceContribution, sla_state, valid_work_cursor,
};
pub use workflows::{
    HUMAN_UI_STATES, HumanAdapterKind, HumanUiState, RPC_WORKFLOW_SERVICES, RpcClassification,
    RpcServiceWorkflow, RpcWorkflow, rpc_workflow,
};

/// Route metadata exported by one independently compiled Console feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureRouteDescriptor {
    pub id: &'static str,
    pub canonical_path: &'static str,
    pub authorization: &'static str,
    pub navigation_group: &'static str,
}

/// Non-secret session identity supplied by the Console shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureSession {
    pub authenticated: bool,
    pub principal_ref: Option<String>,
}

/// Exact active authority scope supplied by the Console shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureScope {
    pub kind: String,
    pub reference: String,
}

pub type FeatureNavigationCallback = Arc<dyn Fn(String) + Send + Sync>;

/// Target-neutral context shared with a registered feature renderer.
#[derive(Clone)]
pub struct FeatureContext {
    /// Stable authorization/session generation used to invalidate feature-local resources.
    pub generation: u64,
    pub locale: Locale,
    pub session: FeatureSession,
    pub active_scope: Option<FeatureScope>,
    pub transport: Arc<FeatureTransportPrimitive>,
    pub navigate: FeatureNavigationCallback,
    pub canonical_path: String,
}

impl PartialEq for FeatureContext {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation
            && self.locale == other.locale
            && self.session == other.session
            && self.active_scope == other.active_scope
            && self.canonical_path == other.canonical_path
            && Arc::ptr_eq(&self.transport, &other.transport)
            && Arc::ptr_eq(&self.navigate, &other.navigate)
    }
}

impl Eq for FeatureContext {}

impl fmt::Debug for FeatureContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FeatureContext")
            .field("generation", &self.generation)
            .field("locale", &self.locale)
            .field("session", &self.session)
            .field("active_scope", &self.active_scope)
            .field("transport", &"<type-erased>")
            .field("navigate", &"<callback>")
            .field("canonical_path", &self.canonical_path)
            .finish()
    }
}

pub type FeaturePathMatcher = fn(&FeatureRegistration, &str) -> bool;
pub type FeatureRenderer = fn(&FeatureContext) -> Element;

#[derive(Debug, Clone, Copy)]
pub struct FeatureRegistration {
    pub id: &'static str,
    /// Stable boundary key retained by the feature registry and rendered surface.
    pub artifact_key: &'static str,
    pub routes: &'static [FeatureRouteDescriptor],
    pub path_matcher: FeaturePathMatcher,
    pub renderer: FeatureRenderer,
}

impl FeatureRegistration {
    pub const fn new(id: &'static str, routes: &'static [FeatureRouteDescriptor]) -> Self {
        Self::with_renderer_and_artifact(id, routes, id, default_feature_renderer)
    }

    pub const fn with_renderer(
        id: &'static str,
        routes: &'static [FeatureRouteDescriptor],
        renderer: FeatureRenderer,
    ) -> Self {
        Self::with_renderer_and_artifact(id, routes, id, renderer)
    }

    pub const fn with_renderer_and_artifact(
        id: &'static str,
        routes: &'static [FeatureRouteDescriptor],
        artifact_key: &'static str,
        renderer: FeatureRenderer,
    ) -> Self {
        Self {
            id,
            artifact_key,
            routes,
            path_matcher: default_feature_path_matcher,
            renderer,
        }
    }

    pub fn matches_path(&self, path: &str) -> bool {
        (self.path_matcher)(self, path)
    }

    pub fn render(&self, context: &FeatureContext) -> Element {
        (self.renderer)(context)
    }
}

pub fn default_feature_path_matcher(registration: &FeatureRegistration, path: &str) -> bool {
    let normalized = normalize_path(path);
    registration.routes.iter().any(|route| {
        normalized == route.canonical_path
            || normalized
                .strip_prefix(route.canonical_path)
                .is_some_and(|suffix| suffix.starts_with('/'))
    })
}

pub fn default_feature_renderer(_context: &FeatureContext) -> Element {
    VNode::empty()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeatureRegistryError {
    DuplicateFeatureId(&'static str),
    DuplicateArtifactKey(&'static str),
    DuplicateRouteId(&'static str),
    DuplicateCanonicalPath(&'static str),
}

#[derive(Debug, Clone)]
pub struct FeatureRegistry {
    registrations: Vec<&'static FeatureRegistration>,
}

impl FeatureRegistry {
    pub fn try_new(
        registrations: impl IntoIterator<Item = &'static FeatureRegistration>,
    ) -> Result<Self, FeatureRegistryError> {
        let mut feature_ids = BTreeSet::new();
        let mut artifact_keys = BTreeSet::new();
        let mut route_ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        let registrations = registrations
            .into_iter()
            .map(|registration| {
                if !feature_ids.insert(registration.id) {
                    return Err(FeatureRegistryError::DuplicateFeatureId(registration.id));
                }
                if !artifact_keys.insert(registration.artifact_key) {
                    return Err(FeatureRegistryError::DuplicateArtifactKey(
                        registration.artifact_key,
                    ));
                }
                for route in registration.routes {
                    if !route_ids.insert(route.id) {
                        return Err(FeatureRegistryError::DuplicateRouteId(route.id));
                    }
                    if !paths.insert(route.canonical_path) {
                        return Err(FeatureRegistryError::DuplicateCanonicalPath(
                            route.canonical_path,
                        ));
                    }
                }
                Ok(registration)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { registrations })
    }

    pub fn registrations(&self) -> &[&'static FeatureRegistration] {
        &self.registrations
    }

    pub fn resolve(&self, path: &str) -> Option<&'static FeatureRegistration> {
        self.registrations
            .iter()
            .copied()
            .find(|registration| registration.matches_path(path))
    }
}

#[macro_export]
macro_rules! declare_feature_registration {
    (
        $surface:ident,
        $feature_id:literal,
        $route_id:literal,
        $canonical_path:literal,
        $navigation_group:literal,
        $artifact_key:literal,
        $framework:ident
    ) => {
        use $framework::prelude::*;

        const FEATURE_ROUTES: &[$crate::FeatureRouteDescriptor] =
            &[$crate::FeatureRouteDescriptor {
                id: $route_id,
                canonical_path: $canonical_path,
                authorization: "authenticated",
                navigation_group: $navigation_group,
            }];

        #[$framework::prelude::component]
        pub fn $surface(context: $crate::FeatureContext) -> $framework::prelude::Element {
            let canonical_path = context.canonical_path.clone();
            let generation = context.generation.to_string();
            // The component observes only the target transport mode. Credentials remain behind
            // the target adapter and server authorization stays authoritative.
            let transport_mode = if context.transport.is_web_same_origin() {
                "same-origin-bff"
            } else {
                "native-opaque-credential"
            };
            let (heading, status) = match context.locale {
                $crate::Locale::En => (
                    concat!($feature_id, " workspace"),
                    "The feature workspace is ready for authorized work.",
                ),
                $crate::Locale::Th => (
                    concat!("พื้นที่ทำงาน ", $feature_id),
                    "พื้นที่ทำงานฟีเจอร์พร้อมสำหรับงานที่ได้รับอนุญาต",
                ),
            };
            $framework::prelude::rsx! {
                section {
                    class: "feature-owned-surface",
                    role: "region",
                    "aria-labelledby": concat!("feature-title-", $feature_id),
                    "data-erpx-feature-artifact": $artifact_key,
                    "data-erpx-feature-id": $feature_id,
                    "data-feature-path": canonical_path,
                    "data-feature-generation": generation,
                    "data-feature-transport": transport_mode,
                    header { class: "feature-owned-surface__header",
                        p { class: "work-os-eyebrow", {$navigation_group} }
                        h2 { id: concat!("feature-title-", $feature_id), {heading} }
                    }
                    p { role: "status", aria_live: "polite", {status} }
                }
            }
        }

        pub use $surface as FeatureSurface;

        fn render_feature_surface(
            context: &$crate::FeatureContext,
        ) -> $framework::prelude::Element {
            $framework::prelude::rsx! { $surface { context: context.clone() } }
        }

        pub static REGISTRATION: $crate::FeatureRegistration =
            $crate::FeatureRegistration::with_renderer_and_artifact(
                $feature_id,
                FEATURE_ROUTES,
                $artifact_key,
                render_feature_surface,
            );
    };
}

#[cfg(test)]
mod feature_registry_tests {
    use super::*;

    fn feature_context(generation: u64) -> FeatureContext {
        FeatureContext {
            generation,
            locale: Locale::En,
            session: FeatureSession {
                authenticated: true,
                principal_ref: Some("principal-safe-ref".into()),
            },
            active_scope: Some(FeatureScope {
                kind: "holding".into(),
                reference: "holding-safe-ref".into(),
            }),
            transport: Arc::new(FeatureTransportPrimitive::web_same_origin()),
            navigate: Arc::new(|_| {}),
            canonical_path: "/a".into(),
        }
    }

    static ROUTE_A: &[FeatureRouteDescriptor] = &[FeatureRouteDescriptor {
        id: "a.route",
        canonical_path: "/a",
        authorization: "authenticated",
        navigation_group: "a",
    }];
    static ROUTE_DUPLICATE_ID: &[FeatureRouteDescriptor] = &[FeatureRouteDescriptor {
        id: "a.route",
        canonical_path: "/b",
        authorization: "authenticated",
        navigation_group: "b",
    }];
    static ROUTE_DUPLICATE_PATH: &[FeatureRouteDescriptor] = &[FeatureRouteDescriptor {
        id: "b.route",
        canonical_path: "/a",
        authorization: "authenticated",
        navigation_group: "b",
    }];
    static A: FeatureRegistration = FeatureRegistration::new("a", ROUTE_A);
    static DUPLICATE_FEATURE: FeatureRegistration =
        FeatureRegistration::new("a", ROUTE_DUPLICATE_PATH);
    static DUPLICATE_ROUTE: FeatureRegistration = FeatureRegistration::new("b", ROUTE_DUPLICATE_ID);
    static DUPLICATE_PATH: FeatureRegistration =
        FeatureRegistration::new("b", ROUTE_DUPLICATE_PATH);
    static DUPLICATE_ARTIFACT: FeatureRegistration =
        FeatureRegistration::with_renderer_and_artifact(
            "b",
            ROUTE_DUPLICATE_PATH,
            "a",
            default_feature_renderer,
        );

    #[test]
    fn feature_registry_rejects_duplicate_identifiers_and_paths() {
        assert_eq!(
            FeatureRegistry::try_new([&A, &DUPLICATE_FEATURE]).unwrap_err(),
            FeatureRegistryError::DuplicateFeatureId("a")
        );
        assert_eq!(
            FeatureRegistry::try_new([&A, &DUPLICATE_ROUTE]).unwrap_err(),
            FeatureRegistryError::DuplicateRouteId("a.route")
        );
        assert_eq!(
            FeatureRegistry::try_new([&A, &DUPLICATE_PATH]).unwrap_err(),
            FeatureRegistryError::DuplicateCanonicalPath("/a")
        );
        assert_eq!(
            FeatureRegistry::try_new([&A, &DUPLICATE_ARTIFACT]).unwrap_err(),
            FeatureRegistryError::DuplicateArtifactKey("a")
        );
    }

    #[test]
    fn feature_context_identity_uses_generation_and_pointer_identity() {
        let context = feature_context(7);
        let clone = context.clone();
        assert_eq!(context, clone);

        let mut next_generation = context.clone();
        next_generation.generation = 8;
        assert_ne!(context, next_generation);

        let mut next_transport = context.clone();
        next_transport.transport = Arc::new(FeatureTransportPrimitive::web_same_origin());
        assert_ne!(context, next_transport);
    }
}
