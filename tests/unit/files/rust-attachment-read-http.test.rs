//! Actual owner-local HTTP router tests for the native Files attachment read.
use erpx_server_files::public::attachment_http::{
    AttachmentCommandHttpWithdrawRequest, AttachmentReadHttpBackend, AttachmentReadHttpFuture,
    AttachmentReadHttpListRequest, AttachmentReadHttpReply, AttachmentReadHttpStatusRequest,
    AttachmentReadHttpTarget, attachment_read_router,
};
use erpx_server_files::public::portal_attachment_http::{
    PortalAttachmentHttpBackend, PortalAttachmentHttpFuture, PortalAttachmentHttpReply,
    PortalAttachmentHttpTarget, PortalAttachmentHttpWithdrawRequest,
    portal_attachment_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl AttachmentReadHttpBackend for Probe {
    fn list<'a>(
        &'a self,
        request: AttachmentReadHttpListRequest<'a>,
    ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply> {
        Box::pin(async move {
            let cookie = request
                .headers
                .get("cookie")
                .and_then(|value| value.to_str().ok());
            self.seen.lock().unwrap().push(format!(
                "list|{}|{}|{}|{}|{}|{}|{}|{cookie:?}",
                request.host,
                request.holding_slug,
                request.filter.target_type(),
                request.filter.target_id(),
                request.filter.purpose_key().unwrap_or(""),
                request.filter.component_kind().unwrap_or(""),
                request.read_only,
            ));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "ATTACHMENT_READ_CHANGED",
                    "Synthetic changed files.",
                ));
            }
            Ok(AttachmentReadHttpReply {
                data: serde_json::json!([]),
                context: serde_json::json!({"userId": "actor"}),
            })
        })
    }
    fn status<'a>(
        &'a self,
        request: AttachmentReadHttpStatusRequest<'a>,
    ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "status|{}|{}|{}",
                request.host, request.holding_slug, request.link_id
            ));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "ATTACHMENT_READ_CHANGED",
                    "Synthetic changed files.",
                ));
            }
            Ok(AttachmentReadHttpReply {
                data: serde_json::json!({"linkId": request.link_id}),
                context: serde_json::json!({"userId": "actor"}),
            })
        })
    }
    fn withdraw<'a>(
        &'a self,
        request: AttachmentCommandHttpWithdrawRequest<'a>,
    ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "withdraw|{}|{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.link_id,
                request.reason,
                request.command_key
            ));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "ATTACHMENT_LINK_LOCKED",
                    "Synthetic closed file.",
                ));
            }
            Ok(AttachmentReadHttpReply {
                data: serde_json::json!({"linkId": request.link_id, "linkState": "withdrawn"}),
                context: serde_json::json!({"userId": "actor"}),
            })
        })
    }
}

struct Noop;
impl AttachmentReadHttpBackend for Noop {
    fn list<'a>(
        &'a self,
        _: AttachmentReadHttpListRequest<'a>,
    ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
    fn status<'a>(
        &'a self,
        _: AttachmentReadHttpStatusRequest<'a>,
    ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
    fn withdraw<'a>(
        &'a self,
        _: AttachmentCommandHttpWithdrawRequest<'a>,
    ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply> {
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
    let target = || AttachmentReadHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(attachment_read_router(vec![target()]).is_ok());
    assert!(attachment_read_router(vec![target(), target()]).is_err());
    assert!(
        attachment_read_router(vec![AttachmentReadHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        attachment_read_router(vec![AttachmentReadHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        attachment_read_router(vec![AttachmentReadHttpTarget {
            holding_slug: "Acme".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        attachment_read_router(vec![AttachmentReadHttpTarget {
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
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nCookie: erpx=session\r\nConnection: close\r\n\r\n"
    )
}

#[allow(clippy::too_many_arguments)]
fn post(
    path: &str,
    host: &str,
    origin: Option<&str>,
    key: Option<&str>,
    content_type: &str,
    body: &str,
) -> String {
    let mut request = format!("POST {path} HTTP/1.1\r\nHost: {host}\r\nCookie: erpx=session\r\n");
    if let Some(origin) = origin {
        request.push_str(&format!("Origin: {origin}\r\n"));
    }
    if let Some(key) = key {
        request.push_str(&format!("Idempotency-Key: {key}\r\n"));
    }
    request.push_str(&format!(
        "Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ));
    request
}

fn target(
    host: &str,
    slug: &str,
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
) -> AttachmentReadHttpTarget {
    AttachmentReadHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: format!("https://{host}"),
        backend: Arc::new(Probe { seen, fail }),
    }
}

#[tokio::test]
async fn actual_router_forwards_exact_set_and_status_reads_with_private_headers() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = attachment_read_router(vec![
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
        get(
            "/api/v1/holdings/acme/attachment-sets?targetType=asset&targetId=asset-1&purposeKey=manual&componentKind=record&expectedTargetVersion=3&readOnly=true",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"], serde_json::json!([]));
    assert_eq!(body["context"]["userId"], "actor");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/attachment-links/link-1/status",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["linkId"], "link-1");
    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            "list|acme.example.test|acme|asset|asset-1|manual|record|true|Some(\"erpx=session\")",
            "status|acme.example.test|acme|link-1",
        ]
    );

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/fail/attachment-sets?targetType=asset&targetId=asset-1",
            "fail.example.test",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "ATTACHMENT_READ_CHANGED");
}

#[tokio::test]
async fn router_rejects_unknown_scope_query_versions_bodies_and_paths() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = attachment_read_router(vec![target(
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
            "/api/v1/holdings/other/attachment-sets?targetType=asset&targetId=asset-1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);

    for path in [
        "/api/v1/holdings/acme/attachment-sets?targetType=asset&targetId=asset-1&extra=1",
        "/api/v1/holdings/acme/attachment-sets?targetType=asset&targetType=asset&targetId=asset-1",
        "/api/v1/holdings/acme/attachment-sets?targetId=asset-1",
        "/api/v1/holdings/acme/attachment-sets?targetType=asset",
        "/api/v1/holdings/acme/attachment-sets?targetType=asset&targetId=asset-1&componentKind=document",
        "/api/v1/holdings/acme/attachment-sets?targetType=asset&targetId=asset-1&expectedTargetVersion=0",
        "/api/v1/holdings/acme/attachment-sets?targetType=asset&targetId=asset-1&readOnly=yes",
        "/api/v1/holdings/acme/attachment-links/link-1/status?targetType=asset",
    ] {
        let (status, _, _) = http(addr, get(path, "acme.example.test")).await;
        assert_eq!(status, 422, "{path}");
    }

    let (status, _, _) = http(
        addr,
        "GET /api/v1/holdings/acme/attachment-sets?targetType=asset&targetId=asset-1 HTTP/1.1\r\nHost: acme.example.test\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string(),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        "GET /api/v1/holdings/acme/attachment-links/link-1/status HTTP/1.1\r\nHost: acme.example.test\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string(),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/attachment-sets?targetType=asset&targetId=asset-1",
            "other.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);

    let long = "f".repeat(201);
    let (status, _, _) = http(
        addr,
        get(
            &format!("/api/v1/holdings/acme/attachment-links/{long}/status"),
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);

    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn router_forwards_one_bounded_withdraw_with_exact_key_and_origin() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = attachment_read_router(vec![
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
        post(
            "/api/v1/holdings/acme/attachment-links/link-1/withdraw",
            "acme.example.test",
            Some("https://acme.example.test"),
            Some("withdraw-key-1"),
            "application/json",
            r#"{"reason":"duplicate upload"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["linkState"], "withdrawn");
    assert_eq!(body["context"]["userId"], "actor");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));
    assert_eq!(
        *seen.lock().unwrap(),
        vec!["withdraw|acme.example.test|acme|link-1|duplicate upload|withdraw-key-1".to_string()]
    );

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/fail/attachment-links/link-2/withdraw",
            "fail.example.test",
            Some("https://fail.example.test"),
            Some("withdraw-key-2"),
            "application/json; charset=utf-8",
            r#"{"reason":"duplicate upload"}"#,
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "ATTACHMENT_LINK_LOCKED");
}

#[tokio::test]
async fn router_rejects_unsafe_or_incomplete_withdraw_requests_before_the_backend() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = attachment_read_router(vec![target(
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

    let path = "/api/v1/holdings/acme/attachment-links/link-1/withdraw";
    for (origin, key, content_type, body, expected) in [
        (
            None,
            Some("withdraw-key-1"),
            "application/json",
            r#"{"reason":"duplicate upload"}"#,
            403,
        ),
        (
            Some("https://other.example.test"),
            Some("withdraw-key-1"),
            "application/json",
            r#"{"reason":"duplicate upload"}"#,
            403,
        ),
        (
            Some("https://acme.example.test"),
            None,
            "application/json",
            r#"{"reason":"duplicate upload"}"#,
            422,
        ),
        (
            Some("https://acme.example.test"),
            Some("short"),
            "application/json",
            r#"{"reason":"duplicate upload"}"#,
            422,
        ),
        (
            Some("https://acme.example.test"),
            Some("withdraw-key-1"),
            "text/plain",
            r#"{"reason":"duplicate upload"}"#,
            422,
        ),
        (
            Some("https://acme.example.test"),
            Some("withdraw-key-1"),
            "application/json",
            r#"{"reason":"ok","extra":true}"#,
            422,
        ),
        (
            Some("https://acme.example.test"),
            Some("withdraw-key-1"),
            "application/json",
            r#"{"reason":4}"#,
            422,
        ),
        (
            Some("https://acme.example.test"),
            Some("withdraw-key-1"),
            "application/json",
            "",
            422,
        ),
    ] {
        let (status, body, _) = http(
            addr,
            post(path, "acme.example.test", origin, key, content_type, body),
        )
        .await;
        assert_eq!(status, expected, "{origin:?} {key:?} {content_type} {body}");
        if expected == 403 {
            assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");
        }
    }

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/attachment-links/link-1/withdraw?reason=x",
            "acme.example.test",
            Some("https://acme.example.test"),
            Some("withdraw-key-1"),
            "application/json",
            r#"{"reason":"duplicate upload"}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/other/attachment-links/link-1/withdraw",
            "acme.example.test",
            Some("https://acme.example.test"),
            Some("withdraw-key-1"),
            "application/json",
            r#"{"reason":"duplicate upload"}"#,
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, _, _) = http(
        addr,
        post(
            path,
            "other.example.test",
            Some("https://acme.example.test"),
            Some("withdraw-key-1"),
            "application/json",
            r#"{"reason":"duplicate upload"}"#,
        ),
    )
    .await;
    assert_eq!(status, 404);

    assert!(seen.lock().unwrap().is_empty());
}

struct PortalProbe {
    seen: Arc<Mutex<Vec<String>>>,
}

impl PortalAttachmentHttpBackend for PortalProbe {
    fn withdraw<'a>(
        &'a self,
        request: PortalAttachmentHttpWithdrawRequest<'a>,
    ) -> PortalAttachmentHttpFuture<'a, PortalAttachmentHttpReply> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "portal|{}|{}|{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.link_id,
                request.reason,
                request.command_key,
                request
                    .headers
                    .get("cookie")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("")
            ));
            Ok(PortalAttachmentHttpReply {
                data: serde_json::json!({
                    "linkId": request.link_id,
                    "linkState": "withdrawn",
                    "withdrawnAt": 30
                }),
            })
        })
    }
}

fn portal_target(
    host: &str,
    slug: &str,
    seen: Arc<Mutex<Vec<String>>>,
) -> PortalAttachmentHttpTarget {
    PortalAttachmentHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: format!("https://{host}"),
        backend: Arc::new(PortalProbe { seen }),
    }
}

#[test]
fn portal_router_requires_distinct_exact_holdings() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let target = || portal_target("acme.example.test", "acme", seen.clone());
    assert!(portal_attachment_http_router(vec![target()]).is_ok());
    assert!(portal_attachment_http_router(vec![target(), target()]).is_err());
    assert!(
        portal_attachment_http_router(vec![PortalAttachmentHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        portal_attachment_http_router(vec![PortalAttachmentHttpTarget {
            holding_slug: "Acme".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        portal_attachment_http_router(vec![PortalAttachmentHttpTarget {
            host: String::new(),
            ..target()
        }])
        .is_err()
    );
}

#[tokio::test]
async fn portal_router_forwards_only_the_exact_origin_key_and_bounded_reason() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = portal_attachment_http_router(vec![portal_target(
        "acme.example.test",
        "acme",
        seen.clone(),
    )])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    let path = "/api/customer-portal/acme/attachments/attachment-links/link-1/withdraw";
    let body = r#"{"reason":"Replace the incorrect photo"}"#;
    for (origin, key, content_type) in [
        (None, Some("withdraw-001"), "application/json"),
        (
            Some("https://foreign.example.test"),
            Some("withdraw-001"),
            "application/json",
        ),
        (Some("https://acme.example.test"), Some("short"), "application/json"),
        (
            Some("https://acme.example.test"),
            Some("withdraw-001"),
            "text/plain",
        ),
    ] {
        let (status, _, _) = http(addr, post(path, "acme.example.test", origin, key, content_type, body)).await;
        assert_eq!(status, if origin.is_none() || origin == Some("https://foreign.example.test") { 403 } else { 422 });
    }
    let (status, _, _) = http(
        addr,
        post(path, "other.example.test", Some("https://other.example.test"), Some("withdraw-001"), "application/json", body),
    )
    .await;
    assert_eq!(status, 404);
    let (status, _, _) = http(
        addr,
        post(path, "acme.example.test", Some("https://acme.example.test"), Some("withdraw-001"), "application/json", "{}"),
    )
    .await;
    assert_eq!(status, 422);
    assert!(seen.lock().unwrap().is_empty());
    let (status, body, headers) = http(
        addr,
        post(
            path,
            "acme.example.test",
            Some("https://acme.example.test"),
            Some("withdraw-001"),
            "application/json; charset=utf-8",
            body,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["linkId"], "link-1");
    assert_eq!(body["data"]["linkState"], "withdrawn");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            "portal|acme.example.test|acme|link-1|Replace the incorrect photo|withdraw-001|erpx=session"
        ]
    );
}
