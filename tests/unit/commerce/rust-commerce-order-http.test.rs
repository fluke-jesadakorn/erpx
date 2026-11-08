//! Actual owner-local HTTP router tests for the native Commerce order slice.
//! The exact owner dispatch is exercised over loopback without a shared
//! composition backend or PostgreSQL-backed request.
use erpx_server_commerce::public::order_http::{
    CommerceOrderHttpBackend, CommerceOrderHttpCommand, CommerceOrderHttpFuture,
    CommerceOrderHttpRequest, CommerceOrderHttpResponse, CommerceOrderHttpState,
    CommerceOrderHttpTarget, commerce_order_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: Option<BusinessError>,
    bad: bool,
}

impl CommerceOrderHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: CommerceOrderHttpRequest<'a>,
    ) -> CommerceOrderHttpFuture<'a, CommerceOrderHttpResponse> {
        Box::pin(async move {
            let seen = match &request.command {
                CommerceOrderHttpCommand::Detail { order_id } => {
                    format!(
                        "detail|{}|{}|{}",
                        request.host, request.holding_slug, order_id
                    )
                }
                CommerceOrderHttpCommand::Transition {
                    order_id,
                    idempotency_key,
                    action,
                    expected_version,
                    reason,
                } => format!(
                    "transition|{}|{}|{}|{}|{}|{}|{}|{}",
                    request.host,
                    request.holding_slug,
                    order_id,
                    idempotency_key,
                    action.as_str(),
                    expected_version,
                    reason.as_deref().unwrap_or(""),
                    request
                        .headers
                        .get("cookie")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("")
                ),
                CommerceOrderHttpCommand::Save {
                    order_id,
                    idempotency_key,
                    input,
                } => format!(
                    "save|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                    request.host,
                    request.holding_slug,
                    order_id.as_deref().unwrap_or(""),
                    idempotency_key,
                    input.kind.as_str(),
                    input.party_id,
                    input.warehouse_id.as_deref().unwrap_or(""),
                    input.order_date,
                    input.due_date,
                    input.lines.len(),
                    input.expected_version.unwrap_or(0),
                ),
            };
            self.seen.lock().unwrap().push(seen);
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(CommerceOrderHttpResponse {
                data: match &request.command {
                    CommerceOrderHttpCommand::Detail { .. } => {
                        if self.bad {
                            serde_json::json!({"order":{},"lines":[]})
                        } else {
                            serde_json::json!({
                                "order":{"id":"order-1","company_id":"company","kind":"quote","state":"draft"},
                                "lines":[{"id":"line-1","order_id":"order-1"}]
                            })
                        }
                    }
                    CommerceOrderHttpCommand::Transition { .. } => {
                        if self.bad {
                            serde_json::json!({"id":"order-1","state":"submitted"})
                        } else {
                            serde_json::json!({
                                "id":"order-1","state":"submitted","version":2,"salesId":null
                            })
                        }
                    }
                    CommerceOrderHttpCommand::Save { order_id, .. } => {
                        if self.bad {
                            serde_json::json!({"id":"order-1"})
                        } else {
                            serde_json::json!({
                                "id":order_id.as_deref().unwrap_or("order-1"),"version":2
                            })
                        }
                    }
                },
                context: serde_json::json!({"userId":"user"}),
            })
        })
    }
}

fn target(
    host: &str,
    slug: &str,
    seen: Arc<Mutex<Vec<String>>>,
    fail: Option<BusinessError>,
    bad: bool,
) -> CommerceOrderHttpTarget {
    CommerceOrderHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: format!("https://{host}"),
        backend: Arc::new(Probe { seen, fail, bad }),
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
async fn actual_router_forwards_detail_and_transition_with_private_headers() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = commerce_order_http_router(vec![target(
        "acme.example.test",
        "acme",
        seen.clone(),
        None,
        false,
    )])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, headers) = http(
        addr,
        get(
            "/api/v1/holdings/acme/commerce/orders/order-1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["order"]["id"], "order-1");
    assert_eq!(body["context"]["userId"], "user");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/commerce/orders/order-1/transition",
            "acme.example.test",
            Some("commerce-command-0001"),
            r#"{"action":"submit","expectedVersion":1}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body["data"],
        serde_json::json!({"id":"order-1","state":"submitted","version":2,"salesId":null})
    );
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        [
            "detail|acme.example.test|acme|order-1",
            "transition|acme.example.test|acme|order-1|commerce-command-0001|submit|1||"
        ]
    );
}

#[tokio::test]
async fn actual_router_forwards_save_create_and_update_exactly() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = commerce_order_http_router(vec![target(
        "acme.example.test",
        "acme",
        seen.clone(),
        None,
        false,
    )])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    let body = r#"{"kind":"quote","partyId":"party-1","warehouseId":"","orderDate":"2026-11-10","dueDate":"2026-11-15","note":"","priceListId":"list-1","discountReason":"Reviewed commercial discount","lines":[{"productId":"product-1","quantity":"1","price":"80","taxBps":700}]}"#;
    let (status, response, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/commerce/orders",
            "acme.example.test",
            Some("commerce-save-0001"),
            body,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        response["data"],
        serde_json::json!({"id":"order-1","version":2})
    );
    let (status, response, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/commerce/orders/order-1",
            "acme.example.test",
            Some("commerce-save-0002"),
            body,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        response["data"],
        serde_json::json!({"id":"order-1","version":2})
    );
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        [
            "save|acme.example.test|acme||commerce-save-0001|quote|party-1||2026-11-10|2026-11-15|1|0",
            "save|acme.example.test|acme|order-1|commerce-save-0002|quote|party-1||2026-11-10|2026-11-15|1|0"
        ]
    );

    for (request, expected) in [
        (
            post(
                "/api/v1/holdings/acme/commerce/orders",
                "acme.example.test",
                None,
                body,
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/acme/commerce/orders",
                "acme.example.test",
                Some("commerce-save-0003"),
                r#"{"kind":"purchase","partyId":"party-1","orderDate":"2026-11-10","dueDate":"2026-11-15","lines":[]}"#,
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/acme/commerce/orders",
                "acme.example.test",
                Some("commerce-save-0004"),
                r#"{"kind":"quote","partyId":"party-1","orderDate":"2026-11-10","dueDate":"2026-11-15","lines":[{"productId":"p","quantity":"1","price":"1","taxBps":10001}]}"#,
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/acme/commerce/orders",
                "acme.example.test",
                Some("commerce-save-0005"),
                r#"{"kind":"quote","partyId":"party-1","orderDate":"2026-11-10","dueDate":"2026-11-15","lines":[{"productId":"p","quantity":"1","price":"1","taxBps":0}],"unknown":true}"#,
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/acme/commerce/orders?print=1",
                "acme.example.test",
                Some("commerce-save-0006"),
                body,
            ),
            422,
        ),
    ] {
        let (status, _, _) = http(addr, request).await;
        assert_eq!(status, expected);
    }
}

#[tokio::test]
async fn router_rejects_unknown_hosts_scopes_queries_and_malformed_bodies() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = commerce_order_http_router(vec![
        target("acme.example.test", "acme", seen.clone(), None, false),
        target(
            "fail.example.test",
            "fail",
            seen,
            Some(BusinessError::new(
                FailureKind::Conflict,
                "COMMERCE_ORDER_CHANGED",
                "The order changed. Reload and try again.",
            )),
            false,
        ),
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    for (request, expected) in [
        (
            get(
                "/api/v1/holdings/acme/commerce/orders/order-1",
                "evil.example.test",
            ),
            404,
        ),
        (
            get(
                "/api/v1/holdings/other/commerce/orders/order-1",
                "acme.example.test",
            ),
            404,
        ),
        (
            get(
                "/api/v1/holdings/acme/commerce/orders/order-1?print=1",
                "acme.example.test",
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/acme/commerce/orders/order-1/transition",
                "acme.example.test",
                None,
                r#"{"action":"submit","expectedVersion":1}"#,
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/acme/commerce/orders/order-1/transition",
                "acme.example.test",
                Some("commerce-command-0001"),
                r#"{"action":"unknown","expectedVersion":1}"#,
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/acme/commerce/orders/order-1/transition",
                "acme.example.test",
                Some("commerce-command-0001"),
                r#"{"action":"submit","expectedVersion":1,"extra":true}"#,
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/fail/commerce/orders/order-1/transition",
                "fail.example.test",
                Some("commerce-command-0002"),
                r#"{"action":"submit","expectedVersion":1}"#,
            ),
            409,
        ),
    ] {
        let (status, _, _) = http(addr, request).await;
        assert_eq!(status, expected);
    }
}

#[test]
fn configuration_rejects_duplicate_targets_and_bad_origins() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    assert!(
        CommerceOrderHttpState::new(vec![target(
            "acme.example.test",
            "acme",
            seen.clone(),
            None,
            false,
        )])
        .is_ok()
    );
    assert!(
        CommerceOrderHttpState::new(vec![
            target("acme.example.test", "acme", seen.clone(), None, false),
            target("acme.example.test", "acme", seen.clone(), None, false),
        ])
        .is_err()
    );
    for origin in [
        "http://acme.example.test",
        "https://acme.example.test/path",
        "https://acme.example.test:8443",
    ] {
        let mut invalid = target("acme.example.test", "acme", seen.clone(), None, false);
        invalid.origin = origin.into();
        assert!(CommerceOrderHttpState::new(vec![invalid]).is_err());
    }
    let mut development = target(
        "acme.erpx.localhost:3443",
        "acme",
        seen.clone(),
        None,
        false,
    );
    development.origin = "https://acme.erpx.localhost:3443".into();
    assert!(CommerceOrderHttpState::new(vec![development]).is_ok());
    let mut bad_slug = target("acme.example.test", "Acme!", seen, None, false);
    bad_slug.holding_slug = "Acme!".into();
    assert!(CommerceOrderHttpState::new(vec![bad_slug]).is_err());
}
