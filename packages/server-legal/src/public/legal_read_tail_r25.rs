//! Native Legal read-tail r25 contract. Callers substitute only trusted aliases,
//! tables and order columns; every statement shape stays owner-controlled.
pub use crate::adapters::legal_read_tail_r25::{
    LEGAL_READ_TAIL_R25_SOURCES_JSON, LegalReadTailR25Sources, legal_read_tail_r25_sources,
    render_legal_jsonb_agg_by_item, render_legal_member_edit_exists,
};
