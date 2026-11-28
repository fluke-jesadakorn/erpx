//! Native Legal approval authorization-guard contract. The two cross-owner
//! projections are supplied as trusted fragments; the statement shapes stay here.
pub use crate::adapters::legal_approval_sources::{
    LEGAL_APPROVAL_SOURCES_JSON, LegalApprovalSources, legal_approval_sources,
    render_legal_approval_current_route, render_legal_reviewer_membership,
};
