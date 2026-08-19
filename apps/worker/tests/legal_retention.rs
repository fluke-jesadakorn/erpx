use erpx_worker::{
    LEGAL_RETENTION_MAX_BATCH_SIZE, LegalRetentionRequest, LegalRetentionWorkerContext,
};
use uuid::Uuid;

fn context() -> LegalRetentionWorkerContext {
    LegalRetentionWorkerContext::new(
        "system-worker:legal-retention",
        "expire legal contract retention",
        Uuid::new_v4(),
    )
    .expect("valid Legal retention context")
}

#[test]
fn request_is_bounded_and_holding_scoped() {
    let request = LegalRetentionRequest::new(
        "holding-a",
        "holding-a",
        LEGAL_RETENTION_MAX_BATCH_SIZE,
        context(),
    )
    .expect("maximum bounded request is valid");
    assert_eq!(request.holding_id, "holding-a");
    assert_eq!(request.data_profile_key, "holding-a");
    assert_eq!(request.batch_size, LEGAL_RETENTION_MAX_BATCH_SIZE);
}

#[test]
fn request_rejects_cross_scope_or_unbounded_input() {
    assert!(LegalRetentionRequest::new("holding/a", "holding-a", 1, context()).is_err());
    assert!(
        LegalRetentionRequest::new(
            "holding-a",
            "holding-a",
            LEGAL_RETENTION_MAX_BATCH_SIZE + 1,
            context(),
        )
        .is_err()
    );
}
