//! Native Files witness-head contract consumed by the delegated read adapters.
//! The frozen predicates live in Rust-owned JSON; TypeScript reads the same
//! contract through its native port and carries no statement token.
pub use crate::adapters::witness_head::{
    attachment_star_id, scan_attempts_or_exists, upload_intents_exists,
    WITNESS_HEAD_SOURCES_JSON,
};
