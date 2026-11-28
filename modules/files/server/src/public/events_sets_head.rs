//! Native Files event/set head contract consumed by the delegated read
//! adapters. The frozen heads live in Rust-owned JSON; TypeScript reads the same
//! contract through its native port and carries no statement token.
pub use crate::adapters::events_sets_head::{
    attachment_events_where, attachment_sets_star_where, EVENTS_SETS_HEAD_SOURCES_JSON,
};
