//! Native Sales quotation and sales-order document list read.
pub use crate::adapters::sales_documents::PgSalesDocumentRead;
pub use crate::application::sales_documents::{
    SalesDocumentReadFuture, SalesDocumentReadRepository, sales_documents,
};
pub use crate::domain::sales_documents::{
    PAGE_SIZE, SalesDocumentCursor, SalesDocumentKind, SalesDocumentReadActor, SalesDocumentRow,
    SalesDocumentsPage, normalize_status, validate_search,
};
