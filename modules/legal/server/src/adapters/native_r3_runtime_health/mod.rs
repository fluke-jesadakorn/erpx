//! Native migration slice for the Legal runtime-health read.
//!
//! `GET .../legal/runtime-health` is still a TypeScript point read: the TS
//! adapter runs two statements (`legal_runtime_health` and the Company
//! capability probe), computes `stale`, and shapes `{companyReady, tasks,
//! notice}`. This module moves the whole operation into the owner: pure types
//! and invariants in `domain`, the query boundary and permission gate in
//! `application`, the real code-owned sqlx statements in `postgres`, and the
//! Axum ingress in `http`. No TypeScript statement or fallback is involved.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    RuntimeHealthAuthority, RuntimeHealthFuture, RuntimeHealthSource, read_runtime_health,
};
pub use domain::{
    FEATURE_PREDICATE_MAX, RUNTIME_HEALTH_NOTICE, RUNTIME_HEALTH_OPERATION, STALE_AFTER_MILLIS,
    RuntimeHealthRead, RuntimeHealthTask, RuntimeHealthTaskRow, RuntimeHealthTarget,
    valid_feature_predicate, valid_slug,
};
pub use http::{router, router_for_holdings, router_with_source};
pub use postgres::PgRuntimeHealthSource;
