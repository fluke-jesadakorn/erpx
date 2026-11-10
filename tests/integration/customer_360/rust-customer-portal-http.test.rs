//! Actual owner-local customer-portal HTTP router tests. Verified portal
//! sessions, purchases, Files and the owned transaction remain composition
//! responsibilities.
use erpx_server_customer_360::public::customer_portal::{
    CustomerPortalProjection, PortalCommandInput, PortalCommandOutcome,
};
use erpx_server_customer_360::public::customer_portal_http::{
    CustomerPortalHttpBackend, CustomerPortalHttpCommandRequest, CustomerPortalHttpFuture,
    CustomerPortalHttpReadRequest, CustomerPortalHttpTarget, customer_portal_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl CustomerPortalHttpBackend for Noop {
    fn projection<'a>(
        &'a self,
        _: CustomerPortalHttpReadRequest<'a>,
    ) -> CustomerPortalHttpFuture<'a, CustomerPortalProjection> {
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
        _: CustomerPortalHttpCommandRequest<'a>,
    ) -> CustomerPortalHttpFuture<'a, PortalCommandOutcome> {
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
    host: String,
    holding: String,
    key: Option<String>,
    action: Option<String>,
    note: Option<String>,
    attachments: Vec<String>,
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
    fail: Option<BusinessError>,
}

impl CustomerPortalHttpBackend for Probe {
    fn projection<'a>(
        &'a self,
        request: CustomerPortalHttpReadRequest<'a>,
    ) -> CustomerPortalHttpFuture<'a, CustomerPortalProjection> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                host: request.host.into(),
                holding: request.holding_slug.into(),
                key: None,
                action: None,
                note: None,
                attachments: Vec::new(),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(CustomerPortalProjection {
                items: Vec::new(),
                projection_version: "customer-order-v1",
            })
        })
    }

    fn command<'a>(
        &'a self,
        request: CustomerPortalHttpCommandRequest<'a>,
    ) -> CustomerPortalHttpFuture<'a, PortalCommandOutcome> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                host: request.host.into(),
                holding: request.holding_slug.into(),
                key: Some(request.key.into()),
                action: Some(request.input.action.as_str().into()),
                note: Some(request.input.note.clone()),
                attachments: request.input.attachment_set_ids.clone(),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(PortalCommandOutcome::new(
                "action".into(),
                "binding".into(),
                "order".into(),
                "request_support",
                "recorded".into(),
                100,
            ))
        })
    }
}

fn target(backend: Arc<dyn CustomerPortalHttpBackend>) -> CustomerPortalHttpTarget {
    CustomerPortalHttpTarget {
        host: "portal.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://portal.example.test".into(),
        backend,
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend: Arc<dyn CustomerPortalHttpBackend> = Arc::new(Noop);
    assert!(customer_portal_http_router(vec![target(backend.clone())]).is_ok());
    assert!(
        customer_portal_http_router(vec![target(backend.clone()), target(backend.clone())])
            .is_err()
    );
    for invalid in [
        CustomerPortalHttpTarget {
            origin: "http://portal.example.test".into(),
            ..target(backend.clone())
        },
        CustomerPortalHttpTarget {
            origin: "https://portal.example.test/path".into(),
            ..target(backend.clone())
        },
        CustomerPortalHttpTarget {
            holding_slug: "Acme".into(),
            ..target(backend.clone())
        },
        CustomerPortalHttpTarget {
            host: String::new(),
            ..target(backend.clone())
        },
    ] {
        assert!(customer_portal_http_router(vec![invalid]).is_err());
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

fn get(path: &str, host: &str, origin: Option<&str>) -> String {
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\n{}Connection: close\r\n\r\n",
        origin
            .map(|value| format!("Origin: {value}\r\n"))
            .unwrap_or_default()
    )
}

fn post(path: &str, host: &str, origin: Option<&str>, key: Option<&str>, body: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}{}Connection: close\r\n\r\n{body}",
        body.len(),
        origin
            .map(|value| format!("Origin: {value}\r\n"))
            .unwrap_or_default(),
        key.map(|value| format!("Idempotency-Key: {value}\r\n"))
            .unwrap_or_default(),
    )
}

const COMMAND: &str = r#"{"bindingId":"binding","expectedBindingVersion":3,"action":"request_support","note":"help","attachmentSetIds":["set-1"]}"#;

#[tokio::test]
async fn actual_router_reads_projection_and_accepts_idempotent_command() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = customer_portal_http_router(vec![
        target(Arc::new(Probe {
            seen: seen.clone(),
            fail: None,
        })),
        CustomerPortalHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: Some(BusinessError::new(
                    FailureKind::Forbidden,
                    "CUSTOMER_PORTAL_BINDING_EXPIRED",
                    "Expired.",
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
            "/api/v1/holdings/acme/customer-portal",
            "portal.example.test",
            None,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["projectionVersion"], "customer-order-v1");
    assert!(raw.contains("cache-control: private, no-store"));
    assert!(raw.contains("referrer-policy: no-referrer"));
    assert!(raw.contains("x-content-type-options: nosniff"));

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/customer-portal",
            "portal.example.test",
            Some("https://portal.example.test"),
            Some("command-key-0001"),
            COMMAND,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["action"], "request_support");
    assert_eq!(body["data"]["bindingId"], "binding");
    assert_eq!(body["data"]["createdAt"], 100);
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            host: "portal.example.test".into(),
            holding: "acme".into(),
            key: Some("command-key-0001".into()),
            action: Some("request_support".into()),
            note: Some("help".into()),
            attachments: vec!["set-1".into()],
        })
    );

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customer-portal?x=1",
            "portal.example.test",
            None,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "CUSTOMER_PORTAL_REQUEST_INVALID");

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customer-portal",
            "portal.example.test",
            Some("https://other.example.test"),
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/customer-portal",
            "other.example.test",
            None,
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn command_requires_origin_key_content_type_and_bounded_body() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = customer_portal_http_router(vec![
        target(Arc::new(Probe {
            seen: seen.clone(),
            fail: None,
        })),
        CustomerPortalHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: Some(BusinessError::new(
                    FailureKind::Forbidden,
                    "CUSTOMER_PORTAL_BINDING_EXPIRED",
                    "Expired.",
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
    let path = "/api/v1/holdings/acme/customer-portal";

    for (origin, key, body, code) in [
        (None, Some("command-key-0001"), COMMAND, "ORIGIN_FORBIDDEN"),
        (
            Some("https://other.example.test"),
            Some("command-key-0001"),
            COMMAND,
            "ORIGIN_FORBIDDEN",
        ),
        (
            Some("https://portal.example.test"),
            None,
            COMMAND,
            "COMMAND_KEY_REQUIRED",
        ),
        (
            Some("https://portal.example.test"),
            Some("short"),
            COMMAND,
            "COMMAND_KEY_REQUIRED",
        ),
        (
            Some("https://portal.example.test"),
            Some("command-key-0001"),
            r#"{"bindingId":"binding","expectedBindingVersion":3,"action":"request_support","unknown":1}"#,
            "CUSTOMER_PORTAL_REQUEST_INVALID",
        ),
    ] {
        let (status, response, _) =
            http(addr, post(path, "portal.example.test", origin, key, body)).await;
        let expected_status = if code == "ORIGIN_FORBIDDEN" { 403 } else { 422 };
        assert_eq!(status, expected_status, "code: {code}");
        assert_eq!(response["error"]["code"], code, "code: {code}");
    }

    let (status, body, _) = http(
        addr,
        post(
            path,
            "portal.example.test",
            Some("https://portal.example.test"),
            Some("command-key-0001"),
            r#"{"bindingId":"binding","expectedBindingVersion":3,"action":"acknowledge_delivery","note":"ok"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "recorded");

    // The fake command backend fails for the other registered target.
    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/fail/customer-portal",
            "fail.example.test",
            Some("https://fail.example.test"),
            Some("command-key-0001"),
            COMMAND,
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "CUSTOMER_PORTAL_BINDING_EXPIRED");
}

#[test]
fn command_input_denies_unknown_and_unbounded_fields() {
    for invalid in [
        r#"{"bindingId":"binding","expectedBindingVersion":3,"action":"acknowledge_delivery","extra":true}"#,
        r#"{"bindingId":"binding","expectedBindingVersion":3,"action":"unknown"}"#,
    ] {
        assert!(serde_json::from_str::<PortalCommandInput>(invalid).is_err());
    }
    for invalid in [
        r#"{"bindingId":"binding","expectedBindingVersion":0,"action":"acknowledge_delivery"}"#,
        r#"{"bindingId":"","expectedBindingVersion":3,"action":"acknowledge_delivery"}"#,
        r#"{"bindingId":"binding","expectedBindingVersion":3,"action":"request_support","attachmentSetIds":[""]}"#,
    ] {
        let input: PortalCommandInput = serde_json::from_str(invalid).unwrap();
        assert!(input.validate().is_err(), "{invalid}");
    }
    let input: PortalCommandInput = serde_json::from_str(COMMAND).unwrap();
    assert_eq!(input.attachment_set_ids, vec!["set-1".to_owned()]);
    input.validate().unwrap();
}
