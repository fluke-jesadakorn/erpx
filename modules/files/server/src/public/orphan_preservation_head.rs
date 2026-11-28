//! Native Files orphan-preservation head contract consumed by the delegated
//! read adapters. The frozen statements live in Rust-owned JSON; TypeScript
//! reads the same contract through its native port and carries no statement
//! token.
pub use crate::adapters::orphan_preservation_head::{
    preservation_pending, preservation_present, ORPHAN_PRESERVATION_HEAD_SOURCES_JSON,
};
