//! The Billing owner's supported owner-local HTTP ingress interface.
pub use crate::adapters::billing_http::{
    BillingCommandHttpRequest, BillingHttpBackend, BillingHttpFuture, BillingHttpResponse,
    BillingHttpState, BillingHttpTarget, BillingWorkspaceHttpRequest, billing_router,
};
