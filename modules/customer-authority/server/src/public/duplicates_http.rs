//! Owner-local Customer Authority duplicate-review HTTP router.
pub use crate::adapters::duplicates_http::{
    CustomerDuplicateHttpBackend, CustomerDuplicateHttpCommand, CustomerDuplicateHttpFuture,
    CustomerDuplicateHttpRequest, CustomerDuplicateHttpResponse, CustomerDuplicateHttpTarget,
    customer_duplicate_http_router,
};
