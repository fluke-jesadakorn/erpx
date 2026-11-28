//! Same-unit Records source guard contract. No owner authorization is implied.
pub const CONTRACT_VERSION: &str = "records.source-record.v1";
pub use crate::application::source_record::{PreparedSource, SourceRecordPort, prepare_source};
pub use crate::domain::source_record::{
    SourceKind, SourceRecord, SourceReference, require_matching_source, validate_reference,
};
