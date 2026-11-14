//! Actual owner-local customer directory HTTP router tests. These prove ingress
//! facts only; Account authority, Company scope and PostgreSQL remain separate
//! composition responsibilities.
use erpx_server_customer_360::public::customer_directory_http::{
    CustomerDirectoryHttpBackend, CustomerDirectoryHttpFuture, CustomerDirectoryHttpReadRequest,
    CustomerDirectoryHttpTarget, customer_directory_http_router,
};
use erpx_server_customer_360::public::directory::{CustomerDirectoryEntry, CustomerDirectoryPage};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl CustomerDirectoryHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: CustomerDirectoryHttpReadRequest<'a>,
    ) -> CustomerDirectoryHttpFuture<'a, CustomerDirectoryPage> {
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
    cursor: Option<String>,
    limit: Option<u16>,
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
    fail: Option<BusinessError>,
}

impl CustomerDirectoryHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: CustomerDirectoryHttpReadRequest<'a>,
    ) -> CustomerDirectoryHttpFuture<'a, CustomerDirectoryPage> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                host: request.host.into(),
                holding: request.holding_slug.into(),
                cursor: request.query.cursor.clone(),
                limit: request.query.limit,
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(CustomerDirectoryPage {
                items: vec![CustomerDirectoryEntry {
                    id: "customer".into(),
                    name: "Acme".into(),
                    code: "C-1".into(),
                }],
                has_more: true,
                next_cursor: Some("next".into()),
            })
        })
    }
}

fn target(backend: Arc<dyn CustomerDirectoryHttpBackend>) -> CustomerDirectoryHttpTarget {
    CustomerDirectoryHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend: Arc<dyn CustomerDirectoryHttpBackend> = Arc::new(Noop);
    assert!(customer_directory_http_router(vec![target(backend.clone())]).is_ok());
    assert!(
        customer_directory_http_router(vec![target(backend.clone()), target(backend.clone())])
            .is_err()
    );
    for invalid in [
        CustomerDirectoryHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target(backend.clone())
        },
        CustomerDirectoryHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target(backend.clone())
        },
        CustomerDirectoryHttpTarget {
            holding_slug: "Acme".into(),
            ..target(backend.clone())
        },
        CustomerDirectoryHttpTarget {
            host: String::new(),
            ..target(backend.clone())
        },
    ] {
        assert!(customer_directory_http_router(vec![invalid]).is_err());
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
async fn actual_router_forwards_bounded_page_and_maps_errors() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = customer_directory_http_router(vec![
        target(Arc::new(Probe {
            seen: seen.clone(),
            fail: None,
        })),
        CustomerDirectoryHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: Some(BusinessError::new(
                    FailureKind::Conflict,
                    "WORK_CONTEXT_CHANGED",
                    "Synthetic context change.",
                )),
            }),
        },
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, raw) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customers?limit=2&cursor=abc",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["items"][0]["id"], "customer");
    assert_eq!(body["data"]["hasMore"], true);
    assert_eq!(body["data"]["nextCursor"], "next");
    assert!(raw.contains("cache-control: private, no-store"));
    assert!(raw.contains("referrer-policy: no-referrer"));
    assert!(raw.contains("x-content-type-options: nosniff"));
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            host: "acme.example.test".into(),
            holding: "acme".into(),
            cursor: Some("abc".into()),
            limit: Some(2),
        })
    );

    let (status, _, _) = http(
        addr,
        get("/api/v1/holdings/acme/customers", "acme.example.test"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            host: "acme.example.test".into(),
            holding: "acme".into(),
            cursor: None,
            limit: None,
        })
    );

    let (status, body, _) = http(
        addr,
        get("/api/v1/holdings/acme/customers", "other.example.test"),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body, _) = http(
        addr,
        get("/api/v1/holdings/fail/customers", "fail.example.test"),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "WORK_CONTEXT_CHANGED");

    for (path, code) in [
        (
            "/api/v1/holdings/acme/customers?limit=0",
            "CUSTOMER_DIRECTORY_LIMIT_INVALID",
        ),
        (
            "/api/v1/holdings/acme/customers?limit=202",
            "CUSTOMER_DIRECTORY_LIMIT_INVALID",
        ),
        (
            "/api/v1/holdings/acme/customers?limit=x",
            "CUSTOMER_DIRECTORY_REQUEST_INVALID",
        ),
        (
            "/api/v1/holdings/acme/customers?unknown=1",
            "CUSTOMER_DIRECTORY_REQUEST_INVALID",
        ),
        (
            "/api/v1/holdings/acme/customers?limit=2&limit=3",
            "CUSTOMER_DIRECTORY_REQUEST_INVALID",
        ),
        (
            "/api/v1/holdings/acme/customers?cursor=",
            "CUSTOMER_DIRECTORY_CURSOR_INVALID",
        ),
    ] {
        let (status, body, _) = http(addr, get(path, "acme.example.test")).await;
        assert_eq!(status, 422, "path: {path}");
        assert_eq!(body["error"]["code"], code, "path: {path}");
    }
}
