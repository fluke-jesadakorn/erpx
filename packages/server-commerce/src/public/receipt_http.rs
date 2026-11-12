//! Owner-local Commerce purchase goods-receipt HTTP ingress.
pub use crate::adapters::receipt_http::{
    CommerceReceiptHttpBackend, CommerceReceiptHttpFuture, CommerceReceiptHttpRequest,
    CommerceReceiptHttpResponse, CommerceReceiptHttpState, CommerceReceiptHttpTarget,
    CommerceOriginPolicy, commerce_receipt_http_router,
    commerce_receipt_http_router_with_origin_policy,
};
