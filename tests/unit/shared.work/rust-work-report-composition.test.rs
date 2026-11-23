//! Composition assembly of native Work report Holding targets.
//!
//! Mirrors the private-development target construction: every configured
//! Holding contributes one exact Host/Holding target, and the owner router
//! resolves each request to its own backend before the backend authorizes the
//! current session. No session, database or business authority is granted here.
use erpx_server_runtime::public::http_support::{HeaderMap, StatusCode};
use erpx_server_work_projection::public::work_report_read_http::{
    WorkReportHttpBackend, WorkReportHttpFuture, WorkReportHttpRequest, WorkReportHttpProjection,
    WorkReportHttpResponse, WorkReportHttpState, WorkReportHttpTarget, parse_work_report_projection,
    work_report_router,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
struct ConfiguredHolding {
    host: &'static str,
    slug: &'static str,
    origin: &'static str,
}

fn holdings() -> Vec<ConfiguredHolding> {
    vec![
        ConfiguredHolding {
            host: "acme.example.test",
            slug: "acme",
            origin: "https://acme.example.test",
        },
        ConfiguredHolding {
            host: "northstar.example.test:3102",
            slug: "northstar",
            origin: "http://127.0.0.1:3102",
        },
    ]
}

#[derive(Default)]
struct Recorder {
    seen: Mutex<Vec<String>>,
}

impl WorkReportHttpBackend for Recorder {
    fn read<'a>(
        &'a self,
        request: WorkReportHttpRequest<'a>,
    ) -> WorkReportHttpFuture<'a, WorkReportHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "read:{}:{}:{:?}:{}:{}:{}:{}",
                request.host,
                request.holding_slug,
                request.projection.mode,
                request.projection.scoped,
                request.projection.start,
                request.projection.end,
                request.projection.export_all
            ));
            Ok(WorkReportHttpResponse {
                data: json!({"counts": {"total": 1}, "rows": []}),
                context: json!({"companyId": "acme-co", "revision": 7}),
            })
        })
    }
}

fn targets() -> (Vec<WorkReportHttpTarget>, Vec<Arc<Recorder>>) {
    let mut backends = Vec::new();
    let targets = holdings()
        .into_iter()
        .map(|holding| {
            let backend = Arc::new(Recorder::default());
            backends.push(backend.clone());
            WorkReportHttpTarget {
                host: holding.host.into(),
                holding_slug: holding.slug.into(),
                origin: holding.origin.into(),
                backend,
            }
        })
        .collect();
    (targets, backends)
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

fn headers(host: &str, origin: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("host", host.parse().unwrap());
    if let Some(origin) = origin {
        headers.insert("origin", origin.parse().unwrap());
    }
    headers
}

fn projection() -> WorkReportHttpProjection {
    parse_work_report_projection(&json!({
        "mode": "period", "scoped": "[{\"id\":\"one\"}]",
        "start": 10, "end": 20, "page": 2, "exportAll": true,
    }))
    .unwrap()
}

#[test]
fn builds_one_router_from_all_configured_holdings() {
    let (targets, _) = targets();
    let state = WorkReportHttpState::new(targets).unwrap();
    assert!(work_report_router(
        holdings()
            .into_iter()
            .map(|holding| WorkReportHttpTarget {
                host: holding.host.into(),
                holding_slug: holding.slug.into(),
                origin: holding.origin.into(),
                backend: Arc::new(Recorder::default()),
            })
            .collect()
    )
    .is_ok());
    assert_eq!(
        block_on(state.read(
            &headers("northstar.example.test:3102", Some("http://127.0.0.1:3102")),
            "northstar",
            &projection()
        ))
        .status(),
        StatusCode::OK
    );
}

#[test]
fn each_holding_resolves_to_its_own_backend_with_exact_projection() {
    let (targets, backends) = targets();
    let state = WorkReportHttpState::new(targets).unwrap();
    let response = block_on(state.read(
        &headers("acme.example.test", Some("https://acme.example.test")),
        "acme",
        &projection(),
    ));
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        backends[0].seen.lock().unwrap().as_slice(),
        ["read:acme.example.test:acme:Period:[{\"id\":\"one\"}]:10:20:true"]
    );
    assert!(backends[1].seen.lock().unwrap().is_empty());
}

#[test]
fn rejects_unknown_holding_and_wrong_origin_per_target() {
    let (targets, backends) = targets();
    let state = WorkReportHttpState::new(targets).unwrap();
    assert_eq!(
        block_on(state.read(
            &headers("northstar.example.test:3102", Some("https://evil.test")),
            "northstar",
            &projection()
        ))
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        block_on(state.read(
            &headers("northstar.example.test:3102", Some("http://127.0.0.1:3102")),
            "acme",
            &projection()
        ))
        .status(),
        StatusCode::NOT_FOUND
    );
    assert!(backends.iter().all(|backend| backend.seen.lock().unwrap().is_empty()));
}
