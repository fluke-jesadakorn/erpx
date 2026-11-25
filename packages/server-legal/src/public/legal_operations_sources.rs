//! Native Legal operations read contract. Callers supply only the trusted
//! register table, visibility predicate and purchase predicate.
pub use crate::adapters::legal_operations_sources::{
    LEGAL_OPERATIONS_SOURCES_JSON, LegalOperationsSources, legal_operations_sources,
    render_legal_hold_preview, render_legal_register_rows, render_legal_rule_impact,
};
