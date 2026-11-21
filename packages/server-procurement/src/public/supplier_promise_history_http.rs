//! Owner-local HTTP ingress for native Procurement supplier promise history.
pub use crate::adapters::supplier_promise_history_http::{
    SupplierPromiseHistoryHttpBackend, SupplierPromiseHistoryHttpFuture,
    SupplierPromiseHistoryHttpRequest, SupplierPromiseHistoryHttpResponse,
    SupplierPromiseHistoryHttpTarget, supplier_promise_history_router,
};
