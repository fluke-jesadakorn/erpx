//! Owner-local axum ingress for the native Procurement supplier-bill detail.
pub use crate::adapters::supplier_bill_detail_http::{
    SupplierBillDetailHttpBackend, SupplierBillDetailHttpFuture,
    SupplierBillDetailHttpRequest, SupplierBillDetailHttpResponse,
    SupplierBillDetailHttpTarget, supplier_bill_detail_router,
};
