//! Native Files settlement-head contract consumed by the delegated read
//! adapters. The frozen heads live in Rust-owned JSON; TypeScript reads the same
//! contract through its native port and carries no statement token.
pub use crate::adapters::settlement_head::{
    attachment_exists, messaging_settlement, package_file_link_target,
    SETTLEMENT_HEAD_SOURCES_JSON,
};
