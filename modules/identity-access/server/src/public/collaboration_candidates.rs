//! Versioned display facts; this is not actor or source authority.
pub const CONTRACT_VERSION: &str = "identity.collaboration-candidates.v1";
pub use crate::adapters::collaboration_candidates::{
    CurrentCollaborationCandidates, read_collaboration_candidates, read_operations_candidates,
};
