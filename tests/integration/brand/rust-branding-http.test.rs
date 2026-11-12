//! Actual owner-local Branding HTTP router tests. These prove ingress facts
//! only; Account authority, Branding policy, snapshots, receipts, audit and
//! outbox remain separate composition responsibilities.
use erpx_server_brand::public::branding_http::{
    BrandingHttpBackend, BrandingHttpCommandRequest, BrandingHttpFuture, BrandingHttpReadRequest,
    BrandingHttpTarget, BrandingPresentationHttpRequest, BrandingPresentationReadKind,
    branding_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl BrandingHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: BrandingHttpReadRequest<'a>,
    ) -> BrandingHttpFuture<'a, serde_json::Value> {
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
        _: BrandingHttpCommandRequest<'a>,
    ) -> BrandingHttpFuture<'a, serde_json::Value> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
    fn presentation<'a>(
        &'a self,
        _: BrandingPresentationHttpRequest<'a>,
    ) -> BrandingHttpFuture<'a, serde_json::Value> {
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SeenCommand {
    host: String,
    holding: String,
    key: String,
    short_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SeenPresentation {
    host: String,
    holding: String,
    holding_name: String,
    kind: String,
    branch: Option<String>,
    mode: Option<String>,
}

struct Probe {
    reads: Arc<Mutex<Vec<SeenRead>>>,
    commands: Arc<Mutex<Vec<SeenCommand>>>,
    presentations: Arc<Mutex<Vec<SeenPresentation>>>,
    fail: Option<BusinessError>,
}

impl BrandingHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: BrandingHttpReadRequest<'a>,
    ) -> BrandingHttpFuture<'a, serde_json::Value> {
        Box::pin(async move {
            self.reads.lock().unwrap().push(SeenRead {
                host: request.host.into(),
                holding: request.holding_slug.into(),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(serde_json::json!({
                "companyId": "erpx-co",
                "holdingId": "h003",
                "storefront": {"shortName": "ERPX", "accentColor": "#3659c9"}
            }))
        })
    }
    fn command<'a>(
        &'a self,
        request: BrandingHttpCommandRequest<'a>,
    ) -> BrandingHttpFuture<'a, serde_json::Value> {
        Box::pin(async move {
            self.commands.lock().unwrap().push(SeenCommand {
                host: request.host.into(),
                holding: request.holding_slug.into(),
                key: request.key.into(),
                short_name: request
                    .body
                    .get("shortName")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(serde_json::json!({"shortName": "ERPX", "accentColor": "#3659c9"}))
        })
    }
    fn presentation<'a>(
        &'a self,
        request: BrandingPresentationHttpRequest<'a>,
    ) -> BrandingHttpFuture<'a, serde_json::Value> {
        Box::pin(async move {
            self.presentations
                .lock()
                .unwrap()
                .push(SeenPresentation {
                    host: request.host.into(),
                    holding: request.holding_slug.into(),
                    holding_name: request.holding_name.into(),
                    kind: request.kind.as_str().into(),
                    branch: request.branch_id.map(str::to_owned),
                    mode: request.requested_mode.map(str::to_owned),
                });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            let workspace = serde_json::json!({
                "scopeKind": "branch",
                "scopeId": "branch-1",
                "label": "North Branch",
                "initials": "NB",
                "primaryUrl": "/api/public/holdings/erpx/media-publications/pub-1/content",
                "inverseUrl": null,
                "documentMode": "both"
            });
            let storefront = serde_json::json!({
                "companyId": "erpx-co",
                "holdingSlug": "erpx",
                "companySlug": "erpx-co",
                "companyName": "ERPX Company",
                "shortName": "ERPX",
                "accentColor": "#3659c9",
                "logoUrl": null,
                "identityRevision": 4
            });
            Ok(match request.kind {
                BrandingPresentationReadKind::Workspace => workspace,
                BrandingPresentationReadKind::Document => serde_json::json!({
                    "mode": "both",
                    "companyName": "ERPX Company",
                    "branchName": "North Branch",
                    "holding": workspace,
                    "branch": workspace
                }),
                BrandingPresentationReadKind::Profiles => serde_json::json!({
                    "holding": workspace,
                    "branches": [workspace],
                    "storefront": storefront
                }),
                BrandingPresentationReadKind::Storefront => storefront,
                BrandingPresentationReadKind::Evidence => serde_json::json!({
                    "id": "branch-1",
                    "branch_name": "North Branch",
                    "document_mode": "both",
                    "updated_at": 1_700_000_000_000_i64,
                    "primary_attachment_id": null,
                    "inverse_attachment_id": "attachment-2"
                }),
            })
        })
    }
}

fn target(backend: Arc<dyn BrandingHttpBackend>) -> BrandingHttpTarget {
    BrandingHttpTarget {
        host: "erpx-internal.example.test".into(),
        holding_slug: "erpx".into(),
        origin: "https://erpx-internal.example.test".into(),
        backend,
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend: Arc<dyn BrandingHttpBackend> = Arc::new(Noop);
    assert!(branding_http_router(vec![target(backend.clone())]).is_ok());
    assert!(
        branding_http_router(vec![target(backend.clone()), target(backend.clone())]).is_err()
    );
    for invalid in [
        BrandingHttpTarget {
            origin: "http://erpx-internal.example.test".into(),
            ..target(backend.clone())
        },
        BrandingHttpTarget {
            origin: "https://erpx-internal.example.test/path".into(),
            ..target(backend.clone())
        },
        BrandingHttpTarget {
            holding_slug: "ERPX".into(),
            ..target(backend.clone())
        },
        BrandingHttpTarget {
            host: String::new(),
            ..target(backend.clone())
        },
    ] {
        assert!(branding_http_router(vec![invalid]).is_err());
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
fn get_named(path: &str, host: &str, holding_name: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nX-Erpx-Holding-Name: {holding_name}\r\nConnection: close\r\n\r\n"
    )
}
fn post(path: &str, host: &str, origin: &str, key: Option<&str>, body: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nContent-Type: application/json\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        key.map(|key| format!("Idempotency-Key: {key}\r\n")).unwrap_or_default(),
        body.len()
    )
}

#[tokio::test]
async fn actual_router_forwards_exact_targets_and_maps_business_errors() {
    let reads = Arc::new(Mutex::new(Vec::new()));
    let commands = Arc::new(Mutex::new(Vec::new()));
    let presentations = Arc::new(Mutex::new(Vec::new()));
    let router = branding_http_router(vec![
        target(Arc::new(Probe {
            reads: reads.clone(),
            commands: commands.clone(),
            presentations: presentations.clone(),
            fail: None,
        })),
        BrandingHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe {
                reads: reads.clone(),
                commands: commands.clone(),
                presentations: presentations.clone(),
                fail: Some(BusinessError::new(
                    FailureKind::Conflict,
                    "CONFLICT",
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

    let path = "/api/v1/holdings/erpx/branding";
    let (status, body, raw) = http(
        addr,
        get(path, "erpx-internal.example.test"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["storefront"]["shortName"], "ERPX");
    assert!(raw.contains("cache-control: private, no-store"));
    assert!(raw.contains("referrer-policy: no-referrer"));
    assert!(raw.contains("x-content-type-options: nosniff"));
    assert_eq!(
        reads.lock().unwrap().last().cloned(),
        Some(SeenRead {
            host: "erpx-internal.example.test".into(),
            holding: "erpx".into(),
        })
    );

    let (status, body, _) = http(
        addr,
        get(path, "other.example.test"),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let presentation = "/api/v1/holdings/erpx/branding/presentation";
    for (query, kind, branch, mode) in [
        ("kind=workspace", "workspace", None, None),
        ("kind=document&branch=branch-1&mode=both", "document", Some("branch-1"), Some("both")),
        ("kind=profiles", "profiles", None, None),
        ("kind=storefront", "storefront", None, None),
        ("kind=evidence&branch=branch-1", "evidence", Some("branch-1"), None),
    ] {
        let (status, body, raw) = http(
            addr,
            get_named(
                &format!("{presentation}?{query}"),
                "erpx-internal.example.test",
                "ERPX",
            ),
        )
        .await;
        assert_eq!(status, 200, "{kind}");
        assert!(raw.contains("cache-control: private, no-store"), "{kind}");
        assert!(body["data"].is_object() || body["data"].is_null(), "{kind}");
        assert_eq!(
            presentations.lock().unwrap().last().cloned(),
            Some(SeenPresentation {
                host: "erpx-internal.example.test".into(),
                holding: "erpx".into(),
                holding_name: "ERPX".into(),
                kind: kind.into(),
                branch: branch.map(str::to_owned),
                mode: mode.map(str::to_owned),
            })
        );
    }
    let (status, body, _) = http(
        addr,
        get_named(
            &format!("{presentation}?kind=workspace"),
            "erpx-internal.example.test",
            "ERPX",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["initials"], "NB");
    assert_eq!(
        body["data"]["primaryUrl"],
        "/api/public/holdings/erpx/media-publications/pub-1/content"
    );
    let (status, body, _) = http(
        addr,
        get(&format!("{presentation}?kind=workspace"), "erpx-internal.example.test"),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "BRANDING_REQUEST_INVALID");
    let overlong = "b".repeat(201);
    for invalid in [
        "kind=unknown".to_owned(),
        "kind=".to_owned(),
        String::new(),
        "kind=evidence".to_owned(),
        "kind=workspace&branch=branch-1".to_owned(),
        format!("kind=document&branch={overlong}"),
    ] {
        let (status, body, _) = http(
            addr,
            get_named(
                &format!("{presentation}?{invalid}"),
                "erpx-internal.example.test",
                "ERPX",
            ),
        )
        .await;
        assert_eq!(status, 422, "{invalid}");
        assert_eq!(body["error"]["code"], "BRANDING_REQUEST_INVALID");
    }
    let (status, body, _) = http(
        addr,
        get_named(&format!("{presentation}?kind=workspace"), "other.example.test", "ERPX"),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
    let (status, body, _) = http(
        addr,
        get_named(
            "/api/v1/holdings/fail/branding/presentation?kind=workspace",
            "fail.example.test",
            "Fail Holding",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "CONFLICT");

    let (status, body, _) = http(
        addr,
        get("/api/v1/holdings/fail/branding", "fail.example.test"),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "CONFLICT");

    let payload = r##"{"shortName":"ERPX","accentColor":"#3659c9","reason":"Bounded branding change"}"##;
    let (status, body, _) = http(
        addr,
        post(path, "erpx-internal.example.test", "https://erpx-internal.example.test", None, payload),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, body, _) = http(
        addr,
        post(
            path,
            "erpx-internal.example.test",
            "https://other.example.test",
            Some("branding-key-1"),
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
            "erpx-internal.example.test",
            "https://erpx-internal.example.test",
            Some("branding-key-1"),
            payload,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["shortName"], "ERPX");
    assert_eq!(
        commands.lock().unwrap().last().cloned(),
        Some(SeenCommand {
            host: "erpx-internal.example.test".into(),
            holding: "erpx".into(),
            key: "branding-key-1".into(),
            short_name: Some("ERPX".into()),
        })
    );

    for invalid in [
        r#"[]"#,
        r#"null"#,
        r#""string""#,
        r#"not-json"#,
    ] {
        let (status, _, _) = http(
            addr,
            post(
                path,
                "erpx-internal.example.test",
                "https://erpx-internal.example.test",
                Some("branding-key-2"),
                invalid,
            ),
        )
        .await;
        assert_eq!(status, 422, "body: {invalid}");
    }
}
