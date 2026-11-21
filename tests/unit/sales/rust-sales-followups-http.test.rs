//! Owner-local HTTP router tests for native Sales customer follow-up commands.
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_sales::public::customer_followup_commands::{
    FollowupHttpBackend, FollowupHttpCommand, FollowupHttpFuture, FollowupHttpRequest,
    FollowupHttpResponse, FollowupHttpTarget, customer_followups_router,
};
use erpx_server_shared::public::BusinessError;
use std::sync::{Arc, Mutex};

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
}

impl FollowupHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: FollowupHttpRequest<'a>,
    ) -> FollowupHttpFuture<'a, FollowupHttpResponse> {
        Box::pin(async move {
            let (id, version, status, body) = match &request.command {
                FollowupHttpCommand::Create { input } => (
                    format!("followup-{}", request.idempotency_key),
                    1,
                    "open",
                    format!("{}|{}|{}|{}", input.order_id, input.owner_id, input.due_date, input.next_action),
                ),
                FollowupHttpCommand::Change { id, input } => (
                    id.clone(),
                    input.expected_version + 1,
                    input.action.status(),
                    format!(
                        "{}|{}|{}",
                        input.action.as_str(),
                        input.owner_id.clone().unwrap_or_default(),
                        input.reason
                    ),
                ),
            };
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}|{}",
                request.host, request.holding_slug, request.idempotency_key, id, body
            ));
            Ok(FollowupHttpResponse {
                data: serde_json::json!({"id": id, "version": version, "status": status}),
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

fn target(seen: Arc<Mutex<Vec<String>>>) -> FollowupHttpTarget {
    FollowupHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(Probe { seen }),
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    assert!(customer_followups_router(vec![target(seen.clone())]).is_ok());
    assert!(customer_followups_router(vec![target(seen.clone()), target(seen.clone())]).is_err());
    for origin in ["http://acme.example.test", "https://acme.example.test/path"] {
        assert!(
            customer_followups_router(vec![FollowupHttpTarget {
                origin: origin.into(),
                ..target(seen.clone())
            }])
            .is_err()
        );
    }
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

fn post(
    path: &str,
    host: &str,
    key: Option<&str>,
    origin: Option<&str>,
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
        "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    ));
    format!("POST {path} HTTP/1.1\r\n{headers}")
}

fn create_body() -> String {
    serde_json::json!({
        "orderId": "order-1",
        "expectedVersion": 1,
        "ownerId": "owner",
        "dueDate": "2026-09-22",
        "nextAction": "Call the customer about installation requirements"
    })
    .to_string()
}

#[tokio::test]
async fn actual_router_forwards_create_and_change_commands() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = customer_followups_router(vec![target(seen.clone())]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, text) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-followups",
            "acme.example.test",
            Some("followup-key-0001"),
            Some("https://acme.example.test"),
            &create_body(),
        ),
    )
    .await;
    assert_eq!(status, 200, "{text}");
    assert_eq!(body["data"]["id"], "followup-followup-key-0001");
    assert_eq!(body["data"]["status"], "open");
    assert_eq!(body["data"]["version"], 1);

    let (status, body, text) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-followups/followup-followup-key-0001",
            "acme.example.test",
            Some("followup-key-0002"),
            None,
            &serde_json::json!({
                "action": "reassign",
                "expectedVersion": 1,
                "ownerId": "manager",
                "dueDate": "2026-09-23",
                "reason": "Retain the original customer promise under a new owner"
            })
            .to_string(),
        ),
    )
    .await;
    assert_eq!(status, 200, "{text}");
    assert_eq!(body["data"]["status"], "open");
    assert_eq!(body["data"]["version"], 2);

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-followups/followup-followup-key-0001",
            "acme.example.test",
            Some("followup-key-0003"),
            None,
            &serde_json::json!({
                "action": "complete",
                "expectedVersion": 2,
                "reason": "Customer confirmed the installation outcome"
            })
            .to_string(),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "completed");

    assert_eq!(seen.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn actual_router_denies_origin_key_and_invalid_bodies() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = customer_followups_router(vec![target(seen)]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-followups",
            "acme.example.test",
            Some("followup-key-0004"),
            Some("https://untrusted.example.test"),
            &create_body(),
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-followups",
            "acme.example.test",
            None,
            None,
            &create_body(),
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");

    for invalid in [
        serde_json::json!({"orderId": "order-1", "expectedVersion": 2, "ownerId": "owner", "dueDate": "2026-09-22", "nextAction": "Call the customer"}).to_string(),
        serde_json::json!({"orderId": "order-1", "expectedVersion": 1, "ownerId": "owner", "dueDate": "2026-02-30", "nextAction": "Call the customer"}).to_string(),
        serde_json::json!({"orderId": "order-1", "expectedVersion": 1, "ownerId": "owner", "dueDate": "2026-09-22", "nextAction": "x"}).to_string(),
        serde_json::json!({"orderId": "order-1", "expectedVersion": 1, "ownerId": "owner", "dueDate": "2026-09-22", "nextAction": "Call the customer", "extra": true}).to_string(),
    ] {
        let (status, _, text) = http(
            addr,
            post(
                "/api/v1/holdings/acme/customer-followups",
                "acme.example.test",
                Some("followup-key-0005"),
                None,
                &invalid,
            ),
        )
        .await;
        assert_eq!(status, 422, "{text}");
    }

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-followups/followup-1",
            "acme.example.test",
            Some("followup-key-0006"),
            None,
            &serde_json::json!({"action": "reassign", "expectedVersion": 1, "reason": "Missing owner and date"}).to_string(),
        ),
    )
    .await;
    assert_eq!(status, 422);
}
