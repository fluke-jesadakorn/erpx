//! Native Work Projection report HTTP ingress on registered Holding targets.
use erpx_platform_runtime_server::public::http_support::{HeaderMap, StatusCode};
use erpx_work_projection_server::public::work_report_read_http::{
    WorkReportHttpBackend, WorkReportHttpFuture, WorkReportHttpRequest, WorkReportHttpProjection,
    WorkReportHttpResponse, WorkReportHttpState, WorkReportHttpTarget, parse_work_report_projection,
    work_report_router,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

struct Fake {
    seen: Mutex<Vec<String>>,
}

impl WorkReportHttpBackend for Fake {
    fn read<'a>(
        &'a self,
        request: WorkReportHttpRequest<'a>,
    ) -> WorkReportHttpFuture<'a, WorkReportHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "read:{}:{}:{}:{}",
                request.host,
                request.holding_slug,
                format!("{:?}", request.projection.mode),
                request.projection.export_all
            ));
            Ok(WorkReportHttpResponse {
                data: json!({
                    "counts": {"total": 1, "open": 1, "drafts": 0, "unknown": 0, "blocked": 0, "units": 0},
                    "money": [], "estimates": [], "stageCounts": [],
                    "rows": [{"id": "one"}], "allRows": []
                }),
                context: json!({"companyId": "acme-co", "revision": 7}),
            })
        })
    }
}

fn state() -> (WorkReportHttpState, Arc<Fake>) {
    let fake = Arc::new(Fake {
        seen: Mutex::new(Vec::new()),
    });
    let state = WorkReportHttpState::new(vec![WorkReportHttpTarget {
        host: "holding.erpxbrowser.test:3102".into(),
        holding_slug: "acme".into(),
        origin: "http://127.0.0.1:3102".into(),
        backend: fake.clone(),
    }])
    .unwrap();
    (state, fake)
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

fn headers(origin: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("host", "holding.erpxbrowser.test:3102".parse().unwrap());
    if let Some(origin) = origin {
        headers.insert("origin", origin.parse().unwrap());
    }
    headers
}

fn projection(export_all: bool) -> WorkReportHttpProjection {
    parse_work_report_projection(&json!({
        "mode": "open", "scoped": "[{\"id\":\"one\"}]",
        "start": 0, "end": 10, "page": 1, "exportAll": export_all,
    }))
    .unwrap()
}

#[test]
fn reads_the_exact_registered_target_and_projection() {
    let (state, fake) = state();
    let response = block_on(state.read(
        &headers(Some("http://127.0.0.1:3102")),
        "acme",
        &projection(true),
    ));
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        fake.seen.lock().unwrap().as_slice(),
        ["read:holding.erpxbrowser.test:3102:acme:Open:true"]
    );
}

#[test]
fn rejects_wrong_origin_and_unknown_holding() {
    let (state, _) = state();
    assert_eq!(
        block_on(state.read(&headers(Some("https://evil.test")), "acme", &projection(false)))
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        block_on(state.read(
            &headers(Some("http://127.0.0.1:3102")),
            "other",
            &projection(false)
        ))
        .status(),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn router_builds_for_the_exact_native_report_path() {
    let backend: Arc<dyn WorkReportHttpBackend> = Arc::new(Fake {
        seen: Mutex::new(Vec::new()),
    });
    assert!(work_report_router(vec![WorkReportHttpTarget {
        host: "holding.erpxbrowser.test:3102".into(),
        holding_slug: "acme".into(),
        origin: "http://127.0.0.1:3102".into(),
        backend,
    }])
    .is_ok());
}
