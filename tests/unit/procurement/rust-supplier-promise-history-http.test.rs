//! Actual owner-local HTTP router tests for native Procurement supplier
//! promise history.
use erpx_server_procurement::public::supplier_promise_history_http::{
    SupplierPromiseHistoryHttpBackend, SupplierPromiseHistoryHttpFuture,
    SupplierPromiseHistoryHttpRequest, SupplierPromiseHistoryHttpResponse,
    SupplierPromiseHistoryHttpTarget, supplier_promise_history_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl SupplierPromiseHistoryHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: SupplierPromiseHistoryHttpRequest<'a>,
    ) -> SupplierPromiseHistoryHttpFuture<'a, SupplierPromiseHistoryHttpResponse> {
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

impl SupplierPromiseHistoryHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: SupplierPromiseHistoryHttpRequest<'a>,
    ) -> SupplierPromiseHistoryHttpFuture<'a, SupplierPromiseHistoryHttpResponse> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap()
                .push(format!("read:{}", request.order_id));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "SUPPLIER_PROMISE_HISTORY_CHANGED",
                    "Synthetic changed supplier history.",
                ));
            }
            Ok(SupplierPromiseHistoryHttpResponse {
                data: serde_json::json!({
                    "order_id": request.order_id,
                    "history": [],
                    "lines": [],
                    "open": []
                }),
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || SupplierPromiseHistoryHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(supplier_promise_history_router(vec![target()]).is_ok());
    assert!(supplier_promise_history_router(vec![target(), target()]).is_err());
    assert!(
        supplier_promise_history_router(vec![SupplierPromiseHistoryHttpTarget {
            origin: "http://acme.example.test".into(),
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
    let text = String::from_utf8_lossy(&bytes);
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

#[tokio::test]
async fn actual_router_retains_target_order_and_error_mapping() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(Probe {
        seen: seen.clone(),
        fail: false,
    });
    let failing = Arc::new(Probe {
        seen: seen.clone(),
        fail: true,
    });
    let router = supplier_promise_history_router(vec![
        SupplierPromiseHistoryHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: probe,
        },
        SupplierPromiseHistoryHttpTarget {
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
    let get = |path: &str| {
        format!("GET {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n")
    };
    let (status, body) = http(
        addr,
        get("/api/v1/holdings/acme/procurement/purchase-orders/order-1/supplier-promises"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["order_id"], "order-1");
    assert_eq!(body["data"]["history"], serde_json::json!([]));
    assert_eq!(body["context"]["userId"], "user");

    let oversized = "a".repeat(201);
    let (status, body) = http(
        addr,
        get(&format!(
            "/api/v1/holdings/acme/procurement/purchase-orders/{oversized}/supplier-promises"
        )),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "PROCUREMENT_REQUEST_INVALID");

    let (status, body) = http(
        addr,
        get("/api/v1/holdings/acme/procurement/purchase-orders/order-1/supplier-promises?view=all"),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "PROCUREMENT_REQUEST_INVALID");

    let (status, _) = http(
        addr,
        get("/api/v1/holdings/missing/procurement/purchase-orders/order-1/supplier-promises"),
    )
    .await;
    assert_eq!(status, 404);

    let (status, body) = http(
        addr,
        "GET /api/v1/holdings/fail/procurement/purchase-orders/order-1/supplier-promises HTTP/1.1\r\nHost: fail.example.test\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "SUPPLIER_PROMISE_HISTORY_CHANGED");
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen, vec!["read:order-1", "read:order-1"]);
    server.abort();
}

#[tokio::test]
async fn response_headers_are_private_and_bounded() {
    let probe = Arc::new(Probe {
        seen: Arc::new(Mutex::new(Vec::new())),
        fail: false,
    });
    let router = supplier_promise_history_router(vec![SupplierPromiseHistoryHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: probe,
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(
            b"GET /api/v1/holdings/acme/procurement/purchase-orders/order-1/supplier-promises HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
    assert!(text.contains("cache-control: private, no-store"));
    assert!(text.contains("referrer-policy: no-referrer"));
    assert!(text.contains("x-content-type-options: nosniff"));
    server.abort();
}
