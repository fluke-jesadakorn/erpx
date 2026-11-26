//! Stable PMS owner exports for the native document entrypoint reads.
//!
//! Composition mounts the axum router with one reader per registered Holding;
//! no TypeScript authority, plan or fallback is part of this port.

pub use crate::adapters::native_documents_http::{
    DocumentParams, DocumentTarget, PostingParams, router_for_holdings, router_with_reader,
    router_with_readers, unavailable_error,
};
pub use crate::adapters::native_documents_postgres::{
    DOCUMENT_SQL, POSTINGS_SQL, PgDocumentRead,
};
pub use crate::application::native_documents::{
    DocumentFuture, DocumentQuery, DocumentRead, PostingQuery,
};
pub use crate::domain::native_documents::{DocumentRow, RelatedPosting};
