//! Native Benefits command HTTP ingress and owner-local router.
pub use crate::adapters::command_http::{
    BenefitsCommandHttpBackend, BenefitsCommandHttpFuture, BenefitsCommandHttpRequest,
    BenefitsCommandHttpResponse, BenefitsCommandHttpTarget, benefits_command_http_router,
};
