//! Native Customer 360 customer directory HTTP ingress and owner-local router.
pub use crate::adapters::directory_http::{
    CustomerDirectoryHttpBackend, CustomerDirectoryHttpFuture, CustomerDirectoryHttpReadRequest,
    CustomerDirectoryHttpTarget, customer_directory_http_router,
};
