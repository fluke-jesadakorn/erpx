//! Native migration lane 13 for commerce.
//!
//! Owner-local successor of the live Commerce identity, invoice/delivery and
//! billing-test entry reads. The axum router owns the SQL, projection and
//! Company scope; the retained TypeScript adapters are strict transports only.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{CommerceEntrypointRead, EntrypointReadFuture, read_facts};
pub use domain::{
    EntrypointFacts, EntrypointKind, EntrypointQuery, invalid as request_invalid, valid_id, validate,
};
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::{PgCommerceEntrypointRead, statement_for};
