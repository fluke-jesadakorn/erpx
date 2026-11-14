//! Owner-local HTTP router tests for native Sales notification commands.
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_sales::public::notification_commands::{
    NotificationHttpBackend, NotificationHttpCommand, NotificationHttpFuture,
    NotificationHttpRequest, NotificationHttpResponse, NotificationHttpTarget,
    sales_notification_commands_router,
};
use std::sync::{Arc, Mutex};

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
}

impl NotificationHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: NotificationHttpRequest<'a>,
    ) -> NotificationHttpFuture<'a, NotificationHttpResponse> {
        Box::pin(async move {
            let (data, label) = match &request.command {
                NotificationHttpCommand::Settings { input } => (
                    serde_json::json!({"companyId": "company", "adapter": input.adapter.as_str(), "version": input.expected_version + 1}),
                    format!("settings|{}|{}", input.expected_version, input.adapter.as_str()),
                ),
                NotificationHttpCommand::Queue { fulfillment_id, input } => (
                    serde_json::json!({"id": format!("customer-notification-{}", request.idempotency_key), "orderId": "order-1", "status": "pending", "version": 1}),
                    format!("queue|{fulfillment_id}|{}|{}", input.expected_version, input.note),
                ),
                NotificationHttpCommand::Retry { notification_id, input } => (
                    serde_json::json!({"id": notification_id, "orderId": "order-1", "status": "pending", "version": input.expected_version + 1}),
                    format!("retry|{notification_id}|{}", input.expected_version),
                ),
            };
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}",
                request.host, request.holding_slug, request.idempotency_key, label
            ));
            Ok(NotificationHttpResponse {
                data,
                context: serde_json::json!({"userId": "owner"}),
            })
        })
    }
}

fn target(seen: Arc<Mutex<Vec<String>>>) -> NotificationHttpTarget {
    NotificationHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(Probe { seen }),
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    assert!(sales_notification_commands_router(vec![target(seen.clone())]).is_ok());
    assert!(sales_notification_commands_router(vec![target(seen.clone()), target(seen.clone())]).is_err());
    assert!(
        sales_notification_commands_router(vec![NotificationHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target(seen)
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

fn post(path: &str, key: Option<&str>, origin: Option<&str>, body: &str) -> String {
    let mut headers = "Host: acme.example.test\r\nConnection: close\r\n".to_owned();
    if let Some(key) = key {
        headers.push_str(&format!("Idempotency-Key: {key}\r\n"));
    }
    if let Some(origin) = origin {
        headers.push_str(&format!("Origin: {origin}\r\n"));
    }
    headers.push_str(&format!(
        "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    ));
    format!("POST {path} HTTP/1.1\r\n{headers}")
}

#[tokio::test]
async fn actual_router_forwards_settings_queue_and_retry() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = sales_notification_commands_router(vec![target(seen.clone())]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, text) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-notifications/settings",
            Some("notification-settings-0001"),
            Some("https://acme.example.test"),
            &serde_json::json!({"expectedVersion": 0, "adapter": "sandbox"}).to_string(),
        ),
    )
    .await;
    assert_eq!(status, 200, "{text}");
    assert_eq!(body["data"]["adapter"], "sandbox");
    assert_eq!(body["data"]["version"], 1);

    let (status, body, text) = http(
        addr,
        post(
            "/api/v1/holdings/acme/fulfillments/fulfillment-1/notify",
            Some("notification-queue-0001"),
            None,
            &serde_json::json!({"expectedVersion": 1, "note": "Carrier confirmed the delivery window"}).to_string(),
        ),
    )
    .await;
    assert_eq!(status, 200, "{text}");
    assert_eq!(body["data"]["status"], "pending");
    assert_eq!(body["data"]["version"], 1);

    let (status, body, text) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-notifications/notice-1/retry",
            Some("notification-retry-0001"),
            None,
            &serde_json::json!({"expectedVersion": 2}).to_string(),
        ),
    )
    .await;
    assert_eq!(status, 200, "{text}");
    assert_eq!(body["data"]["version"], 3);

    assert_eq!(seen.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn actual_router_denies_origin_key_and_invalid_bodies() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = sales_notification_commands_router(vec![target(seen)]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-notifications/settings",
            Some("notification-settings-0002"),
            Some("https://untrusted.example.test"),
            &serde_json::json!({"expectedVersion": 0, "adapter": "sandbox"}).to_string(),
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-notifications/settings",
            None,
            None,
            &serde_json::json!({"expectedVersion": 0, "adapter": "sandbox"}).to_string(),
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");

    for (path, body) in [
        (
            "/api/v1/holdings/acme/customer-notifications/settings",
            serde_json::json!({"expectedVersion": -1, "adapter": "sandbox"}).to_string(),
        ),
        (
            "/api/v1/holdings/acme/customer-notifications/settings",
            serde_json::json!({"expectedVersion": 0, "adapter": "other"}).to_string(),
        ),
        (
            "/api/v1/holdings/acme/fulfillments/fulfillment-1/notify",
            serde_json::json!({"expectedVersion": 1, "note": "x"}).to_string(),
        ),
        (
            "/api/v1/holdings/acme/customer-notifications/notice-1/retry",
            serde_json::json!({"expectedVersion": 1, "extra": true}).to_string(),
        ),
    ] {
        let (status, _, text) = http(
            addr,
            post(path, Some("notification-invalid-0001"), None, &body),
        )
        .await;
        assert_eq!(status, 422, "{text}");
    }
}
