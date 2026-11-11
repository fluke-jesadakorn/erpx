//! Native Staff Work HTTP transport facts over a real loopback listener.
use erpx_server_operations::public::staff_work_http::{
    StaffWorkHttpBackend, StaffWorkHttpCommandRequest, StaffWorkHttpFuture, StaffWorkHttpReadKind,
    StaffWorkHttpReadRequest, StaffWorkHttpResponse, StaffWorkHttpTarget, staff_work_router,
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
    fail: Option<BusinessError>,
}
impl Backend {
    fn new() -> Self {
        Self {
            reads: AtomicUsize::new(0),
            commands: AtomicUsize::new(0),
            seen: std::sync::Mutex::new(Vec::new()),
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
impl StaffWorkHttpBackend for Backend {
    fn read<'a>(
        &'a self,
        request: StaffWorkHttpReadRequest<'a>,
    ) -> StaffWorkHttpFuture<'a, StaffWorkHttpResponse> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(format!(
            "{}:{}:{}",
            request.host,
            request.holding_slug,
            match &request.kind {
                StaffWorkHttpReadKind::Workspace { project_id } => {
                    format!("workspace:{project_id:?}")
                }
                StaffWorkHttpReadKind::Detail(id) => format!("detail:{id}"),
            }
        ));
        Box::pin(async move {
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(StaffWorkHttpResponse {
                data: json!({"tasks": [], "id": "task-1", "state": "ready", "version": 1}),
                context: json!({"userId": "user"}),
            })
        })
    }
    fn command<'a>(
        &'a self,
        request: StaffWorkHttpCommandRequest<'a>,
    ) -> StaffWorkHttpFuture<'a, StaffWorkHttpResponse> {
        self.commands.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(format!(
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
            Ok(StaffWorkHttpResponse {
                data: json!({"id": "task-1", "state": "in_progress", "version": 4}),
                context: json!({"userId": "user"}),
            })
        })
    }
}

fn router(backend: Arc<Backend>) -> Router {
    staff_work_router(vec![StaffWorkHttpTarget {
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
async fn call(
    address: std::net::SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (u16, Value, String) {
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
    (
        status,
        serde_json::from_str(body).unwrap_or(Value::Null),
        head.to_owned(),
    )
}

const SAVE: &str = r#"{"title":"Inspect deliveries","description":"","assigneeUserId":"worker","expectedVersion":1}"#;
const TRANSITION: &str = r#"{"action":"start","expectedVersion":3,"note":""}"#;
const DELEGATE: &str = r#"{"delegateUserId":"delegate","expiresAt":"2026-12-01T00:00:00Z","expectedVersion":3,"reason":"Annual leave"}"#;
const REVOKE: &str = r#"{"expectedVersion":3,"reason":"Returned"}"#;

const JSON: [(&str, &str); 3] = [
    ("origin", "https://acme.example.test"),
    ("idempotency-key", "command-key-01"),
    ("content-type", "application/json"),
];

#[tokio::test]
async fn reads_require_the_registered_host_and_forward_the_project_filter() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, value, head) = call(
        address,
        "GET",
        "/api/v1/holdings/acme/staff-tasks?projectId=project-1",
        &[],
        "",
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert!(head.contains("cache-control: private, no-store"));
    assert_eq!(backend.reads.load(Ordering::SeqCst), 1);
    assert_eq!(
        backend.seen.lock().unwrap().clone(),
        vec!["acme.example.test:acme:workspace:Some(\"project-1\")".to_owned()]
    );

    let (status, _, _) = call(
        address,
        "GET",
        "/api/v1/holdings/acme/staff-tasks?projectId=a&projectId=b",
        &[],
        "",
    )
    .await;
    assert_eq!(status, 422, "duplicate read facts are refused");

    task.abort();
}

#[tokio::test]
async fn list_rejects_unknown_host_before_calling_the_backend() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let listener_request = "GET /api/v1/holdings/acme/staff-tasks HTTP/1.1\r\nHost: evil.example.test\r\nConnection: close\r\n\r\n";
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream.write_all(listener_request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let response = String::from_utf8_lossy(&bytes);
    assert!(response.starts_with("HTTP/1.1 404"));
    assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn save_and_transition_require_origin_key_json_and_forward_typed_commands() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, value, head) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/staff-tasks",
        &JSON,
        SAVE,
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert!(head.contains("x-content-type-options: nosniff"));
    let (status, value, _) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/staff-tasks/task-1/transition",
        &JSON,
        TRANSITION,
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(backend.commands.load(Ordering::SeqCst), 2);
    let seen = backend.seen.lock().unwrap().clone();
    assert!(seen[0].contains("Save {"));
    assert!(seen[1].contains("Transition { id: \"task-1\""));
    task.abort();
}

#[tokio::test]
async fn delegate_revoke_path_supplies_the_delegation_identifier() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, value, _) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/staff-tasks/task-1/delegate",
        &JSON,
        DELEGATE,
    )
    .await;
    assert_eq!(status, 200, "{value}");
    let (status, value, _) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/staff-tasks/task-1/delegate/revoke/delegation-1",
        &JSON,
        REVOKE,
    )
    .await;
    assert_eq!(status, 200, "{value}");
    let seen = backend.seen.lock().unwrap().clone();
    assert!(seen[0].contains("Delegate { id: \"task-1\""));
    assert!(seen[1].contains("delegation_id: \"delegation-1\""));
    task.abort();
}

#[tokio::test]
async fn commands_refuse_missing_or_duplicate_transport_facts() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    for (headers, expected) in [
        (vec![("content-type", "application/json")], 403),
        (
            vec![
                ("origin", "https://acme.example.test"),
                ("content-type", "application/json"),
            ],
            422,
        ),
        (
            vec![
                ("origin", "https://acme.example.test"),
                ("idempotency-key", "key"),
                ("content-type", "text/plain"),
            ],
            422,
        ),
        (
            vec![
                ("origin", "https://evil.example.test"),
                ("idempotency-key", "key"),
                ("content-type", "application/json"),
            ],
            403,
        ),
    ] {
        let (status, value, _) = call(
            address,
            "POST",
            "/api/v1/holdings/acme/staff-tasks",
            &headers,
            SAVE,
        )
        .await;
        assert_eq!(status, expected, "{value}");
    }
    assert_eq!(backend.commands.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn oversized_body_is_refused_as_a_bounded_transport_fact() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let body = format!(r#"{{"title":"{}"}}"#, "x".repeat(70 * 1024));
    let (status, value, _) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/staff-tasks",
        &JSON,
        &body,
    )
    .await;
    assert_eq!(status, 422, "{value}");
    assert_eq!(value["error"]["code"], "BODY_TOO_LARGE");
    assert_eq!(backend.commands.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn backend_business_errors_map_to_the_exact_http_status() {
    let backend = Arc::new(Backend::failing(BusinessError::new(
        FailureKind::Conflict,
        "TASK_VERSION_CHANGED",
        "The task changed.",
    )));
    let (address, task) = start(backend).await;
    let (status, value, _) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/staff-tasks/task-1/transition",
        &JSON,
        TRANSITION,
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(value["error"]["code"], "TASK_VERSION_CHANGED");
    task.abort();
}
