//! Ecommerce owner. The qualified native slice reads the current Company's
//! channel/listing storefront catalog, creates an independent channel draft and
//! performs channel/listing review transitions. Listing creation, cart,
//! checkout, fulfillment and return commands remain in the retained TypeScript
//! owner until their native slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
