#![doc = "PostgreSQL command, query, and projection adapters for Organization."]

mod command;
mod lineage;
mod projection;
mod read;
mod sharing;
mod sharing_read;

pub use command::{
    OrganizationCommandAdapter, OrganizationHierarchyRepository, OrganizationWriteRepository,
};
pub use lineage::{
    MAX_ORGANIZATION_LINEAGE_BATCH_SIZE, OrganizationLineageResolver,
    OrganizationLineageResolverError,
};
pub use projection::{
    OrganizationNodeProjector, OrganizationOutboxPayload, OrganizationSnapshot,
    decode_snapshot_event, lineage_witness_from_snapshot_event,
};
pub use read::{OrganizationReadRepository, decode_cursor, encode_cursor};
pub use sharing::OrganizationSharingSqlxAdapter;
pub use sharing_read::{OrganizationSharingProjector, OrganizationSharingReadAdapter};

pub const CONTEXT_NAME: &str = "organization";
pub const AGGREGATE_TYPE: &str = "organization_node";
pub const PROJECTION_NAME: &str = "organization.nodes";
pub const SNAPSHOT_EVENT_TYPE: &str = "organization.node_snapshot";
pub const LEGACY_NODE_SNAPSHOT_EVENT_VERSION: u32 = 1;
pub const NODE_SNAPSHOT_EVENT_VERSION: u32 = 2;
pub const SHARING_PROJECTION_NAME: &str = "organization.sharing-agreements";
pub const SHARING_SNAPSHOT_EVENT_TYPE: &str = "organization.sharing_snapshot";
pub const SHARING_SNAPSHOT_EVENT_VERSION: u32 = 1;

pub const WRITE_DDL: &str = include_str!("../ddl/write.sql");
pub const READ_DDL: &str = include_str!("../ddl/read.sql");
pub const NORMALIZED_AUTHORITY_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");

/// Returns the canonical node-root scope when `observed` is either already
/// canonical or is the exact Company/Branch witness emitted for
/// Department/Team nodes by snapshot version 1. Every other mismatch is
/// rejected so read and idempotency replay cannot silently heal corruption.
pub(crate) fn canonical_node_scope(
    placement: &erpx_organization::OrganizationPlacement,
    node_id: erpx_organization::OrganizationNodeId,
    observed: &erpx_kernel::AuthorityScope,
) -> Result<erpx_kernel::AuthorityScope, &'static str> {
    let expected = placement
        .active_scope(node_id)
        .map_err(|_| "organization placement is invalid")?;
    if observed == &expected {
        return Ok(expected);
    }
    if legacy_collapsed_scope(placement, node_id).as_ref() == Some(observed) {
        return Ok(expected);
    }
    Err("organization scope is not a canonical or legacy hierarchy witness")
}

pub(crate) fn legacy_collapsed_scope(
    placement: &erpx_organization::OrganizationPlacement,
    node_id: erpx_organization::OrganizationNodeId,
) -> Option<erpx_kernel::AuthorityScope> {
    let holding_id = placement.holding_id(node_id);
    match placement {
        erpx_organization::OrganizationPlacement::Department {
            company_id,
            branch_id,
            ..
        }
        | erpx_organization::OrganizationPlacement::Team {
            company_id,
            branch_id,
            ..
        } => Some(branch_id.map_or_else(
            || erpx_kernel::AuthorityScope::Company {
                holding_id,
                company_id: *company_id,
            },
            |branch_id| erpx_kernel::AuthorityScope::Branch {
                holding_id,
                company_id: *company_id,
                branch_id,
            },
        )),
        _ => None,
    }
}

pub fn node_partition(node_id: erpx_organization::OrganizationNodeId) -> String {
    format!("organization.node:{}", node_id.as_uuid().simple())
}

pub fn sharing_partition(id: erpx_organization::SharingAgreementId) -> String {
    format!("organization.sharing:{}", id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_kernel::AuthorityScope;
    use erpx_organization::{OrganizationNodeId, OrganizationPlacement};
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
    fn node_commands_use_typed_holding_authority() {
        let source = include_str!("command.rs");
        assert!(source.contains("organization_node_authority"));
        assert!(source.contains("commit_normalized"));
        assert!(source.contains("authoritative_version"));
        assert!(!source.contains("load_snapshot"));
        assert!(NORMALIZED_AUTHORITY_DDL.contains("node_name TEXT NOT NULL"));
        assert!(!NORMALIZED_AUTHORITY_DDL.contains("aggregate_components"));
    }

    #[test]
    fn scope_normalization_accepts_only_canonical_or_exact_legacy_witnesses() {
        let holding_id = Uuid::from_u128(1);
        let company_id = Uuid::from_u128(2);
        let branch_id = Uuid::from_u128(3);
        let department_id = OrganizationNodeId::try_from_uuid(Uuid::from_u128(4)).unwrap();
        let department = OrganizationPlacement::Department {
            holding_id,
            company_id,
            branch_id: Some(branch_id),
        };
        let canonical = AuthorityScope::Subtree {
            holding_id,
            organization_node_id: department_id.as_uuid(),
        };
        let legacy = AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        };
        assert_eq!(
            canonical_node_scope(&department, department_id, &canonical),
            Ok(canonical.clone())
        );
        assert_eq!(
            canonical_node_scope(&department, department_id, &legacy),
            Ok(canonical)
        );
        assert!(
            canonical_node_scope(
                &department,
                department_id,
                &AuthorityScope::Company {
                    holding_id,
                    company_id: Uuid::from_u128(99),
                },
            )
            .is_err()
        );

        let company_node = OrganizationNodeId::try_from_uuid(company_id).unwrap();
        let company = OrganizationPlacement::Company { holding_id };
        assert!(
            canonical_node_scope(
                &company,
                company_node,
                &AuthorityScope::Holding { holding_id },
            )
            .is_err()
        );
    }
}
