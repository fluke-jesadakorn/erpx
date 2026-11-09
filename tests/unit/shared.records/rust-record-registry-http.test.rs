//! Actual owner-local HTTP router tests for the native Records registry read.
use erpx_server_records::public::record_registry_http::{
    RecordsRegistryHttpBackend, RecordsRegistryHttpCommand, RecordsRegistryHttpFuture,
    RecordsRegistryHttpRequest, RecordsRegistryHttpResponse, RecordsRegistryHttpTarget,
    records_registry_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl RecordsRegistryHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: RecordsRegistryHttpRequest<'a>,
    ) -> RecordsRegistryHttpFuture<'a, RecordsRegistryHttpResponse> {
        Box::pin(async move {
            let cookie = request
                .headers
                .get("cookie")
                .and_then(|value| value.to_str().ok());
            let (detail, data) = match &request.command {
                RecordsRegistryHttpCommand::List {
                    module_id,
                    section_id,
                } => (
                    format!("list|{module_id}|{section_id}"),
                    serde_json::json!([]),
                ),
                RecordsRegistryHttpCommand::Page {
                    module_id,
                    section_id,
                    q,
                    cursor,
                } => (
                    format!(
                        "page|{module_id}|{section_id}|{}|{}",
                        q.as_deref().unwrap_or_default(),
                        cursor.as_deref().unwrap_or_default()
                    ),
                    serde_json::json!({
                        "items": [],
                        "nextCursor": null,
                        "previousCursor": null,
                        "hasMore": false
                    }),
                ),
                RecordsRegistryHttpCommand::Create {
                    module_id,
                    input,
                    key,
                } => (
                    format!("create|{module_id}|{key}|{}", input.section_id),
                    serde_json::json!({"id": format!("rec-{module_id}-{key}")}),
                ),
            };
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{}|{cookie:?}",
                request.host, request.holding_slug, detail
            ));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "RECORDS_REGISTRY_SOURCE_INVALID",
                    "Synthetic registry failure.",
                ));
            }
            Ok(RecordsRegistryHttpResponse {
                data,
                context: serde_json::json!({"userId": "actor"}),
            })
        })
    }
}

struct Noop;

impl RecordsRegistryHttpBackend for Noop {
    fn execute<'a>(
        &'a self,
        _: RecordsRegistryHttpRequest<'a>,
    ) -> RecordsRegistryHttpFuture<'a, RecordsRegistryHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || RecordsRegistryHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(records_registry_http_router(vec![target()]).is_ok());
    assert!(records_registry_http_router(vec![target(), target()]).is_err());
    assert!(
        records_registry_http_router(vec![RecordsRegistryHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        records_registry_http_router(vec![RecordsRegistryHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        records_registry_http_router(vec![RecordsRegistryHttpTarget {
            holding_slug: "Acme".into(),
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

fn post(path: &str, host: &str, origin: &str, key: &str, body: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nCookie: erpx=session\r\nIdempotency-Key: {key}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn get(path: &str, host: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nCookie: erpx=session\r\nConnection: close\r\n\r\n"
    )
}

fn target(
    host: &str,
    slug: &str,
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
) -> RecordsRegistryHttpTarget {
    RecordsRegistryHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: format!("https://{host}"),
        backend: Arc::new(Probe { seen, fail }),
    }
}

#[tokio::test]
async fn actual_router_forwards_list_and_page_queries_with_private_headers() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = records_registry_http_router(vec![
        target("acme.example.test", "acme", seen.clone(), false),
        target("fail.example.test", "fail", seen.clone(), true),
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, headers) = http(
        addr,
        get(
            "/api/v1/holdings/acme/records/operations?section=overview&paged=1&q=needle+review&cursor=abc",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["items"], serde_json::json!([]));
    assert_eq!(body["context"]["userId"], "actor");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/records/operations?section=overview",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"], serde_json::json!([]));

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/fail/records/operations?section=overview&paged=1",
            "fail.example.test",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "RECORDS_REGISTRY_SOURCE_INVALID");

    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            "acme.example.test|acme|page|operations|overview|needle review|abc|Some(\"erpx=session\")",
            "acme.example.test|acme|list|operations|overview|Some(\"erpx=session\")",
            "fail.example.test|fail|page|operations|overview|||Some(\"erpx=session\")",
        ]
    );
}

#[tokio::test]
async fn invalid_queries_bodies_hosts_and_paths_never_reach_the_backend() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = records_registry_http_router(vec![target(
        "acme.example.test",
        "acme",
        seen.clone(),
        false,
    )])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    for (path, host, expected_status, expected_code) in [
        (
            "/api/v1/holdings/acme/records/operations",
            "acme.example.test",
            422,
            "RECORDS_REGISTRY_REQUEST_INVALID",
        ),
        (
            "/api/v1/holdings/acme/records/operations?section=overview&paged=2",
            "acme.example.test",
            422,
            "RECORDS_REGISTRY_REQUEST_INVALID",
        ),
        (
            "/api/v1/holdings/acme/records/operations?section=overview&q=needle",
            "acme.example.test",
            422,
            "RECORDS_REGISTRY_REQUEST_INVALID",
        ),
        (
            "/api/v1/holdings/acme/records/operations?section=overview&section=drafts",
            "acme.example.test",
            422,
            "RECORDS_REGISTRY_REQUEST_INVALID",
        ),
        (
            "/api/v1/holdings/acme/records/operations?section=overview&paged=1&extra=1",
            "acme.example.test",
            422,
            "RECORDS_REGISTRY_REQUEST_INVALID",
        ),
        (
            "/api/v1/holdings/acme/records/operations?section=overview&paged=1&q=%00",
            "acme.example.test",
            422,
            "RECORDS_REGISTRY_REQUEST_INVALID",
        ),
        (
            "/api/v1/holdings/acme/records/operations?section=overview&paged=1&cursor=",
            "acme.example.test",
            422,
            "INVALID_CURSOR",
        ),
        (
            "/api/v1/holdings/acme/records/Operations?section=overview",
            "acme.example.test",
            404,
            "NOT_FOUND",
        ),
        (
            "/api/v1/holdings/acme/records/operations?section=overview",
            "unknown.example.test",
            404,
            "NOT_FOUND",
        ),
        (
            "/api/v1/holdings/other/records/operations?section=overview",
            "acme.example.test",
            404,
            "NOT_FOUND",
        ),
    ] {
        let (status, body, _) = http(addr, get(path, host)).await;
        assert_eq!(status, expected_status, "{path} on {host}");
        if !expected_code.is_empty() {
            assert_eq!(body["error"]["code"], expected_code, "{path} on {host}");
        }
    }

    for (path, expected_code) in [(
        format!(
            "/api/v1/holdings/acme/records/operations?section=overview&paged=1&q={}",
            "x".repeat(201)
        ),
        "RECORDS_REGISTRY_REQUEST_INVALID",
    )] {
        let (status, body, _) = http(addr, get(&path, "acme.example.test")).await;
        assert_eq!(status, 422, "{path}");
        assert_eq!(body["error"]["code"], expected_code, "{path}");
    }

    let raw = "GET /api/v1/holdings/acme/records/operations?section=overview HTTP/1.1\r\nHost: acme.example.test\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
    let (status, body, _) = http(addr, raw.to_owned()).await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "RECORDS_REGISTRY_REQUEST_INVALID");
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn actual_router_forwards_create_with_origin_idempotency_and_body() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = records_registry_http_router(vec![target(
        "acme.example.test",
        "acme",
        seen.clone(),
        false,
    )])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    let body = r#"{"sectionId":"tickets","title":"Native record","description":"","fields":[{"key":"priority","label":"Priority","type":"text","value":"high"}]}"#;
    let (status, response, headers) = http(
        addr,
        post(
            "/api/v1/holdings/acme/records/support",
            "acme.example.test",
            "https://acme.example.test",
            "records:http-1",
            body,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(response["data"]["id"], "rec-support-records:http-1");
    assert!(headers.contains("cache-control: private, no-store"));
    assert_eq!(
        *seen.lock().unwrap(),
        vec!["acme.example.test|acme|create|support|records:http-1|tickets|Some(\"erpx=session\")"]
    );
    let (status, response, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/records/support",
            "acme.example.test",
            "https://foreign.example.test",
            "records:http-2",
            body,
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(response["error"]["code"], "ORIGIN_FORBIDDEN");
    let (status, response, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/records/support",
            "acme.example.test",
            "https://acme.example.test",
            "short",
            body,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(response["error"]["code"], "IDEMPOTENCY_KEY_REQUIRED");
    let (status, response, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/records/support",
            "acme.example.test",
            "https://acme.example.test",
            "records:http-3",
            "{",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(
        response["error"]["code"],
        "RECORDS_REGISTRY_REQUEST_INVALID"
    );
}
