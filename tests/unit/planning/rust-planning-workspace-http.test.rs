//! Actual owner-local HTTP router tests for the native Planning workspace read.
use erpx_server_manufacturing::public::planning_http::{
    PlanningHttpBackend, PlanningHttpFuture, PlanningHttpRequest, PlanningHttpResponse,
    PlanningHttpTarget, planning_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl PlanningHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: PlanningHttpRequest<'a>,
    ) -> PlanningHttpFuture<'a, PlanningHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
}

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl PlanningHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: PlanningHttpRequest<'a>,
    ) -> PlanningHttpFuture<'a, PlanningHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{}",
                request.host,
                request.holding_slug,
                request
                    .headers
                    .get("host")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
            ));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "PLANNING_WORKSPACE_SOURCE_CHANGED",
                    "Synthetic changed planning source.",
                ));
            }
            Ok(PlanningHttpResponse {
                data: serde_json::json!({
                    "scenarios": [{
                        "id": "scenario",
                        "company_id": "company",
                        "branch_id": "branch",
                        "warehouse_id": "warehouse",
                        "specification_id": "specification",
                        "specification_version": 2,
                        "demand_quantity_milli": 2000,
                        "due_date": "2026-10-04",
                        "capacity_minutes": 120,
                        "status": "calculated",
                        "created_by": "planner",
                        "version": 1,
                        "created_at": 100,
                        "updated_at": 100,
                        "shortages": [],
                        "context": {}
                    }],
                    "recommendations": [],
                    "resources": [],
                    "calendar": [],
                    "allocations": []
                }),
                context: serde_json::json!({"userId": "planner"}),
            })
        })
    }
}

fn target(backend: Arc<dyn PlanningHttpBackend>) -> PlanningHttpTarget {
    PlanningHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

#[test]
fn target_configuration_requires_distinct_exact_https_holdings() {
    let backend = Arc::new(Noop);
    assert!(planning_router(vec![target(backend.clone())]).is_ok());
    assert!(planning_router(vec![target(backend.clone()), target(backend.clone())]).is_err());
    assert!(
        planning_router(vec![PlanningHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target(backend.clone())
        }])
        .is_err()
    );
    assert!(
        planning_router(vec![PlanningHttpTarget {
            holding_slug: "Bad Slug".into(),
            ..target(backend)
        }])
        .is_err()
    );
}

async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let text = String::from_utf8_lossy(&bytes).to_string();
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .unwrap();
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
    (
        status,
        serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        text.to_ascii_lowercase(),
    )
}

#[tokio::test]
async fn actual_router_retains_target_query_and_error_mapping() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(Probe {
        seen: seen.clone(),
        fail: false,
    });
    let failing = Arc::new(Probe {
        seen: seen.clone(),
        fail: true,
    });
    let router = planning_router(vec![
        target(probe),
        PlanningHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: failing,
        },
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    let get = |path: &str| {
        format!("GET {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n")
    };

    let (status, body, headers) = http(addr, get("/api/v1/holdings/acme/planning/workspace")).await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["scenarios"][0]["id"], "scenario");
    assert_eq!(
        body["data"]["scenarios"][0]["shortages"],
        serde_json::json!([])
    );
    assert_eq!(body["context"]["userId"], "planner");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    let (status, body, _) = http(
        addr,
        get("/api/v1/holdings/acme/planning/workspace?view=all"),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "PLANNING_REQUEST_INVALID");

    let (status, _, _) = http(addr, get("/api/v1/holdings/missing/planning/workspace")).await;
    assert_eq!(status, 404);

    let (status, body, _) = http(
        addr,
        "GET /api/v1/holdings/fail/planning/workspace HTTP/1.1\r\nHost: fail.example.test\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "PLANNING_WORKSPACE_SOURCE_CHANGED");

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert!(seen[0].starts_with("acme.example.test|acme|acme.example.test"));
    assert!(seen[1].starts_with("fail.example.test|fail|fail.example.test"));
    server.abort();
}
