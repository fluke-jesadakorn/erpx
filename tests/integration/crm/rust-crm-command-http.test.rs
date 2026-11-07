//! Actual owner-local CRM lead/opportunity command HTTP router tests.
//! These prove ingress facts only; composition, Account session, purchase,
//! Workflow ledger and PostgreSQL remain separate responsibilities.
use erpx_server_crm::public::leads_http::{
    CrmLeadCommandHttpBackend, CrmLeadCommandHttpFuture, CrmLeadCommandHttpRequest,
    CrmLeadCommandHttpTarget, crm_lead_command_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl CrmLeadCommandHttpBackend for Noop {
    fn change_lead<'a>(
        &'a self,
        _: CrmLeadCommandHttpRequest<'a>,
    ) -> CrmLeadCommandHttpFuture<'a> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
    fn change_opportunity<'a>(
        &'a self,
        _: CrmLeadCommandHttpRequest<'a>,
    ) -> CrmLeadCommandHttpFuture<'a> {
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
    kind: &'static str,
    lead_id: String,
    key: String,
    action: String,
    host: String,
    holding: String,
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl CrmLeadCommandHttpBackend for Probe {
    fn change_lead<'a>(
        &'a self,
        request: CrmLeadCommandHttpRequest<'a>,
    ) -> CrmLeadCommandHttpFuture<'a> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                kind: "lead",
                lead_id: request.lead_id.into(),
                key: request.key.as_str().into(),
                action: request.body["action"].as_str().unwrap_or("").into(),
                host: request.host.into(),
                holding: request.holding_slug.into(),
            });
            Ok(serde_json::json!({
                "id": request.lead_id,
                "version": 2,
                "opportunityId": null
            }))
        })
    }
    fn change_opportunity<'a>(
        &'a self,
        request: CrmLeadCommandHttpRequest<'a>,
    ) -> CrmLeadCommandHttpFuture<'a> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                kind: "opportunity",
                lead_id: request.lead_id.into(),
                key: request.key.as_str().into(),
                action: request.body["action"].as_str().unwrap_or("").into(),
                host: request.host.into(),
                holding: request.holding_slug.into(),
            });
            Ok(serde_json::json!({
                "id": request.lead_id,
                "version": 2,
                "leadVersion": 3,
                "stage": "lost"
            }))
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || CrmLeadCommandHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(crm_lead_command_http_router(vec![target()]).is_ok());
    assert!(crm_lead_command_http_router(vec![target(), target()]).is_err());
    assert!(
        crm_lead_command_http_router(vec![CrmLeadCommandHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        crm_lead_command_http_router(vec![CrmLeadCommandHttpTarget {
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

fn post(path: &str, host: &str, key: Option<&str>, origin: Option<&str>, body: &str) -> String {
    let mut request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(key) = key {
        request.push_str(&format!("Idempotency-Key: {key}\r\n"));
    }
    if let Some(origin) = origin {
        request.push_str(&format!("Origin: {origin}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);
    request
}

#[tokio::test]
async fn actual_router_forwards_typed_commands_and_rejects_bad_transport() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = crm_lead_command_http_router(vec![CrmLeadCommandHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(Probe { seen: seen.clone() }),
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, raw) = http(
        addr,
        post(
            "/api/v1/holdings/acme/crm/leads/lead-1/changes",
            "acme.example.test",
            Some("command-key-1"),
            Some("https://acme.example.test"),
            r#"{"action":"disqualify","expectedVersion":1,"reason":"No response"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["id"], "lead-1");
    assert_eq!(body["version"], 2);
    assert!(body["opportunityId"].is_null());
    assert!(raw.contains("cache-control: private, no-store"));
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            kind: "lead",
            lead_id: "lead-1".into(),
            key: "command-key-1".into(),
            action: "disqualify".into(),
            host: "acme.example.test".into(),
            holding: "acme".into(),
        })
    );

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/crm/leads/lead-1/opportunity/changes",
            "acme.example.test",
            Some("command-key-2"),
            None,
            r#"{"action":"lose","expectedVersion":1,"leadVersion":2,"lossReason":"timing","reason":"Deferred"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["stage"], "lost");
    assert_eq!(body["leadVersion"], 3);
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            kind: "opportunity",
            lead_id: "lead-1".into(),
            key: "command-key-2".into(),
            action: "lose".into(),
            host: "acme.example.test".into(),
            holding: "acme".into(),
        })
    );

    for (path, key, origin, payload, expected) in [
        (
            "/api/v1/holdings/acme/crm/leads/lead-1/changes",
            None,
            None,
            r#"{"action":"disqualify","expectedVersion":1,"reason":"No response"}"#,
            "COMMAND_KEY_REQUIRED",
        ),
        (
            "/api/v1/holdings/acme/crm/leads/lead-1/changes",
            Some("command-key-3"),
            Some("https://evil.example.test"),
            r#"{"action":"disqualify","expectedVersion":1,"reason":"No response"}"#,
            "ORIGIN_FORBIDDEN",
        ),
        (
            "/api/v1/holdings/acme/crm/leads/lead-1/changes",
            Some("command-key-4"),
            None,
            r#"{"action":"disqualify","expectedVersion":1,"reason":"No response","extra":true}"#,
            "CRM_COMMAND_REQUEST_INVALID",
        ),
        (
            "/api/v1/holdings/acme/crm/leads/lead-1/changes",
            Some("command-key-5"),
            None,
            r#"{"action":"qualify","expectedVersion":1,"reason":"Qualified"}"#,
            "CRM_COMMAND_NOT_NATIVE",
        ),
    ] {
        let (status, body, _) = http(
            addr,
            post(path, "acme.example.test", key, origin, payload),
        )
        .await;
        assert_eq!(status, if expected == "CRM_COMMAND_NOT_NATIVE" { 503 } else if expected == "ORIGIN_FORBIDDEN" { 403 } else { 422 });
        assert_eq!(body["error"]["code"], expected);
    }

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/other/crm/leads/lead-1/changes",
            "acme.example.test",
            Some("command-key-6"),
            None,
            r#"{"action":"disqualify","expectedVersion":1,"reason":"No response"}"#,
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/crm/leads/lead-1/changes",
            "other.example.test",
            Some("command-key-7"),
            None,
            r#"{"action":"disqualify","expectedVersion":1,"reason":"No response"}"#,
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    server.abort();
}
