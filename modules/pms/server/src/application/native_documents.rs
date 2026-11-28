//! Owner query port for the native PMS document entrypoint reads.
//!
//! The port is pure: composition injects the Pg implementation, tests inject a
//! deterministic probe. Callers pass bounded, already-validated identities only;
//! no caller SQL, table name or projection reaches this boundary.

use crate::domain::native_documents::{DocumentRow, RelatedPosting};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type DocumentFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Exact document identity. The Company scope is carried separately by the
/// caller's verified session, never by this query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentQuery {
    pub id: String,
}

/// Property + document pair for the related-posting read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostingQuery {
    pub property: String,
    pub document: String,
}

pub trait DocumentRead: Send + Sync {
    /// Read the complete document row for one Company-scoped Property. `None`
    /// is returned when the document is absent or outside the Company.
    fn load<'a>(
        &'a self,
        company: &'a str,
        query: &'a DocumentQuery,
    ) -> DocumentFuture<'a, Option<DocumentRow>>;

    /// Read the journal ids posted for the document's guest account. `None` is
    /// returned when the Property is outside the Company.
    fn postings<'a>(
        &'a self,
        company: &'a str,
        query: &'a PostingQuery,
    ) -> DocumentFuture<'a, Option<Vec<RelatedPosting>>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_carry_identities_only() {
        let document = DocumentQuery { id: "doc-1".to_owned() };
        let postings = PostingQuery {
            property: "property-1".to_owned(),
            document: document.id.clone(),
        };
        assert_eq!(document.id, "doc-1");
        assert_eq!(postings.document, document.id);
        assert_eq!(postings.property, "property-1");
    }
}
