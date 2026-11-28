//! Native Legal access-guard contract. Callers supply only the trusted alias for
//! the visibility member guard; the eligibility and preparation probes are fixed.
pub use crate::adapters::legal_access_sources::{
    LEGAL_ACCESS_SOURCES_JSON, LegalAccessSources, legal_access_sources,
    legal_eligible_probe_sql, legal_preparation_guard_sql, render_legal_visibility_member_guard,
};
