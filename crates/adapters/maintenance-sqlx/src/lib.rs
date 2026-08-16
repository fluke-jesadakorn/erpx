#![doc = "PostgreSQL command, query, projection, and Assets fact-inbox adapters for Maintenance."]

mod command;
mod projection;
mod read;
mod records;

pub use command::{
    MaintenanceAssetFactInbox, MaintenanceAssetFactRepository, MaintenanceCommandAdapter,
    MaintenanceWriteRepository,
};
pub use projection::{MaintenanceOutboxPayload, MaintenanceSnapshot, WorkOrderProjector};
pub use read::{MaintenanceReadRepository, decode_cursor, encode_cursor};
pub use records::{
    MaintenanceLifecycleRecordProjector, MaintenanceLifecycleRecordReadAdapter,
    MaintenanceLifecycleRecordSqlxAdapter,
    RECORD_PROJECTION_NAME as LIFECYCLE_RECORD_PROJECTION_NAME,
    RECORD_SNAPSHOT_EVENT_TYPE as LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE,
};

pub const CONTEXT_NAME: &str = "maintenance";
pub const AGGREGATE_TYPE: &str = "work_order";
pub const PROJECTION_NAME: &str = "maintenance.work_orders";
/// Scheduler identity for the Maintenance-owned inbox consuming typed Assets facts.
/// This is deliberately distinct from the published event type and from the
/// work-order read model so each worker claims one exact durable job stream.
pub const ASSET_FACT_PROJECTION_NAME: &str = "maintenance.asset_facts";
pub const SNAPSHOT_EVENT_TYPE: &str = "maintenance.work_order_snapshot";
pub const ASSET_FACT_EVENT_TYPE: &str = "assets.condition_changed";
pub const EVENT_VERSION: u32 = 1;

pub const TENANT_WRITE_FOUNDATION_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0057_cqrs_foundation.sql");
pub const TENANT_WRITE_CONTEXT_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0060_wave6_paired_context_guards.sql");
pub const TENANT_READ_FOUNDATION_DDL: &str =
    include_str!("../../../migration/migrations_tenant_read/0048_cqrs_foundation.sql");
pub const TENANT_READ_CONTEXT_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0052_wave6_paired_context_projections.sql"
);
pub const COMPLETION_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0080_current_record_completion.sql");
pub const COMPLETION_READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0071_current_record_completion_projection.sql"
);
pub const WRITE_DDL: &str = concat!(
    include_str!("../../../migration/migrations_tenant_write/0057_cqrs_foundation.sql"),
    "\n",
    include_str!("../../../migration/migrations_tenant_write/0060_wave6_paired_context_guards.sql"),
    "\n",
    include_str!(
        "../../../migration/migrations_tenant_write/0081_current_normalized_authority.sql"
    )
);
pub const READ_DDL: &str = concat!(
    include_str!("../../../migration/migrations_tenant_read/0048_cqrs_foundation.sql"),
    "\n",
    include_str!(
        "../../../migration/migrations_tenant_read/0052_wave6_paired_context_projections.sql"
    )
);

pub fn work_order_partition(id: erpx_maintenance::WorkOrderId) -> String {
    format!("maintenance.work_order:{}", id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn partition_is_stable_and_context_owned() {
        let id = erpx_maintenance::WorkOrderId::try_from_uuid(Uuid::from_u128(42)).unwrap();
        assert_eq!(
            work_order_partition(id),
            "maintenance.work_order:0000000000000000000000000000002a"
        );
    }

    #[test]
    fn ddl_includes_fact_inbox_source_uniqueness_and_rebuild_rows() {
        assert!(WRITE_DDL.contains("asset_fact_inbox"));
        assert!(WRITE_DDL.contains("source_event_id UUID NOT NULL UNIQUE"));
        assert!(READ_DDL.contains("generation UUID NOT NULL"));
        assert!(READ_DDL.contains("maintenance.work_orders"));
    }

    #[test]
    fn maintenance_never_depends_on_assets_core() {
        let manifest = include_str!("../Cargo.toml");
        assert!(!manifest.contains("erpx-assets ="));
        assert!(!manifest.contains("contexts/assets"));
    }

    #[test]
    fn commands_use_typed_work_order_authority_without_snapshots() {
        let source = include_str!("command.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(!production.contains("load_snapshot"));
        assert!(!production.contains("commit_guarded"));
        assert!(production.contains("work_order_authority"));
        assert!(production.contains("downtime_authority"));
        assert!(production.contains("commit_normalized"));
    }
}
