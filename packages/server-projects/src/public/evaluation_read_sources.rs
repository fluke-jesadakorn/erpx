//! Native Projects evaluation read contract consumed by the delegated
//! adapters. The frozen statements live in Rust-owned JSON; TypeScript reads
//! the same contract through its native port and carries no statement token.
pub use crate::adapters::evaluation_read_sources::{
    native_sources, source_image, source_rows, EVALUATION_READ_SOURCES_JSON,
};
