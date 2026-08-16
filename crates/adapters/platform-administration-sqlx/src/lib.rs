#![doc = "PostgreSQL command, query, and projection adapters for Platform Administration."]

mod command;
mod governance;
mod projection;
mod read;

pub use command::{PlatformAdministrationCommandAdapter, PlatformAdministrationWriteRepository};
pub use governance::{
    GOVERNANCE_PROJECTION_NAME, GOVERNANCE_SNAPSHOT_EVENT_TYPE, PlatformGovernanceRecordProjector,
    PlatformGovernanceRecordReadAdapter, PlatformGovernanceRecordSqlxAdapter, governance_partition,
};
pub use projection::{
    AdminContextProjector, ConnectorProjector, PlatformAdministrationOutboxPayload,
    PlatformAdministrationSnapshot, TenantProjector,
};
pub use read::{PlatformAdministrationReadRepository, decode_cursor, encode_cursor};

pub const CONTEXT_NAME: &str = "platform_administration";
pub const TENANT_AGGREGATE: &str = "platform_tenant";
pub const CONNECTOR_AGGREGATE: &str = "connector_binding";
pub const ADMIN_CONTEXT_AGGREGATE: &str = "tenant_admin_context";
pub const TENANT_PROJECTION: &str = "platform_administration.tenants";
pub const CONNECTOR_PROJECTION: &str = "platform_administration.connectors";
pub const ADMIN_CONTEXT_PROJECTION: &str = "platform_administration.admin_contexts";
pub const TENANT_SNAPSHOT_EVENT: &str = "platform_administration.tenant_snapshot";
pub const CONNECTOR_SNAPSHOT_EVENT: &str = "platform_administration.connector_snapshot";
pub const ADMIN_CONTEXT_SNAPSHOT_EVENT: &str = "platform_administration.admin_context_snapshot";
pub const EVENT_VERSION: u32 = 1;

pub const CONTROL_WRITE_DDL: &str = concat!(
    include_str!(
        "../../../migration/migrations/0075_cqrs_control_foundation_and_commercial_catalog.sql"
    ),
    "\n",
    include_str!("../../../migration/migrations/0076_wave5_control_context_guards.sql"),
    "\n",
    include_str!("../../../migration/migrations/0082_current_normalized_authority.sql")
);
pub const CONTROL_READ_DDL: &str = concat!(
    include_str!(
        "../../../migration/migrations_read/0015_cqrs_control_foundation_and_commercial_catalog.sql"
    ),
    "\n",
    include_str!("../../../migration/migrations_read/0016_wave5_control_context_projections.sql")
);
pub const WRITE_DDL: &str = CONTROL_WRITE_DDL;
pub const READ_DDL: &str = CONTROL_READ_DDL;

pub fn tenant_partition(id: erpx_platform_administration::PlatformTenantId) -> String {
    format!("platform_administration.tenant:{}", id.as_uuid().simple())
}
pub fn connector_partition(id: erpx_platform_administration::ConnectorBindingId) -> String {
    format!(
        "platform_administration.connector:{}",
        id.as_uuid().simple()
    )
}
pub fn admin_context_partition(id: erpx_platform_administration::AdminContextId) -> String {
    format!(
        "platform_administration.admin_context:{}",
        id.as_uuid().simple()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn partitions_are_stable_and_aggregate_specific() {
        let raw = Uuid::from_u128(42);
        assert_ne!(
            tenant_partition(
                erpx_platform_administration::PlatformTenantId::try_from_uuid(raw).unwrap()
            ),
            connector_partition(
                erpx_platform_administration::ConnectorBindingId::try_from_uuid(raw).unwrap()
            )
        );
    }

    #[test]
    fn canonical_control_ddl_contains_foundation_guards_and_generation_projections() {
        for table in ["tenant_guards", "connector_guards", "admin_context_guards"] {
            assert!(CONTROL_WRITE_DDL.contains(table));
        }
        for table in [
            "tenant_projection",
            "connector_projection",
            "admin_context_projection",
        ] {
            assert!(CONTROL_READ_DDL.contains(table));
        }
        assert!(CONTROL_WRITE_DDL.contains("command_receipts"));
        assert!(CONTROL_READ_DDL.contains("projection_generations"));
    }

    #[test]
    fn read_adapter_has_no_write_capability_or_snapshot_fallback() {
        let source = include_str!("read.rs");
        assert!(!source.contains("WritePool"));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
    }
}
