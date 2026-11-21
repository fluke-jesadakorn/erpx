//! Native routine run HTTP ingress facts over a real loopback listener.
use erpx_server_operations::public::routine_commands_http::{
    RoutineHttpBackend, RoutineHttpCommandRequest, RoutineHttpFuture,
    RoutineHttpResponse, RoutineHttpTarget, routine_commands_router,
};
use erpx_server_runtime::public::http_support::{Router, serde_json::Value, serde_json::json, serve};
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
    let mut request = format!(
        "POST {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\nContent-Length: {}\r\n",
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
const BODY: &str = r#"{"dueAt":"2026-12-01T02:00:00Z"}"#;
const HEADERS: &[(&str, &str)] = &[
    ("Origin", "https://acme.example.test"),
    ("Idempotency-Key", "run-key-0001"),
    ("Content-Type", "application/json"),
];

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
    assert!(seen.contains("RunTemplate { template_id: \"template-1\", due_at: \"2026-12-01T02:00:00Z\" }"));
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
