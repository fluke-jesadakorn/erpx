//! Actual owner-local HTTP router tests for the native Manufacturing BOM
//! register read.
use erpx_server_manufacturing::public::bom_http::{
    BomHttpBackend, BomHttpFuture, BomHttpRequest, BomHttpResponse, BomHttpTarget, bom_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl BomHttpBackend for Noop {
    fn read<'a>(&'a self, _: BomHttpRequest<'a>) -> BomHttpFuture<'a, BomHttpResponse> {
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

impl BomHttpBackend for Probe {
    fn read<'a>(&'a self, request: BomHttpRequest<'a>) -> BomHttpFuture<'a, BomHttpResponse> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap()
                .push(format!("read:{}", request.holding_slug));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "MANUFACTURING_BOM_SOURCE_CHANGED",
                    "Synthetic changed BOM.",
                ));
            }
            Ok(BomHttpResponse {
                data: serde_json::json!({
                    "specifications": [{
                        "id": "spec",
                        "company_id": "company",
                        "name": "Bounded assembly",
                        "mode": "bom",
                        "output_product_id": "output",
                        "batch_quantity_milli": 10000,
                        "expected_yield_bps": 9800,
                        "status": "draft",
                        "created_by": "maker",
                        "reviewed_by": null,
                        "version": 1,
                        "created_at": 100,
                        "updated_at": 100,
                        "lines": [],
                        "operations": []
                    }]
                }),
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

fn target(backend: Arc<dyn BomHttpBackend>) -> BomHttpTarget {
    BomHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

#[test]
fn target_configuration_requires_distinct_exact_https_holdings() {
    let backend = Arc::new(Noop);
    assert!(bom_router(vec![target(backend.clone())]).is_ok());
    assert!(bom_router(vec![target(backend.clone()), target(backend.clone())]).is_err());
    assert!(
        bom_router(vec![BomHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target(backend)
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
        text.to_ascii_lowercase(),
    )
}

#[tokio::test]
async fn actual_router_retains_target_holding_and_error_mapping() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(Probe {
        seen: seen.clone(),
        fail: false,
    });
    let failing = Arc::new(Probe {
        seen: seen.clone(),
        fail: true,
    });
    let router = bom_router(vec![
        target(probe),
        BomHttpTarget {
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

    let (status, body, _) = http(
        addr,
        get("/api/v1/holdings/acme/manufacturing/specifications"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["specifications"][0]["id"], "spec");
    assert_eq!(body["data"]["specifications"][0]["status"], "draft");
    assert_eq!(body["context"]["userId"], "user");

    let (status, body, _) = http(
        addr,
        get("/api/v1/holdings/acme/manufacturing/specifications?view=all"),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "MANUFACTURING_REQUEST_INVALID");

    let (status, _, _) = http(
        addr,
        get("/api/v1/holdings/missing/manufacturing/specifications"),
    )
    .await;
    assert_eq!(status, 404);

    let (status, body, _) = http(
        addr,
        "GET /api/v1/holdings/fail/manufacturing/specifications HTTP/1.1\r\nHost: fail.example.test\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "MANUFACTURING_BOM_SOURCE_CHANGED");

    let (status, _, headers) = http(
        addr,
        get("/api/v1/holdings/acme/manufacturing/specifications"),
    )
    .await;
    assert_eq!(status, 200);
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    assert_eq!(
        seen.lock().unwrap().clone(),
        vec!["read:acme", "read:fail", "read:acme"]
    );
    server.abort();
}
