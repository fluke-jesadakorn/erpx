//! Wave-2 native family: Collaboration field-command routing facts.
//!
//! The owner owns both `collaboration_*` SELECTs, the Company fence and the
//! boolean routing projection. TypeScript is a strict transport only.

pub(crate) mod application;
pub(crate) mod domain;
pub(crate) mod http;
pub(crate) mod postgres;

pub use application::FieldCommandQuery;
pub use domain::{CardSource, FieldCommandFacts, FieldCommandRequest, RelationSource};
pub use http::{router, router_for_holdings, router_with_reader, LaneTarget};
pub use postgres::PgFieldCommandQuery;
