//! Native Talent application-detail read adapter.
//!
//! `postgres` is the only layer that executes SQL; `http` is the axum ingress
//! that drives the application handler through the injected owner reader.

pub mod http;
pub mod postgres;

pub use http::{
    TalentDetailTarget, router, router_for_holdings, router_with_reader,
};
pub use postgres::PgTalentDetailRead;
