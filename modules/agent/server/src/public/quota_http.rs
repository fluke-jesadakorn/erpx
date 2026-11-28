//! Native Agent AI-quota command HTTP ingress and owner-local router.
pub use crate::adapters::quota_http::{
    AgentQuotaHttpBackend, AgentQuotaHttpFuture, AgentQuotaHttpKind, AgentQuotaHttpRequest,
    AgentQuotaHttpResponse, AgentQuotaHttpTarget, agent_quota_http_router,
};
