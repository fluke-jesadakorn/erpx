//! Native Legal read-tail r39 contract. Callers substitute only fixed aliases
//! and item references; every table, join and bind stays owner-owned.
pub use crate::adapters::legal_read_tail_r39::{
    LEGAL_READ_TAIL_R39_SOURCES_JSON, LegalReadTailR39Sources, legal_read_tail_r39_sources,
    render_legal_disposed_guard, render_legal_hold_own_target,
};
