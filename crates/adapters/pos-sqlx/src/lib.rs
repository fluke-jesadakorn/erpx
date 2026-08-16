#![doc = "PostgreSQL command, query, and projection adapters for POS."]

mod command;
mod projection;
mod read;
mod records;

pub use command::{PosCommandAdapter, PosWriteRepository};
pub use projection::{
    PosOutboxPayload, PosRefundProjector, PosRefundSnapshot, PosSaleProjector, PosSaleSnapshot,
};
pub use read::{PosSqlxReadRepository, decode_refund_cursor, decode_sale_cursor};
pub use records::{
    PosLifecycleRecordProjector, PosLifecycleRecordReadAdapter, PosLifecycleRecordSqlxAdapter,
    RECORD_PROJECTION_NAME as LIFECYCLE_RECORD_PROJECTION_NAME,
    RECORD_SNAPSHOT_EVENT_TYPE as LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE,
};

pub const CONTEXT_NAME: &str = "pos";
pub const SALE_AGGREGATE_TYPE: &str = "sale";
pub const REFUND_AGGREGATE_TYPE: &str = "refund";
pub const SALE_PROJECTION_NAME: &str = "pos.sales";
pub const REFUND_PROJECTION_NAME: &str = "pos.refunds";
pub const SALE_SNAPSHOT_EVENT_TYPE: &str = "pos.sale_snapshot";
pub const REFUND_SNAPSHOT_EVENT_TYPE: &str = "pos.refund_snapshot";
pub const EVENT_VERSION: u32 = 1;
pub const COMPLETION_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0080_current_record_completion.sql");
pub const COMPLETION_READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0071_current_record_completion_projection.sql"
);

#[cfg(test)]
pub const TENANT_WRITE_FOUNDATION_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0057_cqrs_foundation.sql");
#[cfg(test)]
pub const TENANT_WRITE_CONTEXT_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0061_wave7_transactional_context_guards.sql"
);
#[cfg(test)]
pub const TENANT_READ_FOUNDATION_DDL: &str =
    include_str!("../../../migration/migrations_tenant_read/0048_cqrs_foundation.sql");
#[cfg(test)]
pub const TENANT_READ_CONTEXT_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0053_wave7_transactional_context_projections.sql"
);
#[cfg(test)]
pub const WRITE_DDL: &str = concat!(
    include_str!("../../../migration/migrations_tenant_write/0057_cqrs_foundation.sql"),
    "\n",
    include_str!(
        "../../../migration/migrations_tenant_write/0061_wave7_transactional_context_guards.sql"
    )
);
#[cfg(test)]
pub const READ_DDL: &str = concat!(
    include_str!("../../../migration/migrations_tenant_read/0048_cqrs_foundation.sql"),
    "\n",
    include_str!(
        "../../../migration/migrations_tenant_read/0053_wave7_transactional_context_projections.sql"
    )
);

pub fn sale_partition(sale_id: erpx_pos::SaleId) -> String {
    format!("pos.sale:{}", sale_id.as_uuid().simple())
}

pub fn refund_partition(refund_id: erpx_pos::RefundId) -> String {
    format!("pos.refund:{}", refund_id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn partitions_are_stable_and_do_not_collide() {
        let sale = erpx_pos::SaleId::try_from_uuid(Uuid::from_u128(42)).unwrap();
        let refund = erpx_pos::RefundId::try_from_uuid(Uuid::from_u128(42)).unwrap();
        assert_eq!(
            sale_partition(sale),
            "pos.sale:0000000000000000000000000000002a"
        );
        assert_eq!(
            refund_partition(refund),
            "pos.refund:0000000000000000000000000000002a"
        );
        assert_ne!(sale_partition(sale), refund_partition(refund));
    }

    #[test]
    fn ddl_matches_frozen_transaction_and_projection_contracts() {
        assert!(WRITE_DDL.contains("erpx_pos_write.sale_guards"));
        assert!(WRITE_DDL.contains("erpx_pos_write.refund_line_guards"));
        assert!(READ_DDL.contains("erpx_pos_read.sale_projection"));
        assert!(READ_DDL.contains("erpx_pos_read.refund_projection"));
        assert!(READ_DDL.contains("generation UUID NOT NULL"));
    }

    #[test]
    fn read_adapter_cannot_access_command_snapshots() {
        let source = include_str!("read.rs");
        assert!(!source.contains("WritePool"));
        assert!(!source.contains("aggregate_snapshots"));
        assert!(!source.contains("erpx_pos_write"));
    }
}
