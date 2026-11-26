//! Native migration of the `organization/acceptance` GET route family.
//!
//! The five setup checks, the current acceptance record, the decision flags
//! and the event list are owned end-to-end here: owner-local SQL and row mapping
//! in `postgres`, the decision rules in `application`, invariants in `domain`
//! and an axum ingress that only transports validated facts. The TypeScript
//! `companyBusinessAcceptance` GET authority is retired by the lead once this
//! router is mounted and the shared route branch is switched; no partial
//! projection or TypeScript fallback is used by this slice.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{AcceptanceRead, ReadFuture, checks, handle};
pub use domain::{
    AcceptanceEnvelope, AcceptanceFacts, AcceptanceQuery, AcceptanceResult, AcceptanceRow, Check,
    CompanyRow, EventRow, valid_id,
};
pub use http::{LaneTarget, router, router_for_holdings};
pub use postgres::{
    AUTHORITY_SQL, BRANCHES_SQL, CEO_SQL, COMPANY_SQL, CURRENT_SQL, EVENTS_SQL, LEVELS_SQL,
    PLACEMENT_SQL, PgAcceptanceRead,
};
