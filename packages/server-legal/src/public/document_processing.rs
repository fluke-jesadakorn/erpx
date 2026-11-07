#[cfg(feature = "document-provider")]
pub use crate::adapters::postgres_document_processing::{
    PgLegalDocumentExtractionBridge, prepare_current_legal_extraction,
    retain_current_legal_extraction,
};
pub use crate::application::document_processing::compute_prepared_legal_extraction;
pub use crate::domain::document_processing::{
    ComputedLegalExtraction, LegalExtractionRetention, PreparedLegalExtraction,
    extraction_compute_budget,
};
