#![doc = "PostgreSQL CQRS adapters for Procurement."]

mod command;
mod projection;
mod read;
mod records;

pub use command::{ProcurementCommandAdapter, ProcurementWriteRepository};
pub use projection::{ProcurementOutboxPayload, ProcurementProjector, ProcurementSnapshot};
pub use read::ProcurementReadRepository;
pub use records::{
    ProcurementRecordProjector, ProcurementRecordReadAdapter, ProcurementRecordSqlxAdapter,
    RECORD_PROJECTION_NAME, RECORD_SNAPSHOT_EVENT_TYPE,
};

pub const CONTEXT_NAME: &str = "procurement";
pub const AGGREGATE_TYPE: &str = "purchase_order";
pub const PROJECTION_NAME: &str = "procurement.purchase_orders";
pub const SNAPSHOT_EVENT_TYPE: &str = "procurement.purchase_order_snapshot";
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
pub const NORMALIZED_AUTHORITY_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0081_current_normalized_authority.sql"
);

pub fn purchase_order_partition(id: erpx_procurement::PurchaseOrderId) -> String {
    format!("procurement.purchase_order:{}", id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn contract_names_and_partition_are_stable() {
        let id = erpx_procurement::PurchaseOrderId::try_from_uuid(Uuid::from_u128(42)).unwrap();
        assert_eq!(
            purchase_order_partition(id),
            "procurement.purchase_order:0000000000000000000000000000002a"
        );
        assert!(TENANT_WRITE_CONTEXT_DDL.contains("purchase_order_guards"));
        assert!(TENANT_READ_CONTEXT_DDL.contains("procurement.purchase_orders"));
    }

    #[test]
    fn read_adapter_has_no_write_fallback() {
        let source = include_str!("read.rs");
        assert!(!source.contains("WritePool"));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
    }

    #[test]
    fn purchase_orders_use_typed_header_and_line_authority() {
        let source = include_str!("command.rs");
        assert!(source.contains("purchase_order_authority"));
        assert!(source.contains("purchase_order_line_authority"));
        assert!(source.contains("commit_normalized"));
        assert!(!source.contains("load_snapshot"));
        assert!(NORMALIZED_AUTHORITY_DDL.contains("FOREIGN KEY (tenant_id,purchase_order_id)"));
    }
}
