#![doc = "PostgreSQL command, query, and projection adapters for Organization."]

mod command;
mod projection;
mod read;
mod sharing;
mod sharing_read;

pub use command::{
    OrganizationCommandAdapter, OrganizationHierarchyRepository, OrganizationWriteRepository,
};
pub use projection::{OrganizationNodeProjector, OrganizationOutboxPayload, OrganizationSnapshot};
pub use read::{OrganizationReadRepository, decode_cursor, encode_cursor};
pub use sharing::OrganizationSharingSqlxAdapter;
pub use sharing_read::{OrganizationSharingProjector, OrganizationSharingReadAdapter};

pub const CONTEXT_NAME: &str = "organization";
pub const AGGREGATE_TYPE: &str = "organization_node";
pub const PROJECTION_NAME: &str = "organization.nodes";
pub const SNAPSHOT_EVENT_TYPE: &str = "organization.node_snapshot";
pub const EVENT_VERSION: u32 = 1;
pub const SHARING_PROJECTION_NAME: &str = "organization.sharing-agreements";
pub const SHARING_SNAPSHOT_EVENT_TYPE: &str = "organization.sharing_snapshot";

pub const WRITE_DDL: &str = include_str!("../ddl/write.sql");
pub const READ_DDL: &str = include_str!("../ddl/read.sql");
pub const NORMALIZED_AUTHORITY_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0081_current_normalized_authority.sql"
);

pub fn node_partition(node_id: erpx_organization::OrganizationNodeId) -> String {
    format!("organization.node:{}", node_id.as_uuid().simple())
}

pub fn sharing_partition(id: erpx_organization::SharingAgreementId) -> String {
    format!("organization.sharing:{}", id.as_uuid().simple())
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

    #[test]
    fn node_commands_use_typed_tenant_authority() {
        let source = include_str!("command.rs");
        assert!(source.contains("organization_node_authority"));
        assert!(source.contains("commit_normalized"));
        assert!(source.contains("authoritative_version"));
        assert!(!source.contains("load_snapshot"));
        assert!(NORMALIZED_AUTHORITY_DDL.contains("node_name TEXT NOT NULL"));
        assert!(!NORMALIZED_AUTHORITY_DDL.contains("aggregate_components"));
    }
}
