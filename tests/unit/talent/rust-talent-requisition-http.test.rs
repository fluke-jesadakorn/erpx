//! Actual owner-local HTTP router tests for the native Talent requisition slice.
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_talent::public::requisition_http::{
    TalentRequisitionHttpBackend, TalentRequisitionHttpCommand, TalentRequisitionHttpFuture,
    TalentRequisitionHttpRequest, TalentRequisitionHttpResponse, TalentRequisitionHttpTarget,
    talent_requisition_http_router,
};
use std::sync::{Arc, Mutex};

struct Noop;

impl TalentRequisitionHttpBackend for Noop {
    fn execute<'a>(
        &'a self,
        _: TalentRequisitionHttpRequest<'a>,
    ) -> TalentRequisitionHttpFuture<'a, TalentRequisitionHttpResponse> {
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

impl TalentRequisitionHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: TalentRequisitionHttpRequest<'a>,
    ) -> TalentRequisitionHttpFuture<'a, TalentRequisitionHttpResponse> {
        Box::pin(async move {
            let seen = match &request.command {
                TalentRequisitionHttpCommand::Workspace => "workspace".to_owned(),
                TalentRequisitionHttpCommand::Detail { requisition_id } => {
                    format!("detail|{requisition_id}")
                }
                TalentRequisitionHttpCommand::Create {
                    body,
                    idempotency_key,
                } => format!(
                    "create|{idempotency_key}|{}",
                    body["jobTitle"].as_str().unwrap_or("")
                ),
                TalentRequisitionHttpCommand::Change {
                    requisition_id,
                    body,
                    idempotency_key,
                } => format!(
                    "change|{requisition_id}|{idempotency_key}|{}",
                    body["action"].as_str().unwrap_or("")
                ),
            };
            self.seen.lock().unwrap().push(seen);
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "CONFLICT",
                    "Synthetic changed Talent requisition.",
                ));
            }
            Ok(TalentRequisitionHttpResponse {
                data: match request.command {
                    TalentRequisitionHttpCommand::Workspace => {
                        serde_json::json!({"requisitions": []})
                    }
                    TalentRequisitionHttpCommand::Detail { .. } => serde_json::json!({
                        "requisition": {"id": "talent-req-1"},
                        "events": []
                    }),
                    TalentRequisitionHttpCommand::Create { .. }
                    | TalentRequisitionHttpCommand::Change { .. } => serde_json::json!({
                        "id": "talent-req-1",
                        "status": "draft",
                        "version": 1
                    }),
                },
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || TalentRequisitionHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(talent_requisition_http_router(vec![target()]).is_ok());
    assert!(talent_requisition_http_router(vec![target(), target()]).is_err());
    assert!(
        talent_requisition_http_router(vec![TalentRequisitionHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        talent_requisition_http_router(vec![TalentRequisitionHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        talent_requisition_http_router(vec![TalentRequisitionHttpTarget {
            holding_slug: "ACME".into(),
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

fn post(path: &str, host: &str, key: Option<&str>, body: &str) -> String {
    let key = key
        .map(|value| format!("Idempotency-Key: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n{key}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[tokio::test]
async fn actual_router_forwards_requisition_reads_and_commands() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = talent_requisition_http_router(vec![
        TalentRequisitionHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: false,
            }),
        },
        TalentRequisitionHttpTarget {
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

    let (status, body, headers) = http(
        addr,
        get(
            "/api/v1/holdings/acme/talent/requisitions",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["requisitions"], serde_json::json!([]));
    assert_eq!(body["context"]["userId"], "user");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/talent/requisitions/talent-req-1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["requisition"]["id"], "talent-req-1");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/talent/requisitions",
            "acme.example.test",
            Some("talent-requisition-command-1"),
            r#"{"jobTitle":"Service specialist","openings":2,"targetStartDate":"2026-11-01","description":"Serve customer cases."}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "draft");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/talent/requisitions/talent-req-1",
            "acme.example.test",
            Some("talent-requisition-command-2"),
            r#"{"action":"approve","expectedVersion":1,"reason":"Independent review"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["version"], 1);
    assert_eq!(
        seen.lock().unwrap().clone(),
        vec![
            "workspace",
            "detail|talent-req-1",
            "create|talent-requisition-command-1|Service specialist",
            "change|talent-req-1|talent-requisition-command-2|approve",
        ]
    );

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/fail/talent/requisitions/talent-req-1",
            "fail.example.test",
            Some("talent-requisition-command-3"),
            r#"{"action":"open","expectedVersion":2,"reason":"Approved role is ready"}"#,
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "CONFLICT");

    server.abort();
}

#[tokio::test]
async fn router_rejects_unknown_targets_query_keys_bodies_and_routes() {
    let router = talent_requisition_http_router(vec![TalentRequisitionHttpTarget {
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
    let path = "/api/v1/holdings/acme/talent/requisitions";

    let (status, _, _) = http(addr, get(&format!("{path}?extra=1"), "acme.example.test")).await;
    assert_eq!(status, 422);
    let (status, _, _) = http(addr, get(path, "evil.example.test")).await;
    assert_eq!(status, 404);
    let (status, _, _) = http(addr, get(path, "acme.example.test")).await;
    assert_eq!(status, 200);
    let (status, _, _) = http(
        addr,
        format!("GET {path} HTTP/1.1\r\nHost: acme.example.test\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}", "{}"),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            path,
            "acme.example.test",
            None,
            r#"{"jobTitle":"Service specialist","openings":2,"targetStartDate":"2026-11-01","description":"Serve customer cases."}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);
    let (status, _, _) = http(addr, post(path, "acme.example.test", Some("short"), "{}")).await;
    assert_eq!(status, 422);
    let (status, _, _) = http(
        addr,
        post(
            path,
            "acme.example.test",
            Some("talent-requisition-command-4"),
            "{not-json",
        ),
    )
    .await;
    assert_eq!(status, 422);
    let (status, _, _) = http(
        addr,
        format!(
            "POST {path} HTTP/1.1\r\nHost: acme.example.test\r\nContent-Type: text/plain\r\nIdempotency-Key: talent-requisition-command-5\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
        ),
    )
    .await;
    assert_eq!(status, 422);
    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/talent/unknown",
            "acme.example.test",
            Some("talent-requisition-command-6"),
            "{}",
        ),
    )
    .await;
    assert_eq!(status, 404);

    server.abort();
}
