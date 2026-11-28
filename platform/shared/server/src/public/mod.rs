pub use crate::application::ports::{Clock, IdentifierSource};
pub use crate::domain::error::{BusinessError, FailureKind};
pub use crate::domain::identity::{
    AssignmentId, CommandKey, CompanyId, HoldingId, RecordId, TimestampMillis, UserId,
};
pub use crate::domain::money::DecimalAmount;
pub use crate::domain::source_record::{ConsumptionLot, SourceRecord, SourceRecordKind};
pub mod json_numbers;

/// Portable facts only; Records and consuming owners retain their own ports.
pub mod record_source_contract {
    pub const CONTRACT_VERSION: &str = "record-source.v1";
    pub use crate::domain::source_record::{
        RecordSourceReference as SourceReference, RecordSourceSnapshot as SourceRecord,
        SourceRecordKind as SourceKind,
    };
}
