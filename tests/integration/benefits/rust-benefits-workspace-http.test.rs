//! Actual owner-local Benefits workspace HTTP router tests. These prove
//! ingress facts only; composition, Account session and PostgreSQL remain
//! separate responsibilities.
use erpx_server_benefits::public::workspace_http::{
    BenefitsWorkspaceHttpBackend, BenefitsWorkspaceHttpFuture, BenefitsWorkspaceHttpReadRequest,
    BenefitsWorkspaceHttpResponse, BenefitsWorkspaceHttpTarget, benefits_workspace_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl BenefitsWorkspaceHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: BenefitsWorkspaceHttpReadRequest<'a>,
    ) -> BenefitsWorkspaceHttpFuture<'a, BenefitsWorkspaceHttpResponse> {
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
    cookie: Option<String>,
    personal: bool,
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
    fail: Option<BusinessError>,
}

impl BenefitsWorkspaceHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: BenefitsWorkspaceHttpReadRequest<'a>,
    ) -> BenefitsWorkspaceHttpFuture<'a, BenefitsWorkspaceHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                host: request.host.into(),
                holding: request.holding_slug.into(),
                cookie: request
                    .headers
                    .get("cookie")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned),
                personal: request.personal,
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(BenefitsWorkspaceHttpResponse {
                data: serde_json::json!({
                    "actorId": "user",
                    "canWrite": true,
                    "canReview": false,
                    "canManageDocuments": false,
                    "plans": [{
                        "id": "plan-family",
                        "company_id": "company",
                        "name": "Family medical plan",
                        "provider": "Synthetic provider",
                        "coverage_type": "family",
                        "currency": "THB",
                        "employee_deduction_minor": 12550,
                        "employer_cost_minor": 30000,
                        "effective_from": "2026-01-01",
                        "effective_to": null,
                        "status": "published",
                        "version": 1,
                        "created_by": "owner",
                        "reviewed_by": null,
                        "created_at": 10,
                        "updated_at": 20,
                        "required_eligibility_code": null
                    }],
                    "windows": [],
                    "dependents": [],
                    "elections": [],
                    "purchase": null
                }),
                context: serde_json::json!({
                    "userId": "user",
                    "companyId": "company",
                    "workContext": "a".repeat(64)
                }),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || BenefitsWorkspaceHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(benefits_workspace_http_router(vec![target()]).is_ok());
    assert!(benefits_workspace_http_router(vec![target(), target()]).is_err());
    assert!(
        benefits_workspace_http_router(vec![BenefitsWorkspaceHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        benefits_workspace_http_router(vec![BenefitsWorkspaceHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        benefits_workspace_http_router(vec![BenefitsWorkspaceHttpTarget {
            holding_slug: "Acme".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        benefits_workspace_http_router(vec![BenefitsWorkspaceHttpTarget {
            host: String::new(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        benefits_workspace_http_router(vec![BenefitsWorkspaceHttpTarget {
            origin: "https://acme.example.test:8443".into(),
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

fn get_personal(path: &str, host: &str, value: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nx-erpx-benefits-personal: {value}\r\nConnection: close\r\n\r\n"
    )
}

fn get_with_body(path: &str, host: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nContent-Length: 1\r\nConnection: close\r\n\r\nx"
    )
}

#[tokio::test]
async fn actual_router_forwards_exact_target_scope_and_maps_business_errors() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = benefits_workspace_http_router(vec![
        BenefitsWorkspaceHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: None,
            }),
        },
        BenefitsWorkspaceHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: Some(BusinessError::new(
                    FailureKind::Conflict,
                    "WORK_CONTEXT_CHANGED",
                    "Synthetic changed work context.",
                )),
            }),
        },
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, raw) = http(
        addr,
        "GET /api/v1/holdings/acme/benefits HTTP/1.1\r\nHost: acme.example.test\r\nCookie: __Host-erpx-product=session\r\nConnection: close\r\n\r\n".to_string(),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["plans"][0]["id"], "plan-family");
    assert_eq!(body["data"]["plans"][0]["employee_deduction_minor"], 12_550);
    assert_eq!(body["context"]["companyId"], "company");
    assert!(raw.contains("cache-control: private, no-store"));
    assert!(raw.contains("referrer-policy: no-referrer"));
    assert!(raw.contains("x-content-type-options: nosniff"));
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            host: "acme.example.test".into(),
            holding: "acme".into(),
            cookie: Some("__Host-erpx-product=session".into()),
            personal: false,
        })
    );

    let (status, body, _) = http(
        addr,
        get_personal("/api/v1/holdings/acme/benefits", "acme.example.test", "1"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["actorId"], "user");
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            host: "acme.example.test".into(),
            holding: "acme".into(),
            cookie: None,
            personal: true,
        })
    );

    let (status, body, _) = http(
        addr,
        get("/api/v1/holdings/fail/benefits", "fail.example.test"),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "WORK_CONTEXT_CHANGED");

    for path in [
        "/api/v1/holdings/acme/benefits?view=mine",
        "/api/v1/holdings/acme/benefits?",
        "/api/v1/holdings/acme/benefits?cursor=opaque",
    ] {
        let (status, body, _) = http(addr, get(path, "acme.example.test")).await;
        assert_eq!(status, 422, "path: {path}");
        assert_eq!(body["error"]["code"], "BENEFITS_WORKSPACE_REQUEST_INVALID");
    }

    for value in ["0", "true", "mine", ""] {
        let (status, body, _) = http(
            addr,
            get_personal("/api/v1/holdings/acme/benefits", "acme.example.test", value),
        )
        .await;
        assert_eq!(status, 422, "personal value: {value}");
        assert_eq!(body["error"]["code"], "BENEFITS_WORKSPACE_REQUEST_INVALID");
    }

    let (status, body, _) = http(
        addr,
        "GET /api/v1/holdings/acme/benefits HTTP/1.1\r\nHost: acme.example.test\r\nx-erpx-benefits-personal: 1\r\nx-erpx-benefits-personal: 1\r\nConnection: close\r\n\r\n".to_string(),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "BENEFITS_WORKSPACE_REQUEST_INVALID");

    let (status, body, _) = http(
        addr,
        get_with_body("/api/v1/holdings/acme/benefits", "acme.example.test"),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "BENEFITS_WORKSPACE_REQUEST_INVALID");

    let (status, body, _) = http(
        addr,
        get("/api/v1/holdings/acme/benefits", "other.example.test"),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body, _) = http(
        addr,
        get("/api/v1/holdings/other/benefits", "acme.example.test"),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body, _) = http(
        addr,
        "GET /api/v1/holdings/acme/benefits HTTP/1.1\r\nHost: acme.example.test\r\nHost: other.example.test\r\nConnection: close\r\n\r\n".to_string(),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "BENEFITS_WORKSPACE_REQUEST_INVALID");

    assert_eq!(
        seen.lock().unwrap().len(),
        3,
        "query, personal, body, unknown-target and duplicate-Host requests must not reach the backend"
    );

    server.abort();
}
