//! Ecommerce owner. The qualified native slice reads the current Company's
//! channel/listing storefront catalog, creates independent channel and listing
//! drafts, retitles a draft listing under exact CAS and performs channel/listing
//! review transitions. Cart, checkout, fulfillment and return commands remain in
//! the retained TypeScript owner until their native slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
