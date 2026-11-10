//! Actual owner-local Agent governance HTTP router tests. These prove ingress
//! facts only; Account authority, Agent policy, snapshots, receipts, audit and
//! outbox remain separate composition responsibilities. The platform-owned
//! Agent evidence audit read is not served by this router.
use erpx_server_agent::public::governance::{
    AgentGovernanceHttpBackend, AgentGovernanceHttpCommandRequest, AgentGovernanceHttpFuture,
    AgentGovernanceHttpReadRequest, AgentGovernanceHttpTarget, agent_governance_http_router,
};
use erpx_server_runtime::public::http_support::serve;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl AgentGovernanceHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: AgentGovernanceHttpReadRequest<'a>,
    ) -> AgentGovernanceHttpFuture<'a, serde_json::Value> {
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
        _: AgentGovernanceHttpCommandRequest<'a>,
    ) -> AgentGovernanceHttpFuture<'a, serde_json::Value> {
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
struct SeenRead {
    host: String,
    holding: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SeenCommand {
    host: String,
    holding: String,
    key: String,
    entity: Option<String>,
    action: Option<String>,
}

struct Probe {
    reads: Arc<Mutex<Vec<SeenRead>>>,
    commands: Arc<Mutex<Vec<SeenCommand>>>,
    fail: Option<BusinessError>,
}

impl AgentGovernanceHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: AgentGovernanceHttpReadRequest<'a>,
    ) -> AgentGovernanceHttpFuture<'a, serde_json::Value> {
        Box::pin(async move {
            self.reads.lock().unwrap().push(SeenRead {
                host: request.host.into(),
                holding: request.holding_slug.into(),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(serde_json::json!({
                "agents": [{"id": "agent-1", "status": "approved"}],
                "pools": [],
                "assignments": [],
            }))
        })
    }
    fn command<'a>(
        &'a self,
        request: AgentGovernanceHttpCommandRequest<'a>,
    ) -> AgentGovernanceHttpFuture<'a, serde_json::Value> {
        Box::pin(async move {
            self.commands.lock().unwrap().push(SeenCommand {
                host: request.host.into(),
                holding: request.holding_slug.into(),
                key: request.key.into(),
                entity: request
                    .body
                    .get("entity")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                action: request
                    .body
                    .get("action")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(serde_json::json!({"id": "agent-agent-key", "status": "draft", "version": 1}))
        })
    }
}

fn target(backend: Arc<dyn AgentGovernanceHttpBackend>) -> AgentGovernanceHttpTarget {
    AgentGovernanceHttpTarget {
        host: "erpx-internal.example.test".into(),
        holding_slug: "erpx".into(),
        origin: "https://erpx-internal.example.test".into(),
        backend,
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend: Arc<dyn AgentGovernanceHttpBackend> = Arc::new(Noop);
    assert!(agent_governance_http_router(vec![target(backend.clone())]).is_ok());
    assert!(
        agent_governance_http_router(vec![target(backend.clone()), target(backend.clone())])
            .is_err()
    );
    for invalid in [
        AgentGovernanceHttpTarget {
            origin: "http://erpx-internal.example.test".into(),
            ..target(backend.clone())
        },
        AgentGovernanceHttpTarget {
            origin: "https://erpx-internal.example.test/path".into(),
            ..target(backend.clone())
        },
        AgentGovernanceHttpTarget {
            holding_slug: "ERPX".into(),
            ..target(backend.clone())
        },
        AgentGovernanceHttpTarget {
            host: String::new(),
            ..target(backend.clone())
        },
    ] {
        assert!(agent_governance_http_router(vec![invalid]).is_err());
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
fn post(path: &str, host: &str, origin: &str, key: Option<&str>, body: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nContent-Type: application/json\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        key.map(|key| format!("Idempotency-Key: {key}\r\n"))
            .unwrap_or_default(),
        body.len()
    )
}

#[tokio::test]
async fn actual_router_forwards_exact_targets_and_maps_business_errors() {
    let reads = Arc::new(Mutex::new(Vec::new()));
    let commands = Arc::new(Mutex::new(Vec::new()));
    let router = agent_governance_http_router(vec![
        target(Arc::new(Probe {
            reads: reads.clone(),
            commands: commands.clone(),
            fail: None,
        })),
        AgentGovernanceHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe {
                reads: reads.clone(),
                commands: commands.clone(),
                fail: Some(BusinessError::new(
                    FailureKind::Conflict,
                    "CONFLICT",
                    "Synthetic changed source.",
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

    let path = "/api/v1/holdings/erpx/agents";
    let (status, body, raw) = http(addr, get(path, "erpx-internal.example.test")).await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["agents"][0]["id"], "agent-1");
    assert!(raw.contains("cache-control: private, no-store"));
    assert!(raw.contains("referrer-policy: no-referrer"));
    assert!(raw.contains("x-content-type-options: nosniff"));
    assert_eq!(
        reads.lock().unwrap().last().cloned(),
        Some(SeenRead {
            host: "erpx-internal.example.test".into(),
            holding: "erpx".into(),
        })
    );

    let (status, body, _) = http(addr, get(path, "other.example.test")).await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body, _) = http(
        addr,
        get("/api/v1/holdings/fail/agents", "fail.example.test"),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "CONFLICT");

    let payload =
        r#"{"entity":"agent","name":"Support agent","purpose":"Answer support questions"}"#;
    let (status, body, _) = http(
        addr,
        post(
            path,
            "erpx-internal.example.test",
            "https://erpx-internal.example.test",
            None,
            payload,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, body, _) = http(
        addr,
        post(
            path,
            "erpx-internal.example.test",
            "https://other.example.test",
            Some("agent-key-1"),
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
            Some("agent-key-1"),
            payload,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["id"], "agent-agent-key");
    assert_eq!(
        commands.lock().unwrap().last().cloned(),
        Some(SeenCommand {
            host: "erpx-internal.example.test".into(),
            holding: "erpx".into(),
            key: "agent-key-1".into(),
            entity: Some("agent".into()),
            action: None,
        })
    );

    for invalid in [r#"[]"#, r#"null"#, r#""string""#, r#"not-json"#] {
        let (status, _, _) = http(
            addr,
            post(
                path,
                "erpx-internal.example.test",
                "https://erpx-internal.example.test",
                Some("agent-key-2"),
                invalid,
            ),
        )
        .await;
        assert_eq!(status, 422, "body: {invalid}");
    }
}
