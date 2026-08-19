//! ERPX Console composition root.
//!
//! The active UI is intentionally small: providers, one session bootstrap,
//! Dioxus Router, and nested shells. Route state lives in each leaf component.

#![forbid(unsafe_code)]
#![cfg_attr(feature = "web-split", allow(non_snake_case))]

mod app;
#[cfg(feature = "server")]
mod bff;
#[cfg(any(feature = "web", feature = "server"))]
mod delivery;
#[cfg(feature = "native")]
mod native_delivery;
#[cfg(feature = "native")]
mod native_spike;
pub mod platform;
mod routes;
#[cfg(feature = "server")]
mod server;

#[cfg(any(feature = "web", feature = "server"))]
pub use delivery::{ServerFunctionGateway, session_bootstrap};
#[cfg(feature = "native")]
pub use native_delivery::NativeGrpcGateway;
pub use routes::{App, AppRoute, is_canonical_console_path};

#[cfg(feature = "server")]
pub(crate) const LOGIN_PATH: &str = "/auth/sign-in";

#[cfg(feature = "server")]
pub(crate) fn authenticated_landing_path() -> &'static str {
    "/work"
}

#[cfg(feature = "server")]
pub(crate) const fn is_legacy_login_path(_value: &str) -> bool {
    false
}

#[cfg(feature = "server")]
pub(crate) fn validated_login_return_path(value: &str) -> Option<String> {
    routes::is_canonical_console_path(value, erpx_delivery_contract::ClientChannel::Web)
        .then(|| value.to_owned())
}

#[cfg(feature = "server")]
pub(crate) fn login_return_path_from_url(value: &str) -> Option<String> {
    let query = value.split_once('?')?.1;
    url::form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == "return_to")
        .and_then(|(_, value)| validated_login_return_path(&value))
}

#[cfg(feature = "server")]
pub(crate) fn login_redirect_location(return_to: &str) -> String {
    let encoded: String = url::form_urlencoded::byte_serialize(return_to.as_bytes()).collect();
    format!("{LOGIN_PATH}?return_to={encoded}")
}

#[cfg(feature = "server")]
pub(crate) fn console_route_http_status(path: &str) -> u16 {
    if routes::is_canonical_console_path(path, erpx_delivery_contract::ClientChannel::Web) {
        200
    } else {
        404
    }
}

#[cfg(feature = "server")]
pub(crate) fn navigation_authorizes_console_path(
    path: &str,
    _groups: &[erpx_ui_core::NavGroup],
) -> bool {
    routes::is_canonical_console_path(path, erpx_delivery_contract::ClientChannel::Web)
}

#[cfg(feature = "server")]
pub(crate) async fn fetch_navigation_manifest(
    _access_token: String,
    _principal_ref: String,
    _session_id: String,
    _scope_key: String,
    _development_context_token: Option<String>,
    _include_inaccessible: bool,
) -> Result<Vec<erpx_ui_core::NavGroup>, String> {
    // Navigation is now owned by the typed route catalog. This retained BFF
    // endpoint returns an empty projection until it is removed with v1.
    Ok(Vec::new())
}

#[cfg(feature = "server")]
#[allow(dead_code)]
pub(crate) fn navigation_http_groups(_groups: &[erpx_ui_core::NavGroup]) -> Vec<serde_json::Value> {
    Vec::new()
}

pub fn run() {
    app::launch();
}
