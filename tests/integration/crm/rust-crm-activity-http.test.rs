//! Actual owner-local CRM activity command HTTP router tests.
//! These prove ingress facts only; composition, Account session, purchase,
//! Workflow, Sales recipient, notification and PostgreSQL remain separate
//! responsibilities.
use erpx_server_crm::public::activities_http::{
    CrmActivityCommandHttpBackend, CrmActivityCommandHttpRequest, CrmActivityCommandHttpTarget,
    crm_activity_command_http_router,
};
use erpx_server_crm::public::commands_http::{CommandHttpFuture, CrmCommandHttpReply};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl CrmActivityCommandHttpBackend for Noop {
    fn create_activity<'a>(
        &'a self,
        _: CrmActivityCommandHttpRequest<'a>,
    ) -> CommandHttpFuture<'a> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
    fn change_activity<'a>(
        &'a self,
        _: CrmActivityCommandHttpRequest<'a>,
    ) -> CommandHttpFuture<'a> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
    fn reassign_activity<'a>(
        &'a self,
        _: CrmActivityCommandHttpRequest<'a>,
    ) -> CommandHttpFuture<'a> {
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
    activity_id: Option<String>,
    key: String,
    action: String,
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
}

fn reply(
    request: &CrmActivityCommandHttpRequest<'_>,
    status: &str,
    version: i64,
) -> CrmCommandHttpReply {
    CrmCommandHttpReply {
        data: serde_json::json!({
            "id": request.activity_id.unwrap_or("crm-activity-new"),
            "version": version,
            "status": status
        }),
        context: serde_json::json!({"userId": "owner", "companyId": "company"}),
    }
}

impl Probe {
    fn record(&self, request: &CrmActivityCommandHttpRequest<'_>, kind: &'static str) {
        self.seen.lock().unwrap().push(Seen {
            kind,
            activity_id: request.activity_id.map(str::to_owned),
            key: request.key.as_str().into(),
            action: request
                .body
                .get("action")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .into(),
        });
    }
}

impl CrmActivityCommandHttpBackend for Probe {
    fn create_activity<'a>(
        &'a self,
        request: CrmActivityCommandHttpRequest<'a>,
    ) -> CommandHttpFuture<'a> {
        Box::pin(async move {
            self.record(&request, "create");
            Ok(reply(&request, "open", 1))
        })
    }
    fn change_activity<'a>(
        &'a self,
        request: CrmActivityCommandHttpRequest<'a>,
    ) -> CommandHttpFuture<'a> {
        Box::pin(async move {
            self.record(&request, "change");
            Ok(reply(&request, "cancelled", 2))
        })
    }
    fn reassign_activity<'a>(
        &'a self,
        request: CrmActivityCommandHttpRequest<'a>,
    ) -> CommandHttpFuture<'a> {
        Box::pin(async move {
            self.record(&request, "reassign");
            Ok(reply(&request, "open", 2))
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || CrmActivityCommandHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(crm_activity_command_http_router(vec![target()]).is_ok());
    assert!(crm_activity_command_http_router(vec![target(), target()]).is_err());
    assert!(
        crm_activity_command_http_router(vec![CrmActivityCommandHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(crm_activity_command_http_router(Vec::new()).is_err());
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

const CREATE_PATH: &str = "/api/v1/holdings/acme/crm/activities";
const CHANGE_PATH: &str = "/api/v1/holdings/acme/crm/activities/crm-activity-1/changes";

#[tokio::test]
async fn actual_router_forwards_create_change_and_reassign_separately() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = crm_activity_command_http_router(vec![CrmActivityCommandHttpTarget {
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
            CREATE_PATH,
            "acme.example.test",
            Some("key-create"),
            Some("https://acme.example.test"),
            r#"{"leadId":"lead-1","expectedVersion":1,"activityType":"call","subject":"Call the customer","details":"Discuss scope","ownerId":"owner","dueDate":"2026-11-20"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["version"], 1);
    assert_eq!(body["data"]["status"], "open");
    assert!(raw.contains("cache-control: private, no-store"));
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            kind: "create",
            activity_id: None,
            key: "key-create".into(),
            action: "".into(),
        })
    );

    let (status, body, _) = http(
        addr,
        post(
            CHANGE_PATH,
            "acme.example.test",
            Some("key-complete"),
            None,
            r#"{"action":"cancel","expectedVersion":1,"reason":"No longer needed"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["id"], "crm-activity-1");
    assert_eq!(body["data"]["status"], "cancelled");
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            kind: "change",
            activity_id: Some("crm-activity-1".into()),
            key: "key-complete".into(),
            action: "cancel".into(),
        })
    );

    let (status, body, _) = http(
        addr,
        post(
            CHANGE_PATH,
            "acme.example.test",
            Some("key-reassign"),
            None,
            r#"{"action":"reassign","expectedVersion":1,"ownerId":"owner-2","dueDate":"2026-12-01","reason":"Territory change"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "open");
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            kind: "reassign",
            activity_id: Some("crm-activity-1".into()),
            key: "key-reassign".into(),
            action: "reassign".into(),
        })
    );

    for (key, origin, payload, expected_status, expected_code) in [
        (
            None,
            None,
            r#"{"action":"cancel","expectedVersion":1,"reason":"No longer needed"}"#,
            422,
            "COMMAND_KEY_REQUIRED",
        ),
        (
            Some("key-bad-origin"),
            Some("https://evil.example.test"),
            r#"{"action":"cancel","expectedVersion":1,"reason":"No longer needed"}"#,
            403,
            "ORIGIN_FORBIDDEN",
        ),
        (
            Some("key-outcome"),
            None,
            r#"{"action":"outcome","expectedVersion":1,"reason":"No longer needed"}"#,
            422,
            "VALIDATION_FAILED",
        ),
    ] {
        let (status, body, _) = http(
            addr,
            post(CHANGE_PATH, "acme.example.test", key, origin, payload),
        )
        .await;
        assert_eq!(status, expected_status);
        assert_eq!(body["error"]["code"], expected_code);
    }

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/other/crm/activities",
            "acme.example.test",
            Some("key-scope"),
            None,
            r#"{"leadId":"lead-1","expectedVersion":1,"activityType":"call","subject":"Call the customer","details":"Discuss scope","ownerId":"owner","dueDate":"2026-11-20"}"#,
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    server.abort();
}
