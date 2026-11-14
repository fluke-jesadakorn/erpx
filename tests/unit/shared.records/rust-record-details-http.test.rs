//! Actual owner-local HTTP router tests for the native Records drawer-detail read.
use erpx_server_records::public::record_details_http::{
    RecordsDetailsHttpBackend, RecordsDetailsHttpCommand, RecordsDetailsHttpFuture,
    RecordsDetailsHttpRequest, RecordsDetailsHttpResponse, RecordsDetailsHttpTarget,
    records_details_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl RecordsDetailsHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: RecordsDetailsHttpRequest<'a>,
    ) -> RecordsDetailsHttpFuture<'a, RecordsDetailsHttpResponse> {
        Box::pin(async move {
            let cookie = request
                .headers
                .get("cookie")
                .and_then(|value| value.to_str().ok());
            let RecordsDetailsHttpCommand::Supplement {
                module_id,
                record_id,
                page,
            } = request.command;
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}|{}|{cookie:?}",
                request.host, request.holding_slug, module_id, record_id, page
            ));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "RECORD_CONTEXT_SOURCE_INVALID",
                    "Synthetic detail failure.",
                ));
            }
            Ok(RecordsDetailsHttpResponse {
                data: serde_json::json!({
                    "kind": "workspace_record",
                    "row": {"moduleId": module_id, "id": record_id},
                    "related": [],
                    "history": [],
                    "page": page,
                    "hasNext": false
                }),
                context: serde_json::json!({"userId": "actor"}),
            })
        })
    }
}

struct Noop;

impl RecordsDetailsHttpBackend for Noop {
    fn execute<'a>(
        &'a self,
        _: RecordsDetailsHttpRequest<'a>,
    ) -> RecordsDetailsHttpFuture<'a, RecordsDetailsHttpResponse> {
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
    let target = || RecordsDetailsHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(records_details_http_router(vec![target()]).is_ok());
    assert!(records_details_http_router(vec![target(), target()]).is_err());
    assert!(
        records_details_http_router(vec![RecordsDetailsHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        records_details_http_router(vec![RecordsDetailsHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        records_details_http_router(vec![RecordsDetailsHttpTarget {
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
) -> RecordsDetailsHttpTarget {
    RecordsDetailsHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: format!("https://{host}"),
        backend: Arc::new(Probe { seen, fail }),
    }
}

#[tokio::test]
async fn actual_router_forwards_the_bounded_detail_page_with_private_headers() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = records_details_http_router(vec![
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
            "/api/v1/holdings/acme/records/operations/rec-1/detail?page=2",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["kind"], "workspace_record");
    assert_eq!(body["data"]["row"]["moduleId"], "operations");
    assert_eq!(body["data"]["row"]["id"], "rec-1");
    assert_eq!(body["data"]["page"], 2);
    assert_eq!(body["context"]["userId"], "actor");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));
    assert_eq!(
        *seen.lock().unwrap(),
        vec!["acme.example.test|acme|operations|rec-1|2|Some(\"erpx=session\")"]
    );

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/fail/records/operations/rec-1/detail",
            "fail.example.test",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "RECORD_CONTEXT_SOURCE_INVALID");
}

#[tokio::test]
async fn unknown_targets_extra_query_bodies_and_invalid_pages_are_bounded() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = records_details_http_router(vec![target(
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
            "/api/v1/holdings/acme/records/operations/rec-1/detail?view=mention",
            "acme.example.test",
            422,
            "RECORD_DETAILS_PAGE_INVALID",
        ),
        (
            "/api/v1/holdings/acme/records/operations/rec-1/detail?page=0",
            "acme.example.test",
            422,
            "RECORD_DETAILS_PAGE_INVALID",
        ),
        (
            "/api/v1/holdings/acme/records/operations/rec-1/detail?page=2&page=3",
            "acme.example.test",
            422,
            "RECORD_DETAILS_PAGE_INVALID",
        ),
        (
            "/api/v1/holdings/acme/records/operations/rec-1",
            "acme.example.test",
            404,
            "",
        ),
        (
            "/api/v1/holdings/acme/records/operations/rec-1/detail",
            "unknown.example.test",
            404,
            "NOT_FOUND",
        ),
        (
            "/api/v1/holdings/other/records/operations/rec-1/detail",
            "acme.example.test",
            404,
            "NOT_FOUND",
        ),
        (
            "/api/v1/holdings/acme/records/..%2Foperations/rec-1/detail",
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

    let raw = "GET /api/v1/holdings/acme/records/operations/rec-1/detail HTTP/1.1\r\nHost: acme.example.test\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
    let (status, body, _) = http(addr, raw.to_owned()).await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "RECORD_DETAILS_PAGE_INVALID");
    assert!(seen.lock().unwrap().is_empty());
}
