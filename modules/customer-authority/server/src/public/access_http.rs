//! Owner-local Customer Authority customer-access HTTP router.
pub use crate::adapters::access_http::{
    CustomerAccessHttpBackend, CustomerAccessHttpCommand, CustomerAccessHttpFuture,
    CustomerAccessHttpRequest, CustomerAccessHttpResponse, CustomerAccessHttpTarget,
    customer_access_http_router,
};
