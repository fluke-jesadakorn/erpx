//! Native migration lane 33 for runtime.
//!
//! Owner-local successor of the three `SELECT ... FROM holding_bindings`
//! statements in `adapters/runtime.ts`. The axum router owns the query, the
//! resource-slot projection and the response shape; the retained TypeScript
//! adapter is a strict, fail-closed transport with no SQL fallback.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    HoldingBindingRead, HoldingBindingReadFuture, SharedHoldingBindingRead, unavailable,
};
pub use domain::{
    BindingQuery, HoldingBinding, MAX_SCOPE, MAX_SLUG, parse_scope, select_all, select_one,
    valid_holding_id, valid_slug,
};
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::{
    ACTIVE_SQL, BASE_SQL, BY_SLUG_SQL, DEFAULT_SQL, PgHoldingBindingRead, rendered_active_sql,
    rendered_by_slug_sql, rendered_default_sql,
};
