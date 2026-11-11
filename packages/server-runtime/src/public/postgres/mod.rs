//! Infrastructure adapter API. Portable owner cores define their own ports.
pub use crate::adapters::postgres::PersistenceError;
pub use crate::adapters::postgres::attestation::{
    WorkerAttestationRequest, WorkerDatabaseAttestation, attest_worker_database,
};
pub use crate::adapters::postgres::config::{DatabaseBinding, DatabaseConfig, DatabaseManifest};
pub use crate::adapters::postgres::migrations::{
    MigrationFile, MigrationRun, MigrationStatus, MigrationTrack, inspect_migrations,
    load_migrations, migrate_database,
};
pub use crate::adapters::postgres::pool::{DatabasePool, PoolLimits, connect_pool};
pub use crate::adapters::postgres::request_lease::PgRequestLease;
pub use crate::adapters::postgres::unit_of_work::{
    PgTransactionError, PgUnitOfWork, PgWorkFuture, SessionLock, TransactionOptions,
};

pub use crate::adapters::postgres::jobs::{
    LeaseCompletion, claim, complete, complete_lease, fail, renew, requeue_dead_letter,
    requeue_dead_letter_with_audit,
};
pub use crate::adapters::postgres::outbox_supervisor::{
    OutboxAttestation, OutboxHandlerRegistry, OutboxHolding, OutboxHoldingOutcome,
    OutboxPassReport, OutboxRunSummary, OutboxSupervisor, OutboxSupervisorError,
    OutboxSupervisorPolicy, attest_outbox_holding, failure_log_line, record_outbox_attestation,
    restart_log_line, run_outbox_supervisor,
};
pub use crate::adapters::postgres::worker::{
    OutboxJobHandler, WorkerFuture, WorkerOutcome, WorkerSupervisor, WorkerTarget, worker_failure,
};
pub use crate::domain::jobs::{
    DEFAULT_BUSY_POLL_MILLIS, DEFAULT_IDLE_POLL_MILLIS, DEFAULT_LEASE_MILLIS,
    DEFAULT_LEASE_REFRESH_MILLIS, DEFAULT_MAX_EVENTS_PER_PASS, DEFAULT_RESTART_BACKOFF_MILLIS,
    MAX_ATTEMPTS, MAX_EVENTS_PER_PASS_LIMIT, OutboxJob, OutboxRetrySchedule, retry_delay_millis,
    retry_schedule, truncate_error_code,
};

pub use crate::adapters::postgres::development_lease::LeasedDevelopmentDatabase;

pub use crate::adapters::postgres::development_schema::{
    StartupMigrationInventory, verify_current_migration_inventory,
};
