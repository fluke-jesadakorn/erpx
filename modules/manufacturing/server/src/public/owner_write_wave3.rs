//! Native Manufacturing owner-write registry building blocks for composition.
pub use crate::adapters::owner_write_wave3_postgres::PgOwnerWrite;
pub use crate::application::owner_write_wave3::{
    LedgerEntry, OwnerWriteFuture, OwnerWriteRepository, StoredReceipt, execute_owner_write,
};
pub use crate::domain::owner_write_wave3::{
    OWNER_WRITE_OPERATION, OwnerWrite, company, invalid, kind_spec, parse_owner_write, unavailable,
};
