//! Native Legal portal purchase/witness contract. Callers supply only the
//! owner-built scheduled-feature predicate for the release guard.
pub use crate::adapters::legal_portal_purchases_sources::{
    LEGAL_PORTAL_PURCHASES_SOURCES_JSON, LegalPortalPurchasesSources,
    legal_portal_image_fields_sql, legal_portal_purchases_sources,
    render_legal_portal_release_probe,
};
