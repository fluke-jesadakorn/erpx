#![doc = "PostgreSQL command, query, and projection adapters for Identity & Access."]

mod command;
mod governance;
mod projection;
mod read;

pub use command::{IdentityAccessCommandAdapter, IdentityAccessWriteRepository};
pub use governance::{
    IdentityGovernanceProjector, IdentityGovernanceReadAdapter, IdentityGovernanceSqlxAdapter,
};
pub use projection::{
    IdentityAccessOutboxPayload, IdentityAccessPrincipalProjector, IdentityAccessRoleProjector,
    IdentityAccessSnapshot,
};
pub use read::{IdentityAccessReadRepository, decode_cursor, encode_cursor};

pub const CONTEXT_NAME: &str = "identity_access";
pub const PRINCIPAL_AGGREGATE_TYPE: &str = "access_principal";
pub const ROLE_AGGREGATE_TYPE: &str = "access_role";
pub const PRINCIPAL_PROJECTION_NAME: &str = "identity_access.principals";
pub const ROLE_PROJECTION_NAME: &str = "identity_access.roles";
pub const PRINCIPAL_SNAPSHOT_EVENT_TYPE: &str = "identity_access.principal_snapshot";
pub const ROLE_SNAPSHOT_EVENT_TYPE: &str = "identity_access.role_snapshot";
pub const GOVERNANCE_PROJECTION_NAME: &str = "identity_access.governance";
pub const GOVERNANCE_SNAPSHOT_EVENT_TYPE: &str = "identity_access.governance_snapshot";
pub const EVENT_VERSION: u32 = 1;

pub const WRITE_DDL: &str = include_str!("../ddl/write.sql");
pub const READ_DDL: &str = include_str!("../ddl/read.sql");
/// Typed relational principal/role authority introduced after the shared CQRS
/// foundation. Child assignments, permissions, and group memberships remain
/// relational and holding-composite.
pub const NORMALIZED_AUTHORITY_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");

pub fn principal_partition(id: erpx_identity_access::PrincipalId) -> String {
    format!("identity_access.principal:{}", id.as_uuid().simple())
}

pub fn role_partition(id: erpx_identity_access::RoleId) -> String {
    format!("identity_access.role:{}", id.as_uuid().simple())
}

pub fn governance_partition(id: uuid::Uuid) -> String {
    format!("identity_access.governance:{}", id.simple())
}

#[cfg(test)]
mod tests {
    use erpx_identity_access::{PrincipalId, RoleId};
    use uuid::Uuid;

    use super::*;

    #[test]
    fn partitions_are_stable_and_aggregate_specific() {
        let principal = PrincipalId::try_from_uuid(Uuid::from_u128(42)).unwrap();
        let role = RoleId::try_from_uuid(Uuid::from_u128(42)).unwrap();
        assert_eq!(
            principal_partition(principal),
            "identity_access.principal:0000000000000000000000000000002a"
        );
        assert_eq!(
            role_partition(role),
            "identity_access.role:0000000000000000000000000000002a"
        );
    }

    #[test]
    fn ddl_declares_context_guards_and_rebuildable_projections() {
        assert!(WRITE_DDL.contains("role_code_guards"));
        assert!(WRITE_DDL.contains("assignment_guards"));
        assert!(READ_DDL.contains("principal_projection"));
        assert!(READ_DDL.contains("role_projection"));
    }

    #[test]
    fn read_adapter_has_no_write_side_capability() {
        let source = include_str!("read.rs");
        assert!(!source.contains("WritePool"));
        assert!(!source.contains("aggregate_snapshots"));
        assert!(!source.contains("assignment_guards"));
    }

    #[test]
    fn commands_use_typed_relational_authority_in_the_atomic_transaction() {
        let source = include_str!("command.rs");
        assert!(source.contains("commit_normalized"));
        assert!(source.contains("AtomicCommandGuard"));
        assert!(source.contains("principal_authority"));
        assert!(source.contains("role_authority"));
        assert!(source.contains("assignment_authority"));
        assert!(source.contains("group_membership_authority"));
        assert!(source.contains("role_code_guards"));
        assert!(source.contains("assignment_guards"));
        assert!(source.contains("pg_advisory_xact_lock"));
        assert!(source.contains("CommandFingerprint::RecoverRole"));
        assert!(!source.contains("load_snapshot"));
        assert!(!source.contains("commit_guarded"));
        assert!(!source.contains("transaction.commit"));
        assert!(NORMALIZED_AUTHORITY_DDL.contains("principal_authority"));
        assert!(NORMALIZED_AUTHORITY_DDL.contains("role_permission_authority"));
        assert!(!NORMALIZED_AUTHORITY_DDL.contains("aggregate_components"));
    }
}
