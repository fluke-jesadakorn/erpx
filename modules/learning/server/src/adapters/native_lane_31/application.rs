//! Owner query port and handler. No SQL is written here; the injected reader
//! owns the statement and the projection.
//!
//! `EvidenceService` is the single gate: it validates one bounded request,
//! applies the authorization-safe company scope and returns the pinned row.

use super::domain::{
    EvidenceKind, EvidenceQuery, EvidenceRow, EvidenceSearchQuery, validate, validate_search,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin, sync::Arc};

pub type EvidenceFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the PostgreSQL-backed owner reader behind this port.
pub trait EvidenceReader: Send + Sync {
    fn read<'a>(
        &'a self,
        kind: EvidenceKind,
        query: &'a EvidenceQuery,
        company: &'a str,
        branch: Option<&'a str>,
        department: Option<&'a str>,
    ) -> EvidenceFuture<'a, Option<EvidenceRow>>;

    fn search<'a>(
        &'a self,
        kind: EvidenceKind,
        query: &'a EvidenceSearchQuery,
        company: &'a str,
        branch: Option<&'a str>,
        department: Option<&'a str>,
    ) -> EvidenceFuture<'a, Vec<EvidenceRow>>;
}

pub struct EvidenceService {
    reader: Arc<dyn EvidenceReader>,
}

impl EvidenceService {
    pub fn new(reader: Arc<dyn EvidenceReader>) -> Self {
        Self { reader }
    }

    pub fn read<'a>(
        &'a self,
        query: &'a EvidenceQuery,
        company: &'a str,
        branch: Option<&'a str>,
        department: Option<&'a str>,
    ) -> EvidenceFuture<'a, Option<EvidenceRow>> {
        let kind = match validate(query, company) {
            Ok(kind) => kind,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        self.reader.read(kind, query, company, branch, department)
    }

    pub fn search<'a>(
        &'a self,
        query: &'a EvidenceSearchQuery,
        company: &'a str,
        branch: Option<&'a str>,
        department: Option<&'a str>,
    ) -> EvidenceFuture<'a, Vec<EvidenceRow>> {
        let kind = match validate_search(query, company) {
            Ok(kind) => kind,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        self.reader.search(kind, query, company, branch, department)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_31::domain::{EvidenceSearchQuery, invalid};

    struct RejectingReader;
    impl EvidenceReader for RejectingReader {
        fn read<'a>(
            &'a self,
            _kind: EvidenceKind,
            _query: &'a EvidenceQuery,
            _company: &'a str,
            _branch: Option<&'a str>,
            _department: Option<&'a str>,
        ) -> EvidenceFuture<'a, Option<EvidenceRow>> {
            Box::pin(async { Ok(None) })
        }

        fn search<'a>(
            &'a self,
            _kind: EvidenceKind,
            _query: &'a EvidenceSearchQuery,
            _company: &'a str,
            _branch: Option<&'a str>,
            _department: Option<&'a str>,
        ) -> EvidenceFuture<'a, Vec<EvidenceRow>> {
            Box::pin(async { Ok(vec![]) })
        }
    }

    #[tokio::test]
    async fn service_rejects_before_reader() {
        let service = EvidenceService::new(Arc::new(RejectingReader));
        let query = EvidenceQuery {
            kind: "overdue".into(),
            id: "event-1".into(),
            from: Some(1),
            to_exclusive: Some(2),
        };
        assert_eq!(
            service.read(&query, "co-1", None, None).await,
            Err(invalid())
        );
    }
}
