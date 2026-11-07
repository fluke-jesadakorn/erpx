//! Owner-local HTTP ingress for the native Ecommerce slices.
pub use crate::adapters::ecommerce_http::{
    EcommerceCatalogHttpRequest, EcommerceChannelHttpRequest, EcommerceHttpBackend,
    EcommerceHttpFuture, EcommerceHttpResponse, EcommerceHttpState, EcommerceHttpTarget,
    EcommerceReviewHttpRequest, ecommerce_router,
};
