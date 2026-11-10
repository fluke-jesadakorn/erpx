//! Infrastructure adapter API. Portable owner cores define their own ports.
pub use crate::adapters::postgres::PersistenceError;
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

pub use crate::adapters::postgres::jobs::{claim, complete, fail, renew};
pub use crate::adapters::postgres::worker::{
    OutboxJobHandler, WorkerFuture, WorkerSupervisor, WorkerTarget, worker_failure,
};
pub use crate::domain::jobs::{
    DEFAULT_LEASE_MILLIS, MAX_ATTEMPTS, OutboxJob, retry_delay_millis, truncate_error_code,
};

pub use crate::adapters::postgres::development_lease::LeasedDevelopmentDatabase;

pub use crate::adapters::postgres::development_schema::{
    StartupMigrationInventory, verify_current_migration_inventory,
};
