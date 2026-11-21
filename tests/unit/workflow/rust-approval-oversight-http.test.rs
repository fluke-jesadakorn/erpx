//! Actual owner-local HTTP router tests for Workflow approval oversight.
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::approval_oversight_http::{
    ApprovalOversightHttpBackend, ApprovalOversightHttpCommand, ApprovalOversightHttpFuture,
    ApprovalOversightHttpRequest, ApprovalOversightHttpResponse, ApprovalOversightHttpTarget,
    approval_oversight_http_router,
};
use std::sync::{Arc, Mutex};

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: Option<BusinessError>,
}

impl ApprovalOversightHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: ApprovalOversightHttpRequest<'a>,
    ) -> ApprovalOversightHttpFuture<'a, ApprovalOversightHttpResponse> {
        Box::pin(async move {
            let cookie = request
                .headers
                .get("cookie")
                .and_then(|value| value.to_str().ok());
            let command = match &request.command {
                ApprovalOversightHttpCommand::Queue => "queue".to_owned(),
                ApprovalOversightHttpCommand::Evidence { query } => {
                    format!("evidence:{}:{}", query.kind.as_str(), query.record_id)
                }
            };
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{command}|{cookie:?}",
                request.host, request.holding_slug
            ));
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(ApprovalOversightHttpResponse {
                data: match request.command {
                    ApprovalOversightHttpCommand::Queue => serde_json::json!({
                        "evaluatedAt": 500,
                        "approvals": [{
                            "id": "approval-1",
                            "company_id": "company",
                            "branch_id": null,
                            "department_id": null,
                            "record_type": "expense",
                            "record_id": "expense-1",
                            "state": "pending",
                            "blocked_reason": null,
                            "version": 1,
                            "created_at": 100
                        }]
                    }),
                    ApprovalOversightHttpCommand::Evidence { query } => serde_json::json!({
                        "evaluatedAt": 500,
                        "approval": {
                            "instance": {
                                "id": "approval-1",
                                "company_id": "company",
                                "branch_id": null,
                                "department_id": null,
                                "record_type": query.kind.as_str(),
                                "record_id": query.record_id,
                                "phase": "request",
                                "cycle": 1,
                                "requester_user_id": "requester",
                                "assignee_user_id": null,
                                "state": "pending",
                                "current_step": 0,
                                "blocked_reason": null,
                                "version": 2,
                                "created_at": 100,
                                "updated_at": 200
                            },
                            "steps": [],
                            "history": [],
                            "receipts": [],
                            "audit": [],
                            "outbox": []
                        }
                    }),
                },
                context: serde_json::json!({"userId": "actor"}),
            })
        })
    }
}

struct Noop;

impl ApprovalOversightHttpBackend for Noop {
    fn execute<'a>(
        &'a self,
        _: ApprovalOversightHttpRequest<'a>,
    ) -> ApprovalOversightHttpFuture<'a, ApprovalOversightHttpResponse> {
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
    let target = || ApprovalOversightHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(approval_oversight_http_router(vec![target()]).is_ok());
    assert!(approval_oversight_http_router(vec![target(), target()]).is_err());
    assert!(
        approval_oversight_http_router(vec![ApprovalOversightHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        approval_oversight_http_router(vec![ApprovalOversightHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        approval_oversight_http_router(vec![ApprovalOversightHttpTarget {
            holding_slug: "Acme".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        approval_oversight_http_router(vec![ApprovalOversightHttpTarget {
            host: String::new(),
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
fn request(path: &str, host: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut raw = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nCookie: erpx=session\r\n");
    for (name, value) in headers {
        raw.push_str(&format!("{name}: {value}\r\n"));
    }
    if !body.is_empty() {
        raw.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    raw.push_str("Connection: close\r\n\r\n");
    raw.push_str(body);
    raw
}
fn target(
    host: &str,
    slug: &str,
    seen: Arc<Mutex<Vec<String>>>,
    fail: Option<BusinessError>,
) -> ApprovalOversightHttpTarget {
    ApprovalOversightHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: format!("https://{host}"),
        backend: Arc::new(Probe { seen, fail }),
    }
}

#[tokio::test]
async fn actual_router_forwards_queue_and_evidence_with_private_headers() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = approval_oversight_http_router(vec![
        target("acme.example.test", "acme", seen.clone(), None),
        target(
            "fail.example.test",
            "fail",
            seen.clone(),
            Some(BusinessError::new(
                FailureKind::Conflict,
                "APPROVAL_EVIDENCE_CHANGED",
                "Synthetic changed approval evidence.",
            )),
        ),
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, headers) = http(
        addr,
        request(
            "/api/v1/holdings/acme/workflow/approvals",
            "acme.example.test",
            &[("Origin", "https://acme.example.test")],
            "",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["approvals"][0]["record_id"], "expense-1");
    assert_eq!(body["context"]["userId"], "actor");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    let (status, body, _) = http(
        addr,
        request(
            "/api/v1/holdings/acme/workflow/approvals/expense/expense-1/evidence",
            "acme.example.test",
            &[],
            "",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body["data"]["approval"]["instance"]["record_id"],
        "expense-1"
    );

    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            "acme.example.test|acme|queue|Some(\"erpx=session\")",
            "acme.example.test|acme|evidence:expense:expense-1|Some(\"erpx=session\")",
        ]
    );

    let (status, body, _) = http(
        addr,
        request(
            "/api/v1/holdings/fail/workflow/approvals",
            "fail.example.test",
            &[],
            "",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "APPROVAL_EVIDENCE_CHANGED");
}

#[tokio::test]
async fn foreign_origin_and_unknown_targets_fail_before_the_backend() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = approval_oversight_http_router(vec![target(
        "acme.example.test",
        "acme",
        seen.clone(),
        None,
    )])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, _) = http(
        addr,
        request(
            "/api/v1/holdings/acme/workflow/approvals",
            "acme.example.test",
            &[("Origin", "https://evil.example.test")],
            "",
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, _, _) = http(
        addr,
        request(
            "/api/v1/holdings/other/workflow/approvals",
            "acme.example.test",
            &[],
            "",
        ),
    )
    .await;
    assert_eq!(status, 404);
    let (status, _, _) = http(
        addr,
        request(
            "/api/v1/holdings/acme/workflow/approvals",
            "other.example.test",
            &[],
            "",
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn malformed_selections_queries_and_bodies_fail_before_the_backend() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = approval_oversight_http_router(vec![target(
        "acme.example.test",
        "acme",
        seen.clone(),
        None,
    )])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    for path in [
        "/api/v1/holdings/acme/workflow/approvals?kind=expense",
        "/api/v1/holdings/acme/workflow/approvals/legal/record-1/evidence",
        "/api/v1/holdings/acme/workflow/approvals/expense/has%20space/evidence",
        "/api/v1/holdings/acme/workflow/approvals/expense/record-1",
    ] {
        let (status, _, _) = http(addr, request(path, "acme.example.test", &[], "")).await;
        assert!(status == 404 || status == 422, "{path} returned {status}");
    }
    let (status, _, _) = http(
        addr,
        request(
            "/api/v1/holdings/acme/workflow/approvals",
            "acme.example.test",
            &[],
            "{\"id\":\"injected\"}",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn owner_errors_map_to_bounded_statuses() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut targets = Vec::new();
    for (host, kind) in [
        ("invalid.example.test", FailureKind::InvalidInput),
        ("unauth.example.test", FailureKind::Unauthenticated),
        ("forbidden.example.test", FailureKind::Forbidden),
        ("missing.example.test", FailureKind::NotFound),
        ("unavailable.example.test", FailureKind::Unavailable),
    ] {
        targets.push(target(
            host,
            "acme",
            seen.clone(),
            Some(BusinessError::new(kind, "SYNTHETIC", "Synthetic failure.")),
        ));
    }
    let router = approval_oversight_http_router(targets).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    for (host, status) in [
        ("invalid.example.test", 422),
        ("unauth.example.test", 401),
        ("forbidden.example.test", 403),
        ("missing.example.test", 404),
        ("unavailable.example.test", 503),
    ] {
        let (actual, _, _) = http(
            addr,
            request("/api/v1/holdings/acme/workflow/approvals", host, &[], ""),
        )
        .await;
        assert_eq!(actual, status, "{host}");
    }
}
