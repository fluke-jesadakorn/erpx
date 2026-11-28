//! Native Legal register/detail read contract. Callers supply only the
//! owner-built visibility, hold-scope, link-scope, purchase and hold-own-target
//! fragments.
pub use crate::adapters::legal_service_sources::{
    LEGAL_SERVICE_SOURCES_JSON, LegalServiceSources, legal_service_sources,
    render_legal_hold_preservable, render_legal_holds, render_legal_links, render_legal_list_clause,
    render_legal_state_counts, render_legal_total,
};
