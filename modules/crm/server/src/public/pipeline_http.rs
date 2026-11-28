//! Native CRM pipeline HTTP ingress and owner-local router.
pub use crate::adapters::pipeline_http::{
    CrmPipelineHttpBackend, CrmPipelineHttpFuture, CrmPipelineHttpReadRequest,
    CrmPipelineHttpResponse, CrmPipelineHttpTarget, crm_pipeline_http_router,
};
