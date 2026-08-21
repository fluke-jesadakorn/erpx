//! ERPX Console composition root.
//!
//! The active UI is intentionally small: providers, one session bootstrap,
//! Dioxus Router, and nested shells. Route state lives in each leaf component.

#![forbid(unsafe_code)]
#![cfg_attr(feature = "web-split", allow(non_snake_case))]

mod app;
#[cfg(feature = "server")]
mod bff;
#[cfg(feature = "server")]
mod bff_navigation;
#[cfg(any(feature = "web", feature = "server"))]
mod delivery;
#[cfg(any(feature = "web", feature = "server"))]
mod feature_delivery;
#[cfg(any(feature = "web", feature = "server"))]
mod insight_delivery;
#[cfg(feature = "native")]
mod native_delivery;
#[cfg(feature = "native")]
mod native_spike;
pub mod platform;
#[cfg(any(feature = "web", feature = "server"))]
mod process_delivery;
#[cfg(any(feature = "web", feature = "server"))]
mod record_collaboration_delivery;
mod routes;
#[cfg(any(feature = "web", feature = "server"))]
mod security_delivery;
#[cfg(feature = "server")]
mod server;
#[cfg(any(feature = "web", feature = "server"))]
mod work_delivery;

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
    groups: &[erpx_ui_core::NavGroup],
) -> bool {
    if !routes::is_canonical_console_path(path, erpx_delivery_contract::ClientChannel::Web) {
        return false;
    }
    if matches!(
        path,
        "/auth/accounts" | "/auth/security" | "/auth/devices" | "/auth/recovery"
    ) {
        return true;
    }
    let has_authorized_route = groups
        .iter()
        .flat_map(|group| group.items.iter())
        .any(erpx_ui_core::NavItem::is_authorized);
    if matches!(path, "/work" | "/processes" | "/insights" | "/modules") {
        return has_authorized_route;
    }
    groups
        .iter()
        .flat_map(|group| group.items.iter())
        .filter(|item| item.is_authorized())
        .any(|item| {
            path == item.path
                || (path.starts_with("/identity/") && item.path == "/identity/principal")
                || path
                    .strip_prefix(item.path.as_str())
                    .is_some_and(|suffix| suffix.starts_with('/'))
        })
}

#[cfg(feature = "server")]
#[allow(dead_code)]
pub(crate) fn navigation_http_groups(groups: &[erpx_ui_core::NavGroup]) -> Vec<serde_json::Value> {
    groups
        .iter()
        .map(|group| {
            serde_json::json!({
                "group_id": group.group_id,
                "label": group.label,
                "label_key": group.label_key,
                "items": group.items.iter().map(|item| serde_json::json!({
                    "route_id": item.route_id,
                    "label": item.label,
                    "label_key": item.label_key,
                    "path": item.path,
                    "icon": item.icon,
                    "action_capabilities": item.action_capabilities,
                    "access": if item.is_authorized() { "authorized" } else { "locked" },
                    "required_role_keys": item.required_role_keys(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect()
}

pub fn run() {
    app::launch();
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::navigation_authorizes_console_path;
    use erpx_ui_core::{NavAccess, NavGroup, NavItem};

    fn manifest() -> Vec<NavGroup> {
        vec![NavGroup::new(
            "Business Modules",
            vec![
                NavItem::new("Finance", "/finance", "module"),
                NavItem::new("Operations", "/operations", "module")
                    .with_access(NavAccess::locked(["operations_viewer"])),
            ],
        )]
    }

    #[test]
    fn navigation_fence_allows_only_authorized_manifest_roots_and_children() {
        let groups = manifest();
        assert!(navigation_authorizes_console_path("/finance", &groups));
        assert!(navigation_authorizes_console_path(
            "/finance/journals",
            &groups
        ));
        assert!(!navigation_authorizes_console_path("/operations", &groups));
        assert!(!navigation_authorizes_console_path("/sales", &groups));
        assert!(!navigation_authorizes_console_path(
            "/not-registered",
            &groups
        ));
    }

    #[test]
    fn shell_routes_require_at_least_one_authorized_manifest_route() {
        assert!(navigation_authorizes_console_path("/work", &manifest()));
        assert!(!navigation_authorizes_console_path("/work", &[]));
        assert!(navigation_authorizes_console_path("/auth/security", &[]));
    }
}
