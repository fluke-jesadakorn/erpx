//! Commerce-owned active-product and functional-currency source for the native
//! Ecommerce listing command. Composition maps these facts onto the Ecommerce
//! producer trait and mounts the listing router; the owner functions never
//! commit and never write another owner's tables.
pub use crate::adapters::ecommerce_listing_source::{
    EcommerceListingCurrency, active_product, functional_currency,
};
