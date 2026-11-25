//! Native Brand attachment scope-head contract consumed by the private
//! attachment read. The frozen head lives in Rust-owned JSON; TypeScript reads
//! the same contract and carries no statement token.
pub use crate::adapters::attachment_scope_head::{
    attachment_scope_head, render_attachment_scope_probe,
    ATTACHMENT_SCOPE_HEAD_SOURCES_JSON,
};
