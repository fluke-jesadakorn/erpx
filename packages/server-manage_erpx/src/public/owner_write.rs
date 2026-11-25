//! Native Manage ERPX owner-write command building blocks for composition.
pub use crate::adapters::owner_write_postgres::{OwnerWriteError, PgOwnerWrite};
pub use crate::application::owner_write::{execute_owner_write, OwnerWriteCommand, OwnerWritePort};
pub use crate::domain::owner_write_registry::{
    supported, validate, ME_ATTACHMENT_VERSION_STEP, ME_BETA_FEEDBACK_INSERT,
    ME_BETA_INVITATION_ACCEPT, ME_BETA_INVITATION_INSERT, ME_BETA_MEMBERSHIP_DELETE,
    ME_BETA_MEMBERSHIP_INSERT,
};
