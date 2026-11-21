//! Actual owner-local HTTP router tests for native Position GPO commands.
use erpx_server_identity_access::public::position_gpo_http::{
    PositionGpoHttpBackend, PositionGpoHttpCommand, PositionGpoHttpFuture, PositionGpoHttpRequest,
    PositionGpoHttpResponse, PositionGpoHttpTarget, position_gpo_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl PositionGpoHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: PositionGpoHttpRequest<'a>,
    ) -> PositionGpoHttpFuture<'a, PositionGpoHttpResponse> {
        Box::pin(async move {
            let cookie = request.headers.get("cookie").and_then(|v| v.to_str().ok());
            let seen = match &request.command {
                PositionGpoHttpCommand::Overview => {
                    format!(
                        "overview|{}|{}|{cookie:?}",
                        request.host, request.holding_slug
                    )
                }
                PositionGpoHttpCommand::Preview { input } => format!(
                    "preview|{}|{}|{}|{}|{cookie:?}",
                    request.host, request.holding_slug, input.position_id, input.reason
                ),
                PositionGpoHttpCommand::Publish {
                    input,
                    idempotency_key,
                } => format!(
                    "publish|{}|{}|{}|{}|{}|{cookie:?}",
                    request.host,
                    request.holding_slug,
                    input.input.position_id,
                    input.preview_token,
                    idempotency_key
                ),
                PositionGpoHttpCommand::UnmapPreview { input } => format!(
                    "unmap-preview|{}|{}|{}|{cookie:?}",
                    request.host, request.holding_slug, input.position_id
                ),
                PositionGpoHttpCommand::Unmap {
                    input,
                    idempotency_key,
                } => format!(
                    "unmap|{}|{}|{}|{}|{cookie:?}",
                    request.host, request.holding_slug, input.input.position_id, idempotency_key
                ),
                PositionGpoHttpCommand::Retry {
                    input,
                    idempotency_key,
                } => format!(
                    "retry|{}|{}|{}|{}|{cookie:?}",
                    request.host, request.holding_slug, input.id, idempotency_key
                ),
                PositionGpoHttpCommand::RetryConfig {
                    input,
                    idempotency_key,
                } => format!(
                    "retry-config|{}|{}|{}|{}|{cookie:?}",
                    request.host, request.holding_slug, input.position_id, idempotency_key
                ),
            };
            self.seen.lock().unwrap().push(seen);
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "VERSION_CONFLICT",
                    "Synthetic changed Position GPO.",
                ));
            }
            let data = match &request.command {
                PositionGpoHttpCommand::Overview => serde_json::json!({
                    "policies": [], "jobs": [], "positions": []
                }),
                PositionGpoHttpCommand::Preview { input } => serde_json::json!({
                    "token": "a".repeat(64), "version": input.expected_version + 1
                }),
                PositionGpoHttpCommand::Publish { input, .. } => serde_json::json!({
                    "positionId": input.input.position_id, "version": 2, "status": "pending"
                }),
                PositionGpoHttpCommand::UnmapPreview { input } => serde_json::json!({
                    "positionId": input.position_id, "token": "b".repeat(64)
                }),
                PositionGpoHttpCommand::Unmap { input, .. } => serde_json::json!({
                    "positionId": input.input.position_id, "mapped": false
                }),
                PositionGpoHttpCommand::Retry { input, .. } => serde_json::json!({
                    "id": input.id, "status": "queued"
                }),
                PositionGpoHttpCommand::RetryConfig { input, .. } => serde_json::json!({
                    "positionId": input.position_id, "status": "pending"
                }),
            };
            Ok(PositionGpoHttpResponse { data })
        })
    }
}

struct Noop;

impl PositionGpoHttpBackend for Noop {
    fn execute<'a>(
        &'a self,
        _: PositionGpoHttpRequest<'a>,
    ) -> PositionGpoHttpFuture<'a, PositionGpoHttpResponse> {
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
    let target = || PositionGpoHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(position_gpo_http_router(vec![target()]).is_ok());
    assert!(position_gpo_http_router(vec![target(), target()]).is_err());
    assert!(
        position_gpo_http_router(vec![PositionGpoHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        position_gpo_http_router(vec![PositionGpoHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        position_gpo_http_router(vec![PositionGpoHttpTarget {
            holding_slug: "Acme".into(),
            ..target()
        }])
        .is_err()
    );
}

async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
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
    )
}

fn post(path: &str, host: &str, key: Option<&str>, origin: Option<&str>, body: &str) -> String {
    let key = key
        .map(|value| format!("Idempotency-Key: {value}\r\n"))
        .unwrap_or_default();
    let origin = origin
        .map(|value| format!("Origin: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nCookie: erpx=session\r\n{origin}Content-Type: application/json\r\n{key}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn target(
    host: &str,
    slug: &str,
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
) -> PositionGpoHttpTarget {
    PositionGpoHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: format!("https://{host}"),
        backend: Arc::new(Probe { seen, fail }),
    }
}

const INPUT: &str = r#"{"positionId":"position-1","expectedVersion":1,"connectionId":"connection-1","gpoId":"gpo-1","userFilterGroupId":"group-1","computerTargetOuId":"ou-1","reason":"Publish the reviewed mapping"}"#;
const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[tokio::test]
async fn actual_router_forwards_every_position_gpo_command() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = position_gpo_http_router(vec![
        target("acme.example.test", "acme", seen.clone(), false),
        target("fail.example.test", "fail", seen.clone(), true),
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let publish = format!(
        "{},\"previewToken\":\"{TOKEN}\"}}",
        INPUT.strip_suffix('}').unwrap()
    );
    let unmap = format!(
        r#"{{"positionId":"position-1","expectedVersion":2,"reason":"Remove the mapping","previewToken":"{TOKEN}"}}"#
    );
    let retry =
        r#"{"id":"job-1","expectedVersion":1,"reason":"Retry the blocked delivery"}"#.to_owned();
    let retry_config =
        r#"{"positionId":"position-1","expectedVersion":3,"reason":"Retry the blocked config"}"#
            .to_owned();
    let cases = [
        (
            "/api/v1/holdings/acme/technology/position-gpo/preview",
            "key-preview",
            INPUT.to_owned(),
        ),
        (
            "/api/v1/holdings/acme/technology/position-gpo/publish",
            "key-publish",
            publish,
        ),
        (
            "/api/v1/holdings/acme/technology/position-gpo/unmap-preview",
            "key-unmap-preview",
            r#"{"positionId":"position-1","expectedVersion":2,"reason":"Remove the mapping"}"#
                .to_owned(),
        ),
        (
            "/api/v1/holdings/acme/technology/position-gpo/unmap",
            "key-unmap",
            unmap,
        ),
        (
            "/api/v1/holdings/acme/technology/position-gpo/retry",
            "key-retry",
            retry,
        ),
        (
            "/api/v1/holdings/acme/technology/position-gpo/retry-config",
            "key-retry-config",
            retry_config,
        ),
    ];
    for (path, key, body) in cases {
        let (status, value) = http(
            addr,
            post(path, "acme.example.test", Some(key), None, &body),
        )
        .await;
        assert_eq!(status, 200, "{path} {value}");
        assert!(value.get("data").is_some(), "{path} {value}");
    }
    let (status, value) = http(
        addr,
        format!(
            "GET /api/v1/holdings/acme/technology/position-gpo HTTP/1.1\r\nHost: acme.example.test\r\nCookie: erpx=session\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(value["data"]["policies"], serde_json::json!([]));

    let seen = seen.lock().unwrap();
    assert!(
        seen.iter()
            .any(|line| line.starts_with("overview|acme.example.test|acme|"))
    );
    assert!(seen
        .iter()
        .any(|line| line == "preview|acme.example.test|acme|position-1|Publish the reviewed mapping|Some(\"erpx=session\")"));
    assert!(
        seen.iter()
            .any(|line| line.starts_with("publish|acme.example.test|acme|position-1|"))
    );
    assert!(
        seen.iter()
            .any(|line| line
                == "retry|acme.example.test|acme|job-1|key-retry|Some(\"erpx=session\")")
    );
}

#[tokio::test]
async fn actual_router_rejects_foreign_origin_missing_key_and_unknown_paths() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router =
        position_gpo_http_router(vec![target("acme.example.test", "acme", seen, false)]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    let path = "/api/v1/holdings/acme/technology/position-gpo/publish";
    let body = format!(
        "{},\"previewToken\":\"{TOKEN}\"}}",
        INPUT.strip_suffix('}').unwrap()
    );

    let (status, value) = http(
        addr,
        post(
            path,
            "acme.example.test",
            Some("key-origin-1"),
            Some("https://evil.example.test"),
            &body,
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(value["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, value) = http(addr, post(path, "acme.example.test", None, None, &body)).await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, value) = http(
        addr,
        post(
            path,
            "acme.example.test",
            Some("key-invalid-2"),
            None,
            "{\"positionId\":\"\"}",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "GPO_REQUEST_INVALID");

    let (status, value) = http(
        addr,
        post(
            "/api/v1/holdings/acme/technology/position-gpo/unknown",
            "acme.example.test",
            Some("key-unknown-3"),
            None,
            &body,
        ),
    )
    .await;
    assert_eq!(status, 404);
    let _ = value;

    let (status, _) = http(
        addr,
        "GET /api/v1/holdings/acme/technology/position-gpo?all=1 HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n".to_owned(),
    )
    .await;
    assert_eq!(status, 422);
}

#[tokio::test]
async fn actual_router_maps_owner_conflicts() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router =
        position_gpo_http_router(vec![target("fail.example.test", "fail", seen, true)]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    let (status, value) = http(
        addr,
        post(
            "/api/v1/holdings/fail/technology/position-gpo/preview",
            "fail.example.test",
            Some("key-fail-1"),
            Some("https://fail.example.test"),
            INPUT,
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(value["error"]["code"], "VERSION_CONFLICT");
}
