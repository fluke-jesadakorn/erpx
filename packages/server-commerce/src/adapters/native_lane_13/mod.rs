//! Native migration lane 13 for commerce.
//!
//! Owner-local successor of the five live entrypoint identity reads in
//! `adapters/entrypoint-facts.ts`. The axum router owns the SQL, projection and
//! Company scope; the retained TypeScript adapter is a strict transport only.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{CommerceEntrypointRead, EntrypointReadFuture, read_facts};
pub use domain::{
    EntrypointFacts, EntrypointKind, EntrypointQuery, invalid as request_invalid, valid_id, validate,
};
pub use http::{LaneTarget, router, router_with_reader};
pub use postgres::{PgCommerceEntrypointRead, statement_for};
