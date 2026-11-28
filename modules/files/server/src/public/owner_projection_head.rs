//! Native Files owner-projection contract consumed by the delegated read
//! adapters. The frozen statements live in Rust-owned JSON; TypeScript reads
//! the same contract through its native port and carries no statement token.
pub use crate::adapters::owner_projection_head::{
    attachment_links_projection, attachments_projection, attachment_sets_projection,
    OWNER_PROJECTION_HEAD_SOURCES_JSON,
};
