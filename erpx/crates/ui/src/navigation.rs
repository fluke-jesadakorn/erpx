use crate::redact_deep_link;

/// A route-agnostic navigation entry. Applications may render `path` with Dioxus Router, a
/// native deep-link handler, or a normal anchor without changing the shared shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavItem {
    /// Human-readable label, already localized by the application.
    pub label: String,
    /// Stable application path or deep-link target.
    pub path: String,
    /// Short icon token interpreted by the client theme.
    pub icon: String,
}

impl NavItem {
    /// Create a navigation entry after removing query/fragment material and rejecting
    /// credential-bearing deep links.
    pub fn new(label: impl Into<String>, path: impl Into<String>, icon: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            label: label.into(),
            path: redact_deep_link(&path),
            icon: icon.into(),
        }
    }
}

/// Minimal navigation state shared by responsive shells and mobile navigation drawers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NavigationState {
    active_path: String,
}

impl NavigationState {
    /// Create state for an initial path.
    pub fn new(path: impl AsRef<str>) -> Self {
        Self {
            active_path: redact_deep_link(path.as_ref()),
        }
    }

    /// Return the normalized active path.
    pub fn active_path(&self) -> &str {
        &self.active_path
    }

    /// Set the active path after a successful navigation request.
    pub fn set_active_path(&mut self, path: impl AsRef<str>) {
        self.active_path = redact_deep_link(path.as_ref());
    }

    /// Check whether an item is active, including nested paths.
    pub fn is_active(&self, item: &NavItem) -> bool {
        is_path_active(&self.active_path, &item.path)
    }
}

/// Normalize a client path without changing its meaning.
pub fn normalize_path(path: &str) -> &str {
    let path = path.trim();
    if path.is_empty() {
        return "/";
    }
    if path.starts_with('/') {
        let normalized = path.trim_end_matches('/');
        if normalized.is_empty() {
            "/"
        } else {
            normalized
        }
    } else {
        path
    }
}

fn is_path_active(active: &str, item: &str) -> bool {
    active == item
        || (item != "/"
            && active.starts_with(item)
            && active.as_bytes().get(item.len()) == Some(&b'/'))
}

#[cfg(test)]
mod tests {
    use super::{NavItem, NavigationState, normalize_path};

    #[test]
    fn paths_are_normalized_without_losing_nested_routes() {
        assert_eq!(normalize_path(""), "/");
        assert_eq!(normalize_path("/platform/"), "/platform");
        assert_eq!(normalize_path("platform"), "platform");

        let mut state = NavigationState::new("/platform/customers/123/");
        let platform = NavItem::new("Platform", "/platform", "▦");
        let operations = NavItem::new("Operations", "/operations", "⌁");
        assert!(state.is_active(&platform));
        assert!(!state.is_active(&operations));

        state.set_active_path("/operations");
        assert!(state.is_active(&operations));
        assert!(!state.is_active(&platform));
    }

    #[test]
    fn root_does_not_match_every_nested_path() {
        let state = NavigationState::new("/operations");
        assert!(!state.is_active(&NavItem::new("Home", "/", "⌂")));
    }

    #[test]
    fn navigation_targets_redact_credentials_before_reaching_the_shell() {
        let credential_bearing = NavItem::new(
            "Customer 360",
            "/platform/customers?access_token=secret#details",
            "◎",
        );
        assert_eq!(credential_bearing.path, "/");
        assert!(!credential_bearing.path.contains("secret"));

        let query_only = NavItem::new(
            "Customer 360",
            "/platform/customers?view=health#details",
            "◎",
        );
        assert_eq!(query_only.path, "/platform/customers");
    }

    #[test]
    fn active_paths_reject_external_and_encoded_targets() {
        let state = NavigationState::new("https://evil.example/platform");
        assert_eq!(state.active_path(), "/");

        let mut state = NavigationState::new("/platform");
        state.set_active_path("/platform/%2Fcustomers");
        assert_eq!(state.active_path(), "/");
        state.set_active_path("//evil.example/platform");
        assert_eq!(state.active_path(), "/");
    }

    #[test]
    fn normalize_path_remains_a_non_allocating_display_helper() {
        assert_eq!(normalize_path("/platform/"), "/platform");
    }
}
