#![forbid(unsafe_code)]
//! Agent owner: catalogue/version approval, pool membership, assignments,
//! private-session hold/expire, AI provider settings and per-Company quota.
//!
//! Domain decisions are pure; adapters own SQL and the caller owns the
//! transaction. The platform-owned Agent evidence audit read is not part of
//! this crate and remains in platform_administration.
pub mod public;

mod adapters;
mod application;
mod domain;
