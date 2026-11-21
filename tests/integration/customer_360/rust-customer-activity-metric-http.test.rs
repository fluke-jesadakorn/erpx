//! Actual owner-local Executive customer-activity metric HTTP source tests.
//! These prove ingress facts only; Account authority, source gates and
//! PostgreSQL remain separate composition responsibilities.
use erpx_server_customer_360::public::customer_activity_metric::{
    CustomerActivityBreakdown, CustomerActivityMetricGroup,
};
use erpx_server_customer_360::public::customer_activity_metric_http::{
    CustomerActivityMetricHttpBackend, CustomerActivityMetricHttpFuture,
    CustomerActivityMetricHttpReadRequest, CustomerActivityMetricHttpTarget,
    customer_activity_metric_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl CustomerActivityMetricHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: CustomerActivityMetricHttpReadRequest<'a>,
    ) -> CustomerActivityMetricHttpFuture<'a, Vec<CustomerActivityMetricGroup>> {
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
    breakdown: CustomerActivityBreakdown,
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
    fail: Option<BusinessError>,
}

impl CustomerActivityMetricHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: CustomerActivityMetricHttpReadRequest<'a>,
    ) -> CustomerActivityMetricHttpFuture<'a, Vec<CustomerActivityMetricGroup>> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                host: request.host.into(),
                holding: request.holding_slug.into(),
                from: request.query.from,
                to: request.query.to,
                breakdown: request.query.breakdown,
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(vec![CustomerActivityMetricGroup {
                label: "2026-10".into(),
                value: "9".into(),
            }])
        })
    }
}

fn target(backend: Arc<dyn CustomerActivityMetricHttpBackend>) -> CustomerActivityMetricHttpTarget {
    CustomerActivityMetricHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend: Arc<dyn CustomerActivityMetricHttpBackend> = Arc::new(Noop);
    assert!(customer_activity_metric_http_router(vec![target(backend.clone())]).is_ok());
    assert!(
        customer_activity_metric_http_router(vec![
            target(backend.clone()),
            target(backend.clone())
        ])
        .is_err()
    );
    for invalid in [
        CustomerActivityMetricHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target(backend.clone())
        },
        CustomerActivityMetricHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target(backend.clone())
        },
        CustomerActivityMetricHttpTarget {
            holding_slug: "Acme".into(),
            ..target(backend.clone())
        },
        CustomerActivityMetricHttpTarget {
            host: String::new(),
            ..target(backend.clone())
        },
    ] {
        assert!(customer_activity_metric_http_router(vec![invalid]).is_err());
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

fn get(path: &str, host: &str, origin: Option<&str>) -> String {
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\n{}Connection: close\r\n\r\n",
        origin
            .map(|value| format!("Origin: {value}\r\n"))
            .unwrap_or_default()
    )
}

#[tokio::test]
async fn actual_router_forwards_bounded_metric_and_maps_errors() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = customer_activity_metric_http_router(vec![
        target(Arc::new(Probe {
            seen: seen.clone(),
            fail: None,
        })),
        CustomerActivityMetricHttpTarget {
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

    let path =
        "/api/v1/holdings/acme/customers/activity-metric?from=0&to=1000000&breakdown=source_type";
    let (status, body, raw) = http(addr, get(path, "acme.example.test", None)).await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["groups"][0]["label"], "2026-10");
    assert_eq!(body["data"]["groups"][0]["value"], "9");
    assert!(raw.contains("cache-control: private, no-store"));
    assert!(raw.contains("referrer-policy: no-referrer"));
    assert!(raw.contains("x-content-type-options: nosniff"));
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            host: "acme.example.test".into(),
            holding: "acme".into(),
            from: 0,
            to: 1_000_000,
            breakdown: CustomerActivityBreakdown::SourceType,
        })
    );

    let (status, body, _) = http(
        addr,
        get(
            path,
            "acme.example.test",
            Some("https://other.example.test"),
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, body, _) = http(addr, get(path, "other.example.test", None)).await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/fail/customers/activity-metric?from=0&to=1000000&breakdown=month",
            "fail.example.test",
            None,
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "WORK_CONTEXT_CHANGED");

    for (path, code) in [
        (
            "/api/v1/holdings/acme/customers/activity-metric",
            "CUSTOMER_ACTIVITY_METRIC_REQUEST_INVALID",
        ),
        (
            "/api/v1/holdings/acme/customers/activity-metric?from=0&to=1000000",
            "CUSTOMER_ACTIVITY_METRIC_REQUEST_INVALID",
        ),
        (
            "/api/v1/holdings/acme/customers/activity-metric?from=0&to=1000000&breakdown=week",
            "CUSTOMER_ACTIVITY_METRIC_REQUEST_INVALID",
        ),
        (
            "/api/v1/holdings/acme/customers/activity-metric?from=10&to=10&breakdown=month",
            "CUSTOMER_ACTIVITY_RANGE_INVALID",
        ),
        (
            "/api/v1/holdings/acme/customers/activity-metric?from=0&to=1000000&breakdown=month&unknown=1",
            "CUSTOMER_ACTIVITY_METRIC_REQUEST_INVALID",
        ),
    ] {
        let (status, body, _) = http(addr, get(path, "acme.example.test", None)).await;
        assert_eq!(status, 422, "path: {path}");
        assert_eq!(body["error"]["code"], code, "path: {path}");
    }
}
