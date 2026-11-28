//! Public owner-local contract for the native document-page derivative command.
//!
//! Composition owns the current actor, the owning expense authority, the byte
//! store, the registered Holding targets and the serializable unit. This module
//! exports only stable Files types, ports and router constructors.
pub use crate::adapters::document_derivative_http::{
    document_derivative_router, DocumentDerivativeBackend, DocumentDerivativeReply,
    DocumentDerivativeSaveRequest, DocumentDerivativeTarget,
};
pub use crate::adapters::document_derivative_postgres::{
    ExpenseDerivativeAuthority, PgDocumentDerivativePorts,
};
pub use crate::application::document_derivative::{
    save_document_page, DocumentDerivativeByteStore, DocumentDerivativePorts, SaveDocumentPage,
};
pub use crate::domain::document_derivative::{
    validate_page, validate_type, DerivativeAuthority, DocumentDerivativeResult,
    DocumentPageInput, DocumentPageSource, DERIVATIVE_OPERATION, MAX_PAGE_BYTES, MAX_PAGES,
};
