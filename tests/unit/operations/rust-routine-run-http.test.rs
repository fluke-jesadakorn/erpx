//! Native routine run HTTP ingress facts over a real loopback listener.
use erpx_operations_server::public::routine_commands_http::{
    RoutineHttpBackend, RoutineHttpCommandRequest, RoutineHttpFuture, RoutineHttpResponse,
    RoutineHttpTarget, routine_commands_router,
};
use erpx_platform_runtime_server::public::http_support::{
    Router, serde_json::Value, serde_json::json, serve,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

struct Backend {
    commands: AtomicUsize,
    seen: std::sync::Mutex<Vec<String>>,
}
impl Backend {
    fn new() -> Self {
        Self {
            commands: AtomicUsize::new(0),
            seen: std::sync::Mutex::new(Vec::new()),
        }
    }
}
impl RoutineHttpBackend for Backend {
    fn command<'a>(
        &'a self,
        request: RoutineHttpCommandRequest<'a>,
    ) -> RoutineHttpFuture<'a, RoutineHttpResponse> {
        self.commands.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(format!(
            "{}:{}:{}:{:?}",
            request.host,
            request.holding_slug,
            request.key.as_str(),
            request.command
        ));
        Box::pin(async move {
            Ok(RoutineHttpResponse {
                data: json!({
                    "id": format!("task-{}", request.key.as_str()),
                    "state": "ready",
                    "version": 1
                }),
                context: json!({"userId": "runner"}),
            })
        })
    }

    fn schedules<'a>(
        &'a self,
        _: &'a str,
        _: &'a erpx_platform_runtime_server::public::http_support::HeaderMap,
        template_id: String,
    ) -> RoutineHttpFuture<'a, RoutineHttpResponse> {
        Box::pin(async move {
            Ok(RoutineHttpResponse {
                data: json!({"template":{"id":template_id},"schedules":[],"canEdit":false}),
                context: json!({"userId":"runner"}),
            })
        })
    }

    fn editor_context<'a>(
        &'a self,
        _: &'a str,
        _: &'a erpx_platform_runtime_server::public::http_support::HeaderMap,
    ) -> RoutineHttpFuture<'a, RoutineHttpResponse> {
        Box::pin(async {
            Ok(RoutineHttpResponse {
                data: json!({"projects":[],"options":{"people":[],"levels":[],"departments":[]},"canWrite":false,"purchases":null}),
                context: json!({"userId":"runner"}),
            })
        })
    }
}

fn router(backend: Arc<Backend>) -> Router {
    routine_commands_router(vec![RoutineHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }])
    .unwrap()
}
async fn start(backend: Arc<Backend>) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        serve(listener, router(backend)).await.unwrap();
    });
    (address, task)
}
async fn post(
    address: std::net::SocketAddr,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (u16, Value) {
    request(address, "POST", path, headers, body).await
}

async fn request(
    address: std::net::SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (u16, Value) {
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let response = String::from_utf8_lossy(&bytes);
    let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap();
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn routine_editor_context_only_serves_the_private_ssr_transport() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let path = "/api/v1/holdings/acme/routine-template-editor-context";
    let (status, reply) = request(address, "GET", path, &[], "").await;
    assert_eq!(status, 200);
    assert_eq!(reply["data"]["options"]["people"], json!([]));
    assert_eq!(reply["data"]["canWrite"], false);
    assert_eq!(reply["context"]["userId"], "runner");
    assert_eq!(
        request(
            address,
            "GET",
            path,
            &[("x-erpx-api-transport", "public")],
            ""
        )
        .await
        .0,
        404
    );
    assert_eq!(request(address, "POST", path, &[], "").await.0, 405);
    assert_eq!(request(address, "OPTIONS", path, &[], "").await.0, 204);
    assert_eq!(backend.commands.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn nested_template_read_keeps_legacy_paths_and_public_shape() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    for tail in ["schedules", "schedules/old-id", "run", "other/legacy"] {
        let path = format!("/api/v1/holdings/acme/routine-templates/routine-1/{tail}");
        let (status, payload) = request(
            address,
            "GET",
            &path,
            &[("x-erpx-api-transport", "public")],
            "",
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(
            payload,
            json!({"data":{"template":{"id":"routine-1"},"schedules":[],"canEdit":false}})
        );
    }
    assert_eq!(backend.commands.load(Ordering::SeqCst), 0);
    task.abort();
}
const BODY: &str = r#"{"dueAt":"2026-12-01T02:00:00Z"}"#;
const HEADERS: &[(&str, &str)] = &[
    ("Origin", "https://acme.example.test"),
    ("Idempotency-Key", "run-key-0001"),
    ("Content-Type", "application/json"),
];

#[tokio::test]
async fn schedule_state_routes_dispatch_only_supported_commands() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let base = "/api/v1/holdings/acme/routine-schedules";
    let mut headers = HEADERS.to_vec();
    headers.push(("x-erpx-api-transport", "public"));
    for operation in ["pause", "resume", "end"] {
        let path = format!("{base}/schedule-1/{operation}");
        let (status, payload) = post(address, &path, &headers, r#"{"expectedVersion":1}"#).await;
        assert_eq!(status, 200);
        assert_eq!(
            payload,
            json!({"data":{"id":"task-run-key-0001","version":1,"state":"ready"}})
        );
    }

    let (status, payload) = post(
        address,
        &format!("{base}/schedule-1/pause/extra"),
        HEADERS,
        "{}",
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(payload["error"]["message"], "Staff action not found.");
    let (status, payload) = post(address, base, HEADERS, "{").await;
    assert_eq!(status, 422);
    assert_eq!(payload["error"]["code"], "INVALID_JSON");

    for path in [
        base.to_owned(),
        format!("{base}/schedule-1"),
        format!("{base}/schedule-1/end"),
    ] {
        let (status, payload) = request(address, "GET", &path, &[], "").await;
        assert_eq!(status, 404);
        assert_eq!(payload["error"]["message"], "Staff resource not found.");
        assert_eq!(
            request(address, "HEAD", &path, &[], "").await,
            (422, Value::Null)
        );
        assert_eq!(
            request(address, "OPTIONS", &path, &[], "").await,
            (204, Value::Null)
        );
        assert_eq!(
            request(address, "PUT", &path, &[], "").await,
            (405, Value::Null)
        );
    }
    assert_eq!(backend.commands.load(Ordering::SeqCst), 3);
    let seen = backend.seen.lock().unwrap();
    assert!(seen[0].contains(r#"ChangeScheduleState { id: "schedule-1", resume: false"#));
    assert!(seen[1].contains(r#"ChangeScheduleState { id: "schedule-1", resume: true"#));
    assert!(seen[2].contains(r#"EndSchedule { id: "schedule-1""#));
    task.abort();
}

#[tokio::test]
async fn forwards_the_manual_run_with_the_exact_path_key_and_due_at() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, body) = post(
        address,
        "/api/v1/holdings/acme/routine-templates/template-1/run",
        HEADERS,
        BODY,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({"data": {"id": "task-run-key-0001", "state": "ready", "version": 1},
         "context": {"userId": "runner"}})
    );
    assert_eq!(backend.commands.load(Ordering::SeqCst), 1);
    let seen = backend.seen.lock().unwrap().join("\n");
    assert!(seen.contains("acme.example.test:acme:run-key-0001"));
    assert!(
        seen.contains(
            "RunTemplate { template_id: \"template-1\", due_at: \"2026-12-01T02:00:00Z\" }"
        )
    );
    task.abort();
}

#[tokio::test]
async fn keeps_the_retained_origin_key_and_body_bounds() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, body) = post(
        address,
        "/api/v1/holdings/acme/routine-templates/template-1/run",
        &[
            ("Origin", "https://other.example.test"),
            ("Idempotency-Key", "run-key-0001"),
            ("Content-Type", "application/json"),
        ],
        BODY,
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");
    let (status, body) = post(
        address,
        "/api/v1/holdings/acme/routine-templates/template-1/run",
        &[
            ("Origin", "https://acme.example.test"),
            ("Content-Type", "application/json"),
        ],
        BODY,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");
    let (status, body) = post(
        address,
        "/api/v1/holdings/acme/routine-templates/template-1/run",
        HEADERS,
        r#"{"dueAt":"2026-12-01T02:00:00Z","extra":true}"#,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "ROUTINE_INPUT_INVALID");
    assert_eq!(backend.commands.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn public_transport_keeps_only_the_retired_data_envelope() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let headers: Vec<(&str, &str)> = HEADERS
        .iter()
        .copied()
        .chain([("x-erpx-api-transport", "public")])
        .collect();
    let (status, body) = post(
        address,
        "/api/v1/holdings/acme/routine-templates/template-1/run",
        &headers,
        BODY,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({"data": {"id": "task-run-key-0001", "state": "ready", "version": 1}})
    );
    assert_eq!(backend.commands.load(Ordering::SeqCst), 1);
    let seen = backend.seen.lock().unwrap().join("\n");
    assert!(seen.contains("acme.example.test:acme:run-key-0001"));
    assert!(
        seen.contains(
            "RunTemplate { template_id: \"template-1\", due_at: \"2026-12-01T02:00:00Z\" }"
        )
    );
    task.abort();
}

#[tokio::test]
async fn public_transport_ignores_the_query_private_rejects() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let headers: Vec<(&str, &str)> = HEADERS
        .iter()
        .copied()
        .chain([("x-erpx-api-transport", "public")])
        .collect();
    let (status, body) = post(
        address,
        "/api/v1/holdings/acme/routine-templates/template-1/run?legacy=ignored",
        &headers,
        BODY,
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.get("data").is_some());
    assert!(body.get("context").is_none());
    let (status, body) = post(
        address,
        "/api/v1/holdings/acme/routine-templates/template-1/run?legacy=ignored",
        HEADERS,
        BODY,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "ROUTINE_REQUEST_INVALID");
    assert_eq!(backend.commands.load(Ordering::SeqCst), 1);
    task.abort();
}

#[tokio::test]
async fn public_transport_marker_must_be_single_and_exact() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let duplicate: Vec<(&str, &str)> = HEADERS
        .iter()
        .copied()
        .chain([
            ("x-erpx-api-transport", "public"),
            ("x-erpx-api-transport", "public"),
        ])
        .collect();
    let (status, body) = post(
        address,
        "/api/v1/holdings/acme/routine-templates/template-1/run",
        &duplicate,
        BODY,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "ROUTINE_REQUEST_INVALID");
    let other: Vec<(&str, &str)> = HEADERS
        .iter()
        .copied()
        .chain([("x-erpx-api-transport", "private")])
        .collect();
    let (status, body) = post(
        address,
        "/api/v1/holdings/acme/routine-templates/template-1/run",
        &other,
        BODY,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["context"]["userId"], "runner");
    assert_eq!(backend.commands.load(Ordering::SeqCst), 1);
    task.abort();
}

#[tokio::test]
async fn refuses_a_non_object_or_oversized_routine_run_body() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, body) = post(
        address,
        "/api/v1/holdings/acme/routine-templates/template-1/run",
        HEADERS,
        "[]",
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "ROUTINE_REQUEST_INVALID");
    let oversized = format!(
        r#"{{"dueAt":"2026-12-01T02:00:00Z","padding":"{}"}}"#,
        "x".repeat(70 * 1024)
    );
    let (status, body) = post(
        address,
        "/api/v1/holdings/acme/routine-templates/template-1/run",
        HEADERS,
        &oversized,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "BODY_TOO_LARGE");
    assert_eq!(backend.commands.load(Ordering::SeqCst), 0);
    task.abort();
}
