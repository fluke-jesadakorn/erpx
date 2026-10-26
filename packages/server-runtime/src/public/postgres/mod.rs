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
