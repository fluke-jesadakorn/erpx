//! Native Quality alert acknowledgement transport and admission facts.
use erpx_server_quality::public::quality_alerts::{
    QualityAlertAcknowledgeCommand, QualityAlertAcknowledgementReceipt,
    normalize_quality_alert_acknowledge,
};
use erpx_server_quality::public::quality_http::{
    QualityHttpBackend, QualityHttpCommandRequest, QualityHttpFuture, QualityHttpReadReply,
    QualityHttpReadRequest, QualityHttpTarget, parse_alert_acknowledge, quality_router,
};
use erpx_server_quality::public::quality_read::QualityCancellationReceipt;
use erpx_server_runtime::public::http_support::serde_json::{self, Value, json};
use erpx_server_runtime::public::http_support::{Router, serve};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

#[test]
fn acknowledgement_input_is_trimmed_and_bounded_like_the_retained_schema() {
    let command = parse_alert_acknowledge(
        &json!({
            "expectedVersion": 1,
            "reason": "  Quality manager accepted the escalation  "
        }),
        "quality-alert-due-soon-hold-a",
        key("quality-alert-admission"),
    )
    .unwrap();
    assert_eq!(command.reason, "Quality manager accepted the escalation");
    assert_eq!(command.expected_version, 1);

    let accepted = parse_alert_acknowledge(
        &json!({"expectedVersion": 1, "reason": "Accepted", "acknowledgedBy": "other"}),
        "quality-alert-due-soon-hold-a",
        key("quality-alert-admission"),
    )
    .unwrap();
    assert_eq!(accepted.reason, "Accepted");
    assert_eq!(accepted.expected_version, 1);

    for (body, code) in [
        (json!({"reason": "Accepted"}), "VERSION_REQUIRED"),
        (
            json!({"expectedVersion": 0, "reason": "Accepted"}),
            "VERSION_REQUIRED",
        ),
        (json!({"expectedVersion": 1}), "QUALITY_ALERT_REQUIRED"),
        (
            json!({"expectedVersion": 1, "reason": "  x  "}),
            "QUALITY_ALERT_REASON",
        ),
        (
            json!({"expectedVersion": 1, "reason": "x".repeat(2001)}),
            "QUALITY_ALERT_REASON",
        ),
        (json!(["expectedVersion"]), "QUALITY_ALERT_REQUIRED"),
    ] {
        let error = parse_alert_acknowledge(
            &body,
            "quality-alert-due-soon-hold-a",
            key("quality-alert-admission"),
        )
        .unwrap_err();
        assert_eq!(error.code, code, "{body}");
    }

    let mut command = QualityAlertAcknowledgeCommand {
        key: key("quality-alert-admission"),
        alert_id: erpx_server_shared::public::RecordId::new("alert-a").unwrap(),
        expected_version: 1,
        reason: "  Accepted by manager  ".into(),
    };
    assert_eq!(
        normalize_quality_alert_acknowledge(command.clone())
            .unwrap()
            .reason,
        "Accepted by manager"
    );
    command.reason = "no".into();
    assert_eq!(
        normalize_quality_alert_acknowledge(command)
            .unwrap_err()
            .code,
        "QUALITY_ALERT_REASON"
    );
}

struct Backend {
    acknowledgements: AtomicUsize,
    seen_id: std::sync::Mutex<String>,
    seen_key: std::sync::Mutex<String>,
    seen_body: std::sync::Mutex<Value>,
    fail: Option<BusinessError>,
}

impl Backend {
    fn new() -> Self {
        Self {
            acknowledgements: AtomicUsize::new(0),
            seen_id: std::sync::Mutex::new(String::new()),
            seen_key: std::sync::Mutex::new(String::new()),
            seen_body: std::sync::Mutex::new(Value::Null),
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
        _request: QualityHttpReadRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityHttpReadReply> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }

    fn cancel<'a>(
        &'a self,
        _request: QualityHttpCommandRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityCancellationReceipt> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "QUALITY_CANCELLATION_UNAVAILABLE",
                "Quality cancellation is unavailable.",
            ))
        })
    }

    fn acknowledge<'a>(
        &'a self,
        request: QualityHttpCommandRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityAlertAcknowledgementReceipt> {
        self.acknowledgements.fetch_add(1, Ordering::SeqCst);
        *self.seen_id.lock().unwrap() = request.id.to_owned();
        *self.seen_key.lock().unwrap() = request.key.as_str().to_owned();
        *self.seen_body.lock().unwrap() = request.body.clone();
        Box::pin(async move {
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(QualityAlertAcknowledgementReceipt {
                id: request.id.to_owned(),
                version: 2,
                acknowledged_by: "fixture-b".into(),
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
    path: &str,
    headers: &[(&str, &str)],
    body: Value,
) -> (u16, Value) {
    let mut request =
        format!("POST {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    let text = body.to_string();
    request.push_str(&format!(
        "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{text}",
        text.len()
    ));
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
async fn forwards_one_canonical_acknowledgement_and_preserves_owner_errors() {
    let backend = Arc::new(Backend::new());
    let (address, task) = start(backend.clone()).await;
    let body = json!({
        "expectedVersion": 1,
        "reason": "  Quality manager accepted the escalation  "
    });
    let (status, value) = call(
        address,
        "/api/v1/holdings/acme/quality/quality-alert-due-soon-hold-a/acknowledge",
        &[
            ("Origin", "https://acme.example.test"),
            ("Idempotency-Key", "quality-alert-http"),
        ],
        body.clone(),
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value["data"]["id"], "quality-alert-due-soon-hold-a");
    assert_eq!(value["data"]["version"], 2);
    assert_eq!(value["data"]["acknowledgedBy"], "fixture-b");
    assert_eq!(
        *backend.seen_id.lock().unwrap(),
        "quality-alert-due-soon-hold-a"
    );
    assert_eq!(*backend.seen_key.lock().unwrap(), "quality-alert-http");
    assert_eq!(
        *backend.seen_body.lock().unwrap(),
        json!({"expectedVersion": 1, "reason": "  Quality manager accepted the escalation  "})
    );
    assert_eq!(backend.acknowledgements.load(Ordering::SeqCst), 1);

    let (status, value) = call(
        address,
        "/api/v1/holdings/acme/quality/quality-alert-due-soon-hold-a/acknowledge",
        &[
            ("Origin", "https://evil.example.test"),
            ("Idempotency-Key", "quality-alert-http"),
        ],
        body.clone(),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(value["error"]["code"], "ORIGIN_FORBIDDEN");
    let (status, value) = call(
        address,
        "/api/v1/holdings/acme/quality/quality-alert-due-soon-hold-a/acknowledge",
        &[("Origin", "https://acme.example.test")],
        body.clone(),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "COMMAND_KEY_REQUIRED");
    let (status, value) = call(
        address,
        "/api/v1/holdings/other/quality/quality-alert-due-soon-hold-a/acknowledge",
        &[
            ("Origin", "https://acme.example.test"),
            ("Idempotency-Key", "quality-alert-http"),
        ],
        body.clone(),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(value["error"]["code"], "NOT_FOUND");
    let (status, value) = call(
        address,
        "/api/v1/holdings/acme/quality/quality-alert-due-soon-hold-a/acknowledge",
        &[
            ("Origin", "https://acme.example.test"),
            ("Idempotency-Key", "quality-alert-stale"),
        ],
        json!({"expectedVersion": 0, "reason": "Stale escalation"}),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "VERSION_REQUIRED");
    assert_eq!(backend.acknowledgements.load(Ordering::SeqCst), 1);
    task.abort();

    let backend = Arc::new(Backend::failing(BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_ALERT_ACKNOWLEDGED",
        "This escalation has already been acknowledged.",
    )));
    let (address, task) = start(backend).await;
    let (status, value) = call(
        address,
        "/api/v1/holdings/acme/quality/quality-alert-due-soon-hold-a/acknowledge",
        &[
            ("Origin", "https://acme.example.test"),
            ("Idempotency-Key", "quality-alert-http"),
        ],
        body,
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(value["error"]["code"], "QUALITY_ALERT_ACKNOWLEDGED");
    task.abort();
}
