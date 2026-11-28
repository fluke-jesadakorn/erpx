//! Native Legal canonical attachment contract. Callers supply only the
//! owner-built jsonb-agg, upload-share and assignment-scope fragments.
pub use crate::adapters::legal_canonical_attachment_sources::{
    LEGAL_CANONICAL_ATTACHMENT_SOURCES_JSON, LegalCanonicalAttachmentSources,
    legal_canonical_attachment_sources, render_legal_canonical_current_scope,
    render_legal_canonical_image,
};
