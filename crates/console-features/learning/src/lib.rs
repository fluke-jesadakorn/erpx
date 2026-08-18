#![doc = "Target-neutral learning Console workspace."]
#![forbid(unsafe_code)]

use erpx_ui_enterprise_workspaces::{CanonicalRecordRoute, WorkspaceKind, WorkspaceRoute};

pub const WORKSPACE: WorkspaceKind = WorkspaceKind::Learning;

pub use erpx_ui_enterprise_workspaces::EnterpriseWorkspaceGateway as LearningGateway;

pub fn resolve_route(path: &str) -> Option<WorkspaceRoute> {
    WorkspaceRoute::parse(WORKSPACE, path)
}

pub fn resolve_record_route(path: &str) -> Option<CanonicalRecordRoute> {
    CanonicalRecordRoute::parse(WORKSPACE, path)
}

erpx_ui_enterprise_workspaces::declare_enterprise_workspace_feature!(
    LearningFeatureSurface,
    WORKSPACE,
    "learning",
    "learning.workspace",
    "/learning",
    "people",
    "erpx-feature-learning-v2"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_and_record_routes_are_canonical() {
        assert_eq!(REGISTRATION.routes[0].canonical_path, WORKSPACE.root());
        assert!(!REGISTRATION.routes[0].canonical_path.contains("workspaces"));
        assert!(resolve_route(WORKSPACE.root()).is_some());
        assert!(resolve_route(&WORKSPACE.collection_path()).is_some());
        let detail = WORKSPACE.detail_path("record-001").expect("detail route");
        assert!(matches!(
            resolve_route(&detail),
            Some(WorkspaceRoute::Detail { .. })
        ));
    }

    #[test]
    fn gateway_boundary_has_no_wire_or_database_dependency() {
        let source = include_str!("lib.rs");
        assert!(!source.contains(concat!("erpx_", "proto")));
        assert!(!source.contains(concat!("sqlx", "::")));
        assert!(!source.contains(concat!("ton", "ic::")));
    }

    #[test]
    fn learning_surface_covers_web_desktop_and_e2e_38() {
        use erpx_ui_enterprise_workspaces::{ChannelPolicy, ClientChannel};
        assert_eq!(WORKSPACE.record_families().len(), 4);
        assert_eq!(WORKSPACE.e2e_scenarios(), &["E2E-38"]);
        assert_eq!(WORKSPACE.channel_policy(), ChannelPolicy::WebMacOs);
        assert!(WORKSPACE.channel_policy().supports(ClientChannel::Web));
        assert!(WORKSPACE.channel_policy().supports(ClientChannel::Desktop));
        const { assert!(erpx_ui_enterprise_workspaces::ACCESSIBILITY.focus_heading_after_navigation) };
        const { assert!(erpx_ui_enterprise_workspaces::ACCESSIBILITY.keyboard_reachable_actions) };
    }
}
