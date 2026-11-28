//! Owner-local HTTP ingress for the native POS workspace/day read.
pub use crate::adapters::read_http::{
    PosWorkspaceHttpBackend, PosWorkspaceHttpFuture, PosWorkspaceHttpRequest,
    PosWorkspaceHttpResponse, PosWorkspaceHttpState, PosWorkspaceHttpTarget, pos_workspace_router,
};
