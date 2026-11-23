//! Native PMS room-board HTTP transport facts over a real loopback listener.
use erpx_server_pms::public::pms_http::{
    PmsHttpBackend, PmsHttpFrontDeskRequest, PmsHttpFuture, PmsHttpReadRequest,
    PmsHttpRefundContextRequest, PmsHttpRefundRequest, PmsHttpResponse, PmsHttpTarget,
    parse_refund_command, pms_http_router,
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
    fn refund_context<'a>(
        &'a self,
        request: PmsHttpRefundContextRequest<'a>,
    ) -> PmsHttpFuture<'a, PmsHttpResponse> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(format!(
            "refund:{}:{}:{}",
            request.host, request.holding_slug, request.selection.request_id
        ));
        Box::pin(async move {
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(PmsHttpResponse {
                data: json!({"id": "req-1", "property_id": "prop-1"}),
                context: json!({"userId": "user"}),
            })
        })
    }
    fn refund<'a>(&'a self, request: PmsHttpRefundRequest<'a>) -> PmsHttpFuture<'a, PmsHttpResponse> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(format!(
            "refund-cmd:{}:{}:{}:{}",
            request.host, request.holding_slug, request.action, request.command
        ));
        Box::pin(async move {
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(PmsHttpResponse {
                data: json!({"ok": true, "action": request.action}),
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
async fn call_post(address: std::net::SocketAddr, path: &str, body: &str) -> (u16, Value, String) {
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: acme.example.test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
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

#[tokio::test]
async fn refund_context_returns_request_context_and_private_headers() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, value, head) = call(
        address,
        "/api/v1/holdings/acme/pms/booking-refunds/context?request=req-1",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(value["data"]["id"], "req-1");
    assert_eq!(value["data"]["property_id"], "prop-1");
    assert_eq!(value["context"]["userId"], "user");
    assert!(head.contains("cache-control: private, no-store"));
    assert!(head.contains("referrer-policy: no-referrer"));
    assert!(head.contains("x-content-type-options: nosniff"));
    assert_eq!(
        backend.seen.lock().unwrap().as_slice(),
        ["refund:acme.example.test:acme:req-1"]
    );
    task.abort();
}

#[tokio::test]
async fn refund_context_rejects_unknown_targets_and_malformed_queries_before_the_backend() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    for (path, expected) in [
        ("/api/v1/holdings/other/pms/booking-refunds/context?request=req-1", 404),
        ("/api/v1/holdings/acme/pms/booking-refunds/context", 422),
        (
            "/api/v1/holdings/acme/pms/booking-refunds/context?request=one&request=two",
            422,
        ),
        (
            "/api/v1/holdings/acme/pms/booking-refunds/context?request=",
            422,
        ),
        (
            "/api/v1/holdings/acme/pms/booking-refunds/context?request=%ZZ",
            422,
        ),
        (
            "/api/v1/holdings/acme/pms/booking-refunds/context?unknown=value",
            422,
        ),
        (
            "/api/v1/holdings/acme/pms/booking-refunds/context?request=req-1&company=co-1",
            422,
        ),
    ] {
        let (status, value, _) = call(address, path).await;
        assert_eq!(status, expected, "{path} returned {value}");
    }
    assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn refund_context_route_fails_closed_when_a_backend_does_not_implement_it() {
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
    let (status, value, _) = call(
        address,
        "/api/v1/holdings/acme/pms/booking-refunds/context?request=req-1",
    )
    .await;
    assert_eq!(status, 503);
    assert_eq!(value["error"]["code"], "PMS_REFUND_CONTEXT_UNAVAILABLE");
    task.abort();
}

/// The native refund command ingress must accept every one of the six bounded
/// actions and pass the validated action plus the untouched command to the
/// composition-owned backend, with the standard private transport headers.
#[tokio::test]
async fn refund_command_routes_all_six_actions_to_the_backend() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let bodies: Vec<(&str, &str)> = vec![
        ("request", r#"{"action":"request","requestId":"req-1","propertyId":"prop-1","paymentId":"pay-1","wanted":[{"stayId":"stay-1","amount":1000}],"reason":"Guest asked","requestedBy":"user-1","now":1}"#),
        ("approve", r#"{"action":"approve","id":"req-1"}"#),
        ("settle", r#"{"action":"settle","id":"req-1","now":1}"#),
        ("retry", r#"{"action":"retry","id":"req-1","reason":"provider timeout"}"#),
        ("recover-receipt", r#"{"action":"recover-receipt","id":"req-1","reason":"missing receipt"}"#),
        ("finance", r#"{"action":"finance"}"#),
    ];
    let mut expected = Vec::new();
    for (action, body) in &bodies {
        let (status, value, head) = call_post(
            address,
            "/api/v1/holdings/acme/pms/booking-refunds",
            body,
        )
        .await;
        assert_eq!(status, 200, "{action} returned {value}");
        assert_eq!(value["data"]["ok"], true);
        assert_eq!(value["data"]["action"], *action);
        assert_eq!(value["context"]["userId"], "user");
        assert!(head.contains("cache-control: private, no-store"));
        assert!(head.contains("referrer-policy: no-referrer"));
        assert!(head.contains("x-content-type-options: nosniff"));
        let command = serde_json::from_str::<Value>(body).unwrap().to_string();
        expected.push(format!(
            "refund-cmd:acme.example.test:acme:{action}:{command}"
        ));
    }
    assert_eq!(backend.seen.lock().unwrap().as_slice(), expected);
    assert_eq!(backend.reads.load(Ordering::SeqCst), 6);
    task.abort();
}

#[tokio::test]
async fn refund_command_rejects_unknown_targets_and_unbounded_bodies_before_the_backend() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let path = "/api/v1/holdings/acme/pms/booking-refunds";
    let oversized = format!(r#"{{"action":"approve","id":"{}"}}"#, "x".repeat(257 * 1024));
    for (target, body, expected) in [
        ("other", r#"{"action":"approve","id":"req-1"}"#, 404),
        ("acme", "", 422),
        ("acme", "not json", 422),
        ("acme", r#"{"id":"req-1"}"#, 422),
        ("acme", r#"{"action":123}"#, 422),
        ("acme", r#"{"action":"destroy"}"#, 422),
        ("acme", oversized.as_str(), 422),
    ] {
        let path = path.replace("/holdings/acme/", &format!("/holdings/{target}/"));
        let (status, value, _) = call_post(address, &path, body).await;
        assert_eq!(status, expected, "{path} [{body}] returned {value}");
        if expected == 422 {
            assert_eq!(value["error"]["code"], "PMS_REFUND_COMMAND_INVALID");
        }
    }
    assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn refund_command_route_fails_closed_when_a_backend_does_not_implement_it() {
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
    let (status, value, _) = call_post(
        address,
        "/api/v1/holdings/acme/pms/booking-refunds",
        r#"{"action":"approve","id":"req-1"}"#,
    )
    .await;
    assert_eq!(status, 503);
    assert_eq!(value["error"]["code"], "PMS_REFUND_UNAVAILABLE");
    task.abort();
}

#[test]
fn bounded_refund_command_parser_enforces_allow_list_and_body_limit() {
    let (action, value) = parse_refund_command(br#"{"action":"finance"}"#).unwrap();
    assert_eq!(action, "finance");
    assert_eq!(value["action"], "finance");
    for bad in [
        &b""[..],
        &br#"not json"#[..],
        &br#"{"id":"req-1"}"#[..],
        &br#"{"action":123}"#[..],
        &br#"{"action":"destroy"}"#[..],
    ] {
        assert_eq!(
            parse_refund_command(bad).unwrap_err().code,
            "PMS_REFUND_COMMAND_INVALID"
        );
    }
    let oversized = format!(r#"{{"action":"approve","reason":"{}"}}"#, "x".repeat(257 * 1024));
    assert_eq!(
        parse_refund_command(oversized.as_bytes())
            .unwrap_err()
            .code,
        "PMS_REFUND_COMMAND_INVALID"
    );
}

#[tokio::test]
async fn command_plan_resolves_owner_read_without_accepting_sql_from_the_wire() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, value, _) = call_post(
        address,
        "/api/v1/holdings/acme/pms/command-plan",
        r#"{"kind":"pms_rd_inventory_command","values":["property-1","property-1"],"sql":"DELETE FROM pms_records"}"#,
    )
    .await;
    assert_eq!(status, 200);
    let statement = &value["data"]["statements"][0];
    let sql = statement["sql"].as_str().unwrap();
    assert!(sql.starts_with("SELECT id,kind,data_json,version,updated_at FROM pms_records"));
    assert!(!sql.to_ascii_uppercase().contains("DELETE"));
    assert_eq!(statement["values"], json!(["property-1", "property-1"]));
    // The read plan never reaches the command backend.
    assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn command_plan_resolves_posting_journal_reads() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, value, _) = call_post(
        address,
        "/api/v1/holdings/acme/pms/command-plan",
        r#"{"kind":"pms_rd_posting_charge","values":["property-1","property-1","guest-1"],"sql":"DELETE FROM pms_postings"}"#,
    )
    .await;
    assert_eq!(status, 200);
    let statement = &value["data"]["statements"][0];
    let sql = statement["sql"].as_str().unwrap();
    assert!(sql.starts_with("SELECT journal_id FROM pms_postings"));
    assert!(sql.contains("guestAccountId"));
    assert!(!sql.to_ascii_uppercase().contains("DELETE"));
    assert_eq!(statement["values"], json!(["property-1", "property-1", "guest-1"]));

    let (status, value, _) = call_post(
        address,
        "/api/v1/holdings/acme/pms/command-plan",
        r#"{"kind":"pms_rd_posting_by_record","values":["property-1","entry-1"]}"#,
    )
    .await;
    assert_eq!(status, 200);
    let statement = &value["data"]["statements"][0];
    assert_eq!(
        statement["sql"].as_str().unwrap(),
        "SELECT journal_id FROM pms_postings WHERE property_id=? AND record_id=? ORDER BY source_key LIMIT 1"
    );
    assert_eq!(statement["values"], json!(["property-1", "entry-1"]));

    let (status, _, _) = call_post(
        address,
        "/api/v1/holdings/acme/pms/command-plan",
        r#"{"kind":"pms_rd_posting_charge","values":["property-1"]}"#,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn command_plan_rejects_unregistered_kinds_and_wrong_bind_counts() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    for body in [
        r#"{"kind":"not_a_native_read","values":["property-1","property-1"]}"#,
        r#"{"kind":"pms_rd_inventory_command","values":["property-1"]}"#,
        r#"{"kind":"pms_w001","values":["property-1","property-1"]}"#,
    ] {
        let (status, _, _) = call_post(address, "/api/v1/holdings/acme/pms/command-plan", body).await;
        assert_eq!(status, 422, "{body}");
    }
    task.abort();
}
