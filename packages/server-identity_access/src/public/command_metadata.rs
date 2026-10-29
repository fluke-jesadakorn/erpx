//! Read-only audit/receipt data from the retained current-authority snapshot.
//! Serialization cannot reconstruct or authorize an Actor or queued command.
pub use crate::application::command_metadata::{project_command_iam, project_queued_command_iam};
pub use crate::domain::command_metadata::CommandIamProjection;
