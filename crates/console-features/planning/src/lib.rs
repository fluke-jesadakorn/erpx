#![doc = "Target-neutral planning Console workspace."]
#![forbid(unsafe_code)]

use erpx_ui_enterprise_workspaces::{CanonicalRecordRoute, WorkspaceKind, WorkspaceRoute};

pub const WORKSPACE: WorkspaceKind = WorkspaceKind::Planning;

pub use erpx_ui_enterprise_workspaces::EnterpriseWorkspaceGateway as PlanningGateway;

pub fn resolve_route(path: &str) -> Option<WorkspaceRoute> {
    WorkspaceRoute::parse(WORKSPACE, path)
}

pub fn resolve_record_route(path: &str) -> Option<CanonicalRecordRoute> {
    CanonicalRecordRoute::parse(WORKSPACE, path)
}

erpx_ui_enterprise_workspaces::declare_enterprise_workspace_feature!(
    PlanningFeatureSurface,
    WORKSPACE,
    "planning",
    "planning.workspace",
    "/planning",
    "operations",
    "erpx-feature-planning"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_and_record_routes_are_canonical() {
        assert_eq!(REGISTRATION.routes[0].canonical_path, WORKSPACE.root());
        assert_eq!(REGISTRATION.routes[0].authorization, "authenticated");
        assert_eq!(REGISTRATION.artifact_key, "erpx-feature-planning");
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
    fn planning_surface_covers_scenarios_33_and_34() {
        assert_eq!(WORKSPACE.record_families().len(), 5);
        assert_eq!(WORKSPACE.e2e_scenarios(), &["E2E-33", "E2E-34"]);
        for family in WORKSPACE.record_families() {
            assert!(resolve_record_route(&family.collection_path(WORKSPACE)).is_some());
        }
    }

    #[test]
    fn planning_surface_contract_is_localized_and_desktop_accessible() {
        use erpx_ui_core::Locale;
        use erpx_ui_enterprise_workspaces::{ACCESSIBILITY, ClientChannel};

        assert_ne!(WORKSPACE.label(Locale::En), WORKSPACE.label(Locale::Th));
        assert!(WORKSPACE.channel_policy().supports(ClientChannel::Web));
        assert!(WORKSPACE.channel_policy().supports(ClientChannel::Desktop));
        const { assert!(ACCESSIBILITY.focus_heading_after_navigation) };
        const { assert!(ACCESSIBILITY.keyboard_reachable_actions) };
    }
}
