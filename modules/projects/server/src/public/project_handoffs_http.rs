//! Native Projects handoff-view HTTP ingress.
pub use crate::adapters::project_handoffs_http::{
    HandoffViewHttpBackend, HandoffViewHttpFuture, HandoffViewHttpRequest,
    HandoffViewHttpResponse, HandoffViewHttpTarget, handoff_views_router,
};
