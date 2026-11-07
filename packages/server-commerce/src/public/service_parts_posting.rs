//! Commerce facts only: source lineage and Finance/Inventory/Field authority are
//! checked by those owners in the same transaction, never by this projection.
pub use crate::adapters::service_parts_posting::{
    CurrentServicePartsPosting, PgServicePartsPostingSource, PreparedServicePartsPosting,
    ServicePartsPostingFacts, ServicePartsPostingIntent, ServicePartsPostingMapping,
    lock_service_parts_posting_sources, read_service_parts_posting,
    service_posting_schema_available,
};
pub use crate::domain::service_parts_posting::{PostingAdmission, PostingMode, posting_admission};
