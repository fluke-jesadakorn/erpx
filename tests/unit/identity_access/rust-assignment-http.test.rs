//! Actual owner-local HTTP router tests for native Identity Access assignments.
use erpx_server_identity_access::public::assignment_http::{
    IamAssignmentHttpBackend, IamAssignmentHttpCommand, IamAssignmentHttpFuture,
    IamAssignmentHttpRequest, IamAssignmentHttpResponse, IamAssignmentHttpTarget,
    iam_assignment_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl IamAssignmentHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: IamAssignmentHttpRequest<'a>,
    ) -> IamAssignmentHttpFuture<'a, IamAssignmentHttpResponse> {
        Box::pin(async move {
            let cookie = request.headers.get("cookie").and_then(|v| v.to_str().ok());
            let seen = match &request.command {
                IamAssignmentHttpCommand::Workspace => {
                    format!(
                        "workspace|{}|{}|{cookie:?}",
                        request.host, request.holding_slug
                    )
                }
                IamAssignmentHttpCommand::Revoke {
                    assignment_id,
                    input,
                    idempotency_key,
                } => format!(
                    "revoke|{}|{}|{}|{}|{}|{cookie:?}",
                    request.host,
                    request.holding_slug,
                    assignment_id,
                    input.expected_version,
                    idempotency_key
                ),
            };
            self.seen.lock().unwrap().push(seen);
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "ASSIGNMENT_CHANGED",
                    "Synthetic changed assignment.",
                ));
            }
            Ok(IamAssignmentHttpResponse {
                data: match request.command {
                    IamAssignmentHttpCommand::Workspace => serde_json::json!({
                        "assignments": [],
                        "roles": [],
                        "evaluatedAt": 500,
                        "currentAssignmentId": "assignment-actor",
                        "currentUserId": "actor",
                        "scopeKind": "holding",
                        "scopeNodeId": null
                    }),
                    IamAssignmentHttpCommand::Revoke {
                        assignment_id,
                        input,
                        ..
                    } => serde_json::json!({
                        "id": assignment_id,
                        "status": "revoked",
                        "version": input.expected_version + 1
                    }),
                },
                context: serde_json::json!({"userId": "actor"}),
            })
        })
    }
}

struct Noop;

impl IamAssignmentHttpBackend for Noop {
    fn execute<'a>(
        &'a self,
        _: IamAssignmentHttpRequest<'a>,
    ) -> IamAssignmentHttpFuture<'a, IamAssignmentHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || IamAssignmentHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(iam_assignment_http_router(vec![target()]).is_ok());
    assert!(iam_assignment_http_router(vec![target(), target()]).is_err());
    assert!(
        iam_assignment_http_router(vec![IamAssignmentHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        iam_assignment_http_router(vec![IamAssignmentHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        iam_assignment_http_router(vec![IamAssignmentHttpTarget {
            holding_slug: "Acme".into(),
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
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nCookie: erpx=session\r\nConnection: close\r\n\r\n"
    )
}
fn post(path: &str, host: &str, key: Option<&str>, body: &str) -> String {
    let key = key
        .map(|value| format!("Idempotency-Key: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nCookie: erpx=session\r\nContent-Type: application/json\r\n{key}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn target(
    host: &str,
    slug: &str,
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
) -> IamAssignmentHttpTarget {
    IamAssignmentHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: format!("https://{host}"),
        backend: Arc::new(Probe { seen, fail }),
    }
}

#[tokio::test]
async fn actual_router_forwards_workspace_and_withdrawal_with_private_headers() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = iam_assignment_http_router(vec![
        target("acme.example.test", "acme", seen.clone(), false),
        target("fail.example.test", "fail", seen.clone(), true),
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, headers) = http(
        addr,
        get("/api/v1/holdings/acme/iam/assignments", "acme.example.test"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["currentAssignmentId"], "assignment-actor");
    assert_eq!(body["context"]["userId"], "actor");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/iam/assignments/assignment-target/revoke",
            "acme.example.test",
            Some("iam-revoke-key-1"),
            r#"{"expectedVersion":3,"reason":"Independent withdrawal"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body["data"],
        serde_json::json!({"id": "assignment-target", "status": "revoked", "version": 4})
    );

    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            "workspace|acme.example.test|acme|Some(\"erpx=session\")",
            "revoke|acme.example.test|acme|assignment-target|3|iam-revoke-key-1|Some(\"erpx=session\")",
        ]
    );

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/fail/iam/assignments/assignment-target/revoke",
            "fail.example.test",
            Some("iam-revoke-key-2"),
            r#"{"expectedVersion":3,"reason":"Independent withdrawal"}"#,
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "ASSIGNMENT_CHANGED");
}

#[tokio::test]
async fn router_rejects_unknown_scope_bad_keys_bodies_and_queries() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = iam_assignment_http_router(vec![target(
        "acme.example.test",
        "acme",
        seen.clone(),
        false,
    )])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, _, _) = http(
        addr,
        get(
            "/api/v1/holdings/other/iam/assignments",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, _, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/iam/assignments?scope=all",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        "GET /api/v1/holdings/acme/iam/assignments HTTP/1.1\r\nHost: acme.example.test\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string(),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/iam/assignments/assignment-target/revoke",
            "acme.example.test",
            None,
            r#"{"expectedVersion":3,"reason":"Independent withdrawal"}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/iam/assignments/assignment-target/revoke",
            "acme.example.test",
            Some("short"),
            r#"{"expectedVersion":3,"reason":"Independent withdrawal"}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, body, _) = http(
        addr,
        "POST /api/v1/holdings/acme/iam/assignments/assignment-target/revoke HTTP/1.1\r\nHost: acme.example.test\r\nContent-Type: text/plain\r\nIdempotency-Key: iam-revoke-key-3\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string(),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "IAM_ASSIGNMENT_REQUEST_INVALID");

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/iam/assignments/assignment-target/revoke",
            "acme.example.test",
            Some("iam-revoke-key-4"),
            r#"{"expectedVersion":3,"reason":"Independent withdrawal","extra":true}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/iam/assignments/assignment-target/revoke",
            "acme.example.test",
            Some("iam-revoke-key-5"),
            r#"{"expectedVersion":3}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/iam/assignments/bad/id/revoke",
            "acme.example.test",
            Some("iam-revoke-key-6"),
            r#"{"expectedVersion":3,"reason":"Independent withdrawal"}"#,
        ),
    )
    .await;
    assert_eq!(status, 404);

    assert!(seen.lock().unwrap().is_empty());
}
