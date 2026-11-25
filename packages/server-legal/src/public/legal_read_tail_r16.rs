//! Native Legal read-tail r16 contract. The caller supplies only its trusted
//! predicate and binds; the item-list read prefix stays owner-controlled.
pub use crate::adapters::legal_read_tail_r16::{
    LEGAL_READ_TAIL_R16_SOURCES_JSON, LegalReadTailR16Sources, legal_item_select_sql,
    legal_read_tail_r16_sources,
};
