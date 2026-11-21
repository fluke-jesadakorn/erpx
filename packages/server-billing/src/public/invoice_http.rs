//! The Billing owner's supported Rust invoice HTTP ingress.
pub use crate::adapters::invoice_http::{
    BillingInvoiceHttpBackend, BillingInvoiceHttpFuture, BillingInvoiceHttpRequest,
    BillingInvoiceHttpResponse, BillingInvoiceHttpState, BillingInvoiceHttpTarget,
    billing_invoice_router,
};
