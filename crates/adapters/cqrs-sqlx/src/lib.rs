#![doc = "PostgreSQL adapters for ERPX command persistence and read projections."]

mod command;
mod error;
mod outbox;
mod pools;
mod projection;
mod waiter;

pub use command::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, GuardCommandContext, OutboxEventWrite, RequestHash,
};
pub use error::{CqrsSqlxError, ProjectionFailure};
pub use outbox::{
    ClaimedOutboxEvent, MAX_OUTBOX_CLAIM_SIZE, OutboxDeliveryStore, OutboxRetryPolicy,
};
pub use pools::{
    CqrsDatabaseConfig, CqrsPoolSettings, CqrsPools, DatabaseManifest, DatabasePlane, DatabaseRole,
    ProjectionPool, REQUIRED_SCHEMA_GENERATION, ReadPool, WritePool, verify_database_role,
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
    use std::{fs, path::Path};

    use super::*;

    #[test]
    fn write_contract_keeps_the_command_unit_atomic() {
        for table in ["command_receipts", "audit_log", "outbox_events"] {
            assert!(WRITE_DDL.contains(table), "missing {table}");
        }
        assert!(!WRITE_DDL.contains("aggregate_snapshots"));
        assert!(WRITE_DDL.contains("octet_length(request_hash) = 32"));
        for delivery_field in ["claim_token", "claim_until", "dead_lettered_at"] {
            assert!(
                WRITE_DDL.contains(delivery_field),
                "missing {delivery_field}"
            );
        }
        for contract in [
            "PRIMARY KEY (tenant_id",
            "FORCE ROW LEVEL SECURITY",
            "WITH CHECK (tenant_id =",
        ] {
            assert!(
                WRITE_DDL.contains(contract),
                "missing tenant write contract: {contract}"
            );
        }
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
        for contract in [
            "PRIMARY KEY (tenant_id",
            "FOREIGN KEY (tenant_id, projection_name, generation)",
            "FORCE ROW LEVEL SECURITY",
            "WITH CHECK (tenant_id =",
        ] {
            assert!(
                READ_DDL.contains(contract),
                "missing tenant read contract: {contract}"
            );
        }
    }

    #[test]
    fn pool_capabilities_do_not_expose_a_raw_tenant_connection() {
        let source = include_str!("pools.rs");
        assert!(!source.contains(concat!("as_pg_", "pool")));
        assert!(source.contains("SELECT set_config('app.tenant_id',$1,true)"));
        assert!(source.contains("if actual != expected"));
    }

    #[test]
    fn production_sqlx_adapters_cannot_call_the_removed_snapshot_authority() {
        let adapter_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("cqrs-sqlx has an adapters parent");
        let mut rust_sources = Vec::new();
        collect_rust_sources(adapter_root, &mut rust_sources);
        assert!(
            !rust_sources.is_empty(),
            "adapter source inventory is empty"
        );

        for path in rust_sources {
            let source = fs::read_to_string(&path).expect("adapter source is readable");
            for removed in [
                concat!(".load_", "snapshot("),
                concat!(".commit_", "guarded("),
                concat!("aggregate_", "state:"),
            ] {
                assert!(
                    !source.contains(removed),
                    "removed snapshot authority call {removed} remains in {}",
                    path.display()
                );
            }
        }

        let command = include_str!("command.rs");
        assert!(!command.contains("erpx_cqrs_write.aggregate_snapshots"));
        assert!(!command.contains(concat!("pub aggregate_", "state")));
        assert!(!WRITE_DDL.contains("aggregate_snapshots"));
    }

    fn collect_rust_sources(directory: &Path, sources: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(directory).expect("adapter directory is readable") {
            let path = entry.expect("adapter directory entry is readable").path();
            if path.is_dir() {
                collect_rust_sources(&path, sources);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                sources.push(path);
            }
        }
    }
}
