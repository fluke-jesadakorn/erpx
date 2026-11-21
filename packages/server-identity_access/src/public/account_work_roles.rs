//! Account-only role recovery facts are not business authorization.
pub const CONTRACT_VERSION: &str = "identity.account-work-roles.v1";
pub use crate::adapters::account_work_roles::{
    AccountWorkRoleChoice, AccountWorkSelection, AccountWorkspaceChoice, CurrentAccountWorkRoles,
    RetainedAccountWorkSelection, lock_account_work_roles, read_account_work_roles,
};
