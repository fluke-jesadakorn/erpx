//! Native Quality HTTP transport facts over a real loopback listener.
use erpx_server_quality::public::quality_http::{
    QualityHttpBackend, QualityHttpCommandRequest, QualityHttpFuture, QualityHttpHistoryReply,
    QualityHttpHistoryRequest, QualityHttpHoldCreateRequest, QualityHttpReadReply,
    QualityHttpReadRequest, QualityHttpSaveSpecRequest, QualityHttpTarget,
    parse_quality_history_query, quality_router,
};
use erpx_server_quality::public::quality_commands::{QualityHoldReceipt, QualitySpecReceipt};
use erpx_server_quality::public::quality_read::{
    CancellationAction, HoldStatus, QualityCancellation, QualityCancellationReceipt, QualityDecision,
    QualityDetail, QualityDetailCore, QualityHold, QualityHoldProduct, QualityHoldView, QualityMetric,
    QualityWorkspace, SpecStatus,
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

#[test]
fn history_cursor_requires_complete_canonical_unambiguous_input() {
    let first = parse_quality_history_query(None).unwrap();
    assert!(first.before.is_none() && first.head.is_none() && first.revision.is_none());
    let revision = format!("qch1:{}", "a".repeat(64));
    let encoded = format!("before=200&head=201&revision=qch1%3A{}", "a".repeat(64));
    let cursor = parse_quality_history_query(Some(&encoded)).unwrap();
    assert_eq!(cursor.before, Some(200));
    assert_eq!(cursor.head, Some(201));
    assert_eq!(cursor.revision.as_deref(), Some(revision.as_str()));
    for value in ["0", "-1", "-9007199254740991", "9007199254740991"] {
        let input = format!("before={value}&head=201&revision=qch1%3A{}", "a".repeat(64));
        assert_eq!(parse_quality_history_query(Some(&input)).unwrap().before.unwrap().to_string(), value);
    }
    for invalid in [
        "before=2", "before=01&head=2&revision=x", "before=0&head=2&revision=x",
        "before=2&before=3&head=4&revision=x", "before=2&head=4&revision=x&other=1",
    ] { assert!(parse_quality_history_query(Some(invalid)).is_err()); }
    assert!(parse_quality_history_query(Some(&format!("before=2&head=3&revision=qch1%3A{}", "A".repeat(64)))).is_err());
    for value in ["-0", "-01", "+1", "9007199254740992", "-9007199254740992"] {
        let input = format!("before={value}&head=201&revision=qch1%3A{}", "a".repeat(64));
        assert!(parse_quality_history_query(Some(&input)).is_err());
    }
    // The Next relay measured the raw serialized query and accepted only the
    // three canonical keys, in any order, with percent-decoded values.
    let valid = format!("head=201&revision=qch1%3A{}&before=200", "a".repeat(64));
    assert_eq!(parse_quality_history_query(Some(&valid)).unwrap().before, Some(200));
    let encoded_key = format!("%62efore=200&head=201&revision=qch1%3A{}", "a".repeat(64));
    assert_eq!(parse_quality_history_query(Some(&encoded_key)).unwrap().head, Some(201));
    for invalid in [
        "&".to_owned(),
        format!("before=2&head=3&revision=qch1%3A{}&pad={}", "a".repeat(64), "x".repeat(200)),
        format!("before=2&head=3&revision=qch1%3A{}&revision=qch1%3A{}", "a".repeat(64), "b".repeat(64)),
        format!("before=%202&head=3&revision=qch1%3A{}", "a".repeat(64)),
        format!("before=2&head=3&revision=qch1%3A{}", "a".repeat(63)),
    ] { assert!(parse_quality_history_query(Some(&invalid)).is_err(), "{invalid}"); }
}

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

fn product() -> QualityHoldProduct {
    QualityHoldProduct {
        product_name: "Quality product".into(),
        product_unit: "each".into(),
        tracking_mode: "none".into(),
        warehouse_name: "Quality warehouse".into(),
        lot_code: None,
        serial_number: None,
        expiry_date: None,
        eligible_count: 1,
    }
}

fn metric() -> QualityMetric {
    QualityMetric {
        id: "metric-weight".into(),
        spec_id: Some("spec-1".into()),
        code: "weight".into(),
        label: "Weight".into(),
        unit: "kg".into(),
        minimum: Some("9.9".into()),
        maximum: Some("10.1".into()),
        ordinal: 0,
    }
}

struct Backend {
    reads: AtomicUsize,
    cancels: AtomicUsize,
    creates: AtomicUsize,
    histories: AtomicUsize,
    spec_saves: AtomicUsize,
    spec_actions: AtomicUsize,
    seen_host: std::sync::Mutex<String>,
    seen_key: std::sync::Mutex<String>,
    seen_query: std::sync::Mutex<String>,
    seen_spec_id: std::sync::Mutex<Option<String>>,
    seen_spec_body: std::sync::Mutex<Value>,
    history_page: Option<QualityHttpHistoryReply>,
    fail: Option<BusinessError>,
}

impl Backend {
    fn new() -> Self {
        Self {
            reads: AtomicUsize::new(0),
            cancels: AtomicUsize::new(0),
            creates: AtomicUsize::new(0),
            histories: AtomicUsize::new(0),
            spec_saves: AtomicUsize::new(0),
            spec_actions: AtomicUsize::new(0),
            seen_host: std::sync::Mutex::new(String::new()),
            seen_key: std::sync::Mutex::new(String::new()),
            seen_query: std::sync::Mutex::new(String::new()),
            seen_spec_id: std::sync::Mutex::new(None),
            seen_spec_body: std::sync::Mutex::new(Value::Null),
            history_page: None,
            fail: None,
        }
    }
    fn failing(error: BusinessError) -> Self {
        Self {
            fail: Some(error),
            ..Self::new()
        }
    }
    fn with_history(page: QualityHttpHistoryReply) -> Self {
        Self {
            history_page: Some(page),
            ..Self::new()
        }
    }
}

impl QualityHttpBackend for Backend {
    fn detail_core<'a>(&'a self, request: QualityHttpReadRequest<'a>) -> QualityHttpFuture<'a, QualityDetailCore> {
        Box::pin(async move {
            if request.id != Some("hold-a") { return Err(BusinessError::new(FailureKind::NotFound, "QUALITY_HOLD_NOT_FOUND", "Hold not found.")); }
            Ok(QualityDetailCore {
                hold: QualityHoldView { hold: hold(), product: product(), decision: QualityDecision { can_inspect: false, can_release: true, can_cancel: false, can_review_cancellation: false, can_disposition: false }, next_action: "Approve release of the passing inspection" },
                inspections: vec![], measurements: vec![], dispositions: vec![], spec_metrics: vec![], alerts: vec![],
            })
        })
    }
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
                            product: product(),
                            decision: QualityDecision {
                                can_inspect: false,
                                can_release: true,
                                can_cancel: false,
                                can_review_cancellation: false,
                                can_disposition: false,
                            },
                            next_action: "Approve release of the passing inspection",
                        }],
                        stock: vec![],
                        lots: vec![],
                        products: vec![],
                        specifications: vec![],
                        specification_metrics: vec![],
                        can_hold: true,
                        can_manage_specs: true,
                    },
                ))),
                Some("hold-a") => Ok(QualityHttpReadReply::Detail(Box::new(QualityDetail {
                    hold: QualityHoldView {
                        hold: hold(),
                        product: product(),
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
                    spec_metrics: vec![metric()],
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

    fn hold_create<'a>(
        &'a self,
        request: QualityHttpHoldCreateRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityHoldReceipt> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        *self.seen_key.lock().unwrap() = request.key.as_str().to_owned();
        Box::pin(async move {
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(QualityHoldReceipt {
                id: format!("quality-{}", request.key.as_str()),
                status: HoldStatus::Held,
                version: 1,
            })
        })
    }

    fn save_spec<'a>(
        &'a self,
        request: QualityHttpSaveSpecRequest<'a>,
    ) -> QualityHttpFuture<'a, QualitySpecReceipt> {
        self.spec_saves.fetch_add(1, Ordering::SeqCst);
        let id = request.id.map(str::to_owned);
        let key = request.key.as_str().to_owned();
        *self.seen_spec_id.lock().unwrap() = id.clone();
        *self.seen_key.lock().unwrap() = key.clone();
        *self.seen_spec_body.lock().unwrap() = request.body.clone();
        Box::pin(async move {
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(QualitySpecReceipt {
                id: id.unwrap_or_else(|| format!("quality-spec-{key}")),
                status: SpecStatus::Draft,
                version: 1,
            })
        })
    }

    fn transition_spec<'a>(
        &'a self,
        request: QualityHttpCommandRequest<'a>,
    ) -> QualityHttpFuture<'a, QualitySpecReceipt> {
        self.spec_actions.fetch_add(1, Ordering::SeqCst);
        let id = request.id.to_owned();
        let key = request.key.as_str().to_owned();
        *self.seen_spec_id.lock().unwrap() = Some(id.clone());
        *self.seen_key.lock().unwrap() = key;
        *self.seen_spec_body.lock().unwrap() = request.body.clone();
        Box::pin(async move {
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(QualitySpecReceipt {
                id,
                status: SpecStatus::Active,
                version: 2,
            })
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

    /// Mirrors the composition read: the raw cursor reaches the owner after
    /// admission, is parsed there, and the page is returned as one reply.
    fn history<'a>(
        &'a self,
        request: QualityHttpHistoryRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityHttpHistoryReply> {
        self.histories.fetch_add(1, Ordering::SeqCst);
        *self.seen_query.lock().unwrap() = request.raw_query.unwrap_or("").to_owned();
        Box::pin(async move {
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            let Some(page) = &self.history_page else {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "QUALITY_HISTORY_UNAVAILABLE",
                    "Quality cancellation history is unavailable.",
                ));
            };
            let cursor = parse_quality_history_query(request.raw_query)?;
            if cursor.head.is_some_and(|head| head != page.head_version)
                || cursor.revision.as_deref().is_some_and(|revision| revision != page.history_revision)
            {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "QUALITY_HISTORY_CHANGED",
                    "Reload Quality cancellation history.",
                ));
            }
            let items: Vec<_> = page.items.iter()
                .filter(|item| cursor.before.is_none_or(|before| item.version < before))
                .cloned().collect();
            Ok(QualityHttpHistoryReply {
                next_before: page.next_before.filter(|_| !items.is_empty()),
                items,
                head_version: page.head_version,
                history_revision: page.history_revision.clone(),
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
    match body {
        Some(body) => {
            call_raw(address, method, path, headers, "application/json", &body.to_string()).await
        }
        None => call_raw(address, method, path, headers, "", "").await,
    }
}

async fn call_raw(
    address: std::net::SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    content_type: &str,
    text: &str,
) -> (u16, Value) {
    let mut request =
        format!("{method} {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    if content_type.is_empty() {
        request.push_str("\r\n");
    } else {
        request.push_str(&format!(
            "Content-Type: {content_type}\r\nContent-Length: {}\r\n\r\n{text}",
            text.len()
        ));
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
    assert_eq!(value["data"]["holds"][0]["productName"], "Quality product");
    assert_eq!(value["data"]["holds"][0]["productUnit"], "each");
    assert_eq!(value["data"]["holds"][0]["warehouseName"], "Quality warehouse");
    assert_eq!(value["data"]["holds"][0]["eligibleCount"], 1);
    assert_eq!(value["data"]["stock"], json!([]));
    assert_eq!(value["data"]["lots"], json!([]));
    assert_eq!(value["data"]["products"], json!([]));
    assert_eq!(value["data"]["specifications"], json!([]));
    assert_eq!(value["data"]["specMetrics"], json!([]));
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
    let (status, value) = call(address, "GET", "/api/v1/holdings/acme/quality/hold-a/detail-core", &[], None).await;
    assert_eq!(status, 200);
    assert_eq!(value["data"]["hold"]["id"], "hold-a");
    assert_eq!(value["data"]["cancellationHistory"], "paged");
    assert!(value["data"].get("cancellations").is_none());
    // Router does not parse the untrusted query or access history before the
    // selected backend has a chance to authorize it. The default is unavailable.
    let (status, value) = call(address, "GET", "/api/v1/holdings/acme/quality/hold-a/cancellations?before=bad", &[], None).await;
    assert_eq!(status, 503);
    assert_eq!(value["error"]["code"], "QUALITY_HISTORY_UNAVAILABLE");
    task.abort();
}

#[tokio::test]
async fn public_detail_uses_the_next_wire_and_private_ssr_keeps_metric_aliases() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend).await;
    let (status, private) = call(
        address,
        "GET",
        "/api/v1/holdings/acme/quality/hold-a",
        &[],
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(private["data"].as_object().unwrap().keys().len(), 7);
    let private_metric = &private["data"]["specMetrics"][0];
    assert_eq!(private_metric["minimum"], "9.9");
    assert_eq!(private_metric["maximum"], "10.1");
    assert_eq!(private_metric["minimumValue"], "9.9");
    assert_eq!(private_metric["maximumValue"], "10.1");

    let (status, public) = call(
        address,
        "GET",
        "/api/v1/holdings/acme/quality/hold-a",
        &[("x-erpx-api-transport", "public")],
        None,
    )
    .await;
    assert_eq!(status, 200);
    let public_metric = &public["data"]["specMetrics"][0];
    assert_eq!(public_metric["minimum"], "9.9");
    assert_eq!(public_metric["maximum"], "10.1");
    assert!(public_metric.get("minimumValue").is_none());
    assert!(public_metric.get("maximumValue").is_none());
    let mut private_data = private["data"].clone();
    private_data["specMetrics"][0]
        .as_object_mut()
        .unwrap()
        .remove("minimumValue");
    private_data["specMetrics"][0]
        .as_object_mut()
        .unwrap()
        .remove("maximumValue");
    assert_eq!(public["data"], private_data);
    task.abort();
}

#[tokio::test]
async fn forwards_the_raw_history_cursor_and_returns_the_typed_page_envelope() {
    let revision = format!("qch1:{}", "a".repeat(64));
    let page = QualityHttpHistoryReply {
        items: vec![
            QualityCancellation {
                id: "cancel-2".into(),
                hold_id: "hold-a".into(),
                version: 7,
                action: CancellationAction::Approve,
                reason: "Independent review".into(),
                actor_id: "reviewer".into(),
                created_at: 2,
            },
            QualityCancellation {
                id: "cancel-1".into(),
                hold_id: "hold-a".into(),
                version: 5,
                action: CancellationAction::Request,
                reason: "Incorrect scope".into(),
                actor_id: "maker".into(),
                created_at: 1,
            },
        ],
        next_before: Some(5),
        head_version: 9,
        history_revision: revision.clone(),
    };
    let backend = Arc::new(Backend::with_history(page));
    let (address, task) = start(backend.clone()).await;
    let (status, value) = call(
        address,
        "GET",
        "/api/v1/holdings/acme/quality/hold-a/cancellations",
        &[],
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        value.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(),
        ["data"]
    );
    let data = &value["data"];
    let mut keys = data.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(keys, ["headVersion", "historyRevision", "items", "nextBefore"]);
    assert_eq!(data["items"], json!([
        {"id":"cancel-2","holdId":"hold-a","version":7,"action":"approve","reason":"Independent review","actorId":"reviewer","createdAt":2},
        {"id":"cancel-1","holdId":"hold-a","version":5,"action":"request","reason":"Incorrect scope","actorId":"maker","createdAt":1},
    ]));
    assert_eq!(data["nextBefore"], 5);
    assert_eq!(data["headVersion"], 9);
    assert_eq!(data["historyRevision"], revision);
    assert_eq!(*backend.seen_query.lock().unwrap(), "");

    let raw = format!("before=8&head=9&revision=qch1%3A{}", "a".repeat(64));
    let (status, next) = call(
        address,
        "GET",
        &format!("/api/v1/holdings/acme/quality/hold-a/cancellations?{raw}"),
        &[],
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(next["data"]["items"], data["items"]);
    assert_eq!(*backend.seen_query.lock().unwrap(), raw);
    assert_eq!(backend.histories.load(Ordering::SeqCst), 2);

    let public = [("x-erpx-api-transport", "public")];
    let (status, first) = call(address, "GET",
        "/api/v1/holdings/acme/quality/hold-a/cancellations?&&", &public, None).await;
    assert_eq!(status, 200);
    assert_eq!(first["data"], *data);

    let empty_continuation = format!(
        "/api/v1/holdings/acme/quality/hold-a/cancellations?before=0&head=9&revision=qch1%3A{}", "a".repeat(64),
    );
    let (status, empty) = call(address, "GET", &empty_continuation, &[], None).await;
    assert_eq!(status, 200);
    assert_eq!(empty["data"]["items"], json!([]));
    let (status, error) = call(address, "GET", &empty_continuation, &public, None).await;
    assert_eq!(status, 503);
    assert_eq!(error["error"]["code"], "NATIVE_QUALITY_RESPONSE_INVALID");

    let (status, value) = call(
        address,
        "GET",
        "/api/v1/holdings/acme/quality/hold-a/cancellations?before=bad&head=9&revision=qch1%3Aaaaa",
        &[],
        None,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value, json!({"error":{"code":"QUALITY_HISTORY_CURSOR_INVALID","message":"Reload Quality cancellation history."}}));
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

#[tokio::test]
async fn forwards_one_canonical_hold_create_and_rejects_stale_input() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let key = "quality-hold-http";
    let body = json!({
        "warehouseId": "warehouse",
        "productId": "product",
        "expectedStockVersion": 3,
        "dueDate": "2026-11-10",
        "reason": "Damaged carton requires independent review"
    });
    let (status, value) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/quality",
        &[
            ("Origin", "https://acme.example.test"),
            ("Idempotency-Key", key),
        ],
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value["data"]["id"], format!("quality-{key}"));
    assert_eq!(value["data"]["status"], "held");
    assert_eq!(value["data"]["version"], 1);
    assert_eq!(*backend.seen_key.lock().unwrap(), key);
    assert_eq!(backend.creates.load(Ordering::SeqCst), 1);

    let (status, value) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/quality",
        &[
            ("Origin", "https://acme.example.test"),
            ("Idempotency-Key", "quality-hold-bad-version"),
        ],
        Some(json!({
            "warehouseId": "warehouse",
            "productId": "product",
            "expectedStockVersion": 0,
            "dueDate": "2026-11-10",
            "reason": "Damaged carton requires independent review"
        })),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "VERSION_REQUIRED");
    assert_eq!(backend.creates.load(Ordering::SeqCst), 1);

    let (status, value) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/quality",
        &[
            ("Origin", "https://acme.example.test"),
            ("Idempotency-Key", "quality-hold-bad-date"),
        ],
        Some(json!({
            "warehouseId": "warehouse",
            "productId": "product",
            "expectedStockVersion": 3,
            "dueDate": "2026-02-30",
            "reason": "Damaged carton requires independent review"
        })),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "QUALITY_DUE_DATE_REQUIRED");
    assert_eq!(backend.creates.load(Ordering::SeqCst), 1);

    let (status, value) = call(
        address,
        "POST",
        "/api/v1/holdings/acme/quality",
        &[
            ("Origin", "https://acme.example.test"),
            ("Idempotency-Key", "quality-hold-extra"),
        ],
        Some(json!({
            "warehouseId": "warehouse",
            "productId": "product",
            "expectedStockVersion": 3,
            "dueDate": "2026-11-10",
            "reason": "Damaged carton requires independent review",
            "sourceType": "goods_receipt"
        })),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "QUALITY_HOLD_REQUIRED");
    assert_eq!(backend.creates.load(Ordering::SeqCst), 1);
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

const SPEC_CREATE: &str = r#"{"productId":"product","name":"Cold chain acceptance","metrics":[{"code":"temp","label":"Temperature","unit":"C","minimum":"2","maximum":"8"}]}"#;
const SPEC_EDIT: &str = r#"{"productId":"product","name":"Cold chain acceptance v2","expectedVersion":1,"metrics":[{"code":"temp","label":"Temperature","unit":"C","minimum":"1","maximum":"7"}]}"#;
const SPEC_ACTION: &str = r#"{"expectedVersion":1,"action":"activate","reason":"Independent review confirms the limits"}"#;

#[tokio::test]
async fn public_specification_post_preserves_relay_json_behavior() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let create_path = "/api/v1/holdings/acme/quality/specifications";
    let edit_path = "/api/v1/holdings/acme/quality/specifications/spec-a";
    let malformed = [("x-erpx-api-transport", "public"), ("idempotency-key", "spec-public-malformed")];
    for path in [create_path, edit_path] {
        let (status, error) = call_raw(address, "POST", path, &malformed, "text/plain", "{").await;
        assert_eq!(status, 500);
        assert_eq!(error["error"]["code"], "INTERNAL_ERROR");
    }
    assert_eq!(backend.spec_saves.load(Ordering::SeqCst), 0);
    assert_eq!(backend.spec_actions.load(Ordering::SeqCst), 0);

    let create = [("x-erpx-api-transport", "public"), ("idempotency-key", "spec-public-create")];
    let (status, value) = call_raw(address, "POST", create_path, &create, "text/plain", SPEC_CREATE).await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value, json!({"data":{"id":"quality-spec-spec-public-create","status":"draft","version":1}}));

    let edit = [("x-erpx-api-transport", "public"), ("idempotency-key", "spec-public-edit")];
    let (status, value) = call_raw(address, "POST", edit_path, &edit, "text/plain", SPEC_EDIT).await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value, json!({"data":{"id":"spec-a","status":"draft","version":1}}));

    let action = [("x-erpx-api-transport", "public"), ("idempotency-key", "spec-public-action")];
    let (status, value) = call_raw(address, "POST", edit_path, &action, "text/plain", SPEC_ACTION).await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value, json!({"data":{"id":"spec-a","status":"active","version":2}}));
    assert_eq!(backend.spec_saves.load(Ordering::SeqCst), 2);
    assert_eq!(backend.spec_actions.load(Ordering::SeqCst), 1);
    task.abort();
}

#[tokio::test]
async fn specification_post_returns_exact_receipt_and_dispatches_edit_before_action() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let create_path = "/api/v1/holdings/acme/quality/specifications";
    let edit_path = "/api/v1/holdings/acme/quality/specifications/spec-a";
    let origin = ("Origin", "https://acme.example.test");

    // Private callers keep strict JSON admission even though public transport
    // accepts JSON under any media type.
    let (status, error) = call_raw(
        address,
        "POST",
        create_path,
        &[("idempotency-key", "spec-private-raw")],
        "text/plain",
        SPEC_CREATE,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(error["error"]["code"], "QUALITY_COMMAND_REQUIRED");

    let (status, value) = call(
        address,
        "POST",
        create_path,
        &[origin, ("Idempotency-Key", "spec-create")],
        Some(json!({
            "productId": "product",
            "name": "Cold chain acceptance",
            "metrics": [{"code":"temp","label":"Temperature","unit":"C","minimum":"2","maximum":"8"}],
        })),
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value, json!({"data":{"id":"quality-spec-spec-create","status":"draft","version":1}}));
    assert_eq!(value.as_object().unwrap().len(), 1);
    assert_eq!(value["data"].as_object().unwrap().len(), 3);
    assert_eq!(*backend.seen_spec_id.lock().unwrap(), None);
    assert_eq!(*backend.seen_key.lock().unwrap(), "spec-create");
    assert_eq!(backend.spec_saves.load(Ordering::SeqCst), 1);

    // A body without `action` edits the addressed specification through the
    // save port; it never reaches the transition port.
    let (status, value) = call(
        address,
        "POST",
        edit_path,
        &[origin, ("Idempotency-Key", "spec-edit")],
        Some(json!({
            "productId": "product",
            "name": "Cold chain acceptance v2",
            "expectedVersion": 1,
            "metrics": [{"code":"temp","label":"Temperature","unit":"C","minimum":"1","maximum":"7"}],
        })),
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value, json!({"data":{"id":"spec-a","status":"draft","version":1}}));
    assert_eq!(*backend.seen_spec_id.lock().unwrap(), Some("spec-a".to_owned()));
    assert_eq!(
        backend.seen_spec_body.lock().unwrap()["name"].as_str(),
        Some("Cold chain acceptance v2")
    );
    assert_eq!(backend.spec_saves.load(Ordering::SeqCst), 2);
    assert_eq!(backend.spec_actions.load(Ordering::SeqCst), 0);

    // Presence of `action` selects the transition port: an invalid action is
    // rejected by the action parser, not silently treated as an edit.
    let (status, error) = call(
        address,
        "POST",
        edit_path,
        &[origin, ("Idempotency-Key", "spec-bad-action")],
        Some(json!({"expectedVersion":1,"action":"bogus","reason":"Independent review"})),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(error["error"]["code"], "QUALITY_SPEC_ACTION");
    assert_eq!(backend.spec_saves.load(Ordering::SeqCst), 2);
    assert_eq!(backend.spec_actions.load(Ordering::SeqCst), 0);

    let (status, value) = call(
        address,
        "POST",
        edit_path,
        &[origin, ("Idempotency-Key", "spec-action")],
        Some(json!({
            "expectedVersion": 1,
            "action": "activate",
            "reason": "Independent review confirms the limits",
        })),
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value, json!({"data":{"id":"spec-a","status":"active","version":2}}));
    assert_eq!(*backend.seen_spec_body.lock().unwrap(), json!({
        "expectedVersion": 1,
        "action": "activate",
        "reason": "Independent review confirms the limits",
    }));
    assert_eq!(backend.spec_saves.load(Ordering::SeqCst), 2);
    assert_eq!(backend.spec_actions.load(Ordering::SeqCst), 1);
    task.abort();
}
