//! Owner-local HTTP ingress tests for the native Procurement supplier promise
//! command and its bounded detail read. These prove transport facts only;
//! composition, Account session and PostgreSQL remain separate.
use erpx_server_procurement::public::supplier_promise_command_http::{
    SupplierPromiseCommandHttpBackend, SupplierPromiseCommandHttpFuture,
    SupplierPromiseCommandHttpRequest, SupplierPromiseCommandHttpResponse,
    SupplierPromiseCommandHttpTarget, SupplierPromiseDetailHttpRequest,
    SupplierPromiseDetailHttpResponse, supplier_promise_command_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

const KEY: &str = "procurement-command-1";

#[derive(Clone, Debug)]
struct Seen {
    host: String,
    holding: String,
    order_id: String,
    key: String,
    kind: String,
    expected_version: i64,
    line_count: usize,
    origin: Option<String>,
}

#[derive(Default)]
struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
    fail: Option<BusinessError>,
    bad_receipt: bool,
    detail: Option<serde_json::Value>,
}

impl Probe {
    fn origin(request: &SupplierPromiseCommandHttpRequest<'_>) -> Option<String> {
        request
            .headers
            .get("origin")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    }
}

impl SupplierPromiseCommandHttpBackend for Probe {
    fn record<'a>(
        &'a self,
        request: SupplierPromiseCommandHttpRequest<'a>,
    ) -> SupplierPromiseCommandHttpFuture<'a, SupplierPromiseCommandHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                host: request.host.into(),
                holding: request.holding_slug.into(),
                order_id: request.order_id.into(),
                key: request.key.as_str().into(),
                kind: request.command.kind.clone(),
                expected_version: request.command.expected_version,
                line_count: request.command.lines.len(),
                origin: Self::origin(&request),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            let promise_id = if self.bad_receipt {
                "supplier-promise-foreign-key".to_owned()
            } else {
                format!("supplier-promise-{}", request.key.as_str())
            };
            Ok(SupplierPromiseCommandHttpResponse {
                data: serde_json::json!({
                    "id": request.order_id,
                    "promiseId": promise_id,
                    "version": request.command.expected_version + 1,
                }),
                context: serde_json::json!({"userId": "user", "companyId": "company"}),
            })
        })
    }

    fn detail<'a>(
        &'a self,
        request: SupplierPromiseDetailHttpRequest<'a>,
    ) -> SupplierPromiseCommandHttpFuture<'a, SupplierPromiseDetailHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                host: request.host.into(),
                holding: request.holding_slug.into(),
                order_id: request.order_id.into(),
                key: String::new(),
                kind: "detail".into(),
                expected_version: 0,
                line_count: 0,
                origin: request
                    .headers
                    .get("origin")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(SupplierPromiseDetailHttpResponse {
                data: self.detail.clone().unwrap_or_else(|| {
                    serde_json::json!({
                        "order_id": request.order_id,
                        "history": [],
                        "lines": [],
                        "open": [],
                    })
                }),
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

fn target(host: &str, holding: &str, backend: Arc<Probe>) -> SupplierPromiseCommandHttpTarget {
    SupplierPromiseCommandHttpTarget {
        host: format!("{host}.example.test"),
        holding_slug: holding.into(),
        origin: format!("https://{host}.example.test"),
        backend,
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Probe::default());
    let acme = || target("acme", "acme", backend.clone());
    assert!(supplier_promise_command_http_router(vec![acme()]).is_ok());
    assert!(supplier_promise_command_http_router(vec![acme(), acme()]).is_err());
    assert!(
        supplier_promise_command_http_router(vec![SupplierPromiseCommandHttpTarget {
            origin: "http://acme.example.test".into(),
            ..acme()
        }])
        .is_err()
    );
    assert!(
        supplier_promise_command_http_router(vec![SupplierPromiseCommandHttpTarget {
            holding_slug: "Acme!".into(),
            ..acme()
        }])
        .is_err()
    );
    assert!(
        supplier_promise_command_http_router(vec![SupplierPromiseCommandHttpTarget {
            host: String::new(),
            ..acme()
        }])
        .is_err()
    );
}

async fn start(
    targets: Vec<SupplierPromiseCommandHttpTarget>,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let router = supplier_promise_command_http_router(targets).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    (addr, server)
}

async fn http(addr: SocketAddr, raw: String) -> (u16, String, serde_json::Value) {
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
        .unwrap_or(0);
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    (
        status,
        head.to_ascii_lowercase(),
        serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
    )
}

fn post(path: &str, host: &str, key: Option<&str>, origin: Option<&str>, body: &str) -> String {
    let mut raw = format!("POST {path} HTTP/1.1\r\nHost: {host}\r\n");
    raw.push_str("Content-Type: application/json\r\n");
    if let Some(key) = key {
        raw.push_str(&format!("Idempotency-Key: {key}\r\n"));
    }
    if let Some(origin) = origin {
        raw.push_str(&format!("Origin: {origin}\r\n"));
    }
    raw.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ));
    raw
}

fn get(path: &str, host: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
}

fn command_body() -> String {
    serde_json::json!({
        "expectedVersion": 1,
        "kind": "confirmed",
        "confirmationDate": "2026-10-25",
        "supplierReference": "SUP-REF-1",
        "evidence": "Supplier confirmation evidence",
        "lines": [{
            "orderLineId": "line-a",
            "quantity": "4.000",
            "promisedDate": "2026-10-26"
        }]
    })
    .to_string()
}

const PATH: &str = "/api/v1/holdings/acme/supplier-promises/order-1";

#[tokio::test]
async fn records_supplier_promise_with_exact_mapping_and_private_headers() {
    let probe = Arc::new(Probe::default());
    let (addr, server) = start(vec![target("acme", "acme", probe.clone())]).await;

    let (status, head, body) = http(
        addr,
        post(
            PATH,
            "acme.example.test",
            Some(KEY),
            Some("https://acme.example.test"),
            &command_body(),
        ),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["id"], "order-1");
    assert_eq!(body["data"]["promiseId"], format!("supplier-promise-{KEY}"));
    assert_eq!(body["data"]["version"], 2);
    assert_eq!(body["context"]["userId"], "user");
    assert!(head.contains("cache-control: private, no-store"));
    assert!(head.contains("referrer-policy: no-referrer"));
    assert!(head.contains("x-content-type-options: nosniff"));

    let seen = probe.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].host, "acme.example.test");
    assert_eq!(seen[0].holding, "acme");
    assert_eq!(seen[0].order_id, "order-1");
    assert_eq!(seen[0].key, KEY);
    assert_eq!(seen[0].kind, "confirmed");
    assert_eq!(seen[0].expected_version, 1);
    assert_eq!(seen[0].line_count, 1);
    assert_eq!(seen[0].origin.as_deref(), Some("https://acme.example.test"));

    // The browser Origin header stays optional, matching the retained route.
    let (status, _, _) = http(
        addr,
        post(PATH, "acme.example.test", Some(KEY), None, &command_body()),
    )
    .await;
    assert_eq!(status, 200);
    server.abort();
}

#[tokio::test]
async fn rejects_malformed_commands_before_the_backend() {
    let probe = Arc::new(Probe::default());
    let (addr, server) = start(vec![target("acme", "acme", probe.clone())]).await;

    let cases = [
        (
            "missing key",
            post(PATH, "acme.example.test", None, None, &command_body()),
            422,
            "COMMAND_KEY_REQUIRED",
        ),
        (
            "short key",
            post(
                PATH,
                "acme.example.test",
                Some("short"),
                None,
                &command_body(),
            ),
            422,
            "COMMAND_KEY_REQUIRED",
        ),
        (
            "foreign origin",
            post(
                PATH,
                "acme.example.test",
                Some(KEY),
                Some("https://evil.example.test"),
                &command_body(),
            ),
            403,
            "ORIGIN_FORBIDDEN",
        ),
        (
            "query",
            post(
                &format!("{PATH}?view=all"),
                "acme.example.test",
                Some(KEY),
                None,
                &command_body(),
            ),
            422,
            "SUPPLIER_PROMISE_COMMAND_INVALID",
        ),
        (
            "unknown holding",
            post(
                "/api/v1/holdings/other/supplier-promises/order-1",
                "acme.example.test",
                Some(KEY),
                None,
                &command_body(),
            ),
            404,
            "NOT_FOUND",
        ),
        (
            "unknown host",
            post(PATH, "other.example.test", Some(KEY), None, &command_body()),
            404,
            "NOT_FOUND",
        ),
        (
            "oversized order id",
            post(
                &format!(
                    "/api/v1/holdings/acme/supplier-promises/{}",
                    "a".repeat(201)
                ),
                "acme.example.test",
                Some(KEY),
                None,
                &command_body(),
            ),
            422,
            "SUPPLIER_PROMISE_COMMAND_INVALID",
        ),
        (
            "invalid json",
            post(PATH, "acme.example.test", Some(KEY), None, "{"),
            422,
            "SUPPLIER_PROMISE_COMMAND_INVALID",
        ),
        (
            "unknown field",
            post(
                PATH,
                "acme.example.test",
                Some(KEY),
                None,
                r#"{"expectedVersion":1,"kind":"confirmed","confirmationDate":"2026-10-25","supplierReference":"SUP-REF-1","evidence":"Supplier confirmation evidence","lines":[],"extra":true}"#,
            ),
            422,
            "SUPPLIER_PROMISE_COMMAND_INVALID",
        ),
        (
            "invalid kind",
            post(
                PATH,
                "acme.example.test",
                Some(KEY),
                None,
                &command_body().replace(r#""kind":"confirmed""#, r#""kind":"received""#),
            ),
            422,
            "SUPPLIER_PROMISE_INPUT_INVALID",
        ),
        (
            "attachment evidence",
            post(
                PATH,
                "acme.example.test",
                Some(KEY),
                None,
                &command_body().replace(r#""lines":"#, r#""attachmentSetIds":["set-1"],"lines":"#),
            ),
            422,
            "SUPPLIER_PROMISE_EVIDENCE_UNSUPPORTED",
        ),
    ];
    for (name, raw, expected_status, expected_code) in cases {
        let (status, _, body) = http(addr, raw).await;
        assert_eq!(status, expected_status, "{name}: {body}");
        assert_eq!(body["error"]["code"], expected_code, "{name}: {body}");
    }

    let (status, _, _) = http(
        addr,
        format!(
            "PUT {PATH} HTTP/1.1\r\nHost: acme.example.test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert_eq!(status, 405);

    // Content-Type and body-size bounds are transport facts, not owner rules.
    let no_content_type = format!(
        "POST {PATH} HTTP/1.1\r\nHost: acme.example.test\r\nIdempotency-Key: {KEY}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        command_body().len(),
        command_body()
    );
    let (status, _, body) = http(addr, no_content_type).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["error"]["code"], "SUPPLIER_PROMISE_COMMAND_INVALID");

    let oversized = serde_json::json!({"pad": "a".repeat(70 * 1024)}).to_string();
    let (status, _, body) = http(
        addr,
        post(PATH, "acme.example.test", Some(KEY), None, &oversized),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["error"]["code"], "SUPPLIER_PROMISE_COMMAND_INVALID");

    assert!(
        probe.seen.lock().unwrap().is_empty(),
        "malformed commands must not reach the backend"
    );
    server.abort();
}

#[tokio::test]
async fn maps_backend_and_receipt_failures() {
    let conflict = Arc::new(Probe {
        fail: Some(BusinessError::new(
            FailureKind::Conflict,
            "PURCHASE_ORDER_CHANGED",
            "The purchase order changed.",
        )),
        ..Probe::default()
    });
    let forbidden = Arc::new(Probe {
        fail: Some(BusinessError::new(
            FailureKind::Forbidden,
            "PERMISSION_REQUIRED",
            "Your selected role does not grant purchasing write access.",
        )),
        ..Probe::default()
    });
    let bad_receipt = Arc::new(Probe {
        bad_receipt: true,
        ..Probe::default()
    });
    let (addr, server) = start(vec![
        target("acme", "acme", conflict),
        target("forbidden", "forbidden", forbidden),
        target("badack", "badack", bad_receipt),
    ])
    .await;

    let (status, _, body) = http(
        addr,
        post(PATH, "acme.example.test", Some(KEY), None, &command_body()),
    )
    .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["error"]["code"], "PURCHASE_ORDER_CHANGED");

    let (status, _, body) = http(
        addr,
        post(
            "/api/v1/holdings/forbidden/supplier-promises/order-1",
            "forbidden.example.test",
            Some(KEY),
            None,
            &command_body(),
        ),
    )
    .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["error"]["code"], "PERMISSION_REQUIRED");

    let (status, _, body) = http(
        addr,
        post(
            "/api/v1/holdings/badack/supplier-promises/order-1",
            "badack.example.test",
            Some(KEY),
            None,
            &command_body(),
        ),
    )
    .await;
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["error"]["code"], "COMMAND_RECEIPT_INVALID");

    server.abort();
}

#[tokio::test]
async fn reads_bounded_detail_and_maps_errors() {
    let probe = Arc::new(Probe::default());
    let failing = Arc::new(Probe {
        fail: Some(BusinessError::new(
            FailureKind::Conflict,
            "SUPPLIER_PROMISE_HISTORY_CHANGED",
            "Synthetic changed supplier history.",
        )),
        ..Probe::default()
    });
    let (addr, server) = start(vec![
        target("acme", "acme", probe.clone()),
        target("fail", "fail", failing),
    ])
    .await;

    let (status, head, body) = http(addr, get(PATH, "acme.example.test")).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["order_id"], "order-1");
    assert_eq!(body["data"]["history"], serde_json::json!([]));
    assert_eq!(body["context"]["userId"], "user");
    assert!(head.contains("cache-control: private, no-store"));
    assert_eq!(probe.seen.lock().unwrap()[0].order_id, "order-1");

    let (status, _, body) = http(addr, get(&format!("{PATH}?view=all"), "acme.example.test")).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["error"]["code"], "SUPPLIER_PROMISE_COMMAND_INVALID");

    let (status, _, body) = http(
        addr,
        get(
            &format!(
                "/api/v1/holdings/acme/supplier-promises/{}",
                "a".repeat(201)
            ),
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["error"]["code"], "SUPPLIER_PROMISE_COMMAND_INVALID");

    let (status, _, body) = http(
        addr,
        get(
            "/api/v1/holdings/missing/supplier-promises/order-1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404, "{body}");

    let (status, _, body) = http(
        addr,
        get(
            "/api/v1/holdings/fail/supplier-promises/order-1",
            "fail.example.test",
        ),
    )
    .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["error"]["code"], "SUPPLIER_PROMISE_HISTORY_CHANGED");

    let empty = Arc::new(Probe {
        detail: Some(serde_json::json!("not-an-object")),
        ..Probe::default()
    });
    let (empty_addr, empty_server) = start(vec![target("empty", "empty", empty)]).await;
    let (status, _, body) = http(
        empty_addr,
        get(
            "/api/v1/holdings/empty/supplier-promises/order-1",
            "empty.example.test",
        ),
    )
    .await;
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["error"]["code"], "SUPPLIER_PROMISE_DETAIL_INVALID");
    empty_server.abort();
    server.abort();
}
