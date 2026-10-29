//! Source facts only; callers must retain actual current viewer record policy.
pub use crate::adapters::postgres_processing_status::{
    PgLegalProcessingStatusSource, load_legal_processing_status,
    lock_legal_processing_status_sources,
};
