//! POS owner. The qualified native slice records a cash movement against an
//! open register session. Sale/fulfilment/refund lifecycles, session close and
//! review, Finance posting, Inventory reservations, Stripe and package
//! enforcement remain in the retained TypeScript owner until their native
//! slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
