//! Bounded native generic-record write facts. The composition owns authority,
//! command receipts, audit and outbox.
pub use crate::domain::record_write::{
    CreateRecordInput, MAX_DESCRIPTION, MAX_FIELD_KEY, MAX_FIELD_LABEL, MAX_FIELD_OPTION,
    MAX_FIELD_OPTIONS, MAX_FIELD_STRING, MAX_FIELD_TYPE, MAX_FIELDS, MAX_SECTION_ID, MAX_TITLE,
    RecordFieldInput, SUPPORT_SECTIONS, ValidatedRecord, validate_create,
};
