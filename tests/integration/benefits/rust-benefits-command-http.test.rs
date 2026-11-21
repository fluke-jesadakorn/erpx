//! Actual owner-local Benefits command HTTP router tests. These prove ingress
//! facts only; composition, Account session and PostgreSQL remain separate.
use erpx_server_benefits::public::command_http::{
    BenefitsCommandHttpBackend, BenefitsCommandHttpFuture, BenefitsCommandHttpRequest,
    BenefitsCommandHttpResponse, BenefitsCommandHttpTarget, benefits_command_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Eq, PartialEq)]
struct Seen {
    host: String,
    holding: String,
    cookie: Option<String>,
    key: String,
    origin: Option<String>,
    body: serde_json::Value,
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl BenefitsCommandHttpBackend for Probe {
    fn command<'a>(
        &'a self,
        request: BenefitsCommandHttpRequest<'a>,
    ) -> BenefitsCommandHttpFuture<'a, BenefitsCommandHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                host: request.host.into(),
                holding: request.holding_slug.into(),
                cookie: request
                    .headers
                    .get("cookie")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned),
                key: request.key.as_str().into(),
                origin: request
                    .headers
                    .get("origin")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned),
                body: request.body.clone(),
            });
            Ok(BenefitsCommandHttpResponse {
                data: serde_json::json!({
                    "id": "benefit-plan-plan-command-1",
                    "status": "draft",
                    "version": 1
                }),
                context: serde_json::json!({
                    "userId": "user",
                    "companyId": "company",
                    "workContext": "a".repeat(64)
                }),
            })
        })
    }
}

struct Conflict;

impl BenefitsCommandHttpBackend for Conflict {
    fn command<'a>(
        &'a self,
        _: BenefitsCommandHttpRequest<'a>,
    ) -> BenefitsCommandHttpFuture<'a, BenefitsCommandHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different Benefits request.",
            ))
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Probe {
        seen: Arc::new(Mutex::new(Vec::new())),
    });
    let target = || BenefitsCommandHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(benefits_command_http_router(vec![target()]).is_ok());
    assert!(benefits_command_http_router(vec![target(), target()]).is_err());
    assert!(
        benefits_command_http_router(vec![BenefitsCommandHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        benefits_command_http_router(vec![BenefitsCommandHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        benefits_command_http_router(vec![BenefitsCommandHttpTarget {
            holding_slug: "Acme".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        benefits_command_http_router(vec![BenefitsCommandHttpTarget {
            host: String::new(),
            ..target()
        }])
        .is_err()
    );
    assert!(benefits_command_http_router(Vec::new()).is_err());
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

fn post(path: &str, host: &str, origin: Option<&str>, key: Option<&str>, body: &str) -> String {
    let mut request = format!("POST {path} HTTP/1.1\r\nHost: {host}\r\n");
    if let Some(origin) = origin {
        request.push_str(&format!("Origin: {origin}\r\n"));
    }
    if let Some(key) = key {
        request.push_str(&format!("Idempotency-Key: {key}\r\n"));
    }
    request.push_str("Content-Type: application/json\r\n");
    request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    request.push_str("Connection: close\r\n\r\n");
    request.push_str(body);
    request
}

#[tokio::test]
async fn actual_command_router_forwards_exact_scope_key_body_and_maps_business_errors() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = benefits_command_http_router(vec![
        BenefitsCommandHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe { seen: seen.clone() }),
        },
        BenefitsCommandHttpTarget {
            host: "conflict.example.test".into(),
            holding_slug: "conflict".into(),
            origin: "https://conflict.example.test".into(),
            backend: Arc::new(Conflict),
        },
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    let body = r#"{"entity":"plan","name":"Health Plus","provider":"Synthetic Provider","coverageType":"family","currency":"THB","employeeDeduction":"125.50","employerCost":"300.00","effectiveFrom":"2026-09-01"}"#;

    let (status, reply, raw) = http(
        addr,
        post(
            "/api/v1/holdings/acme/benefits",
            "acme.example.test",
            Some("https://acme.example.test"),
            Some("plan-command-1"),
            body,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(reply["data"]["status"], "draft");
    assert_eq!(reply["data"]["version"], 1);
    assert_eq!(reply["context"]["companyId"], "company");
    assert!(raw.contains("cache-control: private, no-store"));
    assert!(raw.contains("referrer-policy: no-referrer"));
    assert!(raw.contains("x-content-type-options: nosniff"));
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            host: "acme.example.test".into(),
            holding: "acme".into(),
            cookie: None,
            key: "plan-command-1".into(),
            origin: Some("https://acme.example.test".into()),
            body: serde_json::from_str(body).unwrap(),
        })
    );

    let (status, reply, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/benefits",
            "acme.example.test",
            Some("https://evil.example.test"),
            Some("plan-command-2"),
            body,
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(reply["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, reply, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/benefits",
            "acme.example.test",
            None,
            Some("plan-command-3"),
            body,
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(reply["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, reply, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/benefits",
            "acme.example.test",
            Some("https://acme.example.test"),
            None,
            body,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(reply["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, reply, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/benefits",
            "acme.example.test",
            Some("https://acme.example.test"),
            Some("short"),
            body,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(reply["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, reply, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/benefits",
            "acme.example.test",
            Some("https://acme.example.test"),
            Some("plan-command-4"),
            "{not-json",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(reply["error"]["code"], "BENEFITS_COMMAND_REQUEST_INVALID");

    let (status, reply, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/benefits",
            "acme.example.test",
            Some("https://acme.example.test"),
            Some("plan-command-5"),
            "[]",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(reply["error"]["code"], "BENEFITS_COMMAND_REQUEST_INVALID");

    let (status, reply, _) = http(
        addr,
        post(
            "/api/v1/holdings/other/benefits",
            "acme.example.test",
            Some("https://acme.example.test"),
            Some("plan-command-6"),
            body,
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(reply["error"]["code"], "NOT_FOUND");

    let (status, reply, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/benefits?view=mine",
            "acme.example.test",
            Some("https://acme.example.test"),
            Some("plan-command-7"),
            body,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(reply["error"]["code"], "BENEFITS_COMMAND_REQUEST_INVALID");

    let (status, reply, _) = http(
        addr,
        post(
            "/api/v1/holdings/conflict/benefits",
            "conflict.example.test",
            Some("https://conflict.example.test"),
            Some("plan-command-8"),
            body,
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(reply["error"]["code"], "IDEMPOTENCY_CONFLICT");

    assert_eq!(
        seen.lock().unwrap().len(),
        1,
        "failed ingress requests must not reach the owner backend"
    );
    server.abort();
}
