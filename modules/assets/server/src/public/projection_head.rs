//! Native Assets projection-head contract consumed by the delegated read
//! adapters. The frozen templates live in Rust-owned JSON; TypeScript reads the
//! same contract through its native port and carries no statement token.
pub use crate::adapters::projection_head::{
    asset_from_head, asset_projection_head, render_projection_from,
    ASSET_PROJECTION_HEAD_SOURCES_JSON,
};
