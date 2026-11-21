//! Actual owner-local HTTP router tests for the native Manufacturing
//! specification create/review commands.
use erpx_server_manufacturing::public::bom_http::{
    BomHttpBackend, BomHttpFuture, BomHttpRequest, BomHttpResponse, BomHttpTarget, bom_router,
};
use erpx_server_manufacturing::public::specification::{
    SpecificationHttpBackend, SpecificationHttpCommand, SpecificationHttpFuture,
    SpecificationHttpRequest, SpecificationHttpResponse, SpecificationHttpTarget,
    specification_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl SpecificationHttpBackend for Noop {
    fn execute<'a>(
        &'a self,
        _: SpecificationHttpRequest<'a>,
    ) -> SpecificationHttpFuture<'a, SpecificationHttpResponse> {
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

impl SpecificationHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: SpecificationHttpRequest<'a>,
    ) -> SpecificationHttpFuture<'a, SpecificationHttpResponse> {
        Box::pin(async move {
            let label = match &request.command {
                SpecificationHttpCommand::Create {
                    idempotency_key, ..
                } => {
                    format!("create:{}:{idempotency_key}", request.holding_slug)
                }
                SpecificationHttpCommand::Review {
                    specification_id,
                    idempotency_key,
                    ..
                } => format!(
                    "review:{}:{specification_id}:{idempotency_key}",
                    request.holding_slug
                ),
            };
            self.seen.lock().unwrap().push(label);
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "CONFLICT",
                    "Synthetic review conflict.",
                ));
            }
            Ok(SpecificationHttpResponse {
                data: serde_json::json!({
                    "id": "mfg-spec-manufacturing-key-0001",
                    "status": "draft",
                    "version": 1
                }),
                context: serde_json::json!({"userId": "maker"}),
            })
        })
    }
}

struct NoopBom;

impl BomHttpBackend for NoopBom {
    fn read<'a>(&'a self, _: BomHttpRequest<'a>) -> BomHttpFuture<'a, BomHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
}

fn target(backend: Arc<dyn SpecificationHttpBackend>) -> SpecificationHttpTarget {
    SpecificationHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

#[test]
fn target_configuration_requires_distinct_exact_https_holdings() {
    let backend = Arc::new(Noop);
    assert!(specification_router(vec![target(backend.clone())]).is_ok());
    assert!(specification_router(vec![target(backend.clone()), target(backend.clone())]).is_err());
    assert!(
        specification_router(vec![SpecificationHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target(backend.clone())
        }])
        .is_err()
    );
    assert!(
        specification_router(vec![SpecificationHttpTarget {
            holding_slug: "Acme".into(),
            ..target(backend.clone())
        }])
        .is_err()
    );
    assert!(specification_router(vec![]).is_ok());
}

#[test]
fn command_router_merges_with_the_bom_register_read_on_the_same_path() {
    let commands = specification_router(vec![target(Arc::new(Noop))]).unwrap();
    let read = bom_router(vec![BomHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(NoopBom),
    }])
    .unwrap();
    // Axum merges the GET register read and the POST command at one path; a
    // duplicate method would panic here instead of at the composition mount.
    let _merged = commands.merge(read);
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
        text.to_ascii_lowercase(),
    )
}

fn post(path: &str, host: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut raw = format!("POST {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    for (name, value) in headers {
        raw.push_str(&format!("{name}: {value}\r\n"));
    }
    raw.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    raw
}

#[tokio::test]
async fn actual_router_retains_target_origin_idempotency_and_error_mapping() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(Probe {
        seen: seen.clone(),
        fail: false,
    });
    let failing = Arc::new(Probe {
        seen: seen.clone(),
        fail: true,
    });
    let router = specification_router(vec![
        target(probe),
        SpecificationHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: failing,
        },
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let json = [("Content-Type", "application/json")];
    let keyed = [
        ("Content-Type", "application/json"),
        ("Idempotency-Key", "manufacturing-key-0001"),
        ("Origin", "https://acme.example.test"),
    ];
    let create_body = r#"{"name":"Bounded assembly","mode":"bom","outputProductId":"output","batchQuantity":"10","expectedYieldBps":9800,"lines":[{"productId":"material","quantity":"1"}]}"#;

    let (status, body, headers) = http(
        addr,
        post(
            "/api/v1/holdings/acme/manufacturing/specifications",
            "acme.example.test",
            &keyed,
            create_body,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "draft");
    assert_eq!(body["context"]["userId"], "maker");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/manufacturing/specifications",
            "acme.example.test",
            &json,
            create_body,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/manufacturing/specifications",
            "acme.example.test",
            &[
                ("Content-Type", "application/json"),
                ("Idempotency-Key", "manufacturing-key-0002"),
                ("Origin", "https://other.example.test"),
            ],
            create_body,
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/manufacturing/specifications?view=all",
            "acme.example.test",
            &keyed,
            create_body,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "MANUFACTURING_REQUEST_INVALID");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/missing/manufacturing/specifications",
            "acme.example.test",
            &keyed,
            create_body,
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, _body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/manufacturing/specifications",
            "acme.example.test",
            &[
                ("Content-Type", "text/plain"),
                ("Idempotency-Key", "manufacturing-key-0003"),
            ],
            create_body,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/manufacturing/specifications/mfg-spec-1",
            "acme.example.test",
            &keyed,
            r#"{"action":"approve","expectedVersion":1,"reason":"Independent current review."}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["id"], "mfg-spec-manufacturing-key-0001");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/manufacturing/specifications/mfg-spec-1",
            "acme.example.test",
            &keyed,
            r#"{"action":"cancel","expectedVersion":1,"reason":"Independent current review."}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "MANUFACTURING_REQUEST_INVALID");

    let too_long = format!("spec-{}", "a".repeat(230));
    let (status, body, _) = http(
        addr,
        post(
            &format!("/api/v1/holdings/acme/manufacturing/specifications/{too_long}"),
            "acme.example.test",
            &keyed,
            r#"{"action":"retire","expectedVersion":2,"reason":"Independent current review."}"#,
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/fail/manufacturing/specifications/mfg-spec-1",
            "fail.example.test",
            &[
                ("Content-Type", "application/json"),
                ("Idempotency-Key", "manufacturing-key-0004"),
                ("Origin", "https://fail.example.test"),
            ],
            r#"{"action":"approve","expectedVersion":1,"reason":"Independent current review."}"#,
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "CONFLICT");

    assert_eq!(
        seen.lock().unwrap().clone(),
        vec![
            "create:acme:manufacturing-key-0001",
            "review:acme:mfg-spec-1:manufacturing-key-0001",
            "review:fail:mfg-spec-1:manufacturing-key-0004",
        ]
    );
    server.abort();
}
