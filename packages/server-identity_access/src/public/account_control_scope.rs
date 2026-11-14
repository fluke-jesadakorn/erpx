//! Opaque Control Account identity for owner-local preferences; never a Work actor.
pub const CONTRACT_VERSION: &str = "identity.holding-account-control.v1";
pub use crate::adapters::account_control_scope::{
    HoldingAccountControlScope, verify_holding_account_control,
};
