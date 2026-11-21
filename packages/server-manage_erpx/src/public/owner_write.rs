//! Native Manage ERPX owner-write command building blocks for composition.
pub use crate::adapters::owner_write_postgres::{PgOwnerWrite, OwnerWriteError};
pub use crate::application::owner_write::{
    OwnerWriteCommand, OwnerWritePort, execute_owner_write,
};
pub use crate::domain::owner_write_registry::{
    ME_ATTACHMENT_VERSION_STEP, ME_BETA_FEEDBACK_INSERT, ME_BETA_INVITATION_ACCEPT,
    ME_BETA_INVITATION_INSERT, ME_BETA_MEMBERSHIP_DELETE, ME_BETA_MEMBERSHIP_INSERT, supported,
    validate,
};
