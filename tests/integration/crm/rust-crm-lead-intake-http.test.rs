//! Actual owner-local CRM lead create / qualify command HTTP router tests.
//! These prove ingress facts only; composition, Account session, scope,
//! purchase, Finance/Commerce witnesses, Workflow and PostgreSQL remain
//! separate responsibilities.
use erpx_server_crm::public::commands_http::{CommandHttpFuture, CrmCommandHttpReply};
use erpx_server_crm::public::leads_intake_http::{
    CrmLeadIntakeHttpBackend, CrmLeadIntakeHttpRequest, CrmLeadIntakeHttpTarget,
    crm_lead_intake_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl CrmLeadIntakeHttpBackend for Noop {
    fn create_lead<'a>(&'a self, _: CrmLeadIntakeHttpRequest<'a>) -> CommandHttpFuture<'a> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
    fn qualify_lead<'a>(&'a self, _: CrmLeadIntakeHttpRequest<'a>) -> CommandHttpFuture<'a> {
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
    lead_id: Option<String>,
    key: String,
    title: String,
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl CrmLeadIntakeHttpBackend for Probe {
    fn create_lead<'a>(&'a self, request: CrmLeadIntakeHttpRequest<'a>) -> CommandHttpFuture<'a> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                kind: "create",
                lead_id: request.lead_id.map(str::to_owned),
                key: request.key.as_str().into(),
                title: request.body["title"].as_str().unwrap_or("").into(),
            });
            Ok(CrmCommandHttpReply {
                data: serde_json::json!({"id": "lead-new", "version": 1}),
                context: serde_json::json!({"userId": "owner", "companyId": "company"}),
            })
        })
    }
    fn qualify_lead<'a>(&'a self, request: CrmLeadIntakeHttpRequest<'a>) -> CommandHttpFuture<'a> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                kind: "qualify",
                lead_id: request.lead_id.map(str::to_owned),
                key: request.key.as_str().into(),
                title: String::new(),
            });
            Ok(CrmCommandHttpReply {
                data: serde_json::json!({"id": "lead-1", "version": 2, "opportunityId": "opportunity-1"}),
                context: serde_json::json!({"userId": "owner", "companyId": "company"}),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || CrmLeadIntakeHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(crm_lead_intake_http_router(vec![target()]).is_ok());
    assert!(crm_lead_intake_http_router(vec![target(), target()]).is_err());
    assert!(
        crm_lead_intake_http_router(vec![CrmLeadIntakeHttpTarget {
            holding_slug: "Acme".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(crm_lead_intake_http_router(Vec::new()).is_err());
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

const LEAD: &str = r#"{"branchId":"branch-1","title":"Office installation","contactName":"Synthetic contact","contactDetail":"contact@example.test","source":"referral","brief":"Install office","dueDate":"2026-11-20"}"#;
const QUALIFY: &str = r#"{"action":"qualify","expectedVersion":1,"customerId":"customer-1","expectedValue":"1000.00","closeDate":"2026-11-30","reason":"Reviewed customer"}"#;

#[tokio::test]
async fn actual_router_forwards_create_and_qualify_on_separate_routes() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = crm_lead_intake_http_router(vec![CrmLeadIntakeHttpTarget {
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
            "/api/v1/holdings/acme/crm/leads",
            "acme.example.test",
            Some("key-create"),
            Some("https://acme.example.test"),
            LEAD,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["id"], "lead-new");
    assert_eq!(body["context"]["userId"], "owner");
    assert!(raw.contains("cache-control: private, no-store"));
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            kind: "create",
            lead_id: None,
            key: "key-create".into(),
            title: "Office installation".into(),
        })
    );

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/crm/leads/lead-1/qualification/changes",
            "acme.example.test",
            Some("key-qualify"),
            None,
            QUALIFY,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["id"], "lead-1");
    assert_eq!(body["data"]["opportunityId"], "opportunity-1");
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            kind: "qualify",
            lead_id: Some("lead-1".into()),
            key: "key-qualify".into(),
            title: String::new(),
        })
    );

    for (path, key, origin, payload, expected_status, expected_code) in [
        (
            "/api/v1/holdings/acme/crm/leads",
            None,
            None,
            LEAD,
            422,
            "COMMAND_KEY_REQUIRED",
        ),
        (
            "/api/v1/holdings/acme/crm/leads",
            Some("key-bad-origin"),
            Some("https://evil.example.test"),
            LEAD,
            403,
            "ORIGIN_FORBIDDEN",
        ),
        (
            "/api/v1/holdings/acme/crm/leads",
            Some("key-extra"),
            None,
            r#"{"branchId":"branch-1","title":"Office installation","contactName":"Synthetic contact","contactDetail":"contact@example.test","source":"referral","brief":"Install office","dueDate":"2026-11-20","extra":true}"#,
            422,
            "CRM_LEAD_INTAKE_REQUEST_INVALID",
        ),
        (
            "/api/v1/holdings/acme/crm/leads/lead-1/qualification/changes",
            Some("key-wrong-action"),
            None,
            r#"{"action":"disqualify","expectedVersion":1,"reason":"No response"}"#,
            422,
            "CRM_LEAD_INPUT_INVALID",
        ),
    ] {
        let (status, body, _) =
            http(addr, post(path, "acme.example.test", key, origin, payload)).await;
        assert_eq!(status, expected_status);
        assert_eq!(body["error"]["code"], expected_code);
    }

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/other/crm/leads/lead-1/qualification/changes",
            "acme.example.test",
            Some("key-scope"),
            None,
            QUALIFY,
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    server.abort();
}
