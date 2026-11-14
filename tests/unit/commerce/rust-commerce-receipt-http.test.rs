//! Actual owner-local HTTP router tests for the native Commerce purchase
//! goods-receipt slice. The exact owner dispatch is exercised over loopback
//! without a shared composition backend or PostgreSQL-backed request.
use erpx_server_commerce::public::receipt_http::{
    CommerceReceiptHttpBackend, CommerceReceiptHttpFuture, CommerceReceiptHttpRequest,
    CommerceReceiptHttpResponse, CommerceReceiptHttpState, CommerceReceiptHttpTarget,
    CommerceOriginPolicy, commerce_receipt_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

const ROUTE: &str = "/api/v1/holdings/acme/commerce/purchase-orders/order-1/goods-receipts";

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: Option<BusinessError>,
    bad: bool,
}

impl CommerceReceiptHttpBackend for Probe {
    fn receive<'a>(
        &'a self,
        request: CommerceReceiptHttpRequest<'a>,
    ) -> CommerceReceiptHttpFuture<'a, CommerceReceiptHttpResponse> {
        Box::pin(async move {
            let command = &request.command;
            let allocations = command
                .lines
                .iter()
                .map(|line| line.allocations.len())
                .sum::<usize>();
            let first = command
                .lines
                .first()
                .and_then(|line| line.allocations.first())
                .map(|allocation| {
                    format!(
                        "{}:{}:{}:{}:{}",
                        allocation.disposition.as_str(),
                        allocation.quantity_milli,
                        allocation.lot_code.as_deref().unwrap_or(""),
                        allocation.serial_number.as_deref().unwrap_or(""),
                        allocation.quarantine_reason.as_deref().unwrap_or(""),
                    )
                })
                .unwrap_or_default();
            self.seen.lock().unwrap().push(format!(
                "receipt|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.order_id,
                request.idempotency_key,
                command.expected_version,
                command.date,
                command.lines.len(),
                allocations,
                first,
            ));
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(CommerceReceiptHttpResponse {
                data: if self.bad {
                    serde_json::json!({"orderId":"order-1"})
                } else {
                    serde_json::json!({
                        "orderId": request.order_id,
                        "fulfillmentId": "delivery-commerce-receipt-0001",
                        "state": "partially_fulfilled",
                        "version": command.expected_version + 1,
                        "totalValueMinor": 12500,
                        "lines": [{"lineId":"line-1","quantityMilli":2500,"valueMinor":12500}]
                    })
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
) -> CommerceReceiptHttpTarget {
    CommerceReceiptHttpTarget {
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

fn post(path: &str, host: &str, key: Option<&str>, body: &str) -> String {
    let key = key
        .map(|value| format!("Idempotency-Key: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n{key}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn valid_body() -> &'static str {
    r#"{"completionMode":"goods","expectedVersion":3,"date":"2026-11-12","lines":[{"lineId":"line-1","quantity":"2.5","allocations":[{"lotCode":" LOT-1 ","quantity":"1.5","manufacturedDate":"2026-01-01","expiryDate":"2027-01-01","disposition":"available"},{"serialNumber":"SN-1","quantity":"1","disposition":"quarantine","quarantineReason":" Damaged carton ","quarantineDueDate":"2026-11-20"}]}]}"#
}

async fn server(probe: Probe) -> std::net::SocketAddr {
    let router = commerce_receipt_http_router(vec![CommerceReceiptHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(probe),
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    addr
}

#[tokio::test]
async fn actual_router_forwards_one_goods_receipt_with_private_headers() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let addr = server(Probe {
        seen: seen.clone(),
        fail: None,
        bad: false,
    })
    .await;

    let (status, body, headers) = http(
        addr,
        post(
            ROUTE,
            "acme.example.test",
            Some("commerce-receipt-0001"),
            valid_body(),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body["data"],
        serde_json::json!({
            "orderId":"order-1",
            "fulfillmentId":"delivery-commerce-receipt-0001",
            "state":"partially_fulfilled",
            "version":4,
            "totalValueMinor":12500,
            "lines":[{"lineId":"line-1","quantityMilli":2500,"valueMinor":12500}]
        })
    );
    assert_eq!(body["context"]["userId"], "user");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        ["receipt|acme.example.test|acme|order-1|commerce-receipt-0001|3|2026-11-12|1|2|available:1500:LOT-1::"]
    );
}

#[tokio::test]
async fn router_rejects_unknown_hosts_origins_queries_and_malformed_receipts() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = commerce_receipt_http_router(vec![target(
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

    let mut wrong_origin = post(
        ROUTE,
        "acme.example.test",
        Some("commerce-receipt-0002"),
        valid_body(),
    );
    wrong_origin = wrong_origin.replace(
        "Host: acme.example.test\r\n",
        "Host: acme.example.test\r\nOrigin: https://evil.example.test\r\n",
    );

    let duplicate_lot = r#"{"expectedVersion":3,"date":"2026-11-12","lines":[{"lineId":"line-1","quantity":"2","allocations":[{"lotCode":"LOT-1","quantity":"1"},{"lotCode":"LOT-1","quantity":"1"}]}]}"#;
    let no_identity =
        r#"{"expectedVersion":3,"date":"2026-11-12","lines":[{"lineId":"line-1","quantity":"1","allocations":[{"quantity":"1"}]}]}"#;
    let quarantine_without_reason = r#"{"expectedVersion":3,"date":"2026-11-12","lines":[{"lineId":"line-1","quantity":"1","allocations":[{"lotCode":"LOT-1","quantity":"1","disposition":"quarantine"}]}]}"#;
    let serial_over_one = r#"{"expectedVersion":3,"date":"2026-11-12","lines":[{"lineId":"line-1","quantity":"2","allocations":[{"serialNumber":"SN-1","quantity":"2"}]}]}"#;

    for (request, expected_status, expected_code) in [
        (
            post(
                "/api/v1/holdings/acme/commerce/purchase-orders/order-1/goods-receipts",
                "evil.example.test",
                Some("commerce-receipt-0002"),
                valid_body(),
            ),
            404,
            "NOT_FOUND",
        ),
        (
            post(
                "/api/v1/holdings/other/commerce/purchase-orders/order-1/goods-receipts",
                "acme.example.test",
                Some("commerce-receipt-0002"),
                valid_body(),
            ),
            404,
            "NOT_FOUND",
        ),
        (wrong_origin, 403, "ORIGIN_FORBIDDEN"),
        (
            post(
                "/api/v1/holdings/acme/commerce/purchase-orders/order-1/goods-receipts?print=1",
                "acme.example.test",
                Some("commerce-receipt-0002"),
                valid_body(),
            ),
            422,
            "COMMERCE_RECEIPT_REQUEST_INVALID",
        ),
        (
            post(ROUTE, "acme.example.test", None, valid_body()),
            422,
            "COMMAND_KEY_REQUIRED",
        ),
        (
            format!(
                "POST {ROUTE} HTTP/1.1\r\nHost: acme.example.test\r\nContent-Type: text/plain\r\nIdempotency-Key: commerce-receipt-0002\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                valid_body().len(),
                valid_body()
            ),
            422,
            "COMMERCE_RECEIPT_REQUEST_INVALID",
        ),
        (
            post(
                ROUTE,
                "acme.example.test",
                Some("commerce-receipt-0002"),
                r#"{"expectedVersion":3,"date":"2026-11-12","lines":[{"lineId":"line-1","quantity":"1"}],"extra":true}"#,
            ),
            422,
            "COMMERCE_RECEIPT_REQUEST_INVALID",
        ),
        (
            post(
                ROUTE,
                "acme.example.test",
                Some("commerce-receipt-0002"),
                r#"{"completionMode":"service","expectedVersion":3,"date":"2026-11-12","lines":[{"lineId":"line-1","quantity":"1"}]}"#,
            ),
            422,
            "COMMERCE_RECEIPT_REQUEST_INVALID",
        ),
        (
            post(
                ROUTE,
                "acme.example.test",
                Some("commerce-receipt-0002"),
                r#"{"expectedVersion":3,"date":"2026-11-12","lines":[{"lineId":"line-1","quantity":"1","lotId":"lot-1"}]}"#,
            ),
            422,
            "COMMERCE_RECEIPT_REQUEST_INVALID",
        ),
        (
            post(
                ROUTE,
                "acme.example.test",
                Some("commerce-receipt-0002"),
                no_identity,
            ),
            422,
            "STOCK_RECEIPT_IDENTITY_REQUIRED",
        ),
        (
            post(
                ROUTE,
                "acme.example.test",
                Some("commerce-receipt-0002"),
                duplicate_lot,
            ),
            422,
            "LOT_ALLOCATION_DUPLICATE",
        ),
        (
            post(
                ROUTE,
                "acme.example.test",
                Some("commerce-receipt-0002"),
                quarantine_without_reason,
            ),
            422,
            "QUARANTINE_EVIDENCE_REQUIRED",
        ),
        (
            post(
                ROUTE,
                "acme.example.test",
                Some("commerce-receipt-0002"),
                serial_over_one,
            ),
            422,
            "COMMERCE_RECEIPT_REQUEST_INVALID",
        ),
        (
            post(
                ROUTE,
                "acme.example.test",
                Some("commerce-receipt-0002"),
                r#"{"expectedVersion":3,"date":"2026-02-30","lines":[{"lineId":"line-1","quantity":"1"}]}"#,
            ),
            422,
            "COMMERCE_RECEIPT_REQUEST_INVALID",
        ),
        (
            post(
                ROUTE,
                "acme.example.test",
                Some("commerce-receipt-0002"),
                r#"{"expectedVersion":3,"date":"2026-11-12","lines":[{"lineId":"line-1","quantity":"0"}]}"#,
            ),
            422,
            "COMMERCE_RECEIPT_REQUEST_INVALID",
        ),
        (
            post(
                ROUTE,
                "acme.example.test",
                Some("commerce-receipt-0002"),
                r#"{"expectedVersion":3,"date":"2026-11-12","lines":[]}"#,
            ),
            422,
            "COMMERCE_RECEIPT_REQUEST_INVALID",
        ),
        (
            post(
                ROUTE,
                "acme.example.test",
                Some("commerce-receipt-0002"),
                "not-json",
            ),
            422,
            "COMMERCE_RECEIPT_REQUEST_INVALID",
        ),
    ] {
        let (status, body, _) = http(addr, request).await;
        assert_eq!(status, expected_status, "{body}");
        assert_eq!(body["error"]["code"], expected_code, "{body}");
    }

    let (status, _, _) = http(
        addr,
        format!("GET {ROUTE} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n"),
    )
    .await;
    assert_eq!(status, 405);
    assert_eq!(seen.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn backend_errors_and_invalid_receipts_map_to_owner_statuses() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let conflict = server(Probe {
        seen: seen.clone(),
        fail: Some(BusinessError::new(
            FailureKind::Conflict,
            "COMMERCE_ORDER_CHANGED",
            "The purchase order changed. Reload its current receipt source.",
        )),
        bad: false,
    })
    .await;
    let (status, body, _) = http(
        conflict,
        post(
            ROUTE,
            "acme.example.test",
            Some("commerce-receipt-0003"),
            valid_body(),
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "COMMERCE_ORDER_CHANGED");

    let unavailable = server(Probe {
        seen: seen.clone(),
        fail: Some(BusinessError::new(
            FailureKind::Unavailable,
            "COMMERCE_RECEIPT_SOURCE_INVALID",
            "The current Commerce purchase source is invalid.",
        )),
        bad: false,
    })
    .await;
    let (status, body, _) = http(
        unavailable,
        post(
            ROUTE,
            "acme.example.test",
            Some("commerce-receipt-0004"),
            valid_body(),
        ),
    )
    .await;
    assert_eq!(status, 503);
    assert_eq!(body["error"]["code"], "COMMERCE_RECEIPT_SOURCE_INVALID");

    let invalid_document = server(Probe {
        seen,
        fail: None,
        bad: true,
    })
    .await;
    let (status, body, _) = http(
        invalid_document,
        post(
            ROUTE,
            "acme.example.test",
            Some("commerce-receipt-0005"),
            valid_body(),
        ),
    )
    .await;
    assert_eq!(status, 503);
    assert_eq!(body["error"]["code"], "COMMERCE_RECEIPT_SOURCE_INVALID");
}

#[test]
fn configuration_rejects_duplicate_targets_and_bad_origins() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    assert!(
        CommerceReceiptHttpState::new(vec![target(
            "acme.example.test",
            "acme",
            seen.clone(),
            None,
            false,
        )])
        .is_ok()
    );
    assert!(
        CommerceReceiptHttpState::new(vec![
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
        assert!(CommerceReceiptHttpState::new(vec![invalid]).is_err());
    }
    let mut development = target(
        "acme.erpx.localhost:3443",
        "acme",
        seen.clone(),
        None,
        false,
    );
    development.origin = "https://acme.erpx.localhost:3443".into();
    assert!(
        CommerceReceiptHttpState::new_with_origin_policy(
            vec![development],
            CommerceOriginPolicy::NonProduction
        )
        .is_ok()
    );
    let mut bad_slug = target("acme.example.test", "Acme!", seen, None, false);
    bad_slug.holding_slug = "Acme!".into();
    assert!(CommerceReceiptHttpState::new(vec![bad_slug]).is_err());
}
