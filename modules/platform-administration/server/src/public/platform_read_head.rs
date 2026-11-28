//! Native Platform Administration static read/probe head contract. The frozen
//! heads live in Rust-owned JSON; TypeScript imports the same contract and
//! carries no statement token for these read/probe families.
pub use crate::adapters::platform_read_head::{
    PLATFORM_READ_HEAD_SOURCES_JSON, PlatformReadHeads, database_identity_read,
    platform_read_heads, projection_from_head, records_usage_projection,
    render_projection_from_head, render_row_head, render_select_one_from_where, row_by_predicate,
    select_one, select_one_from_where, workforce_scope_probe,
};
