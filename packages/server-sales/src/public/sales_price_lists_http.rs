//! Owner-local HTTP ingress for the native Sales price-list lifecycle.
pub use crate::adapters::price_lists_http::{
    PriceListHttpBackend, PriceListHttpCommand, PriceListHttpFuture, PriceListHttpRequest,
    PriceListHttpResponse, PriceListHttpState, PriceListHttpTarget, sales_price_lists_router,
};
