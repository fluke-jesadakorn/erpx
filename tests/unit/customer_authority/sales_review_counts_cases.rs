//! Focused tests for the fully-native sales customer review counts read.
//!
//! These cases run inside the registered `customer_access` Cargo target, so no
//! Cargo.toml change is needed. They exercise the public owner port and router
//! with a fake source/backend: no SQL string restatement, no server and no TS.
use erpx_customer_authority_server::public::sales_review_counts::{
    ReviewCount, SalesReviewCountsBackend, SalesReviewCountsQuery, SalesReviewCountsRequest,
    SalesReviewCountsSource, ReviewCountsFuture, REVIEW_KINDS, sales_customer_review_counts,
    sales_review_counts_router, validate_counts,
};
use erpx_platform_shared_server::public::FailureKind;
use std::sync::Arc;

#[derive(Default)]
struct FakeSource {
    rows: Vec<ReviewCount>,
    calls: usize,
}

impl SalesReviewCountsSource for FakeSource {
    fn review_counts<'a>(
        &'a mut self,
        _: &'a SalesReviewCountsQuery,
    ) -> ReviewCountsFuture<'a, Vec<ReviewCount>> {
        self.calls += 1;
        let rows = self.rows.clone();
        Box::pin(async move { Ok(rows) })
    }
}

struct FakeBackend;
impl SalesReviewCountsBackend for FakeBackend {
    fn execute<'a>(
        &'a self,
        _: SalesReviewCountsRequest<'a>,
    ) -> ReviewCountsFuture<'a, Vec<ReviewCount>> {
        Box::pin(async { Ok(counts()) })
    }
}

fn query() -> SalesReviewCountsQuery {
    SalesReviewCountsQuery {
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
    }
}

fn counts() -> Vec<ReviewCount> {
    REVIEW_KINDS
        .iter()
        .enumerate()
        .map(|(n, kind)| ReviewCount {
            kind: (*kind).into(),
            count: n as i64,
        })
        .collect()
}

fn target() -> erpx_customer_authority_server::public::sales_review_counts::SalesReviewCountsTarget {
    erpx_customer_authority_server::public::sales_review_counts::SalesReviewCountsTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        company_id: "company".into(),
        backend: Arc::new(FakeBackend),
    }
}

#[tokio::test]
async fn use_case_returns_every_field_of_the_native_payload() {
    let mut source = FakeSource {
        rows: counts(),
        ..FakeSource::default()
    };
    let rows = sales_customer_review_counts(&mut source, query()).await.unwrap();
    assert_eq!(source.calls, 1);
    assert_eq!(rows.len(), 3);
    for (index, row) in rows.iter().enumerate() {
        let value = row.to_json();
        assert_eq!(value["kind"], REVIEW_KINDS[index]);
        assert_eq!(value["count"], index as i64);
    }
}

#[tokio::test]
async fn invalid_scope_is_rejected_before_touching_the_source() {
    let mut source = FakeSource::default();
    let bad = SalesReviewCountsQuery {
        company_id: String::new(),
        ..query()
    };
    let error = sales_customer_review_counts(&mut source, bad).await.unwrap_err();
    assert_eq!(error.code, "CUSTOMER_SALES_REVIEW_COUNTS_INVALID");
    assert_eq!(error.kind, FailureKind::InvalidInput);
    assert_eq!(source.calls, 0);
}

#[tokio::test]
async fn incomplete_payload_is_rejected() {
    let mut source = FakeSource {
        rows: counts()[..2].to_vec(),
        ..FakeSource::default()
    };
    let error = sales_customer_review_counts(&mut source, query()).await.unwrap_err();
    assert_eq!(error.code, "CUSTOMER_SALES_REVIEW_COUNTS_UNAVAILABLE");
    assert_eq!(error.kind, FailureKind::Unavailable);
}

#[test]
fn full_kind_set_validates_but_partial_or_open_sets_do_not() {
    assert_eq!(validate_counts(counts()).unwrap().len(), 3);
    let mut open = counts();
    open[1].kind = "unknown".into();
    assert!(validate_counts(open).is_err());
    assert!(validate_counts(counts()[..1].to_vec()).is_err());
}

#[test]
fn router_accepts_exact_targets_and_rejects_bad_ones() {
    assert!(sales_review_counts_router(vec![target()]).is_ok());
    let mut bad = target();
    bad.company_id.clear();
    assert_eq!(
        sales_review_counts_router(vec![bad]).unwrap_err().code,
        "CUSTOMER_SALES_REVIEW_COUNTS_CONFIG_INVALID"
    );
    let mut bad = target();
    bad.origin = "http://acme.example.test".into();
    assert!(sales_review_counts_router(vec![bad]).is_err());
    assert!(sales_review_counts_router(vec![target(), target()]).is_err());
}
