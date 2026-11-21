//! Native Agent evidence HTTP transport facts over a real loopback listener.
use erpx_server_platform_administration::public::agent_evidence_http::{
    AgentEvidenceHttpBackend, AgentEvidenceHttpFuture, AgentEvidenceHttpQuery,
    AgentEvidenceHttpReadRequest, AgentEvidenceHttpResponse, AgentEvidenceHttpTarget,
    agent_evidence_http_router,
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
    seen: std::sync::Mutex<Vec<String>>,
    fail: Option<BusinessError>,
}
impl Backend {
    fn new() -> Self {
        Self {
            reads: AtomicUsize::new(0),
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
impl AgentEvidenceHttpBackend for Backend {
    fn read<'a>(
        &'a self,
        request: AgentEvidenceHttpReadRequest<'a>,
    ) -> AgentEvidenceHttpFuture<'a, AgentEvidenceHttpResponse> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let query = match request.query {
            AgentEvidenceHttpQuery::Workspace => "workspace".to_owned(),
            AgentEvidenceHttpQuery::Record(query) => {
                format!("{}:{}", query.kind.as_str(), query.id)
            }
        };
        self.seen
            .lock()
            .unwrap()
            .push(format!("{}:{}:{query}", request.host, request.holding_slug));
        Box::pin(async move {
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(AgentEvidenceHttpResponse {
                data: json!({"now": 1, "invocations": [], "sessions": []}),
                context: json!({"userId": "user"}),
            })
        })
    }
}

fn router(backend: Arc<Backend>) -> Router {
    agent_evidence_http_router(vec![AgentEvidenceHttpTarget {
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
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (u16, Value, String) {
    call_with_host(address, "acme.example.test", path, headers, body).await
}

async fn call_with_host(
    address: std::net::SocketAddr,
    host: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (u16, Value, String) {
    let mut request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    if !body.is_empty()
        || headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("content-length"))
    {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("Connection: close\r\n\r\n");
    request.push_str(body);
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let response = String::from_utf8_lossy(&bytes);
    let (head, response_body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap();
    (
        status,
        serde_json::from_str(response_body).unwrap_or(Value::Null),
        head.to_owned(),
    )
}

async fn get(address: std::net::SocketAddr, path: &str) -> (u16, Value, String) {
    call(address, path, &[], "").await
}

#[tokio::test]
async fn actual_router_serves_workspace_and_record_with_private_headers() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, value, head) = get(address, "/api/v1/holdings/acme/agents/evidence").await;
    assert_eq!(status, 200);
    assert_eq!(value["context"]["userId"], "user");
    assert_eq!(value["data"]["now"], 1);
    assert!(head.contains("cache-control: private, no-store"));
    assert!(head.contains("referrer-policy: no-referrer"));
    assert!(head.contains("x-content-type-options: nosniff"));

    let (status, value, _) = get(
        address,
        "/api/v1/holdings/acme/agents/evidence/session/session-1",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(value["data"]["invocations"], json!([]));
    assert_eq!(
        backend.seen.lock().unwrap().as_slice(),
        [
            "acme.example.test:acme:workspace",
            "acme.example.test:acme:session:session-1"
        ]
    );
    assert_eq!(backend.reads.load(Ordering::SeqCst), 2);
    task.abort();
}

#[tokio::test]
async fn matching_origin_is_allowed_and_a_foreign_origin_is_forbidden() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, _, _) = call(
        address,
        "/api/v1/holdings/acme/agents/evidence",
        &[("Origin", "https://acme.example.test")],
        "",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(backend.reads.load(Ordering::SeqCst), 1);

    let (status, value, _) = call(
        address,
        "/api/v1/holdings/acme/agents/evidence",
        &[("Origin", "https://evil.example.test")],
        "",
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(value["error"]["code"], "ORIGIN_FORBIDDEN");
    assert_eq!(backend.reads.load(Ordering::SeqCst), 1);
    task.abort();
}

#[tokio::test]
async fn unknown_targets_and_malformed_selections_fail_before_the_backend() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, _, _) = call(address, "/api/v1/holdings/other/agents/evidence", &[], "").await;
    assert_eq!(status, 404);
    let (status, _, _) = call_with_host(
        address,
        "other.example.test",
        "/api/v1/holdings/acme/agents/evidence",
        &[],
        "",
    )
    .await;
    assert_eq!(status, 404);

    for path in [
        "/api/v1/holdings/acme/agents/evidence?limit=1",
        "/api/v1/holdings/acme/agents/evidence/unknown/id-1",
        "/api/v1/holdings/acme/agents/evidence/session/",
    ] {
        let (status, _, _) = get(address, path).await;
        assert!(status == 404 || status == 422, "{path} returned {status}");
    }
    let long_id = "x".repeat(201);
    let (status, value, _) = get(
        address,
        &format!("/api/v1/holdings/acme/agents/evidence/session/{long_id}"),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "AGENT_EVIDENCE_REQUEST_INVALID");
    assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn request_bodies_are_rejected_on_the_read_route() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, value, _) = call(
        address,
        "/api/v1/holdings/acme/agents/evidence",
        &[],
        "{\"id\":\"injected\"}",
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "AGENT_EVIDENCE_REQUEST_INVALID");
    assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn backend_business_errors_map_to_bounded_statuses() {
    for (kind, status, code) in [
        (FailureKind::Forbidden, 403, "PERMISSION_REQUIRED"),
        (FailureKind::NotFound, 404, "AGENT_RECORD_NOT_FOUND"),
        (FailureKind::Conflict, 409, "WORK_CONTEXT_CHANGED"),
        (
            FailureKind::Unavailable,
            503,
            "AGENT_EVIDENCE_SOURCE_INVALID",
        ),
    ] {
        let backend = Arc::new(Backend::failing(BusinessError::new(kind, code, "Bounded.")));
        let (address, task) = start(backend).await;
        let (actual, value, _) = get(address, "/api/v1/holdings/acme/agents/evidence").await;
        assert_eq!(actual, status);
        assert_eq!(value["error"]["code"], code);
        task.abort();
    }
}

#[test]
fn configuration_requires_exact_distinct_https_targets() {
    let backend = Arc::new(Backend::new());
    let target = |host: &str, slug: &str, origin: &str| AgentEvidenceHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: origin.into(),
        backend: backend.clone(),
    };
    assert_eq!(
        agent_evidence_http_router(vec![
            target("acme.example.test", "acme", "https://acme.example.test"),
            target("acme.example.test", "acme", "https://acme.example.test"),
        ])
        .unwrap_err()
        .code,
        "AGENT_EVIDENCE_CONFIGURATION_INVALID"
    );
    for origin in [
        "http://acme.example.test",
        "https://acme.example.test/path",
        "https://acme.example.test:8443",
        "",
    ] {
        assert_eq!(
            agent_evidence_http_router(vec![target("acme.example.test", "acme", origin)])
                .unwrap_err()
                .code,
            "AGENT_EVIDENCE_CONFIGURATION_INVALID"
        );
    }
    assert!(
        agent_evidence_http_router(vec![target("", "acme", "https://acme.example.test")]).is_err()
    );
    assert!(
        agent_evidence_http_router(vec![target(
            "acme.example.test",
            "",
            "https://acme.example.test"
        )])
        .is_err()
    );
}
