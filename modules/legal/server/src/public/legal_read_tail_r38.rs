//! Native Legal read-tail r38 contract. Callers substitute only trusted table
//! names, aliases and fixed item references; every statement shape stays owned.
pub use crate::adapters::legal_read_tail_r38::{
    LEGAL_READ_TAIL_R38_SOURCES_JSON, LegalReadTailR38Sources, legal_read_tail_r38_sources,
    render_legal_fact_data, render_legal_metric_count_prefix,
};
