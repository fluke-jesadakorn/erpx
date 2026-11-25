//! Native Maintenance read-head contract consumed by the owner TypeScript
//! renderers. The frozen templates live in Rust-owned JSON; TypeScript reads the
//! same contract and carries no statement token.
pub use crate::adapters::read_head::{
    alias_rows, exists_guard, projection_head, record_image, render_alias_rows,
    render_exists_guard, render_projection, render_record_image, render_settlement_ref,
    render_subquery, settlement_ref, subquery_head, READ_HEAD_SOURCES_JSON,
};
