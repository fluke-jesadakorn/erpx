//! Native Ecommerce listing draft/create/change commands.
//!
//! Creation requires a current active Company channel, an active Commerce
//! product, the Company functional currency and one Finance unit-price Money
//! snapshot. Change is the exact expected-version draft retitle performed by the
//! creator; publication, rejection and retirement stay in the independent review
//! port. The HTTP ingress for this slice lives in [`crate::public::ecommerce_http`].
pub use crate::adapters::listing::{
    PgEcommerceListing, PgEcommerceListingCommerceSource, PgEcommerceListingMoneyCapture,
};
pub use crate::application::listing::{EcommerceListingRepository, change_listing, create_listing};
pub use crate::application::{AuditEntry, EcommerceFuture, OutboxEntry, ReceiptEntry};
pub use crate::domain::actor::EcommerceActor;
pub use crate::domain::command::{CommandReceipt, EcommerceReceipt};
pub use crate::domain::listing::{
    CHANGE_OPERATION, CREATE_OPERATION, ChangeListing, CreateListing,
    EcommerceFunctionalCurrency, EcommerceListingMoneyRequest, EcommerceListingMoneySnapshot,
    ListingDraft, ListingRecord, RECORD_TYPE, ValidatedListing, ValidatedListingChange,
    author_required, channel_not_active, currency_mapping_required, listing_exists,
    money_evidence_invalid, money_source_unavailable, product_not_active, replay_change,
    replay_create, source_port_unavailable,
};
