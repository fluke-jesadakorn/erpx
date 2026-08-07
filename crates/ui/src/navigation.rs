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
    /// Server-owned workflow/action discoverability metadata. This can hide or
    /// disable controls, but it is never treated as command authorization.
    pub action_capabilities: Vec<String>,
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
            action_capabilities: Vec::new(),
        }
    }

    /// Attach bounded capability identifiers supplied by the navigation
    /// manifest. Invalid or credential-looking values are dropped before they
    /// can reach rendered attributes or diagnostic output.
    pub fn with_action_capabilities(
        mut self,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.action_capabilities = capabilities
            .into_iter()
            .map(Into::into)
            .filter(|value| valid_action_capability(value))
            .take(64)
            .collect();
        self.action_capabilities.sort();
        self.action_capabilities.dedup();
        self
    }

    /// Whether the manifest advertised a workflow action for this route.
    pub fn allows_action(&self, capability: &str) -> bool {
        valid_action_capability(capability)
            && self
                .action_capabilities
                .binary_search_by(|candidate| candidate.as_str().cmp(capability))
                .is_ok()
    }
}

fn valid_action_capability(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
        && !value.to_ascii_lowercase().contains("token")
        && !value.to_ascii_lowercase().contains("secret")
        && !value.to_ascii_lowercase().contains("password")
}

/// A named primary-navigation work area. Grouping is presentation metadata;
/// every child route remains independently authorized by the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavGroup {
    pub label: String,
    pub items: Vec<NavItem>,
}

impl NavGroup {
    pub fn new(label: impl Into<String>, items: Vec<NavItem>) -> Self {
        Self {
            label: label.into(),
            items,
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
    use super::{NavGroup, NavItem, NavigationState, normalize_path};

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
    fn navigation_groups_preserve_explicit_work_area_counts() {
        let group = NavGroup::new(
            "Customer Operations",
            vec![NavItem::new("Customer 360", "/platform/customers", "◎")],
        );
        assert_eq!(group.label, "Customer Operations");
        assert_eq!(group.items.len(), 1);
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
    fn action_capabilities_are_bounded_sorted_and_never_authorization_material() {
        let item = NavItem::new("Quality", "/workspaces/quality", "quality")
            .with_action_capabilities([
                "TransitionQualityInspection",
                "CreateQualityInspection",
                "CreateQualityInspection",
                "access_token",
                "bad capability",
            ]);
        assert_eq!(
            item.action_capabilities,
            ["CreateQualityInspection", "TransitionQualityInspection"]
        );
        assert!(item.allows_action("CreateQualityInspection"));
        assert!(!item.allows_action("DeleteQualityInspection"));
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
