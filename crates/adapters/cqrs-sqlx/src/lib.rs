#![doc = "PostgreSQL adapters for ERPX command persistence and read projections."]

mod command;
mod error;
mod pools;
mod projection;
mod waiter;

pub use command::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, GuardCommandContext, NoopAtomicCommandGuard, OutboxEventWrite, RequestHash,
    StoredSnapshot,
};
pub use error::{CqrsSqlxError, ProjectionFailure};
pub use pools::{
    CqrsDatabaseConfig, CqrsPoolSettings, CqrsPools, ProjectionPool, ReadPool, WritePool,
};
pub use projection::{
    ApplyDisposition, EventPage, MultiSourceProjectionRun, MultiSourceProjectionRunner,
    ProjectionEvent, ProjectionGeneration, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionSourceCheckpoint, ProjectionSourceFence, ProjectionSourceKey,
    ProjectionStore, Projector, ProjectorContext, RebuildHandle, RetryPolicy, SourceProjectionRun,
};
pub use waiter::{ReadSideReceipt, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig};

/// Version-neutral DDL contract for the command database.
pub const WRITE_DDL: &str = include_str!("../ddl/write.sql");

/// Version-neutral DDL contract for the read/projection database.
pub const READ_DDL: &str = include_str!("../ddl/read.sql");

#[cfg(test)]
mod ddl_contract_tests {
    use super::*;

    #[test]
    fn write_contract_keeps_the_command_unit_atomic() {
        for table in [
            "aggregate_snapshots",
            "command_receipts",
            "audit_log",
            "outbox_events",
        ] {
            assert!(WRITE_DDL.contains(table), "missing {table}");
        }
        assert!(WRITE_DDL.contains("octet_length(request_hash) = 32"));
    }

    #[test]
    fn read_contract_supports_receipts_retries_and_shadow_generations() {
        for table in [
            "projection_generations",
            "projection_checkpoints",
            "projection_applied_events",
            "projection_receipts",
            "projection_failures",
        ] {
            assert!(READ_DDL.contains(table), "missing {table}");
        }
        assert!(READ_DDL.contains("status = 'active'"));
    }
}
