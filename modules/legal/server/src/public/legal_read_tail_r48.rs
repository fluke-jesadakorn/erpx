//! Native Legal read-tail r48 contract. The caller substitutes only its trusted
//! trailing predicate; every table, alias, join and fixed bind stays owner-owned.
pub use crate::adapters::legal_read_tail_r48::{
    LEGAL_READ_TAIL_R48_SOURCES_JSON, LegalReadTailR48Head, LegalReadTailR48Sources,
    legal_read_tail_r48_sources, render_legal_read_tail_r48,
};
