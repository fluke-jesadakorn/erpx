//! Actual owner-local Customer 360 contact-note HTTP router tests. These prove
//! ingress facts only; Account authority, source visibility, same-person policy
//! and PostgreSQL remain separate composition responsibilities.
use erpx_server_customer_360::public::customer_notes::{
    CustomerNoteCommandResult, CustomerNoteHistoryRow, CustomerNoteRow, CustomerNoteSource,
    CustomerNotesView,
};
use erpx_server_customer_360::public::customer_notes_http::{
    CustomerNotesHttpBackend, CustomerNotesHttpCommandRequest, CustomerNotesHttpFuture,
    CustomerNotesHttpReadRequest, CustomerNotesHttpTarget, customer_notes_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl CustomerNotesHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: CustomerNotesHttpReadRequest<'a>,
    ) -> CustomerNotesHttpFuture<'a, CustomerNotesView> {
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
        _: CustomerNotesHttpCommandRequest<'a>,
    ) -> CustomerNotesHttpFuture<'a, CustomerNoteCommandResult> {
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
    customer: Option<String>,
    order: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SeenCommand {
    host: String,
    holding: String,
    key: String,
    id: Option<String>,
}

struct Probe {
    reads: Arc<Mutex<Vec<SeenRead>>>,
    commands: Arc<Mutex<Vec<SeenCommand>>>,
    fail: Option<BusinessError>,
}

impl CustomerNotesHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: CustomerNotesHttpReadRequest<'a>,
    ) -> CustomerNotesHttpFuture<'a, CustomerNotesView> {
        Box::pin(async move {
            self.reads.lock().unwrap().push(SeenRead {
                host: request.host.into(),
                holding: request.holding_slug.into(),
                customer: request.customer_id.map(str::to_owned),
                order: request.order_id.map(str::to_owned),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(CustomerNotesView {
                customer: CustomerNoteSource {
                    id: "customer-1".into(),
                    name: "Acme Customer".into(),
                    code: "C-001".into(),
                    version: 1,
                },
                order: None,
                notes: vec![CustomerNoteRow {
                    id: "contact-1".into(),
                    company_id: "company".into(),
                    customer_id: "customer-1".into(),
                    order_id: None,
                    channel: "phone".into(),
                    occurred_at: 1,
                    body: "Bounded note".into(),
                    status: "active".into(),
                    version: 1,
                    created_by: "user".into(),
                    created_at: 1,
                    updated_at: 1,
                    can_change: true,
                }],
                history: vec![CustomerNoteHistoryRow {
                    id: "contact-event-1".into(),
                    note_id: "contact-1".into(),
                    version: 1,
                    action: "create".into(),
                    body: "Bounded note".into(),
                    reason: "Initial contact record".into(),
                    actor_id: "user".into(),
                    created_at: 1,
                }],
                can_create: true,
            })
        })
    }
    fn command<'a>(
        &'a self,
        request: CustomerNotesHttpCommandRequest<'a>,
    ) -> CustomerNotesHttpFuture<'a, CustomerNoteCommandResult> {
        Box::pin(async move {
            self.commands.lock().unwrap().push(SeenCommand {
                host: request.host.into(),
                holding: request.holding_slug.into(),
                key: request.key.into(),
                id: request
                    .body
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(CustomerNoteCommandResult {
                id: "contact-1".into(),
                version: 1,
                status: "active".into(),
            })
        })
    }
}

fn target(backend: Arc<dyn CustomerNotesHttpBackend>) -> CustomerNotesHttpTarget {
    CustomerNotesHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend: Arc<dyn CustomerNotesHttpBackend> = Arc::new(Noop);
    assert!(customer_notes_http_router(vec![target(backend.clone())]).is_ok());
    assert!(
        customer_notes_http_router(vec![target(backend.clone()), target(backend.clone())])
            .is_err()
    );
    for invalid in [
        CustomerNotesHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target(backend.clone())
        },
        CustomerNotesHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target(backend.clone())
        },
        CustomerNotesHttpTarget {
            holding_slug: "Acme".into(),
            ..target(backend.clone())
        },
        CustomerNotesHttpTarget {
            host: String::new(),
            ..target(backend.clone())
        },
    ] {
        assert!(customer_notes_http_router(vec![invalid]).is_err());
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
        key.map(|key| format!("Idempotency-Key: {key}\r\n")).unwrap_or_default(),
        body.len()
    )
}

#[tokio::test]
async fn actual_router_forwards_exact_sources_and_maps_business_errors() {
    let reads = Arc::new(Mutex::new(Vec::new()));
    let commands = Arc::new(Mutex::new(Vec::new()));
    let router = customer_notes_http_router(vec![
        target(Arc::new(Probe {
            reads: reads.clone(),
            commands: commands.clone(),
            fail: None,
        })),
        CustomerNotesHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe {
                reads: reads.clone(),
                commands: commands.clone(),
                fail: Some(BusinessError::new(
                    FailureKind::Conflict,
                    "CUSTOMER_SOURCE_CHANGED",
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

    let (status, body, raw) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customer-notes?customerId=customer-1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["customer"]["id"], "customer-1");
    assert_eq!(body["data"]["notes"][0]["canChange"], true);
    assert!(raw.contains("cache-control: private, no-store"));
    assert!(raw.contains("referrer-policy: no-referrer"));
    assert!(raw.contains("x-content-type-options: nosniff"));
    assert_eq!(
        reads.lock().unwrap().last().cloned(),
        Some(SeenRead {
            host: "acme.example.test".into(),
            holding: "acme".into(),
            customer: Some("customer-1".into()),
            order: None,
        })
    );

    let (status, _, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customer-notes?orderId=order-1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        reads.lock().unwrap().last().cloned(),
        Some(SeenRead {
            host: "acme.example.test".into(),
            holding: "acme".into(),
            customer: None,
            order: Some("order-1".into()),
        })
    );

    for (path, code) in [
        ("/api/v1/holdings/acme/customer-notes", "SOURCE_REQUIRED"),
        ("/api/v1/holdings/acme/customer-notes?customerId=a&orderId=b", "SOURCE_REQUIRED"),
        ("/api/v1/holdings/acme/customer-notes?customerId=a&customerId=b", "CUSTOMER_NOTE_REQUEST_INVALID"),
        ("/api/v1/holdings/acme/customer-notes?q=a", "CUSTOMER_NOTE_REQUEST_INVALID"),
        ("/api/v1/holdings/acme/customer-notes?customerId=", "CUSTOMER_NOTE_REQUEST_INVALID"),
        ("/api/v1/holdings/acme/customer-notes?customerId=%07", "CUSTOMER_NOTE_REQUEST_INVALID"),
    ] {
        let (status, body, _) = http(addr, get(path, "acme.example.test")).await;
        assert_eq!(status, 422, "path: {path}");
        assert_eq!(body["error"]["code"], code, "path: {path}");
    }

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customer-notes?customerId=customer-1",
            "other.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/fail/customer-notes?customerId=customer-1",
            "fail.example.test",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "CUSTOMER_SOURCE_CHANGED");

    let path = "/api/v1/holdings/acme/customer-notes";
    let payload = r#"{"customerId":"customer-1","expectedVersion":1,"channel":"phone","occurredAt":1,"body":"Bounded note"}"#;
    let (status, body, _) = http(
        addr,
        post(
            path,
            "acme.example.test",
            "https://acme.example.test",
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
            "acme.example.test",
            "https://other.example.test",
            Some("command-key-1"),
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
            "acme.example.test",
            "https://acme.example.test",
            Some("command-key-1"),
            payload,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["id"], "contact-1");
    assert_eq!(
        commands.lock().unwrap().last().cloned(),
        Some(SeenCommand {
            host: "acme.example.test".into(),
            holding: "acme".into(),
            key: "command-key-1".into(),
            id: None,
        })
    );

    let change = r#"{"id":"contact-1","expectedVersion":1,"action":"archive","body":"Bounded note","reason":"Retire note"}"#;
    let (status, body, _) = http(
        addr,
        post(
            path,
            "acme.example.test",
            "https://acme.example.test",
            Some("command-key-2"),
            change,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        commands.lock().unwrap().last().cloned(),
        Some(SeenCommand {
            host: "acme.example.test".into(),
            holding: "acme".into(),
            key: "command-key-2".into(),
            id: Some("contact-1".into()),
        })
    );

    for invalid in [
        r#"{"customerId":"customer-1","expectedVersion":1,"channel":"phone","occurredAt":1,"body":"Bounded note","extra":true}"#,
        r#"{"customerId":"customer-1","expectedVersion":1,"channel":"phone","occurredAt":1,"body":"x"}"#,
        r#"{"customerId":"customer-1","expectedVersion":1,"channel":"fax","occurredAt":1,"body":"Bounded note"}"#,
        r#"{"customerId":"customer-1","orderId":"order-1","expectedVersion":1,"channel":"phone","occurredAt":1,"body":"Bounded note"}"#,
        r#"{"id":"contact-1","expectedVersion":1,"action":"delete","body":"Bounded note","reason":"Retire note"}"#,
    ] {
        let (status, _, _) = http(
            addr,
            post(
                path,
                "acme.example.test",
                "https://acme.example.test",
                Some("command-key-3"),
                invalid,
            ),
        )
        .await;
        assert_eq!(status, 422, "body: {invalid}");
    }
}
