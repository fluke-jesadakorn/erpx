//! Native Files scalar projection-head contract consumed by the delegated read
//! adapters. The frozen heads live in Rust-owned JSON; TypeScript reads the same
//! contract through its native port and carries no statement token.
pub use crate::adapters::projection_head::{
    distinct_set_row, event_id, source_state, PROJECTION_HEAD_SOURCES_JSON,
};
