//! Actual owner-local Customer 360 timeline activity HTTP router tests. These
//! prove ingress facts only; Account authority, source gates, Support purchase
//! and PostgreSQL remain separate composition responsibilities.
use erpx_customer_360_server::public::customer_timeline_activity::{
    CustomerTimelineActivityQuery, CustomerTimelineActivityRow,
};
use erpx_customer_360_server::public::customer_timeline_activity_http::{
    CustomerTimelineActivityHttpBackend, CustomerTimelineActivityHttpFuture,
    CustomerTimelineActivityHttpReadRequest, CustomerTimelineActivityHttpTarget,
    customer_timeline_activity_http_router,
};
use erpx_platform_runtime_server::public::http_support::{serde_json, serve};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl CustomerTimelineActivityHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: CustomerTimelineActivityHttpReadRequest<'a>,
    ) -> CustomerTimelineActivityHttpFuture<'a, Vec<CustomerTimelineActivityRow>> {
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
    from: i64,
    to: i64,
    customer_id: Option<String>,
    query: Option<String>,
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
    fail: Option<BusinessError>,
}

impl CustomerTimelineActivityHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: CustomerTimelineActivityHttpReadRequest<'a>,
    ) -> CustomerTimelineActivityHttpFuture<'a, Vec<CustomerTimelineActivityRow>> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                host: request.host.into(),
                holding: request.holding_slug.into(),
                from: request.query.from,
                to: request.query.to,
                customer_id: request.query.customer_id.clone(),
                query: request.query.query.clone(),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(vec![CustomerTimelineActivityRow {
                id: "order:order-main".into(),
                source_id: "order-main".into(),
                source_type: "order".into(),
                title: "sales · SO-100".into(),
                status: "confirmed".into(),
                version: 1,
                at: 100,
                href: "/commerce/orders/order-main".into(),
                customer_id: "customer".into(),
                customer_name: "Acme Customer".into(),
            }])
        })
    }
}

fn target(backend: Arc<dyn CustomerTimelineActivityHttpBackend>) -> CustomerTimelineActivityHttpTarget {
    CustomerTimelineActivityHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend: Arc<dyn CustomerTimelineActivityHttpBackend> = Arc::new(Noop);
    assert!(customer_timeline_activity_http_router(vec![target(backend.clone())]).is_ok());
    assert!(
        customer_timeline_activity_http_router(vec![target(backend.clone()), target(backend.clone())])
            .is_err()
    );
    for invalid in [
        CustomerTimelineActivityHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target(backend.clone())
        },
        CustomerTimelineActivityHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target(backend.clone())
        },
        CustomerTimelineActivityHttpTarget {
            holding_slug: "Acme".into(),
            ..target(backend.clone())
        },
        CustomerTimelineActivityHttpTarget {
            host: String::new(),
            ..target(backend.clone())
        },
    ] {
        assert!(customer_timeline_activity_http_router(vec![invalid]).is_err());
    }
}

#[test]
fn strict_query_parse() {
    // The public query type is the parse boundary; exercise its strict bounds.
    assert!(CustomerTimelineActivityQuery {
        from: 0,
        to: 1_000_000,
        customer_id: Some("customer".into()),
        query: Some("Acme".into()),
    }
    .validate()
    .is_ok());
    assert!(CustomerTimelineActivityQuery {
        from: 10,
        to: 10,
        customer_id: None,
        query: None,
    }
    .validate()
    .is_err());
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
    (status, serde_json::from_str(body).unwrap_or(serde_json::Value::Null), text)
}

fn get(path: &str, host: &str, origin: Option<&str>) -> String {
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\n{}Connection: close\r\n\r\n",
        origin.map(|value| format!("Origin: {value}\r\n")).unwrap_or_default()
    )
}

#[tokio::test]
async fn actual_router_forwards_bounded_activity_and_maps_errors() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = customer_timeline_activity_http_router(vec![
        target(Arc::new(Probe { seen: seen.clone(), fail: None })),
        CustomerTimelineActivityHttpTarget {
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

    let path = "/api/v1/holdings/acme/customers/timeline-activity?from=0&to=1000000&customerId=customer&query=Acme";
    let (status, body, raw) = http(addr, get(path, "acme.example.test", None)).await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["rows"][0]["id"], "order:order-main");
    assert!(raw.contains("cache-control: private, no-store"));
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            host: "acme.example.test".into(),
            holding: "acme".into(),
            from: 0,
            to: 1_000_000,
            customer_id: Some("customer".into()),
            query: Some("Acme".into()),
        })
    );

    let (status, body, _) = http(
        addr,
        get(path, "acme.example.test", Some("https://other.example.test")),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, body, _) = http(addr, get(path, "other.example.test", None)).await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body, _) = http(
        addr,
        get("/api/v1/holdings/fail/customers/timeline-activity?from=0&to=1000000", "fail.example.test", None),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "WORK_CONTEXT_CHANGED");

    for (path, code) in [
        ("/api/v1/holdings/acme/customers/timeline-activity", "CUSTOMER_TIMELINE_ACTIVITY_REQUEST_INVALID"),
        ("/api/v1/holdings/acme/customers/timeline-activity?from=0", "CUSTOMER_TIMELINE_ACTIVITY_REQUEST_INVALID"),
        ("/api/v1/holdings/acme/customers/timeline-activity?from=10&to=10", "CUSTOMER_TIMELINE_ACTIVITY_QUERY_INVALID"),
        ("/api/v1/holdings/acme/customers/timeline-activity?from=0&to=1000000&customerId=", "CUSTOMER_TIMELINE_ACTIVITY_QUERY_INVALID"),
        ("/api/v1/holdings/acme/customers/timeline-activity?from=0&to=1000000&unknown=1", "CUSTOMER_TIMELINE_ACTIVITY_REQUEST_INVALID"),
        ("/api/v1/holdings/acme/customers/timeline-activity?from=0&to=1000000&from=1", "CUSTOMER_TIMELINE_ACTIVITY_REQUEST_INVALID"),
    ] {
        let (status, body, _) = http(addr, get(path, "acme.example.test", None)).await;
        assert_eq!(status, 422, "path: {path}");
        assert_eq!(body["error"]["code"], code, "path: {path}");
    }
}
