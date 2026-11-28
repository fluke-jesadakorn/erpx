//! Native Records owner-projection contract consumed by delegated read
//! adapters. The frozen statement lives in Rust-owned JSON; TypeScript reads
//! the same contract and carries no statement token.
pub use crate::adapters::owner_projection_head::{
    employee_requests_projection, OWNER_PROJECTION_HEAD_SOURCES_JSON,
};
