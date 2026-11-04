//! Actual owner-local CRM pipeline HTTP router tests.
//! These prove ingress facts only; composition, Account session and PostgreSQL
//! remain separate responsibilities.
use erpx_server_crm::public::pipeline_http::{
    CrmPipelineHttpBackend, CrmPipelineHttpFuture, CrmPipelineHttpReadRequest,
    CrmPipelineHttpResponse, CrmPipelineHttpTarget, crm_pipeline_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl CrmPipelineHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: CrmPipelineHttpReadRequest<'a>,
    ) -> CrmPipelineHttpFuture<'a, CrmPipelineHttpResponse> {
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
    search: String,
    stage: Option<String>,
    cursor: Option<String>,
    host: String,
    holding: String,
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
    fail: Option<BusinessError>,
}

impl CrmPipelineHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: CrmPipelineHttpReadRequest<'a>,
    ) -> CrmPipelineHttpFuture<'a, CrmPipelineHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                search: request.query.search,
                stage: request.query.stage,
                cursor: request.query.cursor,
                host: request.host.into(),
                holding: request.holding_slug.into(),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(CrmPipelineHttpResponse {
                data: serde_json::json!({
                    "items": [],
                    "forecast": {},
                    "canCreate": true,
                    "purchase": null,
                    "nextCursor": null,
                    "hasMore": false
                }),
                context: serde_json::json!({
                    "userId": "user",
                    "companyId": "company",
                    "permissions": ["commerce.read"]
                }),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || CrmPipelineHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(crm_pipeline_http_router(vec![target()]).is_ok());
    assert!(crm_pipeline_http_router(vec![target(), target()]).is_err());
    assert!(
        crm_pipeline_http_router(vec![CrmPipelineHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        crm_pipeline_http_router(vec![CrmPipelineHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        crm_pipeline_http_router(vec![CrmPipelineHttpTarget {
            holding_slug: "Acme".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        crm_pipeline_http_router(vec![CrmPipelineHttpTarget {
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

#[tokio::test]
async fn actual_router_forwards_exact_query_and_maps_business_errors() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = crm_pipeline_http_router(vec![
        CrmPipelineHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: None,
            }),
        },
        CrmPipelineHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: Some(BusinessError::new(
                    FailureKind::Conflict,
                    "CRM_PIPELINE_CHANGED",
                    "Synthetic changed pipeline source.",
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
        get(
            "/api/v1/holdings/acme/crm/leads?q=Acme%20corp&stage=discovery&cursor=a%2Bb%3Dc",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["canCreate"], true);
    assert_eq!(body["context"]["companyId"], "company");
    assert!(raw.contains("cache-control: private, no-store"));
    assert!(raw.contains("referrer-policy: no-referrer"));
    assert!(raw.contains("x-content-type-options: nosniff"));
    assert_eq!(
        seen.lock().unwrap().last().cloned(),
        Some(Seen {
            search: "Acme corp".into(),
            stage: Some("discovery".into()),
            cursor: Some("a+b=c".into()),
            host: "acme.example.test".into(),
            holding: "acme".into(),
        })
    );

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/fail/crm/leads?q=Acme",
            "fail.example.test",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "CRM_PIPELINE_CHANGED");

    for path in [
        "/api/v1/holdings/acme/crm/leads?q=one&q=two",
        "/api/v1/holdings/acme/crm/leads?term=one",
        "/api/v1/holdings/acme/crm/leads?stage=paid",
        "/api/v1/holdings/acme/crm/leads?cursor=",
        "/api/v1/holdings/acme/crm/leads?q=control%07",
    ] {
        let (status, body, _) = http(addr, get(path, "acme.example.test")).await;
        assert_eq!(status, 422, "path: {path}");
        assert!(
            [
                "CRM_PIPELINE_REQUEST_INVALID",
                "CRM_PIPELINE_STAGE_INVALID",
                "CRM_PIPELINE_CURSOR_INVALID"
            ]
            .contains(&body["error"]["code"].as_str().unwrap_or("")),
            "path: {path} code: {}",
            body["error"]["code"]
        );
    }

    let (status, body, _) = http(
        addr,
        get("/api/v1/holdings/acme/crm/leads", "other.example.test"),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body, _) = http(
        addr,
        get("/api/v1/holdings/other/crm/leads", "acme.example.test"),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    server.abort();
}
