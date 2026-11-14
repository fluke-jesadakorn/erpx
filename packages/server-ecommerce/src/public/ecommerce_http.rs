//! Owner-local HTTP ingress for the native Ecommerce slices.
pub use crate::adapters::ecommerce_http::{
    EcommerceCatalogHttpRequest, EcommerceChannelHttpRequest, EcommerceHttpBackend,
    EcommerceHttpFuture, EcommerceHttpResponse, EcommerceHttpState, EcommerceHttpTarget,
    EcommerceReviewHttpRequest, ecommerce_router,
};
pub use crate::adapters::ecommerce_listing_http::{
    EcommerceListingChangeHttpRequest, EcommerceListingCreateHttpRequest,
    EcommerceListingHttpBackend, EcommerceListingHttpState, EcommerceListingHttpTarget,
    ecommerce_listing_router,
};
