//! Native Platform Executive public contract, authorization and HTTP ingress.
#![allow(dead_code)]
use erpx_server_platform_administration::public::platform_executive::*;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Store {
    failed: Vec<PlatformExecutiveGroup>,
    abnormal: Vec<PlatformExecutiveGroup>,
    failed_sources: Vec<ProvisioningFailureSourceRow>,
    abnormal_sources: Vec<AbnormalHealthSourceRow>,
    calls: Mutex<Vec<String>>,
}

impl PlatformExecutiveStore for Store {
    fn failed_provisioning<'a>(
        &'a self,
        _: i64,
        _: i64,
    ) -> PlatformExecutiveFuture<'a, Vec<PlatformExecutiveGroup>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push("failed".into());
            Ok(self.failed.clone())
        })
    }
    fn abnormal_services<'a>(&'a self) -> PlatformExecutiveFuture<'a, Vec<PlatformExecutiveGroup>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push("abnormal".into());
            Ok(self.abnormal.clone())
        })
    }
    fn failed_provisioning_sources<'a>(
        &'a self,
        _: i64,
        _: i64,
        label: &'a str,
        offset: u32,
    ) -> PlatformExecutiveFuture<'a, Vec<ProvisioningFailureSourceRow>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(format!("failed-sources:{label}:{offset}"));
            Ok(self.failed_sources.clone())
        })
    }
    fn abnormal_service_sources<'a>(
        &'a self,
        label: &'a str,
        offset: u32,
    ) -> PlatformExecutiveFuture<'a, Vec<AbnormalHealthSourceRow>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(format!("abnormal-sources:{label}:{offset}"));
            Ok(self.abnormal_sources.clone())
        })
    }
}

fn actor(permission: bool) -> PlatformExecutiveActor {
    PlatformExecutiveActor {
        user_id: "operator".into(),
        assignment_id: "platform-assignment".into(),
        role: "platform_auditor".into(),
        permissions: if permission {
            vec![EXECUTIVE_PERMISSION.into()]
        } else {
            Vec::new()
        },
    }
}

fn group(label: &str, value: &str) -> PlatformExecutiveGroup {
    PlatformExecutiveGroup {
        label: label.into(),
        value: value.into(),
    }
}

#[tokio::test]
async fn public_snapshot_is_bounded_authorized_and_source_revisioned() {
    let store = Store {
        failed: vec![group("2026-06", "2"), group("2026-07", "1")],
        abnormal: vec![group("jobs · degraded", "1")],
        failed_sources: vec![ProvisioningFailureSourceRow {
            created_at: 1,
            action: "failed".into(),
        }],
        abnormal_sources: vec![AbnormalHealthSourceRow {
            component: "jobs".into(),
            status: "degraded".into(),
            checked_at: 2,
        }],
        calls: Mutex::new(Vec::new()),
    };
    let filters = ExecutiveDateRangeInput {
        from: Some("2026-01-01".into()),
        to: Some("2026-12-31".into()),
    };
    let snapshot =
        platform_executive_snapshot(&actor(true), &store, Some(&filters), 1_800_000_000_000)
            .await
            .unwrap();
    assert_eq!(snapshot.scope, EXECUTIVE_SCOPE);
    assert_eq!(snapshot.metrics[0].total, "3");
    assert_eq!(snapshot.metrics[1].total, "1");
    assert_eq!(snapshot.source_revision.len(), 64);
    assert_eq!(
        snapshot.metrics[0].source_references[0].revision,
        snapshot.source_revision
    );
    assert!(
        serde_json::to_string(&snapshot)
            .unwrap()
            .find("2026-06")
            .is_some()
    );
    let request = validate_source_request(
        "failed-provisioning",
        "2026-06",
        "2026-01-01",
        "2026-12-31",
        Some("2"),
    )
    .unwrap();
    let sources = platform_executive_sources(&actor(true), &store, &request)
        .await
        .unwrap();
    assert_eq!(sources.rows.len(), 1);
    assert!(!sources.has_next);
    assert_eq!(sources.page, 2);
    let calls = store.calls.lock().unwrap().clone();
    assert!(calls.iter().any(|value| value == "failed"));
    assert!(
        calls
            .iter()
            .any(|value| value == "failed-sources:2026-06:30")
    );
    assert_eq!(
        platform_executive_snapshot(&actor(false), &store, Some(&filters), 1)
            .await
            .unwrap_err()
            .code,
        "PLATFORM_EXECUTIVE_FORBIDDEN"
    );
}

#[derive(Default)]
struct Probe {
    seen: Mutex<Vec<String>>,
    fail: bool,
}
impl PlatformExecutiveHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: PlatformExecutiveHttpReadRequest<'a>,
    ) -> PlatformExecutiveHttpFuture<'a, PlatformExecutiveHttpResponse> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap()
                .push(format!("{}:{:?}", request.host, request.query.metric));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "PLATFORM_EXECUTIVE_UNAVAILABLE",
                    "Synthetic unavailable.",
                ));
            }
            Ok(PlatformExecutiveHttpResponse {
                data: serde_json::json!({"scope":"platform","metrics":[]}),
                context: serde_json::json!({"userId":"operator"}),
            })
        })
    }
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
async fn public_router_requires_exact_host_and_maps_query_and_backend_failures() {
    let probe = Arc::new(Probe::default());
    let failing = Arc::new(Probe {
        seen: Mutex::new(Vec::new()),
        fail: true,
    });
    let router = platform_executive_router(vec![
        PlatformExecutiveHttpTarget {
            host: "erpx-internal.example.test".into(),
            backend: probe.clone(),
        },
        PlatformExecutiveHttpTarget {
            host: "fail.example.test".into(),
            backend: failing,
        },
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
    });
    let (status, body) = http(
        addr,
        "GET /api/v1/platform/executive HTTP/1.1\r\nHost: erpx-internal.example.test\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["scope"], "platform");
    let (status, _body) = http(
        addr,
        "GET /api/v1/platform/executive?metric=abnormal-platform-services&group=jobs&from=2026-01-01&to=2026-12-31 HTTP/1.1\r\nHost: erpx-internal.example.test\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert_eq!(status, 200);
    let (status, body) = http(
        addr,
        "GET /api/v1/platform/executive?unknown=1 HTTP/1.1\r\nHost: erpx-internal.example.test\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "PLATFORM_EXECUTIVE_QUERY_INVALID");
    let (status, body) = http(
        addr,
        "GET /api/v1/platform/executive HTTP/1.1\r\nHost: fail.example.test\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert_eq!(status, 503);
    assert_eq!(body["error"]["code"], "PLATFORM_EXECUTIVE_UNAVAILABLE");
    let (status, _) = http(
        addr,
        "GET /api/v1/platform/executive HTTP/1.1\r\nHost: missing.example.test\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert_eq!(status, 404);
    assert!(
        probe
            .seen
            .lock()
            .unwrap()
            .iter()
            .any(|value| value.contains("Some(\"abnormal-platform-services\")"))
    );
    server.abort();
}
