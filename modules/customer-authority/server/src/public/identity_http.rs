//! Owner-local Customer Authority customer identity-change HTTP router.
pub use crate::adapters::identity_http::{
    CustomerIdentityHttpBackend, CustomerIdentityHttpCommand, CustomerIdentityHttpFuture,
    CustomerIdentityHttpRequest, CustomerIdentityHttpResponse, CustomerIdentityHttpTarget,
    customer_identity_http_router,
};
