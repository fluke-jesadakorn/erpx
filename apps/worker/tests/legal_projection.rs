use erpx_infrastructure::LegalSourceEventCursor;
use erpx_worker::{LEGAL_PROJECTION_HANDLER_NAME, LegalProjectionRequest};
use uuid::Uuid;

#[test]
fn legal_worker_request_is_bounded_and_named() {
    assert_eq!(LEGAL_PROJECTION_HANDLER_NAME, "legal_documents_projection");
    assert!(LegalProjectionRequest::new("tenant", "holding", None, 1).is_ok());
    assert!(LegalProjectionRequest::new("tenant", "holding", None, 0).is_err());
    assert!(LegalProjectionRequest::new("tenant/unsafe", "holding", None, 1).is_err());
    assert!(
        LegalProjectionRequest::new(
            "tenant",
            "holding",
            Some(LegalSourceEventCursor {
                aggregate_type: "legal_contract".into(),
                aggregate_id: Uuid::nil(),
                aggregate_version: 1,
            }),
            1,
        )
        .is_err()
    );
}
