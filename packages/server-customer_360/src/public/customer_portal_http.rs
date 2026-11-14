//! Native customer-portal HTTP ingress and owner-local router.
pub use crate::adapters::portal_http::{
    CustomerPortalHttpBackend, CustomerPortalHttpCommandRequest, CustomerPortalHttpFuture,
    CustomerPortalHttpReadRequest, CustomerPortalHttpTarget, customer_portal_http_router,
};
