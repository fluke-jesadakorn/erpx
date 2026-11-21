use erpx_server_legal::public::{
    DocumentComputeBudget, document_processing::extraction_compute_budget,
};

#[test]
fn admitted_compute_always_reserves_the_full_retention_and_cleanup_headroom() {
    for (lease, now, expected) in [
        (600_000, 0, Some(540_000)),
        (600_000, 100_000, Some(440_000)),
        (600_000, 539_999, Some(1)),
        (600_000, 540_000, None),
        (600_000, 600_001, None),
        (i64::MAX, i64::MIN, None),
    ] {
        assert_eq!(
            extraction_compute_budget(lease, now).map(|budget| budget.millis()),
            expected
        );
    }
}
#[test]
fn callers_cannot_extend_the_aggregate_worker_limit() {
    assert!(DocumentComputeBudget::new(0).is_err());
    assert!(DocumentComputeBudget::new(540_001).is_err());
    assert_eq!(
        extraction_compute_budget(900_000, 0).unwrap().millis(),
        540_000
    );
}
