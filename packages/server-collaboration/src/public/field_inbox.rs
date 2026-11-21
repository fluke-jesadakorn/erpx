//! Authorized source filtering belongs to the outer native Records composition.
pub const CONTRACT_VERSION: &str = "field-inbox-candidates.v2";
pub use crate::adapters::field_inbox::{
    CurrentFieldInboxDelivery, apply_field_inbox_state, lock_field_inbox_sources,
    read_field_inbox_candidates, read_field_inbox_delivery,
};
pub use crate::domain::field_inbox::{
    FieldInboxCandidate, FieldInboxCursor, FieldInboxPatch, FieldInboxProjection, FieldInboxQuery,
    FieldInboxState,
};
