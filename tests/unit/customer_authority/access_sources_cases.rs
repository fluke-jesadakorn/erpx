//! Focused tests for the fully-native customer-access sources read.
//!
//! These cases run inside the registered `customer_access` Cargo target, so no
//! Cargo.toml change is needed. They exercise the public owner port and router
//! with a fake source/backend: no SQL string restatement, no server and no TS.
use erpx_server_customer_authority::public::access_sources::{
    CUSTOMER_ACCESS_SOURCES_PATH, AccessSourceRow, AccessSourcesBackend, AccessSourcesQuery,
    AccessSourcesRequest, AccessSourcesSource, SourcesFuture, customer_access_sources,
    customer_access_sources_router, validate_rows,
};
use erpx_server_shared::public::FailureKind;
use std::sync::Arc;

#[derive(Default)]
struct FakeSource {
    rows: Vec<AccessSourceRow>,
    calls: usize,
}

impl AccessSourcesSource for FakeSource {
    fn sources<'a>(
        &'a mut self,
        _: &'a AccessSourcesQuery,
    ) -> SourcesFuture<'a, Vec<AccessSourceRow>> {
        self.calls += 1;
        let rows = self.rows.clone();
        Box::pin(async move { Ok(rows) })
    }
}

fn query() -> AccessSourcesQuery {
    AccessSourcesQuery {
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
    }
}

fn row(id: &str) -> AccessSourceRow {
    AccessSourceRow {
        id: id.into(),
        number: "SO-100".into(),
        name: "Acme".into(),
        version: 2,
    }
}

#[tokio::test]
async fn use_case_returns_every_field_of_the_native_payload() {
    let mut source = FakeSource {
        rows: vec![row("a"), row("b")],
        ..FakeSource::default()
    };
    let rows = customer_access_sources(&mut source, query()).await.unwrap();
    assert_eq!(source.calls, 1);
    assert_eq!(rows.len(), 2);
    let value = rows[0].to_json();
    assert_eq!(value["id"], "a");
    assert_eq!(value["number"], "SO-100");
    assert_eq!(value["name"], "Acme");
    assert_eq!(value["version"], 2);
}

#[tokio::test]
async fn invalid_scope_never_reaches_the_source() {
    let mut source = FakeSource::default();
    let bad = AccessSourcesQuery {
        company_id: String::new(),
        ..query()
    };
    let error = customer_access_sources(&mut source, bad).await.unwrap_err();
    assert_eq!(error.kind, FailureKind::InvalidInput);
    assert_eq!(error.code, "CUSTOMER_ACCESS_SOURCES_INVALID");
    assert_eq!(source.calls, 0);
}

#[test]
fn payload_invariants_reject_incomplete_and_duplicate_rows() {
    assert!(validate_rows(vec![row("a"), row("b")]).is_ok());
    assert!(validate_rows(vec![row("a"), row("a")]).is_err());
    assert!(validate_rows(vec![AccessSourceRow {
        version: 0,
        ..row("a")
    }])
    .is_err());
}

struct FakeBackend;
impl AccessSourcesBackend for FakeBackend {
    fn execute<'a>(
        &'a self,
        _: AccessSourcesRequest<'a>,
    ) -> SourcesFuture<'a, Vec<AccessSourceRow>> {
        Box::pin(async { Ok(vec![row("a")]) })
    }
}

#[test]
fn router_uses_the_native_customer_access_sources_path_and_rejects_bad_targets() {
    assert_eq!(
        CUSTOMER_ACCESS_SOURCES_PATH,
        "/api/v1/holdings/{holding_slug}/customer-access/sources"
    );
    let good = erpx_server_customer_authority::public::access_sources::AccessSourcesTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        company_id: "company".into(),
        backend: Arc::new(FakeBackend),
    };
    assert!(customer_access_sources_router(vec![good.clone()]).is_ok());
    let mut bad = good;
    bad.company_id.clear();
    assert!(customer_access_sources_router(vec![bad]).is_err());
}
