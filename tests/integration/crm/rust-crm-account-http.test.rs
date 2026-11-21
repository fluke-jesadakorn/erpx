//! Actual owner-local CRM account identity command HTTP router tests.
//! These prove ingress facts only; composition, Account session, purchase,
//! Finance witness, Workflow ledger and PostgreSQL remain separate
//! responsibilities.
use erpx_server_crm::public::accounts_http::{
    CrmAccountCommandHttpBackend, CrmAccountCommandHttpRequest, CrmAccountCommandHttpTarget,
    crm_account_command_http_router,
};
use erpx_server_crm::public::commands_http::{CommandHttpFuture, CrmCommandHttpReply};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl CrmAccountCommandHttpBackend for Noop {
    fn create_account<'a>(&'a self, _: CrmAccountCommandHttpRequest<'a>) -> CommandHttpFuture<'a> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
    fn link_finance<'a>(&'a self, _: CrmAccountCommandHttpRequest<'a>) -> CommandHttpFuture<'a> {
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
    action: &'static str,
    key: String,
    host: String,
    holding: String,
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Probe {
    fn reply(
        request: &CrmAccountCommandHttpRequest<'_>,
        action: &'static str,
    ) -> CrmCommandHttpReply {
        CrmCommandHttpReply {
            data: serde_json::json!({
                "id": format!("{action}-{}", request.key.as_str()),
                "version": 1
            }),
            context: serde_json::json!({
                "userId": "user",
                "companyId": "company",
                "permissions": ["commerce.sales"]
            }),
        }
    }
}

impl CrmAccountCommandHttpBackend for Probe {
    fn create_account<'a>(
        &'a self,
        request: CrmAccountCommandHttpRequest<'a>,
    ) -> CommandHttpFuture<'a> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                action: "create-account",
                key: request.key.as_str().into(),
                host: request.host.into(),
                holding: request.holding_slug.into(),
            });
            Ok(Self::reply(&request, "account"))
        })
    }
    fn link_finance<'a>(
        &'a self,
        request: CrmAccountCommandHttpRequest<'a>,
    ) -> CommandHttpFuture<'a> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                action: "link-finance",
                key: request.key.as_str().into(),
                host: request.host.into(),
                holding: request.holding_slug.into(),
            });
            Ok(CrmCommandHttpReply {
                data: serde_json::json!({"id": "account-1", "version": 2}),
                context: serde_json::json!({"userId": "reviewer", "companyId": "company"}),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || CrmAccountCommandHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(crm_account_command_http_router(vec![target()]).is_ok());
    assert!(crm_account_command_http_router(vec![target(), target()]).is_err());
    assert!(
        crm_account_command_http_router(vec![CrmAccountCommandHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        crm_account_command_http_router(vec![CrmAccountCommandHttpTarget {
            holding_slug: "Acme".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(crm_account_command_http_router(Vec::new()).is_err());
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
async fn actual_router_forwards_native_identity_commands_and_fails_closed() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = crm_account_command_http_router(vec![CrmAccountCommandHttpTarget {
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

    const PATH: &str = "/api/v1/holdings/acme/crm/identity/changes";

    let (status, body, raw) = http(
        addr,
        post(
            PATH,
            "acme.example.test",
            Some("key-account"),
            Some("https://acme.example.test"),
            r#"{"action":"create-account","kind":"business","name":"Acme Pty Ltd"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["id"], "account-key-account");
    assert_eq!(body["data"]["version"], 1);
    assert_eq!(body["context"]["userId"], "user");
    assert!(raw.contains("cache-control: private, no-store"));
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            action: "create-account",
            key: "key-account".into(),
            host: "acme.example.test".into(),
            holding: "acme".into(),
        })
    );

    let (status, body, _) = http(
        addr,
        post(
            PATH,
            "acme.example.test",
            Some("key-finance"),
            None,
            r#"{"action":"link-finance","accountId":"account-1","customerId":"customer-1","expectedVersion":1,"evidence":"Reviewed customer"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["id"], "account-1");
    assert_eq!(body["data"]["version"], 2);
    assert_eq!(body["context"]["userId"], "reviewer");
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            action: "link-finance",
            key: "key-finance".into(),
            host: "acme.example.test".into(),
            holding: "acme".into(),
        })
    );

    for (key, origin, payload, expected_status, expected_code) in [
        (
            None,
            None,
            r#"{"action":"create-account","kind":"business","name":"Acme Pty Ltd"}"#,
            422,
            "COMMAND_KEY_REQUIRED",
        ),
        (
            Some("key-bad-origin"),
            Some("https://evil.example.test"),
            r#"{"action":"create-account","kind":"business","name":"Acme Pty Ltd"}"#,
            403,
            "ORIGIN_FORBIDDEN",
        ),
        (
            Some("key-extra"),
            None,
            r#"{"action":"create-account","kind":"business","name":"Acme Pty Ltd","extra":true}"#,
            422,
            "CRM_IDENTITY_REQUEST_INVALID",
        ),
        (
            Some("key-contact"),
            None,
            r#"{"action":"create-contact","accountId":"account-1","name":"Contact","detail":"Front desk"}"#,
            503,
            "CRM_IDENTITY_ACTION_NOT_NATIVE",
        ),
    ] {
        let (status, body, _) =
            http(addr, post(PATH, "acme.example.test", key, origin, payload)).await;
        assert_eq!(status, expected_status);
        assert_eq!(body["error"]["code"], expected_code);
    }

    for (path, host) in [
        (
            "/api/v1/holdings/other/crm/identity/changes",
            "acme.example.test",
        ),
        (PATH, "other.example.test"),
    ] {
        let (status, body, _) = http(
            addr,
            post(
                path,
                host,
                Some("key-scope"),
                None,
                r#"{"action":"create-account","kind":"business","name":"Acme Pty Ltd"}"#,
            ),
        )
        .await;
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    server.abort();
}
