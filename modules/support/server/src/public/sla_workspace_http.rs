//! Native Support SLA workspace HTTP ingress contract.
//!
//! Composition supplies the Account proof-derived actor, selected Support
//! scope, `support.sla` purchase gates and Holding transaction through
//! [`SlaWorkspaceHttpBackend`]. This owner router validates only registered
//! Host/Holding targets and rejects query selectors.
pub const CONTRACT_VERSION: &str = "support.sla-workspace-http.v1";

pub use crate::adapters::support_http::SupportHttpFuture;
pub use crate::adapters::sla_workspace_http::{
    sla_workspace_http_router, SlaWorkspaceHttpBackend, SlaWorkspaceHttpRequest,
    SlaWorkspaceHttpTarget,
};
