//! Bounded native generic-record write facts. The composition owns authority,
//! command receipts, audit and outbox.
pub use crate::domain::record_write::{
    ArchiveRecordInput, CreateRecordInput, MAX_DESCRIPTION, MAX_FIELD_KEY, MAX_FIELD_LABEL,
    MAX_FIELD_OPTION, MAX_FIELD_OPTIONS, MAX_FIELD_STRING, MAX_FIELD_TYPE, MAX_FIELDS,
    MAX_REFERENCES, MAX_SAFE, MAX_SECTION_ID, MAX_TITLE, REFERENCE_KINDS, RecordFieldInput,
    ReferenceRecordInput, SUPPORT_SECTIONS, UpdateRecordInput, ValidatedRecord,
    ValidatedRecordUpdate, reference_limit, self_reference, validate_archive, validate_create,
    validate_reference, validate_update, version_conflict,
};
