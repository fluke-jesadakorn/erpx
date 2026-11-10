//! Actual owner-local HTTP router tests for the native Talent lifecycle slice.
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_talent::public::talent_http::{
    TalentLifecycleHttpBackend, TalentLifecycleHttpCommand, TalentLifecycleHttpFuture,
    TalentLifecycleHttpRequest, TalentLifecycleHttpResponse, TalentLifecycleHttpTarget,
    talent_lifecycle_http_router,
};
use std::sync::{Arc, Mutex};

struct Noop;

impl TalentLifecycleHttpBackend for Noop {
    fn execute<'a>(
        &'a self,
        _: TalentLifecycleHttpRequest<'a>,
    ) -> TalentLifecycleHttpFuture<'a, TalentLifecycleHttpResponse> {
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

impl TalentLifecycleHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: TalentLifecycleHttpRequest<'a>,
    ) -> TalentLifecycleHttpFuture<'a, TalentLifecycleHttpResponse> {
        Box::pin(async move {
            let seen = match &request.command {
                TalentLifecycleHttpCommand::CandidateCreate {
                    idempotency_key,
                    body,
                } => format!(
                    "candidate-create|{idempotency_key}|{}",
                    body["legalName"].as_str().unwrap_or("")
                ),
                TalentLifecycleHttpCommand::CandidatePrivacy { candidate_id } => {
                    format!("candidate-privacy|{candidate_id}")
                }
                TalentLifecycleHttpCommand::CandidatePrivacyChange {
                    candidate_id,
                    idempotency_key,
                    body,
                } => format!(
                    "candidate-privacy-change|{candidate_id}|{idempotency_key}|{}",
                    body["action"].as_str().unwrap_or("")
                ),
                TalentLifecycleHttpCommand::ApplicationCreate {
                    idempotency_key,
                    body,
                } => format!(
                    "application-create|{idempotency_key}|{}",
                    body["requisitionId"].as_str().unwrap_or("")
                ),
                TalentLifecycleHttpCommand::ApplicationChange {
                    application_id,
                    idempotency_key,
                    body,
                } => format!(
                    "application-change|{application_id}|{idempotency_key}|{}",
                    body["action"].as_str().unwrap_or("")
                ),
                TalentLifecycleHttpCommand::ProposalCreate {
                    idempotency_key,
                    body,
                } => format!(
                    "proposal-create|{idempotency_key}|{}",
                    body["recommendation"].as_str().unwrap_or("")
                ),
                TalentLifecycleHttpCommand::ProposalChange {
                    proposal_id,
                    idempotency_key,
                    body,
                } => format!(
                    "proposal-change|{proposal_id}|{idempotency_key}|{}",
                    body["action"].as_str().unwrap_or("")
                ),
                TalentLifecycleHttpCommand::OfferCreate {
                    idempotency_key,
                    body,
                } => format!(
                    "offer-create|{idempotency_key}|{}",
                    body["baseAmount"].as_str().unwrap_or("")
                ),
                TalentLifecycleHttpCommand::OfferChange {
                    offer_id,
                    idempotency_key,
                    body,
                } => format!(
                    "offer-change|{offer_id}|{idempotency_key}|{}",
                    body["action"].as_str().unwrap_or("")
                ),
                TalentLifecycleHttpCommand::HireHandoff { handoff_id } => {
                    format!("handoff|{handoff_id}")
                }
                TalentLifecycleHttpCommand::HireHandoffQueue => "handoff-queue".to_owned(),
            };
            self.seen.lock().unwrap().push(seen);
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "CONFLICT",
                    "Synthetic changed Talent record.",
                ));
            }
            Ok(TalentLifecycleHttpResponse {
                data: match request.command {
                    TalentLifecycleHttpCommand::CandidatePrivacy { .. } => serde_json::json!({
                        "candidate": {"id": "talent-candidate-1", "status": "active"},
                        "actions": {"hold": true, "release": false, "withdraw": true, "erase": false},
                        "history": []
                    }),
                    TalentLifecycleHttpCommand::HireHandoffQueue => {
                        serde_json::json!({"handoffs": []})
                    }
                    TalentLifecycleHttpCommand::HireHandoff { .. } => serde_json::json!({
                        "handoff": {"id": "talent-hire-1", "offer_status": "accepted"}
                    }),
                    TalentLifecycleHttpCommand::CandidateCreate { .. } => serde_json::json!({
                        "id": "talent-candidate-1", "status": "active", "version": 1
                    }),
                    TalentLifecycleHttpCommand::CandidatePrivacyChange { .. } => {
                        serde_json::json!({
                            "id": "talent-candidate-1", "status": "active", "version": 2, "privacyHold": true
                        })
                    }
                    TalentLifecycleHttpCommand::ApplicationCreate { .. }
                    | TalentLifecycleHttpCommand::ApplicationChange { .. } => serde_json::json!({
                        "id": "talent-application-1", "status": "applied", "version": 1
                    }),
                    TalentLifecycleHttpCommand::ProposalCreate { .. }
                    | TalentLifecycleHttpCommand::ProposalChange { .. } => serde_json::json!({
                        "id": "talent-proposal-1", "status": "submitted", "version": 1
                    }),
                    TalentLifecycleHttpCommand::OfferCreate { .. }
                    | TalentLifecycleHttpCommand::OfferChange { .. } => serde_json::json!({
                        "id": "talent-offer-1", "status": "draft", "version": 1
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
    let target = || TalentLifecycleHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(talent_lifecycle_http_router(vec![target()]).is_ok());
    assert!(talent_lifecycle_http_router(vec![target(), target()]).is_err());
    assert!(
        talent_lifecycle_http_router(vec![TalentLifecycleHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        talent_lifecycle_http_router(vec![TalentLifecycleHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        talent_lifecycle_http_router(vec![TalentLifecycleHttpTarget {
            holding_slug: "ACME".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        talent_lifecycle_http_router(vec![TalentLifecycleHttpTarget {
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
async fn actual_router_forwards_every_talent_lifecycle_path() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = talent_lifecycle_http_router(vec![
        TalentLifecycleHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: false,
            }),
        },
        TalentLifecycleHttpTarget {
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
    let base = "/api/v1/holdings/acme/talent";
    let host = "acme.example.test";

    let (status, body, headers) = http(
        addr,
        post(
            &format!("{base}/candidates"),
            host,
            Some("talent-candidate-command-1"),
            r#"{"legalName":"Anong Candidate","email":"anong@example.test","consentEvidence":"Signed consent","retentionUntil":"2027-01-01"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "active");
    assert_eq!(body["context"]["userId"], "user");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    for (raw, expected) in [
        (
            get(
                &format!("{base}/candidates/talent-candidate-1/privacy"),
                host,
            ),
            "candidate-privacy|talent-candidate-1",
        ),
        (
            post(
                &format!("{base}/candidates/talent-candidate-1/privacy"),
                host,
                Some("talent-privacy-command-1"),
                r#"{"action":"hold","expectedVersion":1,"reason":"Independent privacy review"}"#,
            ),
            "candidate-privacy-change|talent-candidate-1|talent-privacy-command-1|hold",
        ),
        (
            post(
                &format!("{base}/applications"),
                host,
                Some("talent-application-command-1"),
                r#"{"requisitionId":"talent-req-1","requisitionVersion":2,"candidateId":"talent-candidate-1","ownerId":"owner"}"#,
            ),
            "application-create|talent-application-command-1|talent-req-1",
        ),
        (
            post(
                &format!("{base}/applications/talent-application-1"),
                host,
                Some("talent-application-command-2"),
                r#"{"action":"interview","expectedVersion":1,"reason":"Schedule the interview"}"#,
            ),
            "application-change|talent-application-1|talent-application-command-2|interview",
        ),
        (
            post(
                &format!("{base}/proposals"),
                host,
                Some("talent-proposal-command-1"),
                r#"{"applicationId":"talent-application-1","applicationVersion":1,"recommendation":"hire","scoreBps":8750,"evidence":"Strong interview evidence"}"#,
            ),
            "proposal-create|talent-proposal-command-1|hire",
        ),
        (
            post(
                &format!("{base}/proposals/talent-proposal-1"),
                host,
                Some("talent-proposal-command-2"),
                r#"{"action":"accept-proposal","expectedVersion":1,"reason":"Independent review"}"#,
            ),
            "proposal-change|talent-proposal-1|talent-proposal-command-2|accept-proposal",
        ),
        (
            post(
                &format!("{base}/offers"),
                host,
                Some("talent-offer-command-1"),
                r#"{"applicationId":"talent-application-1","applicationVersion":3,"currency":"THB","baseAmount":"45000.50","startDate":"2026-12-01","expiresOn":"2026-11-25","terms":"Twelve-month contract"}"#,
            ),
            "offer-create|talent-offer-command-1|45000.50",
        ),
        (
            post(
                &format!("{base}/offers/talent-offer-1"),
                host,
                Some("talent-offer-command-2"),
                r#"{"action":"approve-offer","expectedVersion":1,"reason":"Independent approval"}"#,
            ),
            "offer-change|talent-offer-1|talent-offer-command-2|approve-offer",
        ),
        (get(&format!("{base}/hire-handoffs"), host), "handoff-queue"),
        (
            get(&format!("{base}/hire-handoffs/talent-hire-1"), host),
            "handoff|talent-hire-1",
        ),
    ] {
        let (status, body, _) = http(addr, raw).await;
        assert_eq!(status, 200, "{expected}");
        assert_eq!(body["context"]["userId"], "user", "{expected}");
        let observed = seen.lock().unwrap().last().cloned().unwrap();
        assert_eq!(observed, expected);
    }

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/fail/talent/offers/talent-offer-1",
            "fail.example.test",
            Some("talent-offer-command-3"),
            r#"{"action":"send","expectedVersion":2,"reason":"Send the approved offer"}"#,
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "CONFLICT");

    server.abort();
}

#[tokio::test]
async fn router_rejects_unknown_targets_query_keys_bodies_and_routes() {
    let router = talent_lifecycle_http_router(vec![TalentLifecycleHttpTarget {
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
    let path = "/api/v1/holdings/acme/talent/candidates";
    let body = r#"{"legalName":"Anong Candidate","email":"anong@example.test","consentEvidence":"Signed consent","retentionUntil":"2027-01-01"}"#;

    let (status, _, _) = http(addr, get(&format!("{path}?extra=1"), "acme.example.test")).await;
    assert_eq!(status, 405);
    let (status, _, _) = http(addr, get(path, "acme.example.test")).await;
    assert_eq!(status, 405);
    let (status, _, _) = http(
        addr,
        post(path, "evil.example.test", Some("talent-command-1"), body),
    )
    .await;
    assert_eq!(status, 404);
    let (status, _, _) = http(
        addr,
        post(
            &format!("{path}?extra=1"),
            "acme.example.test",
            Some("talent-command-1"),
            body,
        ),
    )
    .await;
    assert_eq!(status, 422);
    let (status, _, _) = http(addr, post(path, "acme.example.test", None, body)).await;
    assert_eq!(status, 422);
    let (status, _, _) = http(addr, post(path, "acme.example.test", Some("short"), body)).await;
    assert_eq!(status, 422);
    let (status, _, _) = http(
        addr,
        post(
            path,
            "acme.example.test",
            Some("talent-command-2"),
            "{not-json",
        ),
    )
    .await;
    assert_eq!(status, 422);
    let (status, _, _) = http(
        addr,
        format!(
            "POST {path} HTTP/1.1\r\nHost: acme.example.test\r\nContent-Type: text/plain\r\nIdempotency-Key: talent-command-3\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
        ),
    )
    .await;
    assert_eq!(status, 422);
    let oversized = "a".repeat(201);
    let (status, _, _) = http(
        addr,
        post(
            &format!("/api/v1/holdings/acme/talent/candidates/{oversized}/privacy"),
            "acme.example.test",
            Some("talent-command-4"),
            r#"{"action":"hold","expectedVersion":1,"reason":"Independent privacy review"}"#,
        ),
    )
    .await;
    assert_eq!(status, 404);
    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/talent/unknown",
            "acme.example.test",
            Some("talent-command-5"),
            "{}",
        ),
    )
    .await;
    assert_eq!(status, 404);

    server.abort();
}
