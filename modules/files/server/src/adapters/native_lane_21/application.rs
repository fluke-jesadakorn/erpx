//! Owner query port and handler for the Files commercial-usage read family.
//!
//! No SQL lives here. The port is injected by composition; the Pg adapter in
//! `postgres.rs` is the only implementation that renders a statement.

use crate::adapters::native_lane_21::domain::{
    ReviewFilesFacts, ReviewFilesRequest, TaxSignatureFacts, TaxSignatureReadKind,
    TaxSignatureRequest, UsageFacts, UsageReadKind, UsageReadRequest, validate_request,
    validate_review_request, validate_tax_request,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type UsageReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The owner-owned reader. `company` is the optional caller scope: these reads
/// are holding-wide, so an absent company is legitimate. The pool behind the
/// reader is bound to the registered Holding target, which is the authority
/// boundary. No statement accepts a company predicate from the transport.
pub trait CommercialUsageRead: Send + Sync {
    fn read<'a>(
        &'a self,
        company: Option<&'a str>,
        kind: UsageReadKind,
        request: &'a UsageReadRequest,
    ) -> UsageReadFuture<'a, UsageFacts>;
}

/// Validate then dispatch one bounded read. This is the single entry point the
/// HTTP adapter may use; it prevents a caller from skipping validation.
pub async fn handle<R: CommercialUsageRead + ?Sized>(
    reader: &R,
    company: Option<&str>,
    request: UsageReadRequest,
) -> Result<UsageFacts, BusinessError> {
    let kind = validate_request(&request)?;
    reader.read(company, kind, &request).await
}

/// Reader port for the company-scoped tax-signature attachment reads.
pub trait TaxSignatureRead: Send + Sync {
    fn read<'a>(
        &'a self,
        company: &'a str,
        kind: TaxSignatureReadKind,
        request: &'a TaxSignatureRequest,
    ) -> UsageReadFuture<'a, TaxSignatureFacts>;
}

/// Validate then dispatch one bounded tax-signature read.
pub async fn handle_tax<R: TaxSignatureRead + ?Sized>(
    reader: &R,
    company: &str,
    request: TaxSignatureRequest,
) -> Result<TaxSignatureFacts, BusinessError> {
    let kind = validate_tax_request(&request)?;
    reader.read(company, kind, &request).await
}

pub trait ReviewFilesRead: Send + Sync {
    fn read<'a>(
        &'a self,
        company: &'a str,
        request: &'a ReviewFilesRequest,
    ) -> UsageReadFuture<'a, ReviewFilesFacts>;
}

pub async fn handle_review<R: ReviewFilesRead + ?Sized>(
    reader: &R,
    company: &str,
    request: ReviewFilesRequest,
) -> Result<ReviewFilesFacts, BusinessError> {
    validate_review_request(&request)?;
    reader.read(company, &request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_21::domain::{
        KIND_EARLIEST_ORIGINAL, KIND_STORED_ORIGINALS, TAX_METADATA, TAX_WORKSPACE,
        TaxSignatureFacts, TaxSignatureReadKind, TaxSignatureRequest, invalid, tax_invalid,
    };
    use std::sync::{Arc, Mutex};

    struct RecordingReader {
        seen: Arc<Mutex<Vec<(Option<String>, UsageReadKind)>>>,
    }

    impl CommercialUsageRead for RecordingReader {
        fn read<'a>(
            &'a self,
            company: Option<&'a str>,
            kind: UsageReadKind,
            _request: &'a UsageReadRequest,
        ) -> UsageReadFuture<'a, UsageFacts> {
            let seen = self.seen.clone();
            Box::pin(async move {
                seen.lock()
                    .expect("mutex")
                    .push((company.map(str::to_owned), kind));
                Ok(UsageFacts::EarliestOriginal { earliest: None })
            })
        }
    }

    #[tokio::test]
    async fn handler_validates_before_dispatch() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = RecordingReader { seen: seen.clone() };
        let mut request = UsageReadRequest {
            kind: KIND_EARLIEST_ORIGINAL.to_owned(),
            start: None,
            end: None,
            branch_id: None,
        };
        let facts = handle(&reader, None, request.clone())
            .await
            .expect("valid read");
        assert_eq!(facts, UsageFacts::EarliestOriginal { earliest: None });
        assert_eq!(
            seen.lock().expect("mutex").as_slice(),
            &[(None, UsageReadKind::EarliestOriginal)]
        );

        let scoped = handle(&reader, Some("company-1"), request.clone())
            .await
            .expect("scoped read");
        assert_eq!(scoped, UsageFacts::EarliestOriginal { earliest: None });
        assert_eq!(
            seen.lock().expect("mutex").as_slice(),
            &[
                (None, UsageReadKind::EarliestOriginal),
                (
                    Some("company-1".to_owned()),
                    UsageReadKind::EarliestOriginal
                )
            ]
        );

        request.kind = KIND_STORED_ORIGINALS.to_owned();
        assert_eq!(handle(&reader, None, request).await, Err(invalid()));
        assert_eq!(seen.lock().expect("mutex").len(), 2);
    }

    struct TaxReader {
        calls: Mutex<Vec<(String, TaxSignatureReadKind)>>,
    }

    impl TaxSignatureRead for TaxReader {
        fn read<'a>(
            &'a self,
            company: &'a str,
            kind: TaxSignatureReadKind,
            _request: &'a TaxSignatureRequest,
        ) -> UsageReadFuture<'a, TaxSignatureFacts> {
            Box::pin(async move {
                self.calls
                    .lock()
                    .expect("mutex")
                    .push((company.to_owned(), kind));
                Ok(TaxSignatureFacts::Workspace { rows: Vec::new() })
            })
        }
    }

    #[tokio::test]
    async fn tax_handler_validates_company_scoped_requests_before_dispatch() {
        let reader = TaxReader {
            calls: Mutex::new(Vec::new()),
        };
        let mut request = TaxSignatureRequest {
            kind: TAX_METADATA.to_owned(),
            id: Some("identity-1".to_owned()),
            link_id: None,
        };
        assert_eq!(
            handle_tax(&reader, "company-1", request.clone()).await,
            Err(tax_invalid())
        );
        assert!(reader.calls.lock().expect("mutex").is_empty());

        request.kind = TAX_WORKSPACE.to_owned();
        request.id = None;
        let facts = handle_tax(&reader, "company-1", request)
            .await
            .expect("valid read");
        assert_eq!(facts, TaxSignatureFacts::Workspace { rows: Vec::new() });
        assert_eq!(
            reader.calls.lock().expect("mutex").as_slice(),
            &[("company-1".to_owned(), TaxSignatureReadKind::Workspace)]
        );
    }
}
