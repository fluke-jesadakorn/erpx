//! Native CRM activity workspace/work GET ingress public surface.
pub use crate::adapters::activity_workspace_http::{
    CrmActivityWorkspaceHttpBackend, CrmActivityWorkspaceHttpFuture,
    CrmActivityWorkspaceHttpResponse, CrmActivityWorkspaceHttpTarget,
    CrmActivityWorkspaceReadKind, CrmActivityWorkspaceReadRequest,
    crm_activity_workspace_http_router,
};
