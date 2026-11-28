//! Owner-local HTTP ingress for native Projects graph reads.
pub use crate::adapters::project_graph_http::{
    ProjectGraphHttpBackend, ProjectGraphHttpFuture, ProjectGraphHttpRequest,
    ProjectGraphHttpResponse, ProjectGraphHttpTarget, project_graph_router,
};
