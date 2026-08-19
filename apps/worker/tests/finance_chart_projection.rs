use erpx_infrastructure::FinanceChartSourceEventCursor;
use erpx_worker::{FinanceChartProjectionRequest, FinanceChartProjectionWorkerError};
use uuid::Uuid;

#[test]
fn request_validation_rejects_unsafe_routes_and_cursors() {
    assert!(FinanceChartProjectionRequest::new("holding", "holding", None, 0).is_err());
    assert!(FinanceChartProjectionRequest::new("holding/path", "holding", None, 1).is_err());
    assert!(
        FinanceChartProjectionRequest::new(
            "holding",
            "holding",
            Some(FinanceChartSourceEventCursor {
                aggregate_id: Uuid::nil(),
                aggregate_version: 1,
            }),
            1,
        )
        .is_err()
    );
}

#[test]
fn worker_errors_are_redacted() {
    let error = FinanceChartProjectionWorkerError::Projection {
        operation: "projection page",
    };
    assert!(!error.to_string().contains("postgres"));
    assert!(!error.to_string().contains("password"));
}
