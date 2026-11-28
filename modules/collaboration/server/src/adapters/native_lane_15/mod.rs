//! Native migration lane 15 for collaboration.
//!
//! The owner owns the four `social_*` beta-facts reads end to end: domain
//! validation, an application query port, sqlx execution and an owner-local
//! axum router. TypeScript is a thin transport only; no SQL fallback remains.

pub(crate) mod application;
pub(crate) mod domain;
pub mod field_command;
pub(crate) mod http;
pub(crate) mod postgres;

pub use application::BetaFactsQuery;
pub use domain::{BetaFactKind, BetaFactRequest, BetaFacts, PostDraft, PostReport, TaskOrigin};
pub use http::{router, router_for_holdings, router_with_reader, LaneTarget};
pub use postgres::PgBetaFactsQuery;
