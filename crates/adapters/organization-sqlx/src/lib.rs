#![doc = "PostgreSQL command, query, and projection adapters for Organization."]

mod command;
mod projection;
mod read;

pub use command::{
    OrganizationCommandAdapter, OrganizationHierarchyRepository, OrganizationWriteRepository,
};
pub use projection::{OrganizationNodeProjector, OrganizationOutboxPayload, OrganizationSnapshot};
pub use read::{OrganizationReadRepository, decode_cursor, encode_cursor};

pub const CONTEXT_NAME: &str = "organization";
pub const AGGREGATE_TYPE: &str = "organization_node";
pub const PROJECTION_NAME: &str = "organization.nodes";
pub const SNAPSHOT_EVENT_TYPE: &str = "organization.node_snapshot";
pub const EVENT_VERSION: u32 = 1;

pub const WRITE_DDL: &str = include_str!("../ddl/write.sql");
pub const READ_DDL: &str = include_str!("../ddl/read.sql");

pub fn node_partition(node_id: erpx_organization::OrganizationNodeId) -> String {
    format!("organization.node:{}", node_id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn partition_is_stable_and_node_specific() {
        let id = erpx_organization::OrganizationNodeId::try_from_uuid(Uuid::from_u128(42)).unwrap();
        assert_eq!(
            node_partition(id),
            "organization.node:0000000000000000000000000000002a"
        );
    }

    #[test]
    fn ddl_declares_atomic_hierarchy_guard_and_generation_rows() {
        assert!(WRITE_DDL.contains("hierarchy_guards"));
        assert!(WRITE_DDL.contains("organization_sibling_code"));
        assert!(READ_DDL.contains("generation UUID NOT NULL"));
        assert!(READ_DDL.contains("organization.nodes"));
    }

    #[test]
    fn read_adapter_has_no_write_side_capability() {
        let source = include_str!("read.rs");
        assert!(!source.contains("WritePool"));
        assert!(!source.contains("aggregate_snapshots"));
        assert!(!source.contains("hierarchy_guards"));
    }
}
