//! Native Quality HTTP transport facts over a real loopback listener.
use erpx_server_quality::public::quality_http::{
    QualityHttpBackend, QualityHttpCommandRequest, QualityHttpFuture, QualityHttpReadReply,
    QualityHttpReadRequest, QualityHttpTarget, quality_router,
};
use erpx_server_quality::public::quality_read::{
    CancellationAction, HoldStatus, QualityCancellationReceipt, QualityDecision, QualityDetail,
    QualityHold, QualityHoldView, QualityWorkspace,
};
use erpx_server_runtime::public::http_support::serde_json::{self, Value, json};
use erpx_server_runtime::public::http_support::{Router, serve};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

fn hold() -> QualityHold {
    QualityHold {
        id: "hold-a".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        warehouse_id: "warehouse".into(),
        product_id: "product".into(),
        lot_id: None,
        reason: "Damaged packaging requires independent review".into(),
        due_date: "2026-11-10".into(),
        status: HoldStatus::AwaitingRelease,
        created_by: "maker".into(),
        last_inspector: Some("inspector".into()),
        last_result: Some(erpx_server_quality::public::quality_read::InspectionResult::Pass),
        released_by: None,
        release_reason: None,
        released_at: None,
        version: 2,
        created_at: 0,
        updated_at: 0,
        spec_id: None,
        spec_version: None,
        quantity_scope: "whole".into(),
        held_quantity_milli: None,
        source_type: None,
        source_id: None,
        cancel_requested_by: None,
        cancel_reason: None,
        cancelled_by: None,
        cancelled_at: None,
        disposition_kind: None,
        disposition_reason: None,
        disposition_requested_by: None,
        disposition_count_id: None,
        disposition_quantity_milli: None,
        disposed_by: None,
        disposed_at: None,
        inspection_count: 1,
    }
}

struct Backend {
    reads: AtomicUsize,
    cancels: AtomicUsize,
    seen_host: std::sync::Mutex<String>,
    seen_key: std::sync::Mutex<String>,
    fail: Option<BusinessError>,
}

impl Backend {
    fn new() -> Self {
        Self {
            reads: AtomicUsize::new(0),
            cancels: AtomicUsize::new(0),
            seen_host: std::sync::Mutex::new(String::new()),
            seen_key: std::sync::Mutex::new(String::new()),
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

impl QualityHttpBackend for Backend {
    fn read<'a>(
        &'a self,
        request: QualityHttpReadRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityHttpReadReply> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        *self.seen_host.lock().unwrap() = request.host.to_owned();
        Box::pin(async move {
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            match request.id {
                None => Ok(QualityHttpReadReply::Workspace(Box::new(
                    QualityWorkspace {
                        holds: vec![QualityHoldView {
                            hold: hold(),
                            decision: QualityDecision {
                                can_inspect: false,
                                can_release: true,
                                can_cancel: false,
                                can_review_cancellation: false,
                                can_disposition: false,
                            },
                            next_action: "Approve release of the passing inspection",
                        }],
                        can_hold: true,
                        can_manage_specs: true,
                    },
                ))),
                Some("hold-a") => Ok(QualityHttpReadReply::Detail(Box::new(QualityDetail {
                    hold: QualityHoldView {
                        hold: hold(),
                        decision: QualityDecision {
                            can_inspect: false,
                            can_release: true,
                            can_cancel: false,
                            can_review_cancellation: false,
                            can_disposition: false,
                        },
                        next_action: "Approve release of the passing inspection",
                    },
                    inspections: vec![],
                    measurements: vec![],
                    cancellations: vec![],
                    dispositions: vec![],
                    spec_metrics: vec![],
                    alerts: vec![],
                }))),
                Some(_) => Err(BusinessError::new(
                    FailureKind::NotFound,
                    "QUALITY_HOLD_NOT_FOUND",
                    "Quality hold not found.",
                )),
            }
        })
    }

    fn cancel<'a>(
        &'a self,
        request: QualityHttpCommandRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityCancellationReceipt> {
        self.cancels.fetch_add(1, Ordering::SeqCst);
        *self.seen_key.lock().unwrap() = request.key.as_str().to_owned();
        Box::pin(async move {
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(QualityCancellationReceipt {
                id: request.id.to_owned(),
                hold_id: request.id.to_owned(),
                cancellation_id: format!("quality-cancel-{}", request.key.as_str()),
                status: HoldStatus::CancelPending,
                version: 2,
            })
        })
    }
}

fn router(backend: Arc<Backend>) -> Router {
    quality_router(vec![QualityHttpTarget {
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
    let app = router(backend);
    let task = tokio::spawn(async move {
        serve(listener, app).await.unwrap();
    });
    (address, task)
}

async fn call(
    address: std::net::SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> (u16, Value) {
    let mut request =
        format!("{method} {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    if let Some(body) = body {
        let text = body.to_string();
        request.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{text}",
            text.len()
        ));
    } else {
        request.push_str("\r\n");
    }
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
    let value = serde_json::from_str(body).unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test]
async fn reads_the_registered_holding_with_session_headers_and_bounded_headers() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, value) = call(
        address,
        "GET",
        "/api/v1/holdings/acme/quality",
        &[
            ("Cookie", "__Host-erpx-product=session"),
            ("x-erpx-context-company", "company"),
            ("x-erpx-context-assignment", "quality-assignment"),
            ("x-erpx-context-revision", "7"),
            ("x-erpx-context-assignment-version", "3"),
        ],
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(value["data"]["holds"][0]["id"], "hold-a");
    assert_eq!(value["data"]["holds"][0]["decision"]["canRelease"], true);
    assert_eq!(*backend.seen_host.lock().unwrap(), "acme.example.test");
    assert_eq!(backend.reads.load(Ordering::SeqCst), 1);

    let (status, value) = call(
        address,
        "GET",
        "/api/v1/holdings/acme/quality/hold-a",
        &[],
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(value["data"]["hold"]["id"], "hold-a");
    let (status, value) = call(
        address,
        "GET",
        "/api/v1/holdings/acme/quality/missing",
        &[],
        None,
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(value["error"]["code"], "QUALITY_HOLD_NOT_FOUND");
    task.abort();
}

#[tokio::test]
async fn rejects_unknown_targets_duplicate_headers_and_malformed_commands() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let (status, _) = call(address, "GET", "/api/v1/holdings/other/quality", &[], None).await;
    assert_eq!(status, 404);
    assert_eq!(backend.reads.load(Ordering::SeqCst), 0);

    let (status, value) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/quality/hold-a/cancel",
        &[("Origin", "https://acme.example.test")],
        Some(json!({"expectedVersion": 1, "action": "request", "reason": "Wrong receipt"})),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "COMMAND_KEY_REQUIRED");
    assert_eq!(backend.cancels.load(Ordering::SeqCst), 0);

    let (status, value) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/quality/hold-a/cancel",
        &[
            ("Origin", "https://evil.example.test"),
            ("Idempotency-Key", "quality-cancel-http"),
        ],
        Some(json!({"expectedVersion": 1, "action": "request", "reason": "Wrong receipt"})),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(value["error"]["code"], "ORIGIN_FORBIDDEN");
    assert_eq!(backend.cancels.load(Ordering::SeqCst), 0);

    let (status, value) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/quality/hold-a/cancel",
        &[
            ("Origin", "https://acme.example.test"),
            ("Idempotency-Key", "quality-cancel-http"),
            ("Origin", "https://acme.example.test"),
        ],
        Some(json!({"expectedVersion": 1, "action": "request", "reason": "Wrong receipt"})),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "QUALITY_REQUEST_INVALID");
    assert_eq!(backend.cancels.load(Ordering::SeqCst), 0);

    let (status, value) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/quality/hold-a/cancel",
        &[
            ("Origin", "https://acme.example.test"),
            ("Idempotency-Key", "quality-cancel-http"),
        ],
        Some(json!({"expectedVersion": 0, "action": "request", "reason": "Wrong receipt"})),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "QUALITY_CANCELLATION_REQUIRED");
    assert_eq!(backend.cancels.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn forwards_one_canonical_cancellation_and_preserves_owner_errors() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let key = "quality-cancel-http";
    let body = json!({
        "expectedVersion": 1,
        "action": "approve",
        "reason": "Independent cancellation review"
    });
    let (status, value) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/quality/hold-a/cancel",
        &[
            ("Origin", "https://acme.example.test"),
            ("Idempotency-Key", key),
        ],
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(value["data"]["holdId"], "hold-a");
    assert_eq!(value["data"]["status"], "cancel_pending");
    assert_eq!(value["data"]["version"], 2);
    assert_eq!(*backend.seen_key.lock().unwrap(), key);
    assert_eq!(backend.cancels.load(Ordering::SeqCst), 1);
    task.abort();

    let backend = Arc::new(Backend::failing(BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_CANCELLATION_CONFLICT",
        "The saved Quality cancellation differs from this request.",
    )));
    let (address, task) = start(backend).await;
    let (status, value) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/quality/hold-a/cancel",
        &[
            ("Origin", "https://acme.example.test"),
            ("Idempotency-Key", key),
        ],
        Some(body),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(value["error"]["code"], "QUALITY_CANCELLATION_CONFLICT");
    task.abort();
}

#[test]
fn rejects_configuration_that_can_escape_the_registered_origin() {
    let error = quality_router(vec![QualityHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test/path".into(),
        backend: Arc::new(Backend::new()),
    }])
    .unwrap_err();
    assert_eq!(
        error.code, "QUALITY_CONFIGURATION_INVALID",
        "{:?}",
        error.kind
    );
    assert_eq!(
        CommandKey::new("short").unwrap_err().code,
        "COMMAND_KEY_REQUIRED"
    );
    assert_eq!(RecordId::new("").unwrap_err().code, "INVALID_IDENTIFIER");
    let _ = CancellationAction::Request;
}
