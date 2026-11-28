//! The owner validates and retains only its own Control preference relations.
pub const CONTRACT_VERSION: &str = "work-projection.account-preferences.v1";
pub use crate::adapters::account_workspace_preferences::{
    AccountCompanyPreference, AccountWorkspacePreference, CurrentAccountWorkspacePreferences,
    lock_account_workspace_preferences, read_account_workspace_preferences,
};
