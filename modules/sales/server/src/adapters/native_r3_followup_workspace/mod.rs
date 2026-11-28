//! Additive native read for `GET /api/v1/holdings/{holding_slug}/customer-followups`.
//!
//! Owner-local read building blocks only. The HTTP route requires a separate
//! composition backend for current IAM and purchase authority; the pool-backed
//! store is not a same-transaction authority boundary and must not be mounted
//! until the composition backend and PostgreSQL parity are verified.
mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    FollowupWorkspaceStore, WorkspaceFuture, WorkspaceInput, WorkspaceRows, read_workspace, render_workspace,
};
pub use domain::{
    WORKSPACE_PATH, WorkspaceQuery, items_sql, order_sql, recipients_sql, request_invalid,
};
pub use http::{
    AuthorizedFollowupWorkspace, FollowupWorkspaceBackend, FollowupWorkspaceTarget, WORKSPACE_CONTEXT_COMPANY, WORKSPACE_CONTEXT_PERMISSIONS,
    WORKSPACE_CONTEXT_USER, WORKSPACE_DEPARTMENT, WORKSPACE_BRANCH, router,
};
pub use postgres::{PgFollowupWorkspaceStore, read_workspace_on_connection};
