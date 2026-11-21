//! Actual owner-local HTTP router tests for native Sales customer-notice
//! reconciliation. The backend is a recording fake; PostgreSQL and composition
//! behavior are proved separately.
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_sales::public::sales_notification_reconciliation::{
    ReconcileHttpBackend, ReconcileHttpFuture, ReconcileHttpRequest, ReconcileHttpResponse,
    ReconcileHttpTarget, sales_notification_reconciliation_router,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl ReconcileHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: ReconcileHttpRequest<'a>,
    ) -> ReconcileHttpFuture<'a, ReconcileHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.notification_id,
                request.idempotency_key,
                request.input.expected_version,
                request.input.attempt_id,
            ));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "CUSTOMER_NOTIFICATION_PROVIDER_UNCONFIRMED",
                    "Synthetic unconfirmed provider.",
                ));
            }
            Ok(ReconcileHttpResponse {
                data: serde_json::json!({
                    "id": request.notification_id,
                    "orderId": "order-1",
                    "status": "sent",
                    "version": request.input.expected_version + 1,
                    "reconciliationId": format!("sales-notification-reconciliation-{}", request.idempotency_key),
                }),
                context: serde_json::json!({"userId": "reviewer"}),
            })
        })
    }
}

fn target(seen: Arc<Mutex<Vec<String>>>, fail: bool) -> ReconcileHttpTarget {
    ReconcileHttpTarget {
        host: if fail { "fail.example.test".into() } else { "acme.example.test".into() },
        holding_slug: if fail { "fail".into() } else { "acme".into() },
        origin: if fail { "https://fail.example.test".into() } else { "https://acme.example.test".into() },
        backend: Arc::new(Probe { seen, fail }),
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    assert!(sales_notification_reconciliation_router(vec![target(seen.clone(), false)]).is_ok());
    assert!(sales_notification_reconciliation_router(vec![
        target(seen.clone(), false),
        target(seen.clone(), false)
    ])
    .is_err());
    for origin in ["http://acme.example.test", "https://acme.example.test/path"] {
        assert!(
            sales_notification_reconciliation_router(vec![ReconcileHttpTarget {
                origin: origin.into(),
                ..target(seen.clone(), false)
            }])
            .is_err()
        );
    }
    assert!(
        sales_notification_reconciliation_router(vec![ReconcileHttpTarget {
            holding_slug: "ACME".into(),
            ..target(seen, false)
        }])
        .is_err()
    );
}

async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let text = String::from_utf8_lossy(&bytes).to_string();
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .unwrap();
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
    (
        status,
        serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        text,
    )
}

fn payload() -> String {
    serde_json::json!({
        "expectedVersion": 2,
        "attemptId": "attempt-1",
        "reason": "Independent current approver verifies the original provider identity"
    })
    .to_string()
}

fn post(
    path: &str,
    host: &str,
    key: Option<&str>,
    origin: Option<&str>,
    content_type: Option<&str>,
    body: &str,
) -> String {
    let mut headers = format!("Host: {host}\r\nConnection: close\r\n");
    if let Some(key) = key {
        headers.push_str(&format!("Idempotency-Key: {key}\r\n"));
    }
    if let Some(origin) = origin {
        headers.push_str(&format!("Origin: {origin}\r\n"));
    }
    headers.push_str(&format!(
        "Content-Type: {}\r\nContent-Length: {}\r\n\r\n{}",
        content_type.unwrap_or("application/json"),
        body.len(),
        body
    ));
    format!("POST {path} HTTP/1.1\r\n{headers}")
}

const PATH: &str = "/api/v1/holdings/acme/customer-notifications/notice-1/reconcile";

#[tokio::test]
async fn actual_router_forwards_the_exact_reconciliation_command() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = sales_notification_reconciliation_router(vec![
        target(seen.clone(), false),
        target(seen.clone(), true),
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, text) = http(
        addr,
        post(
            PATH,
            "acme.example.test",
            Some("sales-notification-reconciliation-0001"),
            Some("https://acme.example.test"),
            None,
            &payload(),
        ),
    )
    .await;
    assert_eq!(status, 200, "{text}");
    assert_eq!(body["data"]["status"], "sent");
    assert_eq!(body["data"]["version"], 3);
    assert_eq!(
        body["data"]["reconciliationId"],
        "sales-notification-reconciliation-sales-notification-reconciliation-0001"
    );
    assert_eq!(body["context"]["userId"], "reviewer");
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        ["acme.example.test|acme|notice-1|sales-notification-reconciliation-0001|2|attempt-1"]
    );
}

#[tokio::test]
async fn actual_router_denies_origin_key_body_and_unconfirmed_truth() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = sales_notification_reconciliation_router(vec![
        target(seen.clone(), false),
        target(seen.clone(), true),
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, _) = http(
        addr,
        post(
            PATH,
            "acme.example.test",
            Some("sales-notification-reconciliation-0002"),
            Some("https://untested.example.test"),
            None,
            &payload(),
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, body, _) = http(
        addr,
        post(PATH, "acme.example.test", None, None, None, &payload()),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");

    for invalid in [
        serde_json::json!({"expectedVersion": 0, "attemptId": "attempt-1", "reason": "Independent review"}).to_string(),
        serde_json::json!({"expectedVersion": 2, "attemptId": "", "reason": "Independent review"}).to_string(),
        serde_json::json!({"expectedVersion": 2, "attemptId": "attempt-1", "reason": "tiny"}).to_string(),
        serde_json::json!({"expectedVersion": 2, "attemptId": "attempt-1", "reason": "Independent review", "extra": true}).to_string(),
    ] {
        let (status, _, text) = http(
            addr,
            post(
                PATH,
                "acme.example.test",
                Some("sales-notification-reconciliation-0003"),
                None,
                None,
                &invalid,
            ),
        )
        .await;
        assert_eq!(status, 422, "{text}");
    }

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/fail/customer-notifications/notice-1/reconcile",
            "fail.example.test",
            Some("sales-notification-reconciliation-0004"),
            None,
            None,
            &payload(),
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "CUSTOMER_NOTIFICATION_PROVIDER_UNCONFIRMED");

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/unrelated",
            "acme.example.test",
            Some("sales-notification-reconciliation-0005"),
            None,
            None,
            &payload(),
        ),
    )
    .await;
    assert_eq!(status, 404);
}
