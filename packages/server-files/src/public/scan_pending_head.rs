//! Native Files scan-pending head contract consumed by the delegated read
//! adapters. The frozen statement lives in Rust-owned JSON; TypeScript reads the
//! same contract through its native port and carries no statement token.
pub use crate::adapters::scan_pending_head::{
    render_scan_pending, scan_pending_head, SCAN_PENDING_HEAD_SOURCES_JSON,
};
