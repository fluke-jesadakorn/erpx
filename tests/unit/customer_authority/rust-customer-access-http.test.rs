//! Actual owner-local HTTP router tests for the native customer access grant.
use erpx_server_customer_authority::public::access_http::{
    CustomerAccessHttpBackend, CustomerAccessHttpCommand, CustomerAccessHttpFuture,
    CustomerAccessHttpRequest, CustomerAccessHttpResponse, CustomerAccessHttpTarget,
    customer_access_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl CustomerAccessHttpBackend for Noop {
    fn execute<'a>(
        &'a self,
        _: CustomerAccessHttpRequest<'a>,
    ) -> CustomerAccessHttpFuture<'a, CustomerAccessHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
}

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl CustomerAccessHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: CustomerAccessHttpRequest<'a>,
    ) -> CustomerAccessHttpFuture<'a, CustomerAccessHttpResponse> {
        Box::pin(async move {
            let seen = match &request.command {
                CustomerAccessHttpCommand::Workspace => "workspace".to_owned(),
                CustomerAccessHttpCommand::Detail { binding_id } => {
                    format!("detail|{binding_id}")
                }
                CustomerAccessHttpCommand::Request {
                    body,
                    idempotency_key,
                } => format!(
                    "request|{idempotency_key}|{}",
                    body["email"].as_str().unwrap_or("")
                ),
                CustomerAccessHttpCommand::Change {
                    binding_id,
                    body,
                    idempotency_key,
                } => format!(
                    "change|{binding_id}|{idempotency_key}|{}",
                    body["action"].as_str().unwrap_or("")
                ),
                CustomerAccessHttpCommand::Invitation {
                    binding_id,
                    body,
                    idempotency_key,
                } => format!(
                    "invitation|{binding_id}|{idempotency_key}|{}",
                    body["action"].as_str().unwrap_or("")
                ),
                CustomerAccessHttpCommand::InvitationHistory { binding_id } => {
                    format!("invitation-history|{binding_id}")
                }
            };
            self.seen.lock().unwrap().push(seen);
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "CUSTOMER_ACCESS_CHANGED",
                    "Synthetic changed customer access source.",
                ));
            }
            Ok(CustomerAccessHttpResponse {
                data: match request.command {
                    CustomerAccessHttpCommand::Workspace => {
                        serde_json::json!({"rows": [], "sources": []})
                    }
                    CustomerAccessHttpCommand::Detail { .. } => serde_json::json!({
                        "binding": {"id": "customer-access-review-1"},
                        "events": [],
                        "reads": []
                    }),
                    CustomerAccessHttpCommand::InvitationHistory { .. } => serde_json::json!({
                        "invitations": [],
                        "invitationEvents": []
                    }),
                    CustomerAccessHttpCommand::Request { .. }
                    | CustomerAccessHttpCommand::Change { .. }
                    | CustomerAccessHttpCommand::Invitation { .. } => serde_json::json!({
                        "id": "customer-access-review-1",
                        "status": "pending",
                        "version": 1
                    }),
                },
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || CustomerAccessHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(customer_access_http_router(vec![target()]).is_ok());
    assert!(
        customer_access_http_router(vec![CustomerAccessHttpTarget {
            origin: "https://acme.example.test:3443".into(),
            ..target()
        }])
        .is_ok()
    );
    assert!(customer_access_http_router(vec![target(), target()]).is_err());
    for origin in [
        "http://acme.example.test",
        "https://acme.example.test:0",
        "https://acme.example.test:65536",
        "https://acme.example.test:abc",
        "https://acme.example.test:3443:1",
    ] {
        assert!(
            customer_access_http_router(vec![CustomerAccessHttpTarget {
                origin: origin.into(),
                ..target()
            }])
            .is_err()
        );
    }
    assert!(
        customer_access_http_router(vec![CustomerAccessHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
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

fn get(path: &str, host: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
}

fn post(path: &str, host: &str, key: Option<&str>, body: &str) -> String {
    let key = key
        .map(|value| format!("Idempotency-Key: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n{key}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[tokio::test]
async fn actual_router_forwards_access_reads_and_commands() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = customer_access_http_router(vec![
        CustomerAccessHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: false,
            }),
        },
        CustomerAccessHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: true,
            }),
        },
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, headers) = http(
        addr,
        get("/api/v1/holdings/acme/customer-access", "acme.example.test"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["rows"], serde_json::json!([]));
    assert_eq!(body["context"]["userId"], "user");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customer-access/customer-access-review-1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["binding"]["id"], "customer-access-review-1");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-access",
            "acme.example.test",
            Some("customer-access-command-1"),
            r#"{"orderId":"order-1","sourceVersion":3,"email":"customer@example.test","expiresAt":1800086400000,"reason":"Verified source"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "pending");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-access/customer-access-review-1",
            "acme.example.test",
            Some("customer-access-command-2"),
            r#"{"action":"approve","expectedVersion":1,"reason":"Independent review"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["id"], "customer-access-review-1");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/fail/customer-access",
            "fail.example.test",
            Some("customer-access-command-3"),
            r#"{"orderId":"order-1","sourceVersion":3,"email":"customer@example.test","expiresAt":1800086400000,"reason":"Verified source"}"#,
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "CUSTOMER_ACCESS_CHANGED");

    assert_eq!(
        seen.lock().unwrap().clone(),
        vec![
            "workspace",
            "detail|customer-access-review-1",
            "request|customer-access-command-1|customer@example.test",
            "change|customer-access-review-1|customer-access-command-2|approve",
            "request|customer-access-command-3|customer@example.test",
        ]
    );

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customer-access?unknown=1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "CUSTOMER_ACCESS_REQUEST_INVALID");

    let (status, _, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customer-access?unknown=1&unknown=2",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customer-access",
            "unknown.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, _, _) = http(
        addr,
        get(
            "/api/v1/holdings/unknown/customer-access",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-access",
            "acme.example.test",
            None,
            r#"{"orderId":"order-1"}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-access",
            "acme.example.test",
            Some("short"),
            r#"{"orderId":"order-1"}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-access/customer-access-review-1",
            "acme.example.test",
            Some("customer-access-command-4"),
            r#"["not","an","object"]"#,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "CUSTOMER_ACCESS_REQUEST_INVALID");

    let (status, body, _) = http(
        addr,
        "POST /api/v1/holdings/acme/customer-access HTTP/1.1\r\nHost: acme.example.test\r\nContent-Type: text/plain\r\nIdempotency-Key: customer-access-command-5\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string(),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "CUSTOMER_ACCESS_REQUEST_INVALID");

    let (status, _, _) = http(
        addr,
        "GET /api/v1/holdings/acme/customer-access HTTP/1.1\r\nHost: acme.example.test\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string(),
    )
    .await;
    assert_eq!(status, 422);

    server.abort();
}
