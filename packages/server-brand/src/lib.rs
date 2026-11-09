#![forbid(unsafe_code)]
//! Brand owner: versioned identity drafts, independent publication, category
//! locks, Branding profiles/media and the Company storefront projection. Domain
//! decisions are pure; adapters own SQL and the caller owns the transaction.
mod adapters;
mod application;
mod domain;
pub mod public;
