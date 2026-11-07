//! Owner-local HTTP ingress for native Projects detail and WBS reads.
pub use crate::adapters::project_wbs_http::{
    ProjectWbsHttpBackend, ProjectWbsHttpFuture, ProjectWbsHttpRequest, ProjectWbsHttpResponse,
    ProjectWbsHttpTarget, projects_router,
};
