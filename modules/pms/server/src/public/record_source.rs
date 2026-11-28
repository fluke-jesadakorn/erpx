pub const CONTRACT_VERSION: &str = "pms.records-source.v1";
pub use crate::adapters::record_source::{
    CurrentRecordSource, lock_record_sources, read_record_source,
};
pub use crate::domain::record_source::{source_allowed, source_title};
