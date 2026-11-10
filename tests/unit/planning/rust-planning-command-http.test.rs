//! Owner-local HTTP router tests for the native Planning commands.
use erpx_server_planning::public::planning_command_http::{
    PlanningCommandHttpBackend, PlanningCommandHttpCommand, PlanningCommandHttpFuture,
    PlanningCommandHttpRequest, PlanningCommandHttpResponse, PlanningCommandHttpTarget,
    planning_command_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
    calls: Arc<AtomicUsize>,
}

impl PlanningCommandHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: PlanningCommandHttpRequest<'a>,
    ) -> PlanningCommandHttpFuture<'a, PlanningCommandHttpResponse> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{}",
                request.host,
                request.holding_slug,
                match &request.command {
                    PlanningCommandHttpCommand::Calculate { .. } => "calculate",
                    PlanningCommandHttpCommand::Cancel { .. } => "cancel",
                    PlanningCommandHttpCommand::Review { .. } => "review",
                    PlanningCommandHttpCommand::CreateResource { .. } => "create-resource",
                    PlanningCommandHttpCommand::UpdateResource { .. } => "update-resource",
                    PlanningCommandHttpCommand::SaveCalendar { .. } => "calendar",
                }
            ));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "PLANNING_SOURCE_CHANGED",
                    "Synthetic changed planning source.",
                ));
            }
            Ok(PlanningCommandHttpResponse {
                data: serde_json::json!({"id": "plan", "status": "calculated"}),
                context: serde_json::json!({"userId": "planner"}),
            })
        })
    }
}

fn target(backend: Arc<dyn PlanningCommandHttpBackend>) -> PlanningCommandHttpTarget {
    PlanningCommandHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

fn probe_with(fail: bool) -> (Arc<Probe>, Arc<Mutex<Vec<String>>>, Arc<AtomicUsize>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::new(AtomicUsize::new(0));
    (
        Arc::new(Probe {
            seen: seen.clone(),
            fail,
            calls: calls.clone(),
        }),
        seen,
        calls,
    )
}

#[test]
fn command_targets_require_distinct_exact_https_holdings() {
    let (backend, _, _) = probe_with(false);
    assert!(planning_command_router(vec![target(backend.clone())]).is_ok());
    assert!(
        planning_command_router(vec![target(backend.clone()), target(backend.clone())]).is_err()
    );
    for origin in ["http://acme.example.test", "https://acme.example.test:0"] {
        assert!(
            planning_command_router(vec![PlanningCommandHttpTarget {
                origin: origin.into(),
                ..target(backend.clone())
            }])
            .is_err()
        );
    }
    assert!(
        planning_command_router(vec![PlanningCommandHttpTarget {
            holding_slug: "Bad Slug".into(),
            ..target(backend)
        }])
        .is_err()
    );
}

async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
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
    )
}

fn post(path: &str, body: &str, key: Option<&str>, content_type: &str) -> String {
    let key = key.map_or_else(String::new, |value| format!("Idempotency-Key: {value}\r\n"));
    format!(
        "POST {path} HTTP/1.1\r\nHost: acme.example.test\r\n{key}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[tokio::test]
async fn command_router_enforces_body_key_query_and_target_contracts() {
    let (probe, seen, calls) = probe_with(false);
    let (failing, _, _) = probe_with(true);
    let router = planning_command_router(vec![
        target(probe),
        PlanningCommandHttpTarget {
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

    let body = r#"{"warehouseId":"warehouse","specificationId":"specification","specificationVersion":2,"demandQuantity":"2","dueDate":"2026-10-01"}"#;
    let (status, reply) = http(
        addr,
        post(
            "/api/v1/holdings/acme/planning/scenarios",
            body,
            Some("planning-key-0003"),
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(reply["data"]["id"], "plan");
    assert_eq!(reply["context"]["userId"], "planner");

    let (status, reply) = http(
        addr,
        post(
            "/api/v1/holdings/acme/planning/scenarios",
            body,
            None,
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(reply["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, reply) = http(
        addr,
        post(
            "/api/v1/holdings/acme/planning/scenarios?view=all",
            body,
            Some("planning-key-0003"),
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(reply["error"]["code"], "PLANNING_REQUEST_INVALID");

    let (status, reply) = http(
        addr,
        post(
            "/api/v1/holdings/acme/planning/scenarios",
            body,
            Some("planning-key-0003"),
            "text/plain",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(reply["error"]["code"], "PLANNING_REQUEST_INVALID");

    let (status, _) = http(
        addr,
        post(
            "/api/v1/holdings/missing/planning/scenarios",
            body,
            Some("planning-key-0003"),
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, reply) = http(
        addr,
        "POST /api/v1/holdings/fail/planning/scenarios HTTP/1.1\r\nHost: fail.example.test\r\nIdempotency-Key: planning-key-0003\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".into(),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(reply["error"]["code"], "PLANNING_SOURCE_CHANGED");

    let long_id = "a".repeat(201);
    let (status, _) = http(
        addr,
        post(
            &format!("/api/v1/holdings/acme/planning/scenarios/{long_id}"),
            r#"{"action":"cancel","expectedVersion":1,"reason":"Replaced"}"#,
            Some("planning-key-0004"),
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/planning/recommendations/recommendation",
            r#"{"action":"approve","expectedVersion":1,"reason":"Independent review"}"#,
            Some("planning-key-0005"),
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, 200);

    let (status, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/planning/resources",
            r#"{"branchId":"branch","code":"CELL","name":"Assembly cell","dailyMinutes":480}"#,
            Some("planning-key-0006"),
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, 200);

    let (status, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/planning/resources/resource/calendar",
            r#"{"expectedVersion":0,"resourceVersion":2,"workDate":"2026-10-04","availableMinutes":60,"reason":"Verified shutdown"}"#,
            Some("planning-key-0007"),
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, 200);

    assert_eq!(calls.load(Ordering::SeqCst), 4);
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen[0], "acme.example.test|acme|calculate");
    assert_eq!(seen[1], "acme.example.test|acme|review");
    assert_eq!(seen[2], "acme.example.test|acme|create-resource");
    assert_eq!(seen[3], "acme.example.test|acme|calendar");
    server.abort();
}
