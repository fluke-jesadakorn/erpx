//! Actual owner-local Platform AI-gateway HTTP router tests. These prove
//! ingress facts only; Platform authority, the target Holding database and all
//! owner effects remain composition responsibilities.
use erpx_server_agent::public::gateway::{
    AiGatewayHttpBackend, AiGatewayHttpCommand, AiGatewayHttpCommandRequest, AiGatewayHttpFuture,
    AiGatewayHttpReadRequest, AiGatewayHttpTarget, ai_gateway_http_router,
};
use erpx_server_runtime::public::http_support::serve;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl AiGatewayHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: AiGatewayHttpReadRequest<'a>,
    ) -> AiGatewayHttpFuture<'a, serde_json::Value> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
    fn command<'a>(
        &'a self,
        _: AiGatewayHttpCommandRequest<'a>,
    ) -> AiGatewayHttpFuture<'a, serde_json::Value> {
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
    kind: String,
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
    fail: Option<BusinessError>,
}

fn view() -> serde_json::Value {
    serde_json::json!({
        "platformEnabled": true,
        "monthlyRequestLimit": 1000,
        "monthlySpendCapSatang": 10000000,
        "reservationSatang": 1000,
        "monthlyRequests": 2,
        "monthlyReservedOrVerifiedSatang": 350,
        "pendingPlatformCostRequests": 1,
        "updatedBy": "platform-admin",
        "updatedAt": 42,
    })
}

fn kind(command: &AiGatewayHttpCommand) -> &'static str {
    match command {
        AiGatewayHttpCommand::SetEnabled(_) => "set-enabled",
        AiGatewayHttpCommand::UpdateLimits(_) => "limits",
        AiGatewayHttpCommand::RecordStatement(_) => "statement",
        AiGatewayHttpCommand::RecordCost(_) => "cost",
        AiGatewayHttpCommand::Reconcile(_) => "reconcile",
    }
}

impl AiGatewayHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: AiGatewayHttpReadRequest<'a>,
    ) -> AiGatewayHttpFuture<'a, serde_json::Value> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                host: request.host.into(),
                holding: request.holding_id.as_str().into(),
                kind: "read".into(),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(view())
        })
    }
    fn command<'a>(
        &'a self,
        request: AiGatewayHttpCommandRequest<'a>,
    ) -> AiGatewayHttpFuture<'a, serde_json::Value> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                host: request.host.into(),
                holding: request.holding_id.as_str().into(),
                kind: kind(&request.command).into(),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(match request.command {
                AiGatewayHttpCommand::SetEnabled(_) | AiGatewayHttpCommand::UpdateLimits(_) => {
                    view()
                }
                _ => serde_json::json!({"ok": true}),
            })
        })
    }
}

fn target(backend: Arc<dyn AiGatewayHttpBackend>) -> AiGatewayHttpTarget {
    AiGatewayHttpTarget {
        host: "erpx-internal.example.test".into(),
        origin: "https://erpx-internal.example.test".into(),
        backend,
    }
}

#[test]
fn target_configuration_requires_distinct_exact_hosts() {
    let backend: Arc<dyn AiGatewayHttpBackend> = Arc::new(Noop);
    assert!(ai_gateway_http_router(vec![target(backend.clone())]).is_ok());
    assert!(
        ai_gateway_http_router(vec![target(backend.clone()), target(backend.clone())]).is_err()
    );
    for invalid in [
        AiGatewayHttpTarget {
            origin: "http://erpx-internal.example.test".into(),
            ..target(backend.clone())
        },
        AiGatewayHttpTarget {
            origin: "https://erpx-internal.example.test/path".into(),
            ..target(backend.clone())
        },
        AiGatewayHttpTarget {
            host: String::new(),
            ..target(backend.clone())
        },
    ] {
        assert!(ai_gateway_http_router(vec![invalid]).is_err());
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
fn post(path: &str, host: &str, origin: &str, body: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[tokio::test]
async fn actual_router_forwards_exact_targets_and_maps_business_errors() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let path = "/api/v1/platform/billing/ai-gateway";
    let router = ai_gateway_http_router(vec![
        target(Arc::new(Probe {
            seen: seen.clone(),
            fail: None,
        })),
        AiGatewayHttpTarget {
            host: "fail.example.test".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: Some(BusinessError::new(
                    FailureKind::Conflict,
                    "AI_GATEWAY_POLICY_CHANGED",
                    "Synthetic changed policy.",
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
            &format!("{path}?holdingId=gateway-holding"),
            "erpx-internal.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["monthlyRequestLimit"], 1000);
    assert!(raw.contains("cache-control: private, no-store"));
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            host: "erpx-internal.example.test".into(),
            holding: "gateway-holding".into(),
            kind: "read".into(),
        })
    );

    for (query, expected) in [
        (path.to_owned(), "HOLDING_REQUIRED"),
        (
            format!("{path}?holdingId=gateway-holding&holdingId=other"),
            "AI_GATEWAY_REQUEST_INVALID",
        ),
        (
            format!("{path}?holdingId=gateway-holding&month=2026-11"),
            "AI_GATEWAY_REQUEST_INVALID",
        ),
        (format!("{path}?holdingId=../bad"), "HOLDING_REQUIRED"),
    ] {
        let (status, body, _) = http(addr, get(&query, "erpx-internal.example.test")).await;
        assert_eq!(status, 422, "query: {query}");
        assert_eq!(body["error"]["code"], expected, "query: {query}");
    }
    let (status, body, _) = http(
        addr,
        get(
            &format!("{path}?holdingId=gateway-holding"),
            "other.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let payload = r#"{"holdingId":"gateway-holding","enabled":false,"expectedUpdatedAt":42,"reason":"Provider incident"}"#;
    let (status, body, _) = http(
        addr,
        post(
            path,
            "erpx-internal.example.test",
            "https://other.example.test",
            payload,
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, body, _) = http(
        addr,
        post(
            path,
            "erpx-internal.example.test",
            "https://erpx-internal.example.test",
            payload,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["platformEnabled"], true);
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            host: "erpx-internal.example.test".into(),
            holding: "gateway-holding".into(),
            kind: "set-enabled".into(),
        })
    );

    let (status, body, _) = http(
        addr,
        post(
            &format!("{path}/limits"),
            "erpx-internal.example.test",
            "https://erpx-internal.example.test",
            r#"{"holdingId":"gateway-holding","enabled":true,"monthlyRequestLimit":1000,"monthlySpendCapSatang":5000,"reservationSatang":500,"reason":"Restore"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["monthlySpendCapSatang"], 10000000);
    assert_eq!(seen.lock().unwrap().last().unwrap().kind, "limits");
    let (status, body, _) = http(
        addr,
        post(
            &format!("{path}/statements"),
            "erpx-internal.example.test",
            "https://erpx-internal.example.test",
            r#"{"holdingId":"gateway-holding","id":"statement-1","provider":"api.provider.example","month":"2026-11","currency":"USD","amountMinor":20,"statementDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["ok"], true);
    assert_eq!(seen.lock().unwrap().last().unwrap().kind, "statement");
    let (status, body, _) = http(
        addr,
        post(
            &format!("{path}/cost-evidence"),
            "erpx-internal.example.test",
            "https://erpx-internal.example.test",
            r#"{"holdingId":"gateway-holding","requestId":"request-1","providerStatementId":"statement-1","currency":"USD","amountMinor":10,"thbSatang":350,"fxEvidenceRef":"bank-rate-1"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["ok"], true);
    assert_eq!(seen.lock().unwrap().last().unwrap().kind, "cost");
    let (status, body, _) = http(
        addr,
        post(
            &format!("{path}/reconcile"),
            "erpx-internal.example.test",
            "https://erpx-internal.example.test",
            r#"{"holdingId":"gateway-holding","statementId":"statement-1"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["ok"], true);
    assert_eq!(seen.lock().unwrap().last().unwrap().kind, "reconcile");

    let (status, body, _) = http(
        addr,
        post(
            &format!("{path}/unknown"),
            "erpx-internal.example.test",
            "https://erpx-internal.example.test",
            payload,
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    for invalid in [
        r#"[]"#,
        r#"null"#,
        r#""string""#,
        "not-json",
        r#"{"holdingId":"gateway-holding","enabled":false,"expectedUpdatedAt":42,"reason":"x","extra":true}"#,
        r#"{"holdingId":"gateway-holding","enabled":"no","expectedUpdatedAt":42,"reason":"Provider incident"}"#,
    ] {
        let (status, body, _) = http(
            addr,
            post(
                path,
                "erpx-internal.example.test",
                "https://erpx-internal.example.test",
                invalid,
            ),
        )
        .await;
        assert_eq!(status, 422, "body: {invalid}");
        assert_eq!(body["error"]["code"], "AI_GATEWAY_REQUEST_INVALID");
    }

    let (status, body, _) = http(
        addr,
        post(
            path,
            "fail.example.test",
            "https://fail.example.test",
            payload,
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "AI_GATEWAY_POLICY_CHANGED");
}
