//! Actual owner-local HTTP router tests for native Sales confirmed cancellation.
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_sales::public::sales_order_cancellation_http::{
    SalesOrderCancellationHttpBackend, SalesOrderCancellationHttpFuture,
    SalesOrderCancellationHttpRequest, SalesOrderCancellationHttpResponse,
    SalesOrderCancellationHttpTarget, sales_order_cancellation_router,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl SalesOrderCancellationHttpBackend for Noop {
    fn cancel<'a>(
        &'a self,
        _: SalesOrderCancellationHttpRequest<'a>,
    ) -> SalesOrderCancellationHttpFuture<'a, SalesOrderCancellationHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Ok,
    Conflict,
    BadReceipt,
}

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    mode: Mode,
}

impl SalesOrderCancellationHttpBackend for Probe {
    fn cancel<'a>(
        &'a self,
        request: SalesOrderCancellationHttpRequest<'a>,
    ) -> SalesOrderCancellationHttpFuture<'a, SalesOrderCancellationHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.order_id,
                request.idempotency_key,
                request.expected_version
            ));
            match self.mode {
                Mode::Conflict => Err(BusinessError::new(
                    FailureKind::Conflict,
                    "ORDER_REVERSAL_REQUIRED",
                    "This order has stock movements.",
                )),
                Mode::BadReceipt => Ok(SalesOrderCancellationHttpResponse {
                    data: serde_json::json!({
                        "id": request.order_id,
                        "state": "confirmed",
                        "version": 4,
                        "salesId": null,
                    }),
                    context: serde_json::json!({"userId": "reviewer"}),
                }),
                Mode::Ok => Ok(SalesOrderCancellationHttpResponse {
                    data: serde_json::json!({
                        "id": request.order_id,
                        "state": "cancelled",
                        "version": request.expected_version + 1,
                        "salesId": null,
                    }),
                    context: serde_json::json!({"userId": "reviewer"}),
                }),
            }
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings_and_https_origin() {
    let backend = Arc::new(Noop);
    let target = || SalesOrderCancellationHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(sales_order_cancellation_router(vec![target()]).is_ok());
    assert!(sales_order_cancellation_router(vec![target(), target()]).is_err());
    for origin in [
        "http://acme.example.test",
        "https://acme.example.test/path",
        "https://other.example.test",
        "https://acme.example.test:443",
    ] {
        assert!(
            sales_order_cancellation_router(vec![SalesOrderCancellationHttpTarget {
                origin: origin.into(),
                ..target()
            }])
            .is_err(),
            "{origin}"
        );
    }
    assert!(
        sales_order_cancellation_router(vec![SalesOrderCancellationHttpTarget {
            holding_slug: "ACME".into(),
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

const PATH: &str = "/api/v1/holdings/acme/sales/orders/sales-confirmed/cancel";

fn post(
    path: &str,
    host: &str,
    origin: Option<&str>,
    key: Option<&str>,
    content_type: Option<&str>,
    body: &str,
) -> String {
    let mut raw = format!("POST {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    if let Some(origin) = origin {
        raw.push_str(&format!("Origin: {origin}\r\n"));
    }
    if let Some(key) = key {
        raw.push_str(&format!("Idempotency-Key: {key}\r\n"));
    }
    if let Some(content_type) = content_type {
        raw.push_str(&format!("Content-Type: {content_type}\r\n"));
    }
    raw.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    raw
}

fn request(key: Option<&str>, origin: Option<&str>, body: &str) -> String {
    post(
        PATH,
        "acme.example.test",
        origin.or(Some("https://acme.example.test")),
        key.or(Some("sales-cancel-0001")),
        Some("application/json"),
        body,
    )
}

async fn router(
    mode: Mode,
    seen: Arc<Mutex<Vec<String>>>,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let router = sales_order_cancellation_router(vec![SalesOrderCancellationHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(Probe { seen, mode }),
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    (addr, server)
}

async fn start(
    mode: Mode,
) -> (
    std::net::SocketAddr,
    Arc<Mutex<Vec<String>>>,
    tokio::task::JoinHandle<()>,
) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (addr, server) = router(mode, seen.clone()).await;
    (addr, seen, server)
}

#[tokio::test]
async fn actual_router_forwards_exact_target_key_and_version() {
    let (addr, seen, server) = start(Mode::Ok).await;
    let (status, body, _) = http(addr, request(None, None, "{\"expectedVersion\":3}")).await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["id"], "sales-confirmed");
    assert_eq!(body["data"]["state"], "cancelled");
    assert_eq!(body["data"]["version"], 4);
    assert_eq!(body["data"]["salesId"], serde_json::Value::Null);
    assert_eq!(body["context"]["userId"], "reviewer");
    assert_eq!(
        seen.lock().unwrap().clone(),
        vec!["acme.example.test|acme|sales-confirmed|sales-cancel-0001|3"]
    );
    server.abort();
}

#[tokio::test]
async fn actual_router_rejects_non_post_and_exact_shape_violations() {
    let (addr, seen, server) = start(Mode::Ok).await;

    let (status, _, _) = http(
        addr,
        format!("GET {PATH} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n"),
    )
    .await;
    assert_eq!(status, 405);

    for (raw, expected) in [
        (
            request(
                None,
                Some("https://evil.example.test"),
                "{\"expectedVersion\":3}",
            ),
            403,
        ),
        (
            post(
                PATH,
                "acme.example.test",
                None,
                Some("sales-cancel-0001"),
                Some("application/json"),
                "{\"expectedVersion\":3}",
            ),
            403,
        ),
        (
            request(None, None, "{\"expectedVersion\":3,\"action\":\"cancel\"}"),
            422,
        ),
        (request(None, None, "{\"expectedVersion\":0}"), 422),
        (request(None, None, "{\"expectedVersion\":\"3\"}"), 422),
        (request(None, None, "{}"), 422),
        (request(Some(""), None, "{\"expectedVersion\":3}"), 422),
        (
            request(Some("bad key"), None, "{\"expectedVersion\":3}"),
            422,
        ),
        (
            request(
                None,
                None,
                &format!(
                    "{{\"expectedVersion\":3,\"pad\":\"{}\"}}",
                    "x".repeat(2_000)
                ),
            ),
            422,
        ),
    ] {
        let (status, body, _) = http(addr, raw).await;
        assert_eq!(status, expected, "{body}");
    }

    let (status, body, _) = http(
        addr,
        post(
            PATH,
            "acme.example.test",
            Some("https://acme.example.test"),
            Some("sales-cancel-0001"),
            Some("text/plain"),
            "{\"expectedVersion\":3}",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "SALES_CANCELLATION_REQUEST_INVALID");

    let (status, _, _) = http(
        addr,
        post(
            PATH,
            "acme.example.test",
            Some("https://acme.example.test"),
            Some("shortkey"),
            Some("application/json"),
            "{\"expectedVersion\":3}",
        ),
    )
    .await;
    assert_eq!(status, 200, "an eight-character valid key is accepted");

    assert_eq!(seen.lock().unwrap().len(), 1);
    server.abort();
}

#[tokio::test]
async fn actual_router_rejects_unknown_target_and_duplicate_query() {
    let (addr, seen, server) = start(Mode::Ok).await;

    let (status, _, _) = http(
        addr,
        post(
            PATH,
            "missing.example.test",
            Some("https://missing.example.test"),
            Some("sales-cancel-0001"),
            Some("application/json"),
            "{\"expectedVersion\":3}",
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/missing/sales/orders/sales-confirmed/cancel",
            "acme.example.test",
            Some("https://acme.example.test"),
            Some("sales-cancel-0001"),
            Some("application/json"),
            "{\"expectedVersion\":3}",
        ),
    )
    .await;
    assert_eq!(status, 404);

    for path in [
        &format!("{PATH}?action=cancel")[..],
        &format!("{PATH}?x=1&x=2")[..],
    ] {
        let (status, body, _) = http(
            addr,
            post(
                path,
                "acme.example.test",
                Some("https://acme.example.test"),
                Some("sales-cancel-0001"),
                Some("application/json"),
                "{\"expectedVersion\":3}",
            ),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "SALES_CANCELLATION_REQUEST_INVALID");
    }
    assert!(seen.lock().unwrap().is_empty());
    server.abort();
}

#[tokio::test]
async fn backend_conflict_and_malformed_receipt_map_to_bounded_errors() {
    let (addr, _, server) = start(Mode::Conflict).await;
    let (status, body, _) = http(addr, request(None, None, "{\"expectedVersion\":3}")).await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "ORDER_REVERSAL_REQUIRED");
    server.abort();

    let (addr, _, server) = start(Mode::BadReceipt).await;
    let (status, body, _) = http(addr, request(None, None, "{\"expectedVersion\":3}")).await;
    assert_eq!(status, 503);
    assert_eq!(body["error"]["code"], "SALES_CANCELLATION_SOURCE_INVALID");
    server.abort();
}

#[tokio::test]
async fn response_headers_are_private_and_bounded() {
    let (addr, _, server) = start(Mode::Ok).await;
    let (_, _, text) = http(addr, request(None, None, "{\"expectedVersion\":3}")).await;
    let text = text.to_ascii_lowercase();
    assert!(text.contains("cache-control: private, no-store"));
    assert!(text.contains("referrer-policy: no-referrer"));
    assert!(text.contains("x-content-type-options: nosniff"));
    server.abort();
}
