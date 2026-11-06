//! Actual owner-local Marketing campaign/attribution HTTP router tests.
//! These prove ingress facts only; composition, Account session and PostgreSQL
//! remain separate responsibilities.
use erpx_server_marketing::public::campaign_attribution_http::{
    MarketingCampaignHttpBackend, MarketingCampaignHttpFuture, MarketingCampaignHttpReadRequest,
    MarketingCampaignHttpResponse, MarketingCampaignHttpTarget, marketing_campaign_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl MarketingCampaignHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: MarketingCampaignHttpReadRequest<'a>,
    ) -> MarketingCampaignHttpFuture<'a, MarketingCampaignHttpResponse> {
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
}

struct Probe {
    seen: Arc<Mutex<Vec<Seen>>>,
    fail: Option<BusinessError>,
}

impl MarketingCampaignHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: MarketingCampaignHttpReadRequest<'a>,
    ) -> MarketingCampaignHttpFuture<'a, MarketingCampaignHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(Seen {
                host: request.host.into(),
                holding: request.holding_slug.into(),
                cookie: request
                    .headers
                    .get("cookie")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned),
            });
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(MarketingCampaignHttpResponse {
                data: serde_json::json!({
                    "campaigns": [{
                        "id": "campaign-1",
                        "company_id": "company",
                        "code": "AUTUMN",
                        "name": "Autumn demand campaign",
                        "version": 1,
                        "created_at": 100
                    }],
                    "attribution": [{
                        "campaign": "AUTUMN",
                        "source": "web",
                        "captured": 3,
                        "pending": 1,
                        "handed_off": 1,
                        "rejected": 1
                    }],
                    "purchase": null
                }),
                context: serde_json::json!({
                    "userId": "user",
                    "companyId": "company",
                    "permissions": ["records.read"]
                }),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || MarketingCampaignHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(marketing_campaign_http_router(vec![target()]).is_ok());
    assert!(marketing_campaign_http_router(vec![target(), target()]).is_err());
    assert!(
        marketing_campaign_http_router(vec![MarketingCampaignHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        marketing_campaign_http_router(vec![MarketingCampaignHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        marketing_campaign_http_router(vec![MarketingCampaignHttpTarget {
            holding_slug: "Acme".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        marketing_campaign_http_router(vec![MarketingCampaignHttpTarget {
            host: String::new(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        marketing_campaign_http_router(vec![MarketingCampaignHttpTarget {
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

fn get_with_body(path: &str, host: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nContent-Length: 1\r\nConnection: close\r\n\r\nx"
    )
}

#[tokio::test]
async fn actual_router_forwards_exact_target_and_maps_business_errors() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = marketing_campaign_http_router(vec![
        MarketingCampaignHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: None,
            }),
        },
        MarketingCampaignHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: Some(BusinessError::new(
                    FailureKind::Conflict,
                    "MARKETING_CAMPAIGN_SOURCE_CHANGED",
                    "Synthetic changed campaign source.",
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
        "GET /api/v1/holdings/acme/marketing/campaign-attribution HTTP/1.1\r\nHost: acme.example.test\r\nCookie: __Host-erpx-product=session\r\nConnection: close\r\n\r\n".to_string(),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["campaigns"][0]["code"], "AUTUMN");
    assert_eq!(body["data"]["attribution"][0]["captured"], 3);
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
        })
    );

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/fail/marketing/campaign-attribution",
            "fail.example.test",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "MARKETING_CAMPAIGN_SOURCE_CHANGED");

    for path in [
        "/api/v1/holdings/acme/marketing/campaign-attribution?q=autumn",
        "/api/v1/holdings/acme/marketing/campaign-attribution?",
        "/api/v1/holdings/acme/marketing/campaign-attribution?cursor=opaque",
    ] {
        let (status, body, _) = http(addr, get(path, "acme.example.test")).await;
        assert_eq!(status, 422, "path: {path}");
        assert_eq!(body["error"]["code"], "MARKETING_CAMPAIGN_REQUEST_INVALID");
    }

    let (status, body, _) = http(
        addr,
        get_with_body(
            "/api/v1/holdings/acme/marketing/campaign-attribution",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "MARKETING_CAMPAIGN_REQUEST_INVALID");

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/marketing/campaign-attribution",
            "other.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/other/marketing/campaign-attribution",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body, _) = http(
        addr,
        "GET /api/v1/holdings/acme/marketing/campaign-attribution HTTP/1.1\r\nHost: acme.example.test\r\nHost: other.example.test\r\nConnection: close\r\n\r\n".to_string(),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "MARKETING_CAMPAIGN_REQUEST_INVALID");

    assert_eq!(
        seen.lock().unwrap().len(),
        2,
        "query, body, unknown-target and duplicate-Host requests must not reach the backend"
    );

    server.abort();
}

mod lead_http {
    use super::{get, get_with_body, http};
    use erpx_server_marketing::public::lead_candidate_http::{
        MarketingLeadHttpBackend, MarketingLeadHttpFuture, MarketingLeadHttpReadRequest,
        MarketingLeadHttpResponse, MarketingLeadHttpTarget, marketing_lead_http_router,
    };
    use erpx_server_runtime::public::http_support::serve;
    use erpx_server_shared::public::{BusinessError, FailureKind};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Seen {
        host: String,
        holding: String,
        candidate: Option<String>,
    }

    struct Probe {
        seen: Arc<Mutex<Vec<Seen>>>,
        fail: Option<BusinessError>,
    }

    impl MarketingLeadHttpBackend for Probe {
        fn read<'a>(
            &'a self,
            request: MarketingLeadHttpReadRequest<'a>,
        ) -> MarketingLeadHttpFuture<'a, MarketingLeadHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(Seen {
                    host: request.host.into(),
                    holding: request.holding_slug.into(),
                    candidate: request.candidate_id.map(str::to_owned),
                });
                if let Some(error) = self.fail.clone() {
                    return Err(error);
                }
                Ok(MarketingLeadHttpResponse {
                    data: if request.candidate_id.is_some() {
                        serde_json::json!({
                            "candidate": {
                                "id": "candidate-1",
                                "company_id": "company",
                                "branch_id": "branch",
                                "department_id": null,
                                "form_id": "form-company",
                                "form_version": 1,
                                "form_name": "Lead form",
                                "contact_name": "Synthetic one",
                                "contact_detail": "Synthetic channel",
                                "consent_evidence": "Synthetic consent",
                                "attribution_json": "{\"campaign\":\"AUTUMN\",\"source\":\"web\"}",
                                "brief": "Synthetic brief",
                                "status": "pending",
                                "crm_lead_id": null,
                                "created_by": "owner",
                                "reviewed_by": null,
                                "version": 1,
                                "created_at": 100,
                                "updated_at": 100
                            },
                            "history": []
                        })
                    } else {
                        serde_json::json!({"assets": []})
                    },
                    context: serde_json::json!({
                        "userId": "user",
                        "companyId": "company",
                        "permissions": ["records.read"]
                    }),
                })
            })
        }
    }

    fn target(backend: Arc<dyn MarketingLeadHttpBackend>) -> MarketingLeadHttpTarget {
        MarketingLeadHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }
    }

    #[test]
    fn configuration_requires_distinct_exact_targets() {
        let backend: Arc<dyn MarketingLeadHttpBackend> = Arc::new(Probe {
            seen: Arc::new(Mutex::new(Vec::new())),
            fail: None,
        });
        assert!(marketing_lead_http_router(vec![target(backend.clone())]).is_ok());
        assert!(
            marketing_lead_http_router(vec![target(backend.clone()), target(backend.clone())])
                .is_err()
        );
        assert!(
            marketing_lead_http_router(vec![MarketingLeadHttpTarget {
                origin: "http://acme.example.test".into(),
                ..target(backend.clone())
            }])
            .is_err()
        );
        assert!(
            marketing_lead_http_router(vec![MarketingLeadHttpTarget {
                holding_slug: "Acme".into(),
                ..target(backend)
            }])
            .is_err()
        );
    }

    #[tokio::test]
    async fn router_forwards_exact_candidate_and_rejects_query_body_and_unknown_targets() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let backend = Arc::new(Probe {
            seen: seen.clone(),
            fail: None,
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = marketing_lead_http_router(vec![target(backend)]).unwrap();
        let server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        let (status, body, text) = http(
            addr,
            get(
                "/api/v1/holdings/acme/marketing/leads/candidate-1",
                "acme.example.test",
            ),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["candidate"]["id"], "candidate-1");
        assert_eq!(body["context"]["companyId"], "company");
        assert!(text.contains("private, no-store"));
        assert!(text.contains("no-referrer"));
        assert!(text.contains("nosniff"));
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &[Seen {
                host: "acme.example.test".into(),
                holding: "acme".into(),
                candidate: Some("candidate-1".into())
            }]
        );

        let (status, body, _) = http(
            addr,
            get(
                "/api/v1/holdings/acme/marketing/assets",
                "acme.example.test",
            ),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["assets"], serde_json::json!([]));
        assert_eq!(seen.lock().unwrap().last().unwrap().candidate, None);

        let (status, body, _) = http(
            addr,
            get(
                "/api/v1/holdings/acme/marketing/leads/candidate-1?query=1",
                "acme.example.test",
            ),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "MARKETING_LEAD_REQUEST_INVALID");

        let (status, body, _) = http(
            addr,
            get_with_body(
                "/api/v1/holdings/acme/marketing/leads/candidate-1",
                "acme.example.test",
            ),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "MARKETING_LEAD_REQUEST_INVALID");

        let (status, body, _) = http(
            addr,
            get(
                "/api/v1/holdings/acme/marketing/leads/candidate-1",
                "other.example.test",
            ),
        )
        .await;
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");

        assert_eq!(
            seen.lock().unwrap().len(),
            2,
            "rejected transport requests must not reach the backend"
        );
        server.abort();
    }

    #[tokio::test]
    async fn backend_denial_maps_to_the_exact_status() {
        let backend = Arc::new(Probe {
            seen: Arc::new(Mutex::new(Vec::new())),
            fail: Some(BusinessError::new(
                FailureKind::Conflict,
                "MARKETING_CANDIDATE_CHANGED",
                "The lead candidate changed.",
            )),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = marketing_lead_http_router(vec![target(backend)]).unwrap();
        let server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        let (status, body, _) = http(
            addr,
            get(
                "/api/v1/holdings/acme/marketing/leads/candidate-1",
                "acme.example.test",
            ),
        )
        .await;
        assert_eq!(status, 409);
        assert_eq!(body["error"]["code"], "MARKETING_CANDIDATE_CHANGED");
        server.abort();
    }
}
