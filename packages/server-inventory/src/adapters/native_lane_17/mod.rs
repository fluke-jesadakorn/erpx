//! Native migration lane 17 for inventory.
//!
//! Owner-local native landed-cost reads. The axum route owns the SQL, the bind
//! order, the Company witness, the projection and the response envelope; the
//! retained TypeScript adapter is a strict, fail-closed transport only.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{InventoryLane17Read, LaneReadFuture};
pub use domain::{
    LaneReadKind, LaneReadRequest, LaneReadResponse, READ_PATH, valid_id, valid_slug,
};
pub use http::{LaneTarget, router, router_for_holdings};
pub use postgres::PgInventoryLane17Read;
