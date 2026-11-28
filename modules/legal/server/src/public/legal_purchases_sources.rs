//! Native Legal purchase/authority/read contract. Callers supply only
//! owner-built projections and trusted feature tables.
pub use crate::adapters::legal_purchases_sources::{
    LEGAL_PURCHASES_SOURCES_JSON, LegalPurchasesSources, legal_purchases_sources,
    legal_replay_receipt_sql, render_legal_authority, render_legal_configuration_image,
    render_legal_feature_image, render_legal_source_witness,
};
