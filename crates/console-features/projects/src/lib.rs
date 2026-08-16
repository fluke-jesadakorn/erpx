#![doc = "Target-neutral projects Console workspace."]
#![forbid(unsafe_code)]

use erpx_ui_enterprise_workspaces::{CanonicalRecordRoute, WorkspaceKind, WorkspaceRoute};

pub const WORKSPACE: WorkspaceKind = WorkspaceKind::Projects;

pub use erpx_ui_enterprise_workspaces::EnterpriseWorkspaceGateway as ProjectsGateway;

pub fn resolve_route(path: &str) -> Option<WorkspaceRoute> {
    WorkspaceRoute::parse(WORKSPACE, path)
}

pub fn resolve_record_route(path: &str) -> Option<CanonicalRecordRoute> {
    CanonicalRecordRoute::parse(WORKSPACE, path)
}

erpx_ui_enterprise_workspaces::declare_enterprise_workspace_feature!(
    ProjectsFeatureSurface,
    WORKSPACE,
    "projects",
    "projects.workspace",
    "/project",
    "operations",
    "erpx-feature-projects-v2"
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
    fn project_surface_covers_hierarchy_dependency_and_handoff_journeys() {
        assert_eq!(WORKSPACE.record_families().len(), 5);
        assert_eq!(
            WORKSPACE.e2e_scenarios(),
            &["E2E-35", "E2E-64", "E2E-65", "E2E-66"]
        );
        for family in WORKSPACE.record_families() {
            assert!(resolve_record_route(&family.collection_path(WORKSPACE)).is_some());
        }
        let source = include_str!("../../../ui-domains/enterprise-workspaces/src/lib.rs");
        assert!(source.contains("Project → Phase or Epic → Item → Sub-item"));
        assert!(source.contains("Cross-owner dependencies require acceptance by both owners"));
    }
}
