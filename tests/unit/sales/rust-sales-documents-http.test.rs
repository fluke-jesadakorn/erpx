//! Actual owner-local HTTP router tests for the native Sales document list.
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_sales::public::sales_documents_http::{
    SalesDocumentsHttpBackend, SalesDocumentsHttpFuture, SalesDocumentsHttpRequest,
    SalesDocumentsHttpResponse, SalesDocumentsHttpTarget, sales_documents_router,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl SalesDocumentsHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: SalesDocumentsHttpRequest<'a>,
    ) -> SalesDocumentsHttpFuture<'a, SalesDocumentsHttpResponse> {
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

impl SalesDocumentsHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: SalesDocumentsHttpRequest<'a>,
    ) -> SalesDocumentsHttpFuture<'a, SalesDocumentsHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}",
                request.kind,
                request.search,
                request.status.unwrap_or(""),
                request.cursor.unwrap_or("")
            ));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "SALES_DOCUMENTS_CHANGED",
                    "Synthetic changed sales document source.",
                ));
            }
            Ok(SalesDocumentsHttpResponse {
                data: serde_json::json!({
                    "items": [{
                        "id": "order-1",
                        "company_id": "company",
                        "branch_id": "branch",
                        "department_id": null,
                        "kind": request.kind,
                        "number": "QT-1",
                        "party_id": "customer",
                        "party_name": "Customer",
                        "state": "draft",
                        "order_date": "2026-11-04",
                        "due_date": "2026-11-18",
                        "currency": "THB",
                        "subtotal_minor": 1000,
                        "tax_minor": 70,
                        "total_minor": 1070,
                        "version": 1,
                        "created_at": 10
                    }],
                    "nextCursor": null,
                    "hasMore": false
                }),
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || SalesDocumentsHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(sales_documents_router(vec![target()]).is_ok());
    assert!(sales_documents_router(vec![target(), target()]).is_err());
    assert!(
        sales_documents_router(vec![SalesDocumentsHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        sales_documents_router(vec![SalesDocumentsHttpTarget {
            origin: "https://acme.example.test/path".into(),
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
    format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
}

#[tokio::test]
async fn actual_router_forwards_exact_query_and_maps_business_errors() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = sales_documents_router(vec![
        SalesDocumentsHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: false,
            }),
        },
        SalesDocumentsHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: true,
            }),
        },
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/sales/documents?kind=quote&q=acme&status=draft&cursor=a%2Bb%3Dc",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["items"][0]["kind"], "quote");
    assert_eq!(body["data"]["hasMore"], false);
    assert_eq!(body["context"]["userId"], "user");

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/fail/sales/documents?kind=sales&q=A%20B",
            "fail.example.test",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "SALES_DOCUMENTS_CHANGED");

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen, vec!["quote|acme|draft|a+b=c", "sales|A B||"]);

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/sales/documents?kind=quote&kind=sales",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "SALES_DOCUMENTS_REQUEST_INVALID");

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/sales/documents?kind=quote&view=all",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "SALES_DOCUMENTS_REQUEST_INVALID");

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/sales/documents?kind=quote&status=paid",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "SALES_DOCUMENT_STATUS_INVALID");

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/sales/documents?kind=purchase",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "SALES_DOCUMENT_KIND_INVALID");

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/sales/documents?q=acme",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "SALES_DOCUMENT_KIND_INVALID");

    let oversized = format!(
        "/api/v1/holdings/acme/sales/documents?kind=quote&q={}",
        "x".repeat(201)
    );
    let (status, body, _) = http(addr, get(&oversized, "acme.example.test")).await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "SALES_DOCUMENTS_REQUEST_INVALID");

    let (status, _, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/sales/documents?kind=quote",
            "missing.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, _, _) = http(
        addr,
        get(
            "/api/v1/holdings/missing/sales/documents?kind=quote",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);

    server.abort();
}

#[tokio::test]
async fn response_headers_are_private_and_bounded() {
    let router = sales_documents_router(vec![SalesDocumentsHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(Probe {
            seen: Arc::new(Mutex::new(Vec::new())),
            fail: false,
        }),
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    let (_, _, text) = http(
        addr,
        get(
            "/api/v1/holdings/acme/sales/documents?kind=quote",
            "acme.example.test",
        ),
    )
    .await;
    let text = text.to_ascii_lowercase();
    assert!(text.contains("cache-control: private, no-store"));
    assert!(text.contains("referrer-policy: no-referrer"));
    assert!(text.contains("x-content-type-options: nosniff"));
    server.abort();
}
