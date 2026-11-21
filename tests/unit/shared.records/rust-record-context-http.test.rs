//! Actual owner-local HTTP router tests for the native Records context read.
use erpx_server_records::public::record_context_http::{
    RecordsContextHttpBackend, RecordsContextHttpCommand, RecordsContextHttpFuture,
    RecordsContextHttpRequest, RecordsContextHttpResponse, RecordsContextHttpTarget,
    records_context_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl RecordsContextHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: RecordsContextHttpRequest<'a>,
    ) -> RecordsContextHttpFuture<'a, RecordsContextHttpResponse> {
        Box::pin(async move {
            let cookie = request
                .headers
                .get("cookie")
                .and_then(|value| value.to_str().ok());
            let RecordsContextHttpCommand::Context {
                module_id,
                record_id,
            } = request.command;
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}|{cookie:?}",
                request.host, request.holding_slug, module_id, record_id
            ));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "RECORD_CONTEXT_SOURCE_INVALID",
                    "Synthetic context failure.",
                ));
            }
            Ok(RecordsContextHttpResponse {
                data: serde_json::json!({
                    "record": {"moduleId": module_id, "id": record_id},
                    "references": [],
                    "backlinks": [],
                    "mentions": [],
                    "suggestions": [],
                    "history": []
                }),
                context: serde_json::json!({"userId": "actor"}),
            })
        })
    }
}

struct Noop;

impl RecordsContextHttpBackend for Noop {
    fn execute<'a>(
        &'a self,
        _: RecordsContextHttpRequest<'a>,
    ) -> RecordsContextHttpFuture<'a, RecordsContextHttpResponse> {
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
    let target = || RecordsContextHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(records_context_http_router(vec![target()]).is_ok());
    assert!(records_context_http_router(vec![target(), target()]).is_err());
    assert!(
        records_context_http_router(vec![RecordsContextHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        records_context_http_router(vec![RecordsContextHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        records_context_http_router(vec![RecordsContextHttpTarget {
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
) -> RecordsContextHttpTarget {
    RecordsContextHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: format!("https://{host}"),
        backend: Arc::new(Probe { seen, fail }),
    }
}

#[tokio::test]
async fn actual_router_forwards_context_with_private_headers() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = records_context_http_router(vec![
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
            "/api/v1/holdings/acme/records/operations/rec-1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["record"]["moduleId"], "operations");
    assert_eq!(body["data"]["record"]["id"], "rec-1");
    assert_eq!(body["context"]["userId"], "actor");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));
    assert_eq!(
        *seen.lock().unwrap(),
        vec!["acme.example.test|acme|operations|rec-1|Some(\"erpx=session\")"]
    );

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/fail/records/operations/rec-1",
            "fail.example.test",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "RECORD_CONTEXT_SOURCE_INVALID");
}

#[tokio::test]
async fn unknown_targets_queries_bodies_and_invalid_paths_are_bounded_404() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = records_context_http_router(vec![target(
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
            "/api/v1/holdings/acme/records/operations/rec-1?view=mention",
            "acme.example.test",
            422,
            "RECORD_CONTEXT_REQUEST_INVALID",
        ),
        (
            "/api/v1/holdings/acme/records/operations/rec-1",
            "unknown.example.test",
            404,
            "NOT_FOUND",
        ),
        (
            "/api/v1/holdings/other/records/operations/rec-1",
            "acme.example.test",
            404,
            "NOT_FOUND",
        ),
        (
            "/api/v1/holdings/acme/records/..%2Foperations/rec-1",
            "acme.example.test",
            404,
            "NOT_FOUND",
        ),
        (
            "/api/v1/holdings/acme/records/operations/",
            "acme.example.test",
            404,
            "",
        ),
    ] {
        let (status, body, _) = http(addr, get(path, host)).await;
        assert_eq!(status, expected_status, "{path} on {host}");
        if !expected_code.is_empty() {
            assert_eq!(body["error"]["code"], expected_code, "{path} on {host}");
        }
    }

    let raw = "GET /api/v1/holdings/acme/records/operations/rec-1 HTTP/1.1\r\nHost: acme.example.test\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
    let (status, body, _) = http(addr, raw.to_owned()).await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "RECORD_CONTEXT_REQUEST_INVALID");
    assert!(seen.lock().unwrap().is_empty());
}
