//! Query port and handler for the lane 19 PMS evidence read. No SQL here: the
//! owner reader executes the statement and returns an already-projected value.

use std::{future::Future, pin::Pin};

use erpx_platform_shared_server::public::BusinessError;

use super::domain::{EvidenceRequest, GuestAccountEvidence, validate_request};

pub type Lane19ReadFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the owner reader behind this port. The reader owns the
/// SQL, the transaction and the projection; the handler only transports facts.
pub trait GuestAccountEvidenceRead: Send + Sync {
    fn evidence<'a>(
        &'a self,
        request: &'a EvidenceRequest,
    ) -> Lane19ReadFuture<'a, Option<GuestAccountEvidence>>;
}

/// Validate the bounded request before any owner read is attempted.
pub async fn read_guest_account_evidence(
    reader: &dyn GuestAccountEvidenceRead,
    request: EvidenceRequest,
) -> Result<Option<GuestAccountEvidence>, BusinessError> {
    validate_request(&request)?;
    reader.evidence(&request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_19::domain::{GuestAccountEvidence, invalid};
    use serde_json::json;

    struct FakeReader {
        called: std::sync::atomic::AtomicBool,
        evidence: Option<GuestAccountEvidence>,
    }

    impl GuestAccountEvidenceRead for FakeReader {
        fn evidence<'a>(
            &'a self,
            _request: &'a EvidenceRequest,
        ) -> Lane19ReadFuture<'a, Option<GuestAccountEvidence>> {
            self.called
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async move { Ok(self.evidence.clone()) })
        }
    }

    fn request(id: &str) -> EvidenceRequest {
        EvidenceRequest {
            company: "company-1".into(),
            branch: None,
            id: id.into(),
        }
    }

    #[test]
    fn rejects_invalid_requests_without_touching_the_reader() {
        let reader = FakeReader {
            called: std::sync::atomic::AtomicBool::new(false),
            evidence: None,
        };
        let future = tokio_test_block_on(read_guest_account_evidence(&reader, request("bad/id")));
        assert_eq!(future.unwrap_err(), invalid());
        assert!(!reader.called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn returns_the_owner_projection() {
        let reader = FakeReader {
            called: std::sync::atomic::AtomicBool::new(false),
            evidence: Some(GuestAccountEvidence {
                id: "acct-1".into(),
                property_id: "prop-1".into(),
                hotel: "Riverside".into(),
                business_date: "2026-11-25".into(),
                currency: "THB".into(),
                account: json!({}),
                entries: vec![],
            }),
        };
        let value = tokio_test_block_on(read_guest_account_evidence(&reader, request("acct-1")))
            .expect("handler")
            .expect("evidence");
        assert_eq!(value.id, "acct-1");
        assert!(reader.called.load(std::sync::atomic::Ordering::SeqCst));
    }

    fn tokio_test_block_on<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(future)
    }
}
