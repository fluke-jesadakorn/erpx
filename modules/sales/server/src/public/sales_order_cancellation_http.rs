//! Owner-local HTTP ingress for native Sales confirmed sales-order cancellation.
pub use crate::adapters::sales_order_cancellation_http::{
    SalesOrderCancellationHttpBackend, SalesOrderCancellationHttpFuture,
    SalesOrderCancellationHttpRequest, SalesOrderCancellationHttpResponse,
    SalesOrderCancellationHttpTarget, sales_order_cancellation_router,
};
