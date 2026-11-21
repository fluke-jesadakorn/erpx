//! Native Platform Administration owner-write building blocks for composition.
pub use crate::adapters::owner_write_postgres::{PgOwnerWrite, OwnerWriteError};
pub use crate::application::owner_write::{OwnerWriteCommand, OwnerWritePort, execute_owner_write};
pub use crate::domain::owner_write_registry::{
    supported, validate, PL_ATTACHMENT_VERSION_STEP, PL_LEGAL_RELEASE_REVISION_INIT,
    PL_RELEASE_ACCEPTANCE_INSERT, PL_RELEASE_GATE_REVIEW_UPSERT, PL_RELEASE_REVISION_INIT,
    PL_RELEASE_REVOKE, PL_RELEASE_VERSION_STEP,
};
