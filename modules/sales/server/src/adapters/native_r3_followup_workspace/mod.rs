//! Additive native read for `GET /api/v1/holdings/{holding_slug}/customer-followups`.
//!
//! Rust owns the follow-up/customer-order statement, the owner-name and
//! recipient-eligibility projections and the exact workspace envelope. The
//! owner pool is injected; no SQL or TypeScript fallback enters the wire path.
mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    FollowupWorkspaceStore, WorkspaceFuture, WorkspaceInput, WorkspaceRows, read_workspace,
};
pub use domain::{
    WORKSPACE_PATH, WorkspaceQuery, items_sql, order_sql, recipients_sql, request_invalid,
};
pub use http::{
    FollowupWorkspaceTarget, WORKSPACE_CONTEXT_COMPANY, WORKSPACE_CONTEXT_PERMISSIONS,
    WORKSPACE_CONTEXT_USER, WORKSPACE_DEPARTMENT, WORKSPACE_BRANCH, router,
};
pub use postgres::PgFollowupWorkspaceStore;
