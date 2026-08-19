#![doc = "PostgreSQL command, source, read and projection adapters for Operations."]
#![forbid(unsafe_code)]

mod authority;
mod authority_source;
mod command;
mod projection;
mod read;
mod records;
mod source;

pub use command::{OperationsCommandAdapter, OperationsWriteRepository};
pub use projection::{
    OperationsExceptionProjector, OperationsOutboxPayload, OperationsSnapshot,
    OperationsSourceSignalProjector, OperationsWorkQueuePolicyProjector,
};
pub use read::{OperationsReadRepository, decode_cursor, encode_cursor};
pub use records::{
    OperationsLifecycleRecordProjector, OperationsLifecycleRecordReadAdapter,
    OperationsLifecycleRecordSqlxAdapter,
    RECORD_PROJECTION_NAME as LIFECYCLE_RECORD_PROJECTION_NAME,
    RECORD_SNAPSHOT_EVENT_TYPE as LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE,
};
pub use source::{OperationsSourceAdapter, OperationsSourceHighWatermarkReader, SourceCheckpoint};

pub const CONTEXT_NAME: &str = "operations";
pub const EXCEPTION_AGGREGATE_TYPE: &str = "operations_exception";
pub const POLICY_AGGREGATE_TYPE: &str = "operations_work_queue_policy";
pub const EXCEPTION_PROJECTION: &str = "operations.exceptions";
pub const SOURCE_SIGNAL_PROJECTION: &str = "operations.source_signals";
pub const POLICY_PROJECTION: &str = "operations.work_queue_policies";
pub const EXCEPTION_SNAPSHOT_EVENT: &str = "operations.exception_snapshot";
pub const POLICY_SNAPSHOT_EVENT: &str = "operations.work_queue_policy_snapshot";
pub const EVENT_VERSION: u32 = 1;
#[cfg(test)]
const NORMALIZED_AUTHORITY_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
pub const COMPLETION_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
pub const COMPLETION_READ_DDL: &str =
    include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");

#[cfg(test)]
const WRITE_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
#[cfg(test)]
const READ_DDL: &str = include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");

pub fn exception_partition(id: erpx_operations::ExceptionId) -> String {
    format!("operations.exception:{}", id.as_uuid().simple())
}

pub fn policy_partition(id: erpx_operations::WorkQueuePolicyId) -> String {
    format!("operations.policy:{}", id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn frozen_schema_and_partitions_are_stable() {
        let exception = erpx_operations::ExceptionId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let policy = erpx_operations::WorkQueuePolicyId::try_from_uuid(Uuid::from_u128(2)).unwrap();
        assert!(exception_partition(exception).starts_with("operations.exception:"));
        assert!(policy_partition(policy).starts_with("operations.policy:"));
        assert!(WRITE_DDL.contains("source_ingestion_checkpoints"));
        assert!(READ_DDL.contains(EXCEPTION_PROJECTION));
        assert!(READ_DDL.contains(SOURCE_SIGNAL_PROJECTION));
        assert!(READ_DDL.contains(POLICY_PROJECTION));
    }

    #[test]
    fn read_adapter_has_no_write_store_fallback() {
        let source = include_str!("read.rs");
        assert!(!source.contains(concat!("Write", "Pool")));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
    }

    #[test]
    fn source_adapter_cas_is_atomic_and_database_qualified() {
        let source = include_str!("source.rs");
        assert!(source.contains("source_checkpoint_authority"));
        assert!(source.contains("FOR UPDATE"));
        assert!(source.contains("position.database.as_str()"));
        assert!(source.contains("commit_normalized"));
        assert!(!source.contains("load_snapshot"));
        assert!(!source.contains("commit_guarded"));
    }

    #[test]
    fn all_source_fact_variants_have_typed_relational_authority() {
        let mapping = include_str!("authority_source.rs");
        for table in [
            "source_commercial_authority",
            "source_customer_authority",
            "source_marketing_authority",
            "source_marketing_approval_authority",
            "source_crm_authority",
            "source_procurement_authority",
            "source_inventory_authority",
            "source_assets_authority",
            "source_maintenance_authority",
            "source_quality_authority",
            "source_sales_authority",
            "source_people_payroll_authority",
            "source_pos_sale_authority",
            "source_pos_refund_authority",
            "source_finance_authority",
            "source_legal_authority",
            "source_support_authority",
            "source_support_incident_authority",
        ] {
            assert!(mapping.contains(table), "missing live mapping for {table}");
            assert!(NORMALIZED_AUTHORITY_DDL.contains(table));
        }
        assert!(!mapping.contains("serde_json::to_value(fact)"));
        assert!(!NORMALIZED_AUTHORITY_DDL.contains("aggregate_components"));
    }
}
