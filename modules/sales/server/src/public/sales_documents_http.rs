//! Owner-local HTTP ingress for the native Sales document list.
pub use crate::adapters::sales_documents_http::{
    SalesDocumentsHttpBackend, SalesDocumentsHttpFuture, SalesDocumentsHttpRequest,
    SalesDocumentsHttpResponse, SalesDocumentsHttpTarget, sales_documents_router,
};
