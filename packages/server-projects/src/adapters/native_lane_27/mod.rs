//! Native migration lane 27 for Projects.
//!
//! The owner-local Projects actuals reads (work logs and cost allocations)
//! moved from `adapters/actuals.ts` to this axum + sqlx module. The retained TS
//! adapter is a thin transport; there is no TypeScript SQL or money fallback.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    ProjectActualsRead, read_cost_allocations, read_handoffs, read_visible_project,
    read_visible_projects, read_work_logs,
};
pub use domain::{
    ActualsScope, CostAllocationRow, HandoffScope, LaneTarget, ProjectRow, WorkLogRow,
    check_project, company_scope, minor_to_decimal, scope,
};
pub use http::{router, router_for_holdings, router_with_reader};
pub use postgres::PgProjectActualsRead;
