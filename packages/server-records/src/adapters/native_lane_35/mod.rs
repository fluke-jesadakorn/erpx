//! Native migration lane 35 for records.
//!
//! Owner-local work-report read family: the Rust side owns the SQL, the Company
//! scope, the projection and the strict response shape. The TypeScript adapter
//! is a fail-closed transport only.

pub mod application;
pub mod domain;
pub mod http;
pub mod owner_scope;
pub mod postgres;

pub use application::{LaneFuture, LaneQueryHandler, WorkReportSource};
pub use domain::{
    CandidateRow, EventRow, LaneQuery, LaneResponse, LaneTarget, SubmitTime, WorkReportCandidate,
    WorkReportEvent,
};
pub use http::{router, router_for_holdings, router_with_reader, validate_targets};
pub use owner_scope::{
    OwnerScope, OwnerScopeQuery, OwnerScopeTarget, PgOwnerScopeSource,
    owner_scope_router, owner_scope_router_for_holdings, owner_scope_router_with_reader,
};
pub use postgres::PgWorkReportSource;
