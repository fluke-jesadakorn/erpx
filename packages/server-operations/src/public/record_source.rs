//! Owning source facts only. Caller retains selected IAM, scope and purchase proofs.
pub const CONTRACT_VERSION: &str = "operations.records-source.v1";
pub use crate::adapters::record_source::{
    CurrentRecordSource, lock_record_sources, read_record_source,
};
