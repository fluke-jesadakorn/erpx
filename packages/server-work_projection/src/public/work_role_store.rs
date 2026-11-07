//! Work Projection owns preference/initialization policy; Identity owns grants
//! and selected-session writes. A returned intent never authorizes a command.
pub const CONTRACT_VERSION:&str="work-projection.role-initialization.v1";
pub use crate::domain::work_role_store::{InitialWorkRole,WorkRoleReference,WorkRoleInitialization};
pub use crate::application::work_role_store::initial_work_role;
