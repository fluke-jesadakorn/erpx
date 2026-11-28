//! Native Legal record-source guard contract. Callers supply only trusted
//! surface expressions and owner-built scope, assignment, release and purchase
//! fragments.
pub use crate::adapters::legal_record_sources::{
    LEGAL_RECORD_SOURCES_JSON, LegalRecordSources, legal_record_sources,
    render_legal_actual_source, render_legal_native_source, render_legal_workspace_guard,
};
