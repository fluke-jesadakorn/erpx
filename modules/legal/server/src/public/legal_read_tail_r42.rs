//! Native Legal read-tail r42 contract. The caller substitutes only a fixed
//! projection and trusted fact table; the search predicate stays owner-owned.
pub use crate::adapters::legal_read_tail_r42::{
    LEGAL_READ_TAIL_R42_SOURCES_JSON, LegalReadTailR42Sources, legal_read_tail_r42_sources,
    render_legal_feature_search,
};
