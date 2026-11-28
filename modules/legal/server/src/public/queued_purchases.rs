//! Native Legal queued-purchase read contract. The caller supplies only the
//! fixed job table and its trusted document-column fragment.
pub use crate::adapters::queued_purchases::{
    QUEUED_PURCHASES_SOURCES_JSON, QueuedPurchasesSources, queued_processing_source_guard,
    queued_purchases_sources, render_queued_job_source,
};
