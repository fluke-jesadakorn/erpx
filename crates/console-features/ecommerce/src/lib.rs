#![doc = "Target-neutral ecommerce Console workspace."]
#![forbid(unsafe_code)]

use erpx_ui_enterprise_workspaces::{CanonicalRecordRoute, WorkspaceKind, WorkspaceRoute};

pub const WORKSPACE: WorkspaceKind = WorkspaceKind::Ecommerce;

pub use erpx_ui_enterprise_workspaces::EnterpriseWorkspaceGateway as EcommerceGateway;

pub fn resolve_route(path: &str) -> Option<WorkspaceRoute> {
    WorkspaceRoute::parse(WORKSPACE, path)
}

pub fn resolve_record_route(path: &str) -> Option<CanonicalRecordRoute> {
    CanonicalRecordRoute::parse(WORKSPACE, path)
}

erpx_ui_enterprise_workspaces::declare_enterprise_workspace_feature!(
    EcommerceFeatureSurface,
    WORKSPACE,
    "ecommerce",
    "ecommerce.workspace",
    "/commerce",
    "customer",
    "erpx-feature-ecommerce"
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
    fn commerce_surface_supports_web_and_macos_and_covers_e2e_36() {
        use erpx_ui_enterprise_workspaces::{ChannelPolicy, ClientChannel};
        assert_eq!(WORKSPACE.record_families().len(), 5);
        assert_eq!(WORKSPACE.e2e_scenarios(), &["E2E-36"]);
        assert_eq!(WORKSPACE.channel_policy(), ChannelPolicy::WebMacOs);
        assert!(WORKSPACE.channel_policy().supports(ClientChannel::Desktop));
    }
}
