//! Native Legal attachment-fact guard contract. Callers supply only the trusted
//! target table/extra predicate and the Files attachment projection fragments.
pub use crate::adapters::legal_attachment_facts_sources::{
    LEGAL_ATTACHMENT_FACTS_SOURCES_JSON, LegalAttachmentFactsSources,
    legal_attachment_facts_sources, render_legal_file_preservations,
    render_legal_owner_version_guard,
};
