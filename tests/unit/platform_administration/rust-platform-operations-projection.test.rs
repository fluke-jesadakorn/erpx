//! Native operations projection public contract and HTTP ingress.
#![allow(dead_code)]
use erpx_server_platform_administration::public::platform_operations::*;
use erpx_server_platform_administration::public::platform_operations_projection::*;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

fn projection() -> PlatformOperationsProjection {
    PlatformOperationsSummary {
        generated_at: 10,
        actor_role: "platform_admin".into(),
        control_health: vec![ControlServiceHealth {
            service: "jobs".into(),
            heartbeat_at: 9,
            details: "{}".into(),
        }],
        holdings: vec![PlatformHoldingOperation {
            id: "h001".into(),
            slug: "acme".into(),
            display_name: "Acme".into(),
            status: "attention".into(),
            pending_events: 1,
            dead_letter_events: 1,
            claimed_events: 0,
            oldest_pending_at: Some(3),
            job_heartbeat_at: Some(9),
            error: None,
            dead_letters: Vec::new(),
            migrations: PlatformMigrationState {
                track: "holding-write".into(),
                applied: 10,
                pending: Vec::new(),
                latest_applied: Some("0010".into()),
                error: None,
            },
        }],
        totals: PlatformOperationTotals {
            holdings: 1,
            pending_events: 1,
            dead_letter_events: 1,
            claimed_events: 0,
        },
    }
}

#[test]
fn projection_retains_the_retained_summary_wire_shape() {
    let value = serde_json::to_value(projection()).unwrap();
    assert_eq!(value["generatedAt"], 10);
    assert_eq!(value["actorRole"], "platform_admin");
    assert_eq!(value["holdings"][0]["displayName"], "Acme");
    assert_eq!(value["holdings"][0]["migrations"]["track"], "holding-write");
    assert_eq!(value["totals"]["deadLetterEvents"], 1);
}

#[derive(Default)]
struct Probe {
    seen: Mutex<Vec<String>>,
    fail: bool,
}
impl PlatformOperationsProjectionHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: PlatformOperationsProjectionHttpReadRequest<'a>,
    ) -> PlatformOperationsProjectionHttpFuture<'a, PlatformOperationsProjectionHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(request.host.into());
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "PLATFORM_OPERATIONS_UNAVAILABLE",
                    "Synthetic unavailable.",
                ));
            }
            Ok(PlatformOperationsProjectionHttpResponse {
                data: serde_json::to_value(projection()).unwrap(),
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
async fn projection_router_requires_exact_host_and_maps_failures() {
    let probe = Arc::new(Probe::default());
    let failing = Arc::new(Probe {
        seen: Mutex::new(Vec::new()),
        fail: true,
    });
    let router = platform_operations_projection_router(vec![
        PlatformOperationsProjectionHttpTarget {
            host: "erpx-internal.example.test".into(),
            backend: probe.clone(),
        },
        PlatformOperationsProjectionHttpTarget {
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
        "GET /api/v1/platform/operations/projection HTTP/1.1\r\nHost: erpx-internal.example.test\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["actorRole"], "platform_admin");
    assert_eq!(body["data"]["totals"]["holdings"], 1);
    let (status, body) = http(
        addr,
        "GET /api/v1/platform/operations/projection HTTP/1.1\r\nHost: fail.example.test\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert_eq!(status, 503);
    assert_eq!(body["error"]["code"], "PLATFORM_OPERATIONS_UNAVAILABLE");
    let (status, _) = http(
        addr,
        "GET /api/v1/platform/operations/projection HTTP/1.1\r\nHost: missing.example.test\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(
        probe.seen.lock().unwrap().clone(),
        vec!["erpx-internal.example.test"]
    );
    server.abort();
}
