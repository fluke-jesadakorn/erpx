//! Native PMS room-board HTTP transport facts over a real loopback listener.
use erpx_server_pms::public::pms_http::{
    PmsHttpBackend, PmsHttpFrontDeskRequest, PmsHttpFuture, PmsHttpReadRequest, PmsHttpResponse,
    PmsHttpTarget, pms_http_router,
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
impl PmsHttpBackend for Backend {
    fn read<'a>(&'a self, request: PmsHttpReadRequest<'a>) -> PmsHttpFuture<'a, PmsHttpResponse> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(format!(
            "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
            request.host,
            request.holding_slug,
            request.selection.section.as_str(),
            request.selection.property_id.as_deref().unwrap_or("-"),
            request.selection.status,
            request.selection.q,
            request.selection.page,
            request.selection.selected_id.as_deref().unwrap_or("-"),
            request.selection.from.as_deref().unwrap_or("-"),
            request.selection.to.as_deref().unwrap_or("-")
        ));
        Box::pin(async move {
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(PmsHttpResponse {
                data: json!({"properties": [], "snapshot": null}),
                context: json!({"userId": "user"}),
            })
        })
    }
    fn front_desk<'a>(
        &'a self,
        request: PmsHttpFrontDeskRequest<'a>,
    ) -> PmsHttpFuture<'a, PmsHttpResponse> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(format!(
            "front:{}:{}:{}:{}:{}:{}:{}:{}:{}",
            request.host,
            request.holding_slug,
            request.selection.status,
            request.selection.property_id.as_deref().unwrap_or("-"),
            request.selection.q,
            request.selection.page,
            request.selection.selected_id.as_deref().unwrap_or("-"),
            request.selection.from.as_deref().unwrap_or("-"),
            request.selection.to.as_deref().unwrap_or("-")
        ));
        Box::pin(async move {
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(PmsHttpResponse {
                data: json!({"properties": [], "snapshot": null}),
                context: json!({"userId": "user"}),
            })
        })
    }
}

/// A room-board-only backend must not accidentally serve the front desk.
struct BoardOnlyBackend;
impl PmsHttpBackend for BoardOnlyBackend {
    fn read<'a>(&'a self, _request: PmsHttpReadRequest<'a>) -> PmsHttpFuture<'a, PmsHttpResponse> {
        Box::pin(async {
            Ok(PmsHttpResponse {
                data: json!({"properties": [], "snapshot": null}),
                context: json!({"userId": "user"}),
            })
        })
    }
}

fn router(backend: Arc<Backend>) -> Router {
    pms_http_router(vec![PmsHttpTarget {
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

#[tokio::test]
async fn actual_router_retains_registered_target_selection_and_private_headers() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, value, head) = call(
        address,
        "/api/v1/holdings/acme/pms/rooms?section=housekeeping&property=property-1&status=dirty&q=Suite&building=Main&floor=1&typeId=type-1&page=2&selectedId=room-1&from=2026-11-01&to=2026-11-10",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(value["context"]["userId"], "user");
    assert!(head.contains("cache-control: private, no-store"));
    assert!(head.contains("referrer-policy: no-referrer"));
    assert!(head.contains("x-content-type-options: nosniff"));
    assert_eq!(
        backend.seen.lock().unwrap().as_slice(),
        [
            "acme.example.test:acme:housekeeping:property-1:dirty:Suite:2:room-1:2026-11-01:2026-11-10"
        ]
    );
    task.abort();
}

#[tokio::test]
async fn unknown_targets_duplicate_or_malformed_queries_are_rejected_before_the_backend() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    for (path, expected) in [
        ("/api/v1/holdings/other/pms/rooms", 404),
        (
            "/api/v1/holdings/acme/pms/rooms?property=one&property=two",
            422,
        ),
        ("/api/v1/holdings/acme/pms/rooms?section=reports", 422),
        ("/api/v1/holdings/acme/pms/rooms?page=not-a-number", 422),
        ("/api/v1/holdings/acme/pms/rooms?page=10001", 422),
        ("/api/v1/holdings/acme/pms/rooms?from=2026-11-01", 422),
        (
            "/api/v1/holdings/acme/pms/rooms?from=2026-11-10&to=2026-11-01",
            422,
        ),
        ("/api/v1/holdings/acme/pms/rooms?property=%ZZ", 422),
        ("/api/v1/holdings/acme/pms/rooms?property=", 422),
        ("/api/v1/holdings/acme/pms/rooms?unknown=value", 422),
    ] {
        let (status, value, _) = call(address, path).await;
        assert_eq!(status, expected, "{path} returned {value}");
    }
    assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn backend_business_errors_map_to_bounded_statuses() {
    let backend = Arc::new(Backend::failing(BusinessError::new(
        FailureKind::Conflict,
        "PMS_RECORD_CHANGED",
        "Review the current hotel state.",
    )));
    let (address, task) = start(backend).await;
    let (status, value, _) = call(address, "/api/v1/holdings/acme/pms/rooms").await;
    assert_eq!(status, 409);
    assert_eq!(value["error"]["code"], "PMS_RECORD_CHANGED");
    task.abort();
}

#[tokio::test]
async fn front_desk_router_retains_registered_target_queue_and_private_headers() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, value, head) = call(
        address,
        "/api/v1/holdings/acme/pms/front-desk?status=arrivals&property=property-1&q=Guest&page=1&selectedId=stay-1&from=2026-11-01&to=2026-11-10",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(value["context"]["userId"], "user");
    assert!(head.contains("cache-control: private, no-store"));
    assert!(head.contains("referrer-policy: no-referrer"));
    assert!(head.contains("x-content-type-options: nosniff"));
    assert_eq!(
        backend.seen.lock().unwrap().as_slice(),
        ["front:acme.example.test:acme:arrivals:property-1:Guest:1:stay-1:2026-11-01:2026-11-10"]
    );
    task.abort();
}

#[tokio::test]
async fn front_desk_rejects_unknown_targets_and_malformed_queries_before_the_backend() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    for (path, expected) in [
        ("/api/v1/holdings/other/pms/front-desk", 404),
        (
            "/api/v1/holdings/acme/pms/front-desk?property=one&property=two",
            422,
        ),
        (
            "/api/v1/holdings/acme/pms/front-desk?status=payment_pending",
            422,
        ),
        (
            "/api/v1/holdings/acme/pms/front-desk?page=not-a-number",
            422,
        ),
        ("/api/v1/holdings/acme/pms/front-desk?page=10001", 422),
        ("/api/v1/holdings/acme/pms/front-desk?from=2026-11-01", 422),
        (
            "/api/v1/holdings/acme/pms/front-desk?from=2026-11-10&to=2026-11-01",
            422,
        ),
        ("/api/v1/holdings/acme/pms/front-desk?property=%ZZ", 422),
        ("/api/v1/holdings/acme/pms/front-desk?property=", 422),
        ("/api/v1/holdings/acme/pms/front-desk?unknown=value", 422),
    ] {
        let (status, value, _) = call(address, path).await;
        assert_eq!(status, expected, "{path} returned {value}");
    }
    assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn front_desk_route_fails_closed_when_a_backend_does_not_implement_it() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        serve(
            listener,
            pms_http_router(vec![PmsHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                origin: "https://acme.example.test".into(),
                backend: Arc::new(BoardOnlyBackend),
            }])
            .unwrap(),
        )
        .await
        .unwrap();
    });
    let (status, value, _) = call(address, "/api/v1/holdings/acme/pms/front-desk").await;
    assert_eq!(status, 503);
    assert_eq!(value["error"]["code"], "PMS_FRONT_DESK_UNAVAILABLE");
    task.abort();
}

#[test]
fn configuration_rejects_non_https_and_duplicate_targets() {
    let backend = Arc::new(Backend::new());
    let target = || PmsHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(pms_http_router(vec![target()]).is_ok());
    assert!(
        pms_http_router(vec![PmsHttpTarget {
            origin: "https://acme.example.test:3443".into(),
            ..target()
        }])
        .is_ok()
    );
    assert!(pms_http_router(vec![target(), target()]).is_err());
    assert!(
        pms_http_router(vec![PmsHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    for origin in [
        "https://acme.example.test:0",
        "https://acme.example.test:65536",
        "https://acme.example.test:abc",
        "https://acme.example.test:3443:1",
    ] {
        assert!(
            pms_http_router(vec![PmsHttpTarget {
                origin: origin.into(),
                ..target()
            }])
            .is_err()
        );
    }
    assert!(
        pms_http_router(vec![PmsHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
}
