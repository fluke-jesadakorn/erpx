#![doc = "PostgreSQL CQRS adapters for Inventory."]

mod command;
mod projection;
mod read;
mod records;

pub use command::{InventoryCommandAdapter, InventoryWriteRepository};
pub use projection::{InventoryOutboxPayload, InventoryProjector, InventorySnapshot};
pub use read::InventoryReadRepository;
pub use records::{
    InventoryLifecycleRecordProjector, InventoryLifecycleRecordReadAdapter,
    InventoryLifecycleRecordSqlxAdapter,
    RECORD_PROJECTION_NAME as LIFECYCLE_RECORD_PROJECTION_NAME,
    RECORD_SNAPSHOT_EVENT_TYPE as LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE,
};

pub const CONTEXT_NAME: &str = "inventory";
pub const AGGREGATE_TYPE: &str = "stock_item";
pub const PROJECTION_NAME: &str = "inventory.stock_items";
pub const SNAPSHOT_EVENT_TYPE: &str = "inventory.stock_item_snapshot";
pub const EVENT_VERSION: u32 = 1;
pub const COMPLETION_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0080_current_record_completion.sql");
pub const COMPLETION_READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0071_current_record_completion_projection.sql"
);
pub const NORMALIZED_AUTHORITY_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0081_current_normalized_authority.sql"
);

pub const TENANT_WRITE_FOUNDATION_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0057_cqrs_foundation.sql");
pub const TENANT_WRITE_CONTEXT_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0060_wave6_paired_context_guards.sql");
pub const TENANT_READ_FOUNDATION_DDL: &str =
    include_str!("../../../migration/migrations_tenant_read/0048_cqrs_foundation.sql");
pub const TENANT_READ_CONTEXT_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0052_wave6_paired_context_projections.sql"
);
pub const WRITE_DDL: &str = concat!(
    include_str!("../../../migration/migrations_tenant_write/0057_cqrs_foundation.sql"),
    "\n",
    include_str!("../../../migration/migrations_tenant_write/0060_wave6_paired_context_guards.sql")
);
pub const READ_DDL: &str = concat!(
    include_str!("../../../migration/migrations_tenant_read/0048_cqrs_foundation.sql"),
    "\n",
    include_str!(
        "../../../migration/migrations_tenant_read/0052_wave6_paired_context_projections.sql"
    )
);

pub fn stock_item_partition(id: erpx_inventory::StockItemId) -> String {
    format!("inventory.stock_item:{}", id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn contract_names_and_partition_are_stable() {
        let id = erpx_inventory::StockItemId::try_from_uuid(Uuid::from_u128(42)).unwrap();
        assert_eq!(
            stock_item_partition(id),
            "inventory.stock_item:0000000000000000000000000000002a"
        );
        assert!(TENANT_WRITE_CONTEXT_DDL.contains("movement_guards"));
        assert!(TENANT_READ_CONTEXT_DDL.contains("inventory.stock_items"));
    }

    #[test]
    fn read_adapter_has_no_write_fallback() {
        let source = include_str!("read.rs");
        assert!(!source.contains("WritePool"));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
    }

    #[test]
    fn stock_commands_use_typed_root_and_child_authority() {
        let source = include_str!("command.rs");
        assert!(source.contains("stock_item_authority"));
        assert!(source.contains("movement_authority"));
        assert!(source.contains("reservation_authority"));
        assert!(source.contains("commit_normalized"));
        assert!(!source.contains("load_snapshot"));
        assert!(NORMALIZED_AUTHORITY_DDL.contains("FOREIGN KEY (tenant_id,stock_item_id)"));
    }
}
