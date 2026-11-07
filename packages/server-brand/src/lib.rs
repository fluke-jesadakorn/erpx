#![forbid(unsafe_code)]
//! Brand owner: versioned identity drafts, independent publication, category
//! locks and the Company storefront projection. Domain decisions are pure; the
//! adapters own SQL and the caller owns the Holding transaction.
mod adapters;
mod application;
mod domain;
pub mod public;
