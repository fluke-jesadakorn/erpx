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

#[tokio::test]
async fn command_plan_returns_only_owner_authored_select_statements() {
    let router = talent_requisition_http_router(vec![TalentRequisitionHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(Noop),
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/talent/command-plan",
            "acme.example.test",
            None,
            r#"{"kind":"talent_rd_offers","values":["app-1"]}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body["data"]["statements"][0]["sql"],
        serde_json::json!("SELECT * FROM talent_offers WHERE application_id=?")
    );
    assert_eq!(body["data"]["statements"][0]["values"], serde_json::json!(["app-1"]));

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/talent/command-plan",
            "acme.example.test",
            None,
            r#"{"kind":"SELECT * FROM talent_offers","values":[]}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/talent/command-plan",
            "acme.example.test",
            None,
            r#"{"kind":"talent_rd_offers","values":[]}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/talent/command-plan",
            "other.example.test",
            None,
            r#"{"kind":"talent_rd_offers","values":["app-1"]}"#,
        ),
    )
    .await;
    assert_eq!(status, 404);

    server.abort();
}

#[tokio::test]
async fn command_plan_returns_owner_authored_privacy_history_read() {
    let router = talent_requisition_http_router(vec![TalentRequisitionHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(Noop),
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/talent/command-plan",
            "acme.example.test",
            None,
            r#"{"kind":"talent_rd_privacy_history","values":["cand-1","company"]}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body["data"]["statements"][0]["sql"],
        serde_json::json!(
            "SELECT id,action,candidate_version,created_at FROM talent_candidate_privacy_events WHERE candidate_id=? AND company_id=? ORDER BY candidate_version,id"
        )
    );
    assert_eq!(
        body["data"]["statements"][0]["values"],
        serde_json::json!(["cand-1", "company"])
    );

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/talent/command-plan",
            "acme.example.test",
            None,
            r#"{"kind":"talent_rd_privacy_history","values":["cand-1"]}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);

    server.abort();
}

#[tokio::test]
async fn command_plan_returns_owner_authored_hire_handoff_reads() {
    let router = talent_requisition_http_router(vec![TalentRequisitionHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(Noop),
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/talent/command-plan",
            "acme.example.test",
            None,
            r#"{"kind":"talent_rd_consume_hire_handoff_receipt","values":["handoff-1","company"]}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body["data"]["statements"][0]["sql"],
        serde_json::json!(
            "SELECT id FROM talent_hire_consumption_receipts WHERE handoff_id=? AND company_id=?"
        )
    );
    assert_eq!(
        body["data"]["statements"][0]["values"],
        serde_json::json!(["handoff-1", "company"])
    );

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/talent/command-plan",
            "acme.example.test",
            None,
            r#"{"kind":"talent_rd_change_candidate_privacy_row","values":["cand-1","company"]}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body["data"]["statements"][0]["sql"],
        serde_json::json!(
            "SELECT id,status,privacy_hold,privacy_requested_by,version,retention_until FROM talent_candidates WHERE id=? AND company_id=?"
        )
    );
    assert_eq!(
        body["data"]["statements"][0]["values"],
        serde_json::json!(["cand-1", "company"])
    );

    // Wrong bind arity fails closed for both registered kinds.
    for kind in [
        "talent_rd_consume_hire_handoff_receipt",
        "talent_rd_change_candidate_privacy_row",
    ] {
        let payload = format!(r#"{{"kind":"{kind}","values":["cand-1"]}}"#);
        let (status, _, _) = http(
            addr,
            post(
                "/api/v1/holdings/acme/talent/command-plan",
                "acme.example.test",
                None,
                &payload,
            ),
        )
        .await;
        assert_eq!(status, 422);
    }

    server.abort();
}

#[tokio::test]
async fn command_plan_returns_owner_authored_workspace_list_reads() {
    let router = talent_requisition_http_router(vec![TalentRequisitionHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(Noop),
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let cases = [
        (
            "talent_rd_workspace_requisitions",
            r#"["company","branch","branch",null,null]"#,
            "SELECT * FROM talent_requisitions WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY updated_at DESC",
        ),
        (
            "talent_rd_workspace_candidates",
            r#"["company"]"#,
            "SELECT id,legal_name,email,phone,retention_until,status,version,updated_at FROM talent_candidates WHERE company_id=? ORDER BY updated_at DESC",
        ),
    ];
    for (kind, values, sql) in cases {
        let payload = format!(r#"{{"kind":"{kind}","values":{values}}}"#);
        let (status, body, _) = http(
            addr,
            post(
                "/api/v1/holdings/acme/talent/command-plan",
                "acme.example.test",
                None,
                &payload,
            ),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["statements"][0]["sql"], serde_json::json!(sql));
        assert_eq!(
            body["data"]["statements"][0]["values"],
            serde_json::from_str::<serde_json::Value>(values).unwrap()
        );

        let bad = format!(r#"{{"kind":"{kind}","values":[]}}"#);
        let (status, _, _) = http(
            addr,
            post(
                "/api/v1/holdings/acme/talent/command-plan",
                "acme.example.test",
                None,
                &bad,
            ),
        )
        .await;
        assert_eq!(status, 422);
    }

    server.abort();
}

#[tokio::test]
async fn command_plan_returns_owner_authored_sensitive_offer_read() {
    let router = talent_requisition_http_router(vec![TalentRequisitionHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(Noop),
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let sql = "SELECT o.company_id,o.version,r.branch_id,r.department_id,a.owner_id,CASE WHEN (o.status<>'accepted' OR EXISTS(SELECT 1 FROM talent_hire_handoffs handoff JOIN talent_hire_consumption_receipts receipt ON receipt.handoff_id=handoff.id AND receipt.company_id=handoff.company_id AND receipt.source_offer_version=handoff.source_offer_version WHERE handoff.offer_id=o.id AND handoff.company_id=o.company_id)) THEN 0 ELSE 1 END pending_hire_delivery FROM talent_offers o JOIN talent_applications a ON a.id=o.application_id AND a.company_id=o.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id WHERE o.id=? AND o.company_id=?";
    let payload = r#"{"kind":"talent_rd_sensitive_offer","values":["offer-1","company"]}"#;
    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/talent/command-plan",
            "acme.example.test",
            None,
            payload,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["statements"][0]["sql"], serde_json::json!(sql));
    assert_eq!(
        body["data"]["statements"][0]["values"],
        serde_json::json!(["offer-1", "company"])
    );

    for bad in [
        r#"{"kind":"talent_rd_sensitive_offer","values":["offer-1"]}"#,
        r#"{"kind":"talent_rd_sensitive_offer","values":[]}"#,
    ] {
        let (status, _, _) = http(
            addr,
            post(
                "/api/v1/holdings/acme/talent/command-plan",
                "acme.example.test",
                None,
                bad,
            ),
        )
        .await;
        assert_eq!(status, 422);
    }

    server.abort();
}

#[tokio::test]
async fn command_plan_returns_owner_authored_hire_handoff_fact_reads() {
    let router = talent_requisition_http_router(vec![TalentRequisitionHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(Noop),
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let cases = [
        (
            "talent_rd_hire_source_row",
            r#"["handoff-1","company"]"#,
            "SELECT h.*,o.version offer_version,o.status offer_status,o.start_date,o.currency,o.base_amount_minor,o.accepted_evidence,r.job_title,c.version candidate_version,c.status candidate_status,c.retention_until,c.legal_name,c.email,c.phone,c.privacy_hold,c.consent_evidence FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id AND o.company_id=h.company_id JOIN talent_applications a ON a.id=o.application_id AND a.company_id=h.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=h.company_id AND r.branch_id=h.branch_id AND r.department_id IS NOT DISTINCT FROM h.department_id JOIN talent_candidates c ON c.id=h.candidate_id AND c.id=a.candidate_id AND c.company_id=h.company_id WHERE h.id=? AND h.company_id=?",
        ),
        (
            "talent_rd_hire_handoff_queue",
            r#"["company"]"#,
            "SELECT h.*,o.application_id,o.currency,o.base_amount_minor,o.start_date,o.version offer_version,c.legal_name,c.version candidate_version,c.status candidate_status,r.job_title,x.id receipt_id,x.person_id,x.engagement_id FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id JOIN talent_candidates c ON c.id=h.candidate_id JOIN talent_applications a ON a.id=o.application_id JOIN talent_requisitions r ON r.id=a.requisition_id LEFT JOIN talent_hire_consumption_receipts x ON x.handoff_id=h.id WHERE h.company_id=? ORDER BY h.created_at,h.id",
        ),
    ];
    for (kind, values, sql) in cases {
        let payload = format!(r#"{{"kind":"{kind}","values":{values}}}"#);
        let (status, body, _) = http(
            addr,
            post(
                "/api/v1/holdings/acme/talent/command-plan",
                "acme.example.test",
                None,
                &payload,
            ),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["statements"][0]["sql"], serde_json::json!(sql));
        assert_eq!(
            body["data"]["statements"][0]["values"],
            serde_json::from_str::<serde_json::Value>(values).unwrap()
        );

        let bad = format!(r#"{{"kind":"{kind}","values":[]}}"#);
        let (status, _, _) = http(
            addr,
            post(
                "/api/v1/holdings/acme/talent/command-plan",
                "acme.example.test",
                None,
                &bad,
            ),
        )
        .await;
        assert_eq!(status, 422);
    }

    server.abort();
}
