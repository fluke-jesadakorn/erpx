//! Native Operations HTTP transport facts over a real loopback listener.
use erpx_server_operations::public::operations_http::{
    OperationsHttpBackend, OperationsHttpCommandRequest, OperationsHttpFuture,
    OperationsHttpReadKind, OperationsHttpReadRequest, OperationsHttpResponse,
    OperationsHttpTarget, operations_router,
};
use erpx_server_runtime::public::http_support::{
    Router, serde_json::Value, serde_json::json, serve,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

struct Backend {
    reads: AtomicUsize,
    commands: AtomicUsize,
    seen: std::sync::Mutex<Vec<String>>,
    seen_commands: std::sync::Mutex<Vec<String>>,
    fail: Option<BusinessError>,
}
impl Backend {
    fn new() -> Self {
        Self {
            reads: AtomicUsize::new(0),
            commands: AtomicUsize::new(0),
            seen: std::sync::Mutex::new(Vec::new()),
            seen_commands: std::sync::Mutex::new(Vec::new()),
            fail: None,
        }
    }
    fn failing(error: BusinessError) -> Self {
        Self {
            fail: Some(error),
            ..Self::new()
        }
    }
}
impl OperationsHttpBackend for Backend {
    fn read<'a>(
        &'a self,
        request: OperationsHttpReadRequest<'a>,
    ) -> OperationsHttpFuture<'a, OperationsHttpResponse> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(format!(
            "{}:{}:{}",
            request.host,
            request.holding_slug,
            match &request.kind {
                OperationsHttpReadKind::Workspace => "workspace".to_owned(),
                OperationsHttpReadKind::Detail(id) => format!("detail:{id}"),
            }
        ));
        Box::pin(async move {
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            let data = match &request.kind {
                OperationsHttpReadKind::Workspace => json!({
                    "exceptions": [],
                    "policies": [],
                    "people": [],
                    "actorId": "user",
                    "canWrite": false,
                    "canReview": false,
                    "summary": {"active": 0, "awaitingReview": 0, "escalated": 0, "overdue": 0, "byArea": {}}
                }),
                OperationsHttpReadKind::Detail(id) => json!({
                    "exceptions": [],
                    "policies": [],
                    "people": [],
                    "actorId": "user",
                    "canWrite": false,
                    "canReview": false,
                    "summary": {"active": 0, "awaitingReview": 0, "escalated": 0, "overdue": 0, "byArea": {}},
                    "row": {"id": id, "company_id": "company", "source": {"id": "source"}},
                    "history": []
                }),
            };
            Ok(OperationsHttpResponse {
                data,
                context: json!({"userId": "user"}),
            })
        })
    }
    fn command<'a>(
        &'a self,
        request: OperationsHttpCommandRequest<'a>,
    ) -> OperationsHttpFuture<'a, OperationsHttpResponse> {
        self.commands.fetch_add(1, Ordering::SeqCst);
        self.seen_commands.lock().unwrap().push(format!(
            "{}:{}:{}:{:?}",
            request.host,
            request.holding_slug,
            request.key.as_str(),
            request.command
        ));
        Box::pin(async move {
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(OperationsHttpResponse {
                data: json!({"id": "operations-exception-command-key-01", "status": "open", "version": 1}),
                context: json!({"userId": "user"}),
            })
        })
    }
}

fn router(backend: Arc<Backend>) -> Router {
    operations_router(vec![OperationsHttpTarget {
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
async fn call(address: std::net::SocketAddr, path: &str) -> (u16, Value, String) {
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n");
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
    (
        status,
        serde_json::from_str(body).unwrap_or(Value::Null),
        head.to_owned(),
    )
}

async fn post(
    address: std::net::SocketAddr,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (u16, Value, String) {
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
    (
        status,
        serde_json::from_str(body).unwrap_or(Value::Null),
        head.to_owned(),
    )
}

const OPEN_COMMAND: &str = r#"{"entity":"exception","source":{"kind":"staff_task","id":"task-open","version":1},"policyId":"policy-service","policyVersion":2,"title":"Open task","issue":"Synthetic issue","priority":"normal","ownerId":"owner"}"#;

#[tokio::test]
async fn command_route_requires_origin_key_json_and_forwards_the_typed_command() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let headers = [
        ("origin", "https://acme.example.test"),
        ("idempotency-key", "command-key-01"),
        ("content-type", "application/json"),
    ];
    let (status, value, head) = post(
        address,
        "/api/v1/holdings/acme/operations",
        &headers,
        OPEN_COMMAND,
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value["data"]["version"], 1);
    assert!(head.contains("cache-control: private, no-store"));
    assert_eq!(backend.commands.load(Ordering::SeqCst), 1);
    let seen = backend.seen_commands.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].starts_with("acme.example.test:acme:command-key-01:OpenException("));

    task.abort();
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    for (path, headers, expected) in [
        (
            "/api/v1/holdings/acme/operations",
            vec![
                ("idempotency-key", "command-key-01"),
                ("content-type", "application/json"),
            ],
            403,
        ),
        (
            "/api/v1/holdings/acme/operations",
            vec![
                ("origin", "https://other.example.test"),
                ("idempotency-key", "command-key-01"),
                ("content-type", "application/json"),
            ],
            403,
        ),
        (
            "/api/v1/holdings/acme/operations",
            vec![
                ("origin", "https://acme.example.test"),
                ("content-type", "application/json"),
            ],
            422,
        ),
        (
            "/api/v1/holdings/acme/operations",
            vec![
                ("origin", "https://acme.example.test"),
                ("idempotency-key", "command-key-01"),
                ("content-type", "text/plain"),
            ],
            422,
        ),
        (
            "/api/v1/holdings/acme/operations?id=exception-open",
            vec![
                ("origin", "https://acme.example.test"),
                ("idempotency-key", "command-key-01"),
                ("content-type", "application/json"),
            ],
            422,
        ),
        (
            "/api/v1/holdings/other/operations",
            vec![
                ("origin", "https://acme.example.test"),
                ("idempotency-key", "command-key-01"),
                ("content-type", "application/json"),
            ],
            404,
        ),
    ] {
        let (status, value, _) = post(address, path, &headers, OPEN_COMMAND).await;
        assert_eq!(status, expected, "{path} returned {value}");
    }
    assert_eq!(backend.commands.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn command_route_rejects_malformed_unknown_and_oversized_bodies_before_the_backend() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let headers = [
        ("origin", "https://acme.example.test"),
        ("idempotency-key", "command-key-01"),
        ("content-type", "application/json"),
    ];
    for body in [
        "not-json",
        "[]",
        r#"{"entity":"unknown"}"#,
        r#"{"entity":"exception","id":"exception","action":"acknowledge","expectedVersion":1,"reason":"Reviewed evidence","extra":true}"#,
        r#"{"entity":"policy","area":"service","name":"Service queue","targetMinutes":60,"sourceKinds":["staff_task"],"extra":true}"#,
    ] {
        let (status, value, _) =
            post(address, "/api/v1/holdings/acme/operations", &headers, body).await;
        assert_eq!(status, 422, "{body} returned {value}");
    }
    let oversized = format!(
        r#"{{"entity":"policy","padding":"{}"}}"#,
        "x".repeat(70_000)
    );
    let (status, _, _) = post(
        address,
        "/api/v1/holdings/acme/operations",
        &headers,
        &oversized,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(backend.commands.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn command_backend_conflicts_map_without_fallback() {
    let backend = Arc::new(Backend::failing(BusinessError::new(
        FailureKind::Conflict,
        "OPERATIONS_STATE_CHANGED",
        "Review the current exception state.",
    )));
    let (address, task) = start(backend).await;
    let (status, value, _) = post(
        address,
        "/api/v1/holdings/acme/operations",
        &[
            ("origin", "https://acme.example.test"),
            ("idempotency-key", "command-key-01"),
            ("content-type", "application/json"),
        ],
        OPEN_COMMAND,
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(value["error"]["code"], "OPERATIONS_STATE_CHANGED");
    task.abort();
}

#[tokio::test]
async fn actual_router_retains_registered_target_query_and_private_response_headers() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, value, head) = call(address, "/api/v1/holdings/acme/operations").await;
    assert_eq!(status, 200);
    assert_eq!(value["data"]["summary"]["active"], 0);
    assert!(head.contains("cache-control: private, no-store"));
    assert!(head.contains("referrer-policy: no-referrer"));
    assert!(head.contains("x-content-type-options: nosniff"));

    let (status, value, _) = call(
        address,
        "/api/v1/holdings/acme/operations?id=exception-open",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(value["data"]["row"]["id"], "exception-open");
    assert_eq!(
        backend.seen.lock().unwrap().as_slice(),
        [
            "acme.example.test:acme:workspace",
            "acme.example.test:acme:detail:exception-open"
        ]
    );
    task.abort();
}

#[tokio::test]
async fn unknown_targets_duplicate_ids_malformed_queries_and_backend_errors_are_bounded() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    for (path, expected) in [
        ("/api/v1/holdings/other/operations", 404),
        ("/api/v1/holdings/acme/operations?id=one&id=two", 422),
        ("/api/v1/holdings/acme/operations?id=%ZZ", 422),
        ("/api/v1/holdings/acme/operations?id=", 422),
    ] {
        let (status, value, _) = call(address, path).await;
        assert_eq!(status, expected, "{path} returned {value}");
    }
    assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    task.abort();

    let backend = Arc::new(Backend::failing(BusinessError::new(
        FailureKind::Conflict,
        "OPERATIONS_STATE_CHANGED",
        "Review the current exception state.",
    )));
    let (address, task) = start(backend).await;
    let (status, value, _) = call(address, "/api/v1/holdings/acme/operations").await;
    assert_eq!(status, 409);
    assert_eq!(value["error"]["code"], "OPERATIONS_STATE_CHANGED");
    task.abort();
}

#[test]
fn configuration_rejects_non_https_and_duplicate_targets() {
    let backend = Arc::new(Backend::new());
    let target = || OperationsHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(operations_router(vec![target()]).is_ok());
    assert!(operations_router(vec![target(), target()]).is_err());
    assert!(
        operations_router(vec![OperationsHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        operations_router(vec![OperationsHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
}
