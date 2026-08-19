#![doc = "PostgreSQL CQRS command and projection adapters for Projects."]
#![forbid(unsafe_code)]

mod command;
mod directory;
mod projection;
mod read;

pub use command::{ProjectsCommandAdapter, ProjectsWriteRepository};
pub use projection::{
    ProjectsHierarchyProjector, ProjectsOutboxPayload, ProjectsProjector, ProjectsRecordProjector,
};
pub use read::ProjectsReadRepository;

pub const CONTEXT_NAME: &str = "projects";
pub const PROJECT_AGGREGATE_TYPE: &str = "project";
pub const WORKFLOW_AGGREGATE_TYPE: &str = "project_workflow_template";
pub const DEPENDENCY_AGGREGATE_TYPE: &str = "project_dependency";
pub const HANDOFF_AGGREGATE_TYPE: &str = "project_handoff";
pub const PROJECT_PROJECTION: &str = "projects.projects";
pub const HIERARCHY_PROJECTION: &str = "projects.hierarchy";
pub const RECORD_PROJECTION: &str = "projects.records";
pub const PROJECT_PROJECTION_FACT_EVENT: &str = "projects.project_projection_fact";
pub const PROJECT_RECORD_PROJECTION_FACT_EVENT: &str = "projects.record_projection_fact";
pub const EVENT_VERSION: u32 = 1;

#[cfg(test)]
const WRITE_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
#[cfg(test)]
const READ_DDL: &str = include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");
#[cfg(test)]
const RECORD_READ_DDL: &str =
    include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");

pub fn project_partition(id: erpx_projects::ProjectId) -> String {
    format!("projects.project:{}", id.as_uuid().simple())
}

pub fn workflow_partition(id: erpx_projects::WorkflowTemplateId) -> String {
    format!("projects.workflow:{}", id.as_uuid().simple())
}

pub fn dependency_partition(id: erpx_projects::DependencyId) -> String {
    format!("projects.dependency:{}", id.as_uuid().simple())
}

pub fn handoff_partition(id: erpx_projects::HandoffId) -> String {
    format!("projects.handoff:{}", id.as_uuid().simple())
}

#[cfg(test)]
#[path = "../../schema_test.rs"]
mod schema_test;
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn schema_contract_has_authoritative_adjacency_and_disposable_closure() {
        assert!(WRITE_DDL.contains("CREATE TABLE erpx_projects_write.project_node_guards"));
        assert!(WRITE_DDL.contains("DEFERRABLE INITIALLY DEFERRED"));
        assert!(WRITE_DDL.contains("reject_project_node_cycle"));
        assert!(WRITE_DDL.contains("workflow_template_versions"));
        assert!(WRITE_DDL.contains("dependency_guards"));
        assert!(WRITE_DDL.contains("handoff_guards"));
        assert!(READ_DDL.contains("hierarchy_closure_projection"));
        assert!(RECORD_READ_DDL.contains("project_record_projection"));
        assert!(RECORD_READ_DDL.contains("projects.records"));
        assert!(READ_DDL.contains("ON DELETE CASCADE"));
        assert!(RECORD_READ_DDL.contains("ON DELETE CASCADE"));
    }

    #[test]
    fn aggregate_partitions_are_stable_and_disjoint() {
        let project = erpx_projects::ProjectId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let workflow =
            erpx_projects::WorkflowTemplateId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let dependency = erpx_projects::DependencyId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let handoff = erpx_projects::HandoffId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        assert_ne!(project_partition(project), workflow_partition(workflow));
        assert_ne!(dependency_partition(dependency), handoff_partition(handoff));
    }

    #[test]
    fn projection_adapter_never_reads_authoritative_snapshots() {
        let source = include_str!("projection.rs");
        assert!(!source.contains(concat!("Write", "Pool")));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
    }

    #[test]
    fn every_projects_table_is_holding_bound_and_forces_rls() {
        for (ddl, schema, tables) in [
            (
                WRITE_DDL,
                "erpx_projects_write",
                &[
                    "project_guards",
                    "project_node_guards",
                    "project_node_assignees",
                    "project_node_attachments",
                    "project_node_references",
                    "project_time_facts",
                    "project_cost_facts",
                    "workflow_template_versions",
                    "workflow_states",
                    "workflow_transitions",
                    "dependency_guards",
                    "dependency_acceptances",
                    "handoff_guards",
                ][..],
            ),
            (
                READ_DDL,
                "erpx_projects_read",
                &[
                    "project_projection",
                    "hierarchy_closure_projection",
                    "dependency_projection",
                    "handoff_projection",
                ][..],
            ),
        ] {
            let owned = schema_test::select_schema_statements(ddl, &[schema]);
            let expected_count = if schema == "erpx_projects_read" {
                tables.len() + 1
            } else {
                tables.len()
            };
            assert_eq!(owned.matches("CREATE TABLE").count(), expected_count);
            assert!(owned.contains("current_setting('app.holding_id', true)"));
            assert!(owned.contains("ENABLE ROW LEVEL SECURITY"));
            assert!(owned.contains("FORCE ROW LEVEL SECURITY"));
            assert!(owned.contains("holding_id UUID NOT NULL DEFAULT"));
            for table in tables {
                assert!(owned.contains(&format!("'{table}'")));
            }
        }
        let command = include_str!("command.rs");
        assert!(command.contains("set_config('app.holding_id'"));
        assert!(command.contains("WHERE holding_id=$1"));
        let owned = schema_test::select_schema_statements(WRITE_DDL, &["erpx_projects_write"]);
        assert!(!owned.contains("state JSONB"));
        assert!(!command.contains("load_snapshot"));
        assert!(!command.contains("ProjectsOutboxPayload::Snapshot"));
        assert!(command.contains("load_normalized_aggregate"));
        assert!(command.contains("commit_normalized(command, &guard)"));
        assert!(RECORD_READ_DDL.contains("current_holding_id()"));
        assert!(RECORD_READ_DDL.contains("ENABLE ROW LEVEL SECURITY"));
        assert!(RECORD_READ_DDL.contains("FORCE ROW LEVEL SECURITY"));
        assert!(RECORD_READ_DDL.contains("WITH CHECK"));
    }
}
