//! Native Benefits enrollment workspace HTTP ingress and owner-local router.
pub use crate::adapters::workspace_http::{
    BenefitsDetailHttpRequest, BenefitsWorkspaceHttpBackend, BenefitsWorkspaceHttpFuture,
    BenefitsWorkspaceHttpReadRequest, BenefitsWorkspaceHttpResponse, BenefitsWorkspaceHttpTarget,
    benefits_workspace_http_router,
};
