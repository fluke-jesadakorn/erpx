//! Native Billing owner-wide write command.
pub use crate::adapters::billing_owner_write::{
    BillingOwnerStatement, BillingOwnerWriteInput, billing_owner_statement,
};
pub use crate::application::billing_owner_write::build_owner_write;
pub use crate::domain::billing_owner_write::{
    OwnerWriteKind, OWNER_WRITE_KINDS,
};
