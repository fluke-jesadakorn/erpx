//! Native Records registry list/search/page read and owner-local HTTP ingress.
pub use crate::adapters::record_registry::PgRecordRegistry;
pub use crate::application::record_registry::{
    RecordRegistryFuture, RecordRegistryGuard, RecordRegistryRepository, record_registry_list,
    record_registry_page,
};
pub use crate::domain::record_context::RecordView;
pub use crate::domain::record_registry::{
    CONTRACT_VERSION, MAX_CURSOR, MAX_FETCH, MAX_LIST_ROWS, MAX_SCAN_ROWS, MAX_SEARCH_BYTES,
    MAX_SEARCH_CHARS, MAX_SECTION_ID, PAGE_SIZE, RegistryCursor, RegistryPage, RegistryQuery,
    decode_cursor, encode_cursor, invalid_cursor, registry_scope, registry_source_invalid,
    request_invalid, validate_registry_row,
};
