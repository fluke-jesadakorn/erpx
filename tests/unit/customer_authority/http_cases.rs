//! Owner-local HTTP router tests for native duplicate-review and identity ingress.
use erpx_server_customer_authority::public::duplicates_http::{
    CustomerDuplicateHttpBackend, CustomerDuplicateHttpCommand, CustomerDuplicateHttpFuture,
    CustomerDuplicateHttpRequest, CustomerDuplicateHttpResponse, CustomerDuplicateHttpTarget,
    customer_duplicate_http_router,
};
use erpx_server_customer_authority::public::identity_http::{
    CustomerIdentityHttpBackend, CustomerIdentityHttpCommand, CustomerIdentityHttpFuture,
    CustomerIdentityHttpRequest, CustomerIdentityHttpResponse, CustomerIdentityHttpTarget,
    customer_identity_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct DuplicateProbe {
    seen: Arc<Mutex<Vec<String>>>,
}

impl CustomerDuplicateHttpBackend for DuplicateProbe {
    fn execute<'a>(
        &'a self,
        request: CustomerDuplicateHttpRequest<'a>,
    ) -> CustomerDuplicateHttpFuture<'a, CustomerDuplicateHttpResponse> {
        Box::pin(async move {
            let seen = match &request.command {
                CustomerDuplicateHttpCommand::Workspace => "workspace".to_owned(),
                CustomerDuplicateHttpCommand::Detail { review_id } => {
                    format!("detail|{review_id}")
                }
                CustomerDuplicateHttpCommand::Request {
                    idempotency_key, ..
                } => format!("request|{idempotency_key}"),
                CustomerDuplicateHttpCommand::Change {
                    review_id,
                    idempotency_key,
                    ..
                } => format!("change|{review_id}|{idempotency_key}"),
            };
            self.seen.lock().unwrap().push(seen);
            let data = match &request.command {
                CustomerDuplicateHttpCommand::Workspace => {
                    serde_json::json!({"rows": [], "customers": [], "canCreate": true})
                }
                CustomerDuplicateHttpCommand::Detail { .. } => {
                    serde_json::json!({"row": {"id": "customer-duplicate-1"}, "history": []})
                }
                CustomerDuplicateHttpCommand::Request { .. } => {
                    serde_json::json!({"id": "customer-duplicate-1", "status": "pending", "version": 1})
                }
                CustomerDuplicateHttpCommand::Change { .. } => {
                    serde_json::json!({"id": "customer-duplicate-1", "status": "duplicate", "version": 2})
                }
            };
            Ok(CustomerDuplicateHttpResponse {
                data,
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

struct IdentityProbe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl CustomerIdentityHttpBackend for IdentityProbe {
    fn execute<'a>(
        &'a self,
        request: CustomerIdentityHttpRequest<'a>,
    ) -> CustomerIdentityHttpFuture<'a, CustomerIdentityHttpResponse> {
        Box::pin(async move {
            let seen = match &request.command {
                CustomerIdentityHttpCommand::Workspace => "workspace".to_owned(),
                CustomerIdentityHttpCommand::Detail { change_id } => format!("detail|{change_id}"),
                CustomerIdentityHttpCommand::Request {
                    idempotency_key, ..
                } => format!("request|{idempotency_key}"),
                CustomerIdentityHttpCommand::Change {
                    change_id,
                    idempotency_key,
                    ..
                } => format!("change|{change_id}|{idempotency_key}"),
            };
            self.seen.lock().unwrap().push(seen);
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "CUSTOMER_IDENTITY_CHANGED",
                    "Synthetic changed identity source.",
                ));
            }
            let data = match &request.command {
                CustomerIdentityHttpCommand::Workspace => {
                    serde_json::json!({"rows": [], "customers": [], "canCreate": true})
                }
                CustomerIdentityHttpCommand::Detail { .. } => {
                    serde_json::json!({"row": {"id": "customer-identity-1"}, "history": []})
                }
                CustomerIdentityHttpCommand::Request { .. } => {
                    serde_json::json!({"id": "customer-identity-1", "status": "pending", "version": 1})
                }
                CustomerIdentityHttpCommand::Change { .. } => {
                    serde_json::json!({"id": "customer-identity-1", "status": "applied", "version": 2})
                }
            };
            Ok(CustomerIdentityHttpResponse {
                data,
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(DuplicateProbe {
        seen: Arc::new(Mutex::new(Vec::new())),
    });
    let target = || CustomerDuplicateHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(customer_duplicate_http_router(vec![target()]).is_ok());
    assert!(customer_duplicate_http_router(vec![target(), target()]).is_err());
    for origin in [
        "http://acme.example.test",
        "https://acme.example.test:0",
        "https://acme.example.test/path",
    ] {
        assert!(
            customer_duplicate_http_router(vec![CustomerDuplicateHttpTarget {
                origin: origin.into(),
                ..target()
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
async fn duplicate_router_forwards_reads_and_commands() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = customer_duplicate_http_router(vec![CustomerDuplicateHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(DuplicateProbe { seen: seen.clone() }),
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, headers) = http(
        addr,
        get("/api/v1/holdings/acme/customer-duplicates", "acme.example.test"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["canCreate"], true);
    assert_eq!(body["context"]["userId"], "user");
    assert!(headers.contains("cache-control: private, no-store"));

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customer-duplicates?id=customer-duplicate-1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["row"]["id"], "customer-duplicate-1");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-duplicates",
            "acme.example.test",
            Some("customer-duplicate-command-1"),
            r#"{"leftCustomerId":"customer-a","rightCustomerId":"customer-b","evidence":"Matching evidence"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "pending");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-duplicates",
            "acme.example.test",
            Some("customer-duplicate-command-2"),
            r#"{"id":"customer-duplicate-1","action":"duplicate","expectedVersion":1,"canonicalCustomerId":"customer-a","reason":"Independent review"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "duplicate");

    assert_eq!(
        seen.lock().unwrap().clone(),
        vec![
            "workspace",
            "detail|customer-duplicate-1",
            "request|customer-duplicate-command-1",
            "change|customer-duplicate-1|customer-duplicate-command-2",
        ]
    );

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customer-duplicates?unknown=1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "CUSTOMER_DUPLICATE_REQUEST_INVALID");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-duplicates",
            "acme.example.test",
            None,
            r#"{}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-duplicates",
            "acme.example.test",
            Some("customer-duplicate-command-3"),
            r#"["not","an","object"]"#,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "CUSTOMER_DUPLICATE_REQUEST_INVALID");

    let (status, body, _) = http(
        addr,
        get("/api/v1/holdings/acme/customer-duplicates", "unknown.example.test"),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    server.abort();
}

#[tokio::test]
async fn identity_router_forwards_reads_commands_and_owner_errors() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = customer_identity_http_router(vec![
        CustomerIdentityHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(IdentityProbe {
                seen: seen.clone(),
                fail: false,
            }),
        },
        CustomerIdentityHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(IdentityProbe {
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

    let (status, body, _) = http(
        addr,
        get("/api/v1/holdings/acme/customer-identity", "acme.example.test"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["canCreate"], true);

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customer-identity?id=customer-identity-1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["row"]["id"], "customer-identity-1");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-identity",
            "acme.example.test",
            Some("customer-identity-command-1"),
            r#"{"customerId":"customer-1","expectedCustomerVersion":1,"proposed":{"name":"Reviewed name"},"evidence":"Customer evidence"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "pending");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-identity",
            "acme.example.test",
            Some("customer-identity-command-2"),
            r#"{"id":"customer-identity-1","action":"apply","expectedVersion":1,"reason":"Independent review"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "applied");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/fail/customer-identity",
            "fail.example.test",
            Some("customer-identity-command-3"),
            r#"{"id":"customer-identity-1","action":"apply","expectedVersion":1,"reason":"Independent review"}"#,
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "CUSTOMER_IDENTITY_CHANGED");

    assert_eq!(
        seen.lock().unwrap().clone(),
        vec![
            "workspace",
            "detail|customer-identity-1",
            "request|customer-identity-command-1",
            "change|customer-identity-1|customer-identity-command-2",
            "change|customer-identity-1|customer-identity-command-3",
        ]
    );

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customer-identity?id=customer-identity-1&x=1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "CUSTOMER_IDENTITY_CHANGE_NOT_FOUND");

    let (status, body, _) = http(
        addr,
        "POST /api/v1/holdings/acme/customer-identity HTTP/1.1\r\nHost: acme.example.test\r\nContent-Type: text/plain\r\nIdempotency-Key: customer-identity-command-4\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string(),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "CUSTOMER_IDENTITY_CHANGE_INVALID");

    server.abort();
}
