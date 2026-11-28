//! Native migration of the `organization/drafts` GET route family.
//!
//! The active draft, its scheduled requests and the recent submitted-draft list
//! are owned end-to-end here: owner-local SQL and row projection in `postgres`,
//! invariants in `domain`, a SQL-free application port, and an axum ingress that
//! only transports validated facts. The TypeScript `getOrganizationDraft`
//! authority is retired by the lead once this router is mounted and the shared
//! route branch is switched; no partial projection or TypeScript fallback is
//! used by this slice.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{DraftRead, ReadFuture, handle};
pub use domain::{
    DraftReadEnvelope, DraftReadQuery, DraftReadResult, DraftRow, RecentRow, RequestRow,
    REQUIRED_PERMISSIONS, valid_id,
};
pub use http::{LaneTarget, router, router_for_holdings};
pub use postgres::{ACTIVE_SQL, BY_ID_SQL, PgDraftRead, RECENT_SQL, REQUESTS_SQL};
