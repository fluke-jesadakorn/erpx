#![doc = "Target-neutral talent Console workspace."]
#![forbid(unsafe_code)]

use erpx_ui_enterprise_workspaces::{CanonicalRecordRoute, WorkspaceKind, WorkspaceRoute};

pub const WORKSPACE: WorkspaceKind = WorkspaceKind::Talent;

pub use erpx_ui_enterprise_workspaces::EnterpriseWorkspaceGateway as TalentGateway;

pub fn resolve_route(path: &str) -> Option<WorkspaceRoute> {
    WorkspaceRoute::parse(WORKSPACE, path)
}

pub fn resolve_record_route(path: &str) -> Option<CanonicalRecordRoute> {
    CanonicalRecordRoute::parse(WORKSPACE, path)
}

erpx_ui_enterprise_workspaces::declare_enterprise_workspace_feature!(
    TalentFeatureSurface,
    WORKSPACE,
    "talent",
    "talent.workspace",
    "/talent",
    "people",
    "erpx-feature-talent"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_and_record_routes_are_canonical() {
        assert_eq!(REGISTRATION.routes[0].canonical_path, WORKSPACE.root());
        assert_eq!(REGISTRATION.routes[0].authorization, "authenticated");
        assert_eq!(REGISTRATION.artifact_key, "erpx-feature-talent");
        assert!(!REGISTRATION.routes[0].canonical_path.contains("workspaces"));
        assert!(REGISTRATION.matches_path(WORKSPACE.root()));
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
    fn talent_surface_supports_web_and_macos_and_covers_e2e_37() {
        use erpx_ui_enterprise_workspaces::{ChannelPolicy, ClientChannel};
        assert_eq!(WORKSPACE.record_families().len(), 7);
        assert_eq!(WORKSPACE.e2e_scenarios(), &["E2E-37"]);
        assert_eq!(WORKSPACE.channel_policy(), ChannelPolicy::WebMacOs);
        assert!(WORKSPACE.channel_policy().supports(ClientChannel::Desktop));
        assert!(!WORKSPACE.channel_policy().supports(ClientChannel::Ios));
    }

    #[test]
    fn talent_surface_contract_is_localized_and_keyboard_accessible() {
        use erpx_ui_core::Locale;
        use erpx_ui_enterprise_workspaces::ACCESSIBILITY;

        assert_ne!(WORKSPACE.label(Locale::En), WORKSPACE.label(Locale::Th));
        const { assert!(ACCESSIBILITY.focus_heading_after_navigation) };
        const { assert!(ACCESSIBILITY.keyboard_reachable_actions) };
    }
}
