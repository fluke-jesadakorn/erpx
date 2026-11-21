//! Actual owner-local HTTP router tests for the native Sales price-list
//! lifecycle. The backend is a recording fake; the shared composition backend
//! and PostgreSQL HTTP path are separate concerns.
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_sales::public::sales_price_lists_http::{
    PriceListHttpBackend, PriceListHttpCommand, PriceListHttpFuture, PriceListHttpRequest,
    PriceListHttpResponse, PriceListHttpTarget, sales_price_lists_router,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl PriceListHttpBackend for Noop {
    fn execute<'a>(
        &'a self,
        _: PriceListHttpRequest<'a>,
    ) -> PriceListHttpFuture<'a, PriceListHttpResponse> {
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

impl PriceListHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: PriceListHttpRequest<'a>,
    ) -> PriceListHttpFuture<'a, PriceListHttpResponse> {
        Box::pin(async move {
            let receipt = match &request.command {
                PriceListHttpCommand::Create {
                    idempotency_key,
                    input,
                } => {
                    self.seen.lock().unwrap().push(format!(
                        "create|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                        idempotency_key,
                        input.name,
                        input.currency,
                        input.customer_id.as_deref().unwrap_or(""),
                        input.valid_from,
                        input.valid_until,
                        input.max_auto_discount_bps,
                        input.items.len(),
                        input.items[0].product_id,
                        input.items[0].price_minor,
                    ));
                    serde_json::json!({
                        "id": "price-list-price-list-http-0001",
                        "status": "draft",
                        "version": 1
                    })
                }
                PriceListHttpCommand::Update {
                    price_list_id,
                    idempotency_key,
                    input,
                } => {
                    self.seen.lock().unwrap().push(format!(
                        "update|{}|{}|{}|{}",
                        price_list_id, idempotency_key, input.expected_version, input.input.name,
                    ));
                    serde_json::json!({
                        "id": price_list_id,
                        "status": "draft",
                        "version": input.expected_version + 1
                    })
                }
                PriceListHttpCommand::Decide {
                    price_list_id,
                    idempotency_key,
                    input,
                } => {
                    self.seen.lock().unwrap().push(format!(
                        "decide|{}|{}|{}|{}|{}",
                        price_list_id,
                        idempotency_key,
                        input.action.as_str(),
                        input.expected_version,
                        input.reason,
                    ));
                    serde_json::json!({
                        "id": price_list_id,
                        "status": if input.action.as_str() == "activate" { "active" } else { "cancelled" },
                        "version": input.expected_version + 1
                    })
                }
            };
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "PRICE_LIST_CHANGED",
                    "Synthetic changed price list.",
                ));
            }
            Ok(PriceListHttpResponse {
                data: receipt,
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || PriceListHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(sales_price_lists_router(vec![target()]).is_ok());
    assert!(sales_price_lists_router(vec![target(), target()]).is_err());
    assert!(
        sales_price_lists_router(vec![PriceListHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        sales_price_lists_router(vec![PriceListHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        sales_price_lists_router(vec![PriceListHttpTarget {
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

fn body(name: &str) -> String {
    serde_json::json!({
        "name": name,
        "currency": "thb",
        "customerId": "customer",
        "validFrom": "2026-11-01",
        "validUntil": "2026-11-30",
        "maxAutoDiscountBps": 500,
        "items": [
            {"productId": "product-1", "price": "100.50", "taxBps": 700},
            {"productId": "product-2", "price": "20", "taxBps": 700}
        ]
    })
    .to_string()
}

fn update_body(name: &str, version: i64) -> String {
    let mut value: serde_json::Value = serde_json::from_str(&body(name)).unwrap();
    value["expectedVersion"] = serde_json::json!(version);
    value.to_string()
}

fn post(
    path: &str,
    host: &str,
    key: Option<&str>,
    origin: Option<&str>,
    content_type: Option<&str>,
    body: &str,
) -> String {
    let mut headers = format!("Host: {host}\r\nConnection: close\r\n");
    if let Some(key) = key {
        headers.push_str(&format!("Idempotency-Key: {key}\r\n"));
    }
    if let Some(origin) = origin {
        headers.push_str(&format!("Origin: {origin}\r\n"));
    }
    headers.push_str(&format!(
        "Content-Type: {}\r\nContent-Length: {}\r\n\r\n{}",
        content_type.unwrap_or("application/json"),
        body.len(),
        body
    ));
    format!("POST {path} HTTP/1.1\r\n{headers}")
}

fn targets(seen: Arc<Mutex<Vec<String>>>) -> Vec<PriceListHttpTarget> {
    vec![
        PriceListHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: false,
            }),
        },
        PriceListHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe { seen, fail: true }),
        },
    ]
}

#[tokio::test]
async fn actual_router_forwards_create_update_and_decision_commands() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = sales_price_lists_router(targets(seen.clone())).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, text) = http(
        addr,
        post(
            "/api/v1/holdings/acme/sales/pricing",
            "acme.example.test",
            Some("price-list-http-0001"),
            Some("https://acme.example.test"),
            None,
            &body("Standard pricing"),
        ),
    )
    .await;
    assert_eq!(status, 200, "{text}");
    assert_eq!(body["data"]["status"], "draft");
    assert_eq!(body["data"]["version"], 1);
    assert_eq!(body["context"]["userId"], "user");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/sales/pricing/price-list-http-0001/edit",
            "acme.example.test",
            Some("price-list-http-0002"),
            None,
            None,
            &update_body("Updated pricing", 1),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["id"], "price-list-http-0001");
    assert_eq!(body["data"]["version"], 2);

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/sales/pricing/price-list-http-0001",
            "acme.example.test",
            Some("price-list-http-0003"),
            None,
            None,
            &serde_json::json!({
                "expectedVersion": 2,
                "action": "activate",
                "reason": "Independent review"
            })
            .to_string(),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "active");
    assert_eq!(body["data"]["version"], 3);

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 3);
    assert_eq!(
        seen[0],
        "create|price-list-http-0001|Standard pricing|thb|customer|2026-11-01|2026-11-30|500|2|product-1|10050"
    );
    assert_eq!(
        seen[1],
        "update|price-list-http-0001|price-list-http-0002|1|Updated pricing"
    );
    assert_eq!(
        seen[2],
        "decide|price-list-http-0001|price-list-http-0003|activate|2|Independent review"
    );

    server.abort();
}

#[tokio::test]
async fn actual_router_rejects_malformed_requests_before_the_backend() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = sales_price_lists_router(targets(seen.clone())).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    for (raw, code) in [
        (
            post(
                "/api/v1/holdings/acme/sales/pricing",
                "acme.example.test",
                None,
                None,
                None,
                &body("Standard pricing"),
            ),
            "COMMAND_KEY_REQUIRED",
        ),
        (
            post(
                "/api/v1/holdings/acme/sales/pricing",
                "acme.example.test",
                Some("price-list-http-0004"),
                None,
                Some("text/plain"),
                &body("Standard pricing"),
            ),
            "PRICE_LIST_REQUEST_INVALID",
        ),
        (
            post(
                "/api/v1/holdings/acme/sales/pricing?view=all",
                "acme.example.test",
                Some("price-list-http-0005"),
                None,
                None,
                &body("Standard pricing"),
            ),
            "PRICE_LIST_REQUEST_INVALID",
        ),
        (
            post(
                "/api/v1/holdings/acme/sales/pricing",
                "acme.example.test",
                Some("price-list-http-0006"),
                None,
                None,
                "{}",
            ),
            "PRICE_LIST_REQUEST_INVALID",
        ),
        (
            post(
                "/api/v1/holdings/acme/sales/pricing",
                "acme.example.test",
                Some("price-list-http-0007"),
                None,
                None,
                &body("Standard pricing").replace("\"items\"", "\"extra\":1,\"items\""),
            ),
            "PRICE_LIST_REQUEST_INVALID",
        ),
        (
            post(
                "/api/v1/holdings/acme/sales/pricing",
                "acme.example.test",
                Some("price-list-http-0008"),
                None,
                None,
                &body("Standard pricing").replace("100.50", "100.505"),
            ),
            "PRICE_LIST_REQUEST_INVALID",
        ),
        (
            post(
                "/api/v1/holdings/acme/sales/pricing",
                "acme.example.test",
                Some("price-list-http-0009"),
                None,
                None,
                &body("Standard pricing").replace("\"taxBps\":700", "\"taxBps\":10001"),
            ),
            "PRICE_LIST_REQUEST_INVALID",
        ),
        (
            post(
                "/api/v1/holdings/acme/sales/pricing/price-list-http-0001",
                "acme.example.test",
                Some("price-list-http-0010"),
                None,
                None,
                &serde_json::json!({
                    "expectedVersion": 1,
                    "action": "retire",
                    "reason": "Independent review"
                })
                .to_string(),
            ),
            "PRICE_LIST_REQUEST_INVALID",
        ),
        (
            post(
                "/api/v1/holdings/acme/sales/pricing",
                "acme.example.test",
                Some("x".repeat(101).as_str()),
                None,
                None,
                &body("Standard pricing"),
            ),
            "PRICE_LIST_REQUEST_INVALID",
        ),
        (
            post(
                "/api/v1/holdings/acme/sales/pricing",
                "acme.example.test",
                Some("price-list-http-0011"),
                Some("https://evil.example.test"),
                None,
                &body("Standard pricing"),
            ),
            "ORIGIN_FORBIDDEN",
        ),
    ] {
        let (status, value, text) = http(addr, raw).await;
        assert!(
            matches!(status, 403 | 422),
            "unexpected status {status}: {text}"
        );
        assert_eq!(value["error"]["code"], code, "{text}");
    }

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/sales/pricing",
            "missing.example.test",
            Some("price-list-http-0012"),
            None,
            None,
            &body("Standard pricing"),
        ),
    )
    .await;
    assert_eq!(status, 404);
    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/missing/sales/pricing",
            "acme.example.test",
            Some("price-list-http-0013"),
            None,
            None,
            &body("Standard pricing"),
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert!(seen.lock().unwrap().is_empty());

    server.abort();
}

#[tokio::test]
async fn actual_router_maps_owner_conflict_and_private_headers() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = sales_price_lists_router(targets(seen)).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, payload, _text) = http(
        addr,
        post(
            "/api/v1/holdings/fail/sales/pricing",
            "fail.example.test",
            Some("price-list-http-0014"),
            None,
            None,
            &body("Standard pricing"),
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(payload["error"]["code"], "PRICE_LIST_CHANGED");

    let (status, _, text) = http(
        addr,
        post(
            "/api/v1/holdings/acme/sales/pricing",
            "acme.example.test",
            Some("price-list-http-0015"),
            None,
            None,
            &body("Standard pricing"),
        ),
    )
    .await;
    assert_eq!(status, 200);
    let lower = text.to_ascii_lowercase();
    assert!(lower.contains("cache-control: private, no-store"));
    assert!(lower.contains("referrer-policy: no-referrer"));
    assert!(lower.contains("x-content-type-options: nosniff"));

    server.abort();
}
