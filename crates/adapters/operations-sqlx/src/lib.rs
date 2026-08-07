#![doc = "PostgreSQL command, source, read and projection adapters for Operations."]
#![forbid(unsafe_code)]

mod command;
mod projection;
mod read;
mod source;

pub use command::{OperationsCommandAdapter, OperationsWriteRepository};
pub use projection::{
    OperationsExceptionProjector, OperationsOutboxPayload, OperationsSnapshot,
    OperationsSourceSignalProjector, OperationsWorkQueuePolicyProjector,
};
pub use read::{OperationsReadRepository, decode_cursor, encode_cursor};
pub use source::{OperationsSourceAdapter, SourceCheckpoint};

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
const WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0063_wave9_operations_guards.sql");
#[cfg(test)]
const READ_DDL: &str =
    include_str!("../../../migration/migrations_tenant_read/0055_wave9_operations_projections.sql");

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
        assert!(source.contains("source_ingestion_checkpoints"));
        assert!(source.contains("FOR UPDATE"));
        assert!(source.contains("position.database.as_str()"));
        assert!(source.contains("commit_guarded"));
    }
}
