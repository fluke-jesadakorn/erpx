//! Native PMS owner-write port (wf1a2 full retirement slice).
//!
//! Composition supplies the current Account/assignment authority and the
//! serializable unit; this module mints no authority. The owner domain owns
//! every SQL statement and the executor writes only pms-owned rows.
pub use crate::adapters::pms_owner_write_http::{
    PmsOwnerWriteBackend, PmsOwnerWriteFuture, PmsOwnerWriteRequest, PmsOwnerWriteTarget,
    pms_owner_write_company_router, pms_owner_write_router,
};
pub use crate::adapters::pms_owner_write_postgres::PgPmsOwnerWrite;
pub use crate::application::pms_owner_write::{
    OwnerWriteLedgerEntry, PmsOwnerWriteActor, PmsOwnerWriteFuture as PmsOwnerWriteUseCaseFuture,
    PmsOwnerWriteRepository, StoredOwnerReceipt, execute as execute_owner_write,
};
pub use crate::domain::pms_owner_write::{
    KINDS, PmsOwnerStatement, statement as owner_write_statement, supported as owner_write_kind_supported,
};
