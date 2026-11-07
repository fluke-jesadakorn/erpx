//! Actual owner-local Customer 360 timeline HTTP router tests. These prove
//! ingress facts only; composition, Account session, Support purchase and
//! PostgreSQL remain separate responsibilities.
use erpx_server_customer_360::public::customer_timeline_http::{
    CustomerTimelineHttpBackend, CustomerTimelineHttpFuture, CustomerTimelineHttpReadRequest,
    CustomerTimelineHttpResponse, CustomerTimelineHttpTarget, customer_timeline_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl CustomerTimelineHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: CustomerTimelineHttpReadRequest<'a>,
    ) -> CustomerTimelineHttpFuture<'a, CustomerTimelineHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Seen {
    host: String,
    holding: String,
    customer: String,
    cursor: Option<String>,
    limit: Option<u16>,
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
    fail: Option<BusinessError>,
}

impl CustomerTimelineHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: CustomerTimelineHttpReadRequest<'a>,
    ) -> CustomerTimelineHttpFuture<'a, CustomerTimelineHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                host: request.host.into(),
                holding: request.holding_slug.into(),
                customer: request.customer_id.into(),
                cursor: request.query.cursor,
                limit: request.query.limit,
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(CustomerTimelineHttpResponse {
                data: serde_json::json!({
                    "customer": {"id": "customer-1", "name": "Acme Customer", "code": "C-001"},
                    "items": [],
                    "hasMore": false,
                    "nextCursor": null,
                    "coverage": ["Sales quotations/orders"],
                    "limitations": "Internal staff projection."
                }),
                context: serde_json::json!({
                    "userId": "user",
                    "companyId": "company",
                    "permissions": ["commerce.read"]
                }),
            })
        })
    }
}

fn target(backend: Arc<dyn CustomerTimelineHttpBackend>) -> CustomerTimelineHttpTarget {
    CustomerTimelineHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend: Arc<dyn CustomerTimelineHttpBackend> = Arc::new(Noop);
    assert!(customer_timeline_http_router(vec![target(backend.clone())]).is_ok());
    assert!(
        customer_timeline_http_router(vec![target(backend.clone()), target(backend.clone())])
            .is_err()
    );
    for invalid in [
        CustomerTimelineHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target(backend.clone())
        },
        CustomerTimelineHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target(backend.clone())
        },
        CustomerTimelineHttpTarget {
            holding_slug: "Acme".into(),
            ..target(backend.clone())
        },
        CustomerTimelineHttpTarget {
            host: String::new(),
            ..target(backend.clone())
        },
    ] {
        assert!(customer_timeline_http_router(vec![invalid]).is_err());
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

#[tokio::test]
async fn actual_router_forwards_exact_customer_query_and_maps_business_errors() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = customer_timeline_http_router(vec![
        target(Arc::new(Probe {
            seen: seen.clone(),
            fail: None,
        })),
        CustomerTimelineHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: Some(BusinessError::new(
                    FailureKind::Conflict,
                    "CUSTOMER_TIMELINE_SOURCE_CHANGED",
                    "Synthetic changed timeline source.",
                )),
            }),
        },
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, raw) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customers/customer-1/timeline?cursor=a%2Bb%3Dc&limit=7",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["customer"]["name"], "Acme Customer");
    assert_eq!(body["context"]["companyId"], "company");
    assert!(raw.contains("cache-control: private, no-store"));
    assert!(raw.contains("referrer-policy: no-referrer"));
    assert!(raw.contains("x-content-type-options: nosniff"));
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            host: "acme.example.test".into(),
            holding: "acme".into(),
            customer: "customer-1".into(),
            cursor: Some("a+b=c".into()),
            limit: Some(7),
        })
    );

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/fail/customers/customer-1/timeline",
            "fail.example.test",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "CUSTOMER_TIMELINE_SOURCE_CHANGED");

    for path in [
        "/api/v1/holdings/acme/customers/customer-1/timeline?cursor=one&cursor=two",
        "/api/v1/holdings/acme/customers/customer-1/timeline?q=one",
        "/api/v1/holdings/acme/customers/customer-1/timeline?limit=0",
        "/api/v1/holdings/acme/customers/customer-1/timeline?limit=101",
        "/api/v1/holdings/acme/customers/customer-1/timeline?limit=abc",
        "/api/v1/holdings/acme/customers/customer-1/timeline?cursor=",
        "/api/v1/holdings/acme/customers/customer-1/timeline?cursor=%07",
        "/api/v1/holdings/acme/customers/customer-1/timeline?limit=7&limit=8",
    ] {
        let (status, body, _) = http(addr, get(path, "acme.example.test")).await;
        assert_eq!(status, 422, "path: {path}");
        assert!(
            [
                "CUSTOMER_TIMELINE_REQUEST_INVALID",
                "CUSTOMER_TIMELINE_CURSOR_INVALID",
                "CUSTOMER_TIMELINE_LIMIT_INVALID"
            ]
            .contains(&body["error"]["code"].as_str().unwrap_or("")),
            "path: {path} code: {}",
            body["error"]["code"]
        );
    }

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customers/customer%20one/timeline",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "CUSTOMER_TIMELINE_CUSTOMER_INVALID");

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customers/customer-1/timeline",
            "other.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/other/customers/customer-1/timeline",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, _body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customers/customer-1/timeline/extra",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);

    // A GET with a body is refused before the owner backend runs.
    let (status, body, _) = http(
        addr,
        "GET /api/v1/holdings/acme/customers/customer-1/timeline HTTP/1.1\r\n\
         Host: acme.example.test\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"
            .into(),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "CUSTOMER_TIMELINE_REQUEST_INVALID");

    server.abort();
}
