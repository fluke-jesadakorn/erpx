//! Workspace paths are selectors; they do not convey permissions or Company scope.
pub const CONTRACT_VERSION: &str = "work-projection.workspace-preference.v1";
pub use crate::domain::workspace_preference::{
    WorkspacePreferenceScope, workspace_preference_scope,
};
