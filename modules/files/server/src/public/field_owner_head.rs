//! Native Files Field-owner head contract consumed by the delegated read
//! adapters. The frozen heads live in Rust-owned JSON; TypeScript reads the same
//! contract through its native port and carries no statement token.
pub use crate::adapters::field_owner_head::{
    attachment_id_and, field_set_upload, field_upload_ancestor, FIELD_OWNER_HEAD_SOURCES_JSON,
};
