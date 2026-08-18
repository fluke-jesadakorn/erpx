//! Renderer-neutral services used by the console composition root.
//!
//! Native builds use Dioxus History and Rust-owned adapters. They never fall
//! back to a browser history, WebView bridge, or plaintext credential store.

use std::{cell::RefCell, collections::BTreeMap, fmt, future::Future, pin::Pin, rc::Rc};

use dioxus::{history::MemoryHistory, prelude::History};

pub mod credentials;
pub mod external;
pub mod http;
pub mod native;
pub mod navigation;
pub mod passkey;
pub mod preferences;

/// Client environment selected by the composition root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientTarget {
    Web,
    Server,
    MacOs,
    Windows,
    Linux,
    Ios,
    Android,
}

/// Rendering backend selected for the current build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererKind {
    WebDom,
    ServerSsr,
    NativeGpu,
}

/// Truthful availability of a platform service for the selected client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityStatus {
    Available,
    Unsupported,
    Deferred,
    Blocked(&'static str),
}

impl CapabilityStatus {
    pub const fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }
}

/// Platform behavior that UI components may expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformCapabilities {
    pub target: ClientTarget,
    pub renderer: RendererKind,
    pub navigation: CapabilityStatus,
    pub secure_storage: CapabilityStatus,
    pub passkey: CapabilityStatus,
    pub deep_link: CapabilityStatus,
    pub external_url: CapabilityStatus,
}

/// Network endpoints accepted by native clients. Construction and validation
/// live in the typed HTTP adapter; this value never contains credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeEndpointConfig {
    pub auth_http: String,
    pub public_http: String,
    pub grpc: String,
}

/// Common interface shape implemented by native platform adapters. Future
/// targets keep compiling against this boundary while returning `Deferred`.
pub trait NativePlatformBindings {
    fn target(&self) -> ClientTarget;
    fn capability(&self, service: NativeCapability) -> CapabilityStatus;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeCapability {
    Http,
    SecureStorage,
    Navigation,
    Preferences,
    ExternalAction,
    Passkey,
}

/// A route rejected before it reaches the authoritative route registries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsafeDeepLink;

impl fmt::Display for UnsafeDeepLink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("deep link is not a safe local console path")
    }
}

impl std::error::Error for UnsafeDeepLink {}

/// Validate the transport-level shape of an incoming app link.
///
/// Feature route registries remain authoritative after this coarse safety
/// boundary. Query strings are intentionally rejected because deep links may
/// not carry credentials or other ambient state.
pub fn validate_deep_link(path: &str) -> Result<String, UnsafeDeepLink> {
    let normalized = path.trim();
    let lowercase = normalized.to_ascii_lowercase();
    let unsafe_encoding = ["%2f", "%5c", "%00", "%0a", "%0d"]
        .iter()
        .any(|needle| lowercase.contains(needle));
    let credential_hint = ["token", "secret", "password", "credential"]
        .iter()
        .any(|needle| lowercase.contains(needle));
    if normalized.is_empty()
        || !normalized.starts_with('/')
        || normalized.starts_with("//")
        || normalized.contains(['\\', '?', '#', '\0'])
        || unsafe_encoding
        || credential_hint
        || normalized.len() > 2_048
    {
        return Err(UnsafeDeepLink);
    }
    Ok(normalized.to_owned())
}

/// Renderer-provided navigation and native app-link ingress.
pub trait NavigationHost {
    fn current_path(&self) -> String;
    fn push(&self, path: &str) -> Result<(), UnsafeDeepLink>;
    fn replace(&self, path: &str) -> Result<(), UnsafeDeepLink>;
    fn back(&self);
    fn forward(&self);
    fn subscribe_deep_links(&self, subscriber: DeepLinkSubscriber);
    fn receive_deep_link(&self, path: &str) -> Result<(), UnsafeDeepLink>;
}

pub type DeepLinkSubscriber = Rc<dyn Fn(String)>;
type DeepLinkSubscribers = Rc<RefCell<Vec<DeepLinkSubscriber>>>;

/// A Dioxus History adapter shared by browser and native renderers.
#[derive(Clone)]
pub struct DioxusNavigationHost {
    history: Rc<dyn History>,
    deep_link_subscribers: DeepLinkSubscribers,
}

impl fmt::Debug for DioxusNavigationHost {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DioxusNavigationHost")
            .field("current_path", &self.current_path())
            .finish_non_exhaustive()
    }
}

impl DioxusNavigationHost {
    pub fn new(history: Rc<dyn History>) -> Self {
        Self {
            history,
            deep_link_subscribers: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn memory(initial_path: &str) -> Result<Self, UnsafeDeepLink> {
        let initial_path = validate_deep_link(initial_path)?;
        let history = MemoryHistory::with_initial_path(initial_path);
        Ok(Self::new(Rc::new(history)))
    }
}

impl NavigationHost for DioxusNavigationHost {
    fn current_path(&self) -> String {
        self.history.current_route()
    }

    fn push(&self, path: &str) -> Result<(), UnsafeDeepLink> {
        self.history.push(validate_deep_link(path)?);
        Ok(())
    }

    fn replace(&self, path: &str) -> Result<(), UnsafeDeepLink> {
        self.history.replace(validate_deep_link(path)?);
        Ok(())
    }

    fn back(&self) {
        self.history.go_back();
    }

    fn forward(&self) {
        self.history.go_forward();
    }

    fn subscribe_deep_links(&self, subscriber: DeepLinkSubscriber) {
        self.deep_link_subscribers.borrow_mut().push(subscriber);
    }

    fn receive_deep_link(&self, path: &str) -> Result<(), UnsafeDeepLink> {
        let path = validate_deep_link(path)?;
        self.history.push(path.clone());
        for subscriber in self.deep_link_subscribers.borrow().iter() {
            subscriber(path.clone());
        }
        Ok(())
    }
}

/// Non-secret preference keys. Credentials cannot be represented by this API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PreferenceKey {
    Locale,
    Theme,
    NavigationDisclosure,
}

pub trait PreferenceStore {
    fn load(&self, key: PreferenceKey) -> Option<String>;
    fn store(&self, key: PreferenceKey, value: &str);
}

#[derive(Debug, Clone, Default)]
pub struct MemoryPreferenceStore {
    values: Rc<RefCell<BTreeMap<PreferenceKey, String>>>,
}

impl PreferenceStore for MemoryPreferenceStore {
    fn load(&self, key: PreferenceKey) -> Option<String> {
        self.values.borrow().get(&key).cloned()
    }

    fn store(&self, key: PreferenceKey, value: &str) {
        self.values.borrow_mut().insert(key, value.to_owned());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalActionError {
    Unsupported,
    UnsafeUrl,
}

pub trait ExternalActionHost {
    fn restart(&self) -> Result<(), ExternalActionError>;
    fn open_external_url(&self, url: &str) -> Result<(), ExternalActionError>;
    fn system_locale(&self) -> Option<String>;
}

#[derive(Debug, Clone, Default)]
pub struct DisabledExternalActionHost;

impl ExternalActionHost for DisabledExternalActionHost {
    fn restart(&self) -> Result<(), ExternalActionError> {
        Err(ExternalActionError::Unsupported)
    }

    fn open_external_url(&self, url: &str) -> Result<(), ExternalActionError> {
        if !(url.starts_with("https://") || url.starts_with("mailto:")) {
            return Err(ExternalActionError::UnsafeUrl);
        }
        Err(ExternalActionError::Unsupported)
    }

    fn system_locale(&self) -> Option<String> {
        None
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Default)]
pub struct WebExternalActionHost;

#[cfg(target_arch = "wasm32")]
impl ExternalActionHost for WebExternalActionHost {
    fn restart(&self) -> Result<(), ExternalActionError> {
        web_sys::window()
            .ok_or(ExternalActionError::Unsupported)?
            .location()
            .reload()
            .map_err(|_| ExternalActionError::Unsupported)
    }

    fn open_external_url(&self, url: &str) -> Result<(), ExternalActionError> {
        if !url.starts_with("https://") {
            return Err(ExternalActionError::UnsafeUrl);
        }
        web_sys::window()
            .ok_or(ExternalActionError::Unsupported)?
            .open_with_url_and_target(url, "_blank")
            .map(|_| ())
            .map_err(|_| ExternalActionError::Unsupported)
    }

    fn system_locale(&self) -> Option<String> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasskeyCeremony {
    Registration,
    Authentication,
}

#[derive(Debug, Clone)]
pub enum PasskeyServerOptions {
    Registration(webauthn_rs_proto::CreationChallengeResponse),
    Authentication(webauthn_rs_proto::RequestChallengeResponse),
}

#[derive(Debug, Clone)]
pub struct PasskeyRequest {
    pub ceremony: PasskeyCeremony,
    pub relying_party_id: String,
    pub origin: String,
    pub server_options: PasskeyServerOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PasskeyResponse {
    pub credential_id: Vec<u8>,
    pub response_json: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasskeyError {
    Cancelled,
    Unsupported,
    InvalidOrigin,
    Platform,
}

pub type PasskeyFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PasskeyResponse, PasskeyError>> + 'a>>;

pub trait PasskeyProvider {
    fn register(&self, request: PasskeyRequest) -> PasskeyFuture<'_>;
    fn authenticate(&self, request: PasskeyRequest) -> PasskeyFuture<'_>;
}

#[derive(Debug, Clone, Default)]
pub struct DisabledPasskeyProvider;

impl PasskeyProvider for DisabledPasskeyProvider {
    fn register(&self, _request: PasskeyRequest) -> PasskeyFuture<'_> {
        Box::pin(async { Err(PasskeyError::Unsupported) })
    }

    fn authenticate(&self, _request: PasskeyRequest) -> PasskeyFuture<'_> {
        Box::pin(async { Err(PasskeyError::Unsupported) })
    }
}

/// Typed services supplied once at the console composition root.
#[derive(Clone)]
pub struct ConsolePlatform {
    pub capabilities: PlatformCapabilities,
    pub http: Rc<dyn http::HttpHost>,
    pub credentials: credentials::RuntimeCredentialSlot,
    pub navigation: Rc<dyn NavigationHost>,
    pub preferences: Rc<dyn PreferenceStore>,
    pub external_actions: Rc<dyn ExternalActionHost>,
    pub passkeys: Rc<dyn PasskeyProvider>,
}

impl fmt::Debug for ConsolePlatform {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConsolePlatform")
            .field("capabilities", &self.capabilities)
            .finish_non_exhaustive()
    }
}

impl ConsolePlatform {
    pub fn try_web(history: Rc<dyn History>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let (capabilities, passkeys): (PlatformCapabilities, Rc<dyn PasskeyProvider>) = {
            let mut capabilities = web_capabilities();
            let passkeys: Rc<dyn PasskeyProvider> = if passkey::WebPasskeyProvider::is_available() {
                capabilities.passkey = CapabilityStatus::Available;
                Rc::new(passkey::WebPasskeyProvider)
            } else {
                Rc::new(DisabledPasskeyProvider)
            };
            (capabilities, passkeys)
        };
        #[cfg(not(target_arch = "wasm32"))]
        let capabilities = web_capabilities();
        #[cfg(not(target_arch = "wasm32"))]
        let passkeys: Rc<dyn PasskeyProvider> = Rc::new(DisabledPasskeyProvider);
        Self {
            capabilities,
            http: web_http_host(),
            credentials: credentials::RuntimeCredentialSlot::new(),
            navigation: Rc::new(DioxusNavigationHost::new(history)),
            preferences: web_preference_store(),
            external_actions: current_external_action_host(),
            passkeys,
        }
    }

    pub fn try_server(history: Rc<dyn History>) -> Self {
        let (http, _) = http::DeferredHttpHost::for_target(ClientTarget::Server);
        Self {
            capabilities: server_capabilities(),
            http: Rc::new(http),
            credentials: credentials::RuntimeCredentialSlot::new(),
            navigation: Rc::new(DioxusNavigationHost::new(history)),
            preferences: Rc::new(MemoryPreferenceStore::default()),
            external_actions: Rc::new(DisabledExternalActionHost),
            passkeys: Rc::new(DisabledPasskeyProvider),
        }
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "native", target_os = "macos"))]
    pub fn try_macos(
        history: Rc<dyn History>,
        endpoints: &NativeEndpointConfig,
    ) -> Result<Self, http::HttpError> {
        let navigation = navigation::MacOsNavigationHost::with_history(
            history,
            Rc::new(crate::composition::is_registered_console_path),
        )
        .map_err(|_| http::HttpError::InvalidPath)?;
        let preference_path = preferences::MacOsPreferenceStore::default_path()
            .ok_or(http::HttpError::InvalidPolicy)?;
        let preferences = preferences::MacOsPreferenceStore::open(preference_path)
            .map_err(|_| http::HttpError::Protocol)?;
        Ok(Self {
            capabilities: current_capabilities(),
            http: Rc::new(http::ReqwestHttpHost::new(endpoints)?),
            credentials: credentials::RuntimeCredentialSlot::new(),
            navigation: Rc::new(navigation),
            preferences: Rc::new(preferences),
            external_actions: Rc::new(external::MacOsExternalActionHost),
            passkeys: Rc::new(passkey::MacOsPasskeyProvider),
        })
    }

    #[cfg(all(
        not(target_arch = "wasm32"),
        feature = "native",
        not(target_os = "macos")
    ))]
    fn try_future_native(history: Rc<dyn History>) -> Self {
        let (http, _) = http::DeferredHttpHost::for_target(native_client_target());
        Self {
            capabilities: current_capabilities(),
            http: Rc::new(http),
            credentials: credentials::RuntimeCredentialSlot::new(),
            navigation: Rc::new(DioxusNavigationHost::new(history)),
            preferences: Rc::new(MemoryPreferenceStore::default()),
            external_actions: Rc::new(DisabledExternalActionHost),
            passkeys: Rc::new(DisabledPasskeyProvider),
        }
    }

    pub fn with_history(history: Rc<dyn History>) -> Self {
        #[cfg(feature = "server")]
        return Self::try_server(history);

        #[cfg(all(not(feature = "server"), feature = "web"))]
        return Self::try_web(history);

        #[cfg(all(not(feature = "server"), not(feature = "web"), feature = "native"))]
        {
            #[cfg(not(target_os = "macos"))]
            return Self::try_future_native(history);

            #[cfg(target_os = "macos")]
            {
                let http = std::env::var("ERPX_NATIVE_HTTP_ORIGIN")
                    .unwrap_or_else(|_| crate::transport::auth_http_origin());
                let grpc = std::env::var("ERPX_NATIVE_GRPC_ENDPOINT")
                    .unwrap_or_else(|_| crate::transport::GRPC_ENDPOINT.to_owned());
                let endpoints = NativeEndpointConfig {
                    auth_http: http.clone(),
                    public_http: http,
                    grpc,
                };
                return Self::try_macos(history, &endpoints)
                    .expect("native endpoint configuration must satisfy release policy");
            }
        }

        #[allow(unreachable_code)]
        Self::try_server(history)
    }
}

#[cfg(target_arch = "wasm32")]
fn web_http_host() -> Rc<dyn http::HttpHost> {
    Rc::new(http::WebFetchHttpHost::default())
}

#[cfg(target_arch = "wasm32")]
fn web_preference_store() -> Rc<dyn PreferenceStore> {
    Rc::new(preferences::WebPreferenceStore)
}

#[cfg(not(target_arch = "wasm32"))]
fn web_preference_store() -> Rc<dyn PreferenceStore> {
    Rc::new(MemoryPreferenceStore::default())
}

#[cfg(not(target_arch = "wasm32"))]
fn web_http_host() -> Rc<dyn http::HttpHost> {
    let (host, _) = http::DeferredHttpHost::for_target(ClientTarget::Web);
    Rc::new(host)
}

#[cfg(target_arch = "wasm32")]
fn current_external_action_host() -> Rc<dyn ExternalActionHost> {
    Rc::new(external::TypedWebExternalActionHost)
}

#[cfg(not(target_arch = "wasm32"))]
fn current_external_action_host() -> Rc<dyn ExternalActionHost> {
    Rc::new(DisabledExternalActionHost)
}

pub const fn current_capabilities() -> PlatformCapabilities {
    #[cfg(feature = "server")]
    return server_capabilities();

    #[cfg(all(not(feature = "server"), feature = "web"))]
    return web_capabilities();

    #[cfg(all(not(feature = "server"), not(feature = "web"), feature = "native"))]
    return PlatformCapabilities {
        target: native_client_target(),
        renderer: RendererKind::NativeGpu,
        navigation: native_capability(CapabilityStatus::Available),
        secure_storage: native_capability(CapabilityStatus::Available),
        passkey: native_capability(CapabilityStatus::Blocked(
            "macOS passkey adapter is not certified",
        )),
        deep_link: native_capability(CapabilityStatus::Available),
        external_url: native_capability(CapabilityStatus::Blocked(
            "macOS external action adapter is not certified",
        )),
    };

    #[allow(unreachable_code)]
    PlatformCapabilities {
        target: ClientTarget::Server,
        renderer: RendererKind::NativeGpu,
        navigation: CapabilityStatus::Unsupported,
        secure_storage: CapabilityStatus::Unsupported,
        passkey: CapabilityStatus::Unsupported,
        deep_link: CapabilityStatus::Unsupported,
        external_url: CapabilityStatus::Unsupported,
    }
}

const fn web_capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        target: ClientTarget::Web,
        renderer: RendererKind::WebDom,
        navigation: CapabilityStatus::Available,
        secure_storage: CapabilityStatus::Available,
        passkey: CapabilityStatus::Unsupported,
        deep_link: CapabilityStatus::Unsupported,
        external_url: CapabilityStatus::Available,
    }
}

const fn server_capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        target: ClientTarget::Server,
        renderer: RendererKind::ServerSsr,
        navigation: CapabilityStatus::Available,
        secure_storage: CapabilityStatus::Available,
        passkey: CapabilityStatus::Unsupported,
        deep_link: CapabilityStatus::Unsupported,
        external_url: CapabilityStatus::Unsupported,
    }
}

#[cfg(all(not(feature = "server"), not(feature = "web"), feature = "native"))]
const fn native_client_target() -> ClientTarget {
    #[cfg(target_os = "macos")]
    return ClientTarget::MacOs;
    #[cfg(target_os = "windows")]
    return ClientTarget::Windows;
    #[cfg(target_os = "linux")]
    return ClientTarget::Linux;
    #[cfg(target_os = "ios")]
    return ClientTarget::Ios;
    #[cfg(target_os = "android")]
    return ClientTarget::Android;
    #[allow(unreachable_code)]
    ClientTarget::Linux
}

#[cfg(all(not(feature = "server"), not(feature = "web"), feature = "native"))]
const fn native_capability(macos: CapabilityStatus) -> CapabilityStatus {
    #[cfg(target_os = "macos")]
    return macos;
    #[allow(unreachable_code)]
    {
        let _ = macos;
        CapabilityStatus::Deferred
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_supports_push_replace_back_forward() {
        let navigation = DioxusNavigationHost::memory("/").expect("memory history");
        navigation.push("/finance").expect("push");
        navigation.replace("/finance/ledger").expect("replace");
        assert_eq!(navigation.current_path(), "/finance/ledger");
        navigation.back();
        assert_eq!(navigation.current_path(), "/");
        navigation.forward();
        assert_eq!(navigation.current_path(), "/finance/ledger");
    }

    #[test]
    fn unsafe_deep_links_are_rejected_without_notifying_subscribers() {
        let navigation = DioxusNavigationHost::memory("/").expect("memory history");
        let notifications = Rc::new(RefCell::new(Vec::new()));
        let received = Rc::clone(&notifications);
        navigation.subscribe_deep_links(Rc::new(move |path| {
            received.borrow_mut().push(path);
        }));

        for unsafe_path in [
            "https://example.com/finance",
            "//example.com/finance",
            "/finance?token=secret",
            "/finance%2f..%2fadmin",
            "/finance\\admin",
        ] {
            assert!(navigation.receive_deep_link(unsafe_path).is_err());
        }
        assert!(notifications.borrow().is_empty());
        navigation
            .receive_deep_link("/finance/ledger")
            .expect("safe deep link");
        assert_eq!(notifications.borrow().as_slice(), ["/finance/ledger"]);
    }

    #[test]
    fn preferences_cannot_address_credentials() {
        let store = MemoryPreferenceStore::default();
        store.store(PreferenceKey::Locale, "th-TH");
        store.store(PreferenceKey::Theme, "dark");
        assert_eq!(store.load(PreferenceKey::Locale).as_deref(), Some("th-TH"));
        assert_eq!(store.load(PreferenceKey::Theme).as_deref(), Some("dark"));
    }

    #[cfg(feature = "native")]
    #[test]
    fn native_capabilities_are_fail_closed_until_os_adapters_are_certified() {
        let capabilities = current_capabilities();
        assert_eq!(capabilities.renderer, RendererKind::NativeGpu);
        if cfg!(target_os = "macos") {
            assert_eq!(capabilities.target, ClientTarget::MacOs);
            assert_eq!(capabilities.navigation, CapabilityStatus::Available);
            assert_eq!(capabilities.deep_link, CapabilityStatus::Available);
            assert!(matches!(capabilities.passkey, CapabilityStatus::Blocked(_)));
            assert!(matches!(
                capabilities.external_url,
                CapabilityStatus::Blocked(_)
            ));
        } else {
            assert_eq!(capabilities.navigation, CapabilityStatus::Deferred);
            assert_eq!(capabilities.secure_storage, CapabilityStatus::Deferred);
            assert_eq!(capabilities.passkey, CapabilityStatus::Deferred);
        }
    }
}
