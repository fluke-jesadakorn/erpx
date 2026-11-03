//! Only Work Projection's preference relation is read/written by this port.
pub const CONTRACT_VERSION:&str="work-projection.selection-preference.v1";
pub use crate::adapters::identity_selection_preferences::{CurrentWorkRolePreference,lock_work_role_preferences,read_work_role_preference};
