//! Owner-local HTTP adapter tests for the native Ecommerce catalog and
//! channel/review commands. The exact owner dispatch is exercised without
//! opening a socket; mounting the router into composition remains an
//! integration step.
use erpx_server_ecommerce::public::ecommerce_http::{
    EcommerceCatalogHttpRequest, EcommerceChannelHttpRequest, EcommerceHttpBackend,
    EcommerceHttpFuture, EcommerceHttpResponse, EcommerceHttpState, EcommerceHttpTarget,
    EcommerceReviewHttpRequest, ecommerce_router,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, StatusCode, body::to_bytes, header, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: Option<BusinessError>,
    bad: bool,
}

impl EcommerceHttpBackend for Probe {
    fn catalog<'a>(
        &'a self,
        request: EcommerceCatalogHttpRequest<'a>,
    ) -> EcommerceHttpFuture<'a, EcommerceHttpResponse> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap()
                .push(format!("catalog|{}|{}", request.host, request.holding_slug));
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(EcommerceHttpResponse {
                data: if self.bad {
                    json!({"channels":[],"listings":{}})
                } else {
                    json!({
                        "channels":[{
                            "id":"channel-1","company_id":"company","name":"Storefront",
                            "channel_type":"web","configuration_json":"{}","status":"active",
                            "created_by":"creator","reviewed_by":null,"version":2,
                            "created_at":1,"updated_at":2
                        }],
                        "listings":[]
                    })
                },
                context: json!({"userId":"user"}),
            })
        })
    }

    fn create_channel<'a>(
        &'a self,
        request: EcommerceChannelHttpRequest<'a>,
    ) -> EcommerceHttpFuture<'a, EcommerceHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "channel|{}|{}|{}|{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.key,
                request.name,
                request.channel_type,
                request.configuration,
                request
                    .headers
                    .get("cookie")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
            ));
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(EcommerceHttpResponse {
                data: if self.bad {
                    json!({"id":"channel-1","status":"draft"})
                } else {
                    json!({"id":"channel-1","status":"draft","version":1})
                },
                context: json!({"userId":"user"}),
            })
        })
    }

    fn review<'a>(
        &'a self,
        request: EcommerceReviewHttpRequest<'a>,
    ) -> EcommerceHttpFuture<'a, EcommerceHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "review|{}|{}|{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.key,
                request.id,
                request.expected_version,
                request.reason
            ));
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(EcommerceHttpResponse {
                data: if self.bad {
                    json!({"id":"channel-1","status":"active"})
                } else {
                    json!({"id":"channel-1","status":"active","version":4})
                },
                context: json!({"userId":"user"}),
            })
        })
    }
}

fn target(backend: Arc<dyn EcommerceHttpBackend>) -> EcommerceHttpTarget {
    EcommerceHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

fn headers(overrides: &[(&'static str, &'static str)]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("host", "acme.example.test"),
        ("content-type", "application/json"),
        ("idempotency-key", "ecommerce-command-0001"),
        ("origin", "https://acme.example.test"),
        ("cookie", "__Host-erpx-product=session"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    for (name, value) in overrides {
        headers.insert(*name, HeaderValue::from_static(value));
    }
    headers
}

fn channel_body() -> Vec<u8> {
    br#"{"name":"  Storefront  ","channelType":"web","configuration":{"storefront":"https://shop.example"}}"#.to_vec()
}

fn review_body() -> Vec<u8> {
    br#"{"kind":"channel","id":"channel-1","action":"activate","expectedVersion":3,"reason":"Independent review of current terms"}"#.to_vec()
}

async fn reply(
    response: erpx_server_runtime::public::http_support::Response,
) -> (StatusCode, serde_json::Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[test]
fn catalog_forwards_the_exact_target_and_returns_private_json() {
    sqlx::test_block_on(async {
        let probe = Arc::new(Probe::default());
        let state = EcommerceHttpState::new(vec![target(probe.clone())]).unwrap();
        let response = state.catalog(&headers(&[]), "acme", None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        assert_eq!(
            response.headers().get(header::REFERRER_POLICY).unwrap(),
            "no-referrer"
        );
        assert_eq!(
            response
                .headers()
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .unwrap(),
            "nosniff"
        );
        let (_, value) = reply(response).await;
        assert_eq!(value["data"]["channels"][0]["id"], "channel-1");
        assert_eq!(value["context"], json!({"userId":"user"}));
        assert_eq!(
            probe.seen.lock().unwrap().as_slice(),
            ["catalog|acme.example.test|acme"]
        );
    });
}

#[test]
fn catalog_rejects_unknown_targets_origin_queries_and_malformed_backend_data() {
    sqlx::test_block_on(async {
        let state = EcommerceHttpState::new(vec![target(Arc::new(Probe::default()))]).unwrap();
        for (headers, expected) in [
            (
                headers(&[("host", "evil.example.test")]),
                StatusCode::NOT_FOUND,
            ),
            (
                headers(&[("origin", "https://evil.example.test")]),
                StatusCode::FORBIDDEN,
            ),
        ] {
            let response = state.catalog(&headers, "acme", None).await;
            assert_eq!(response.status(), expected);
        }
        let response = state.catalog(&headers(&[]), "other", None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let response = state
            .catalog(&headers(&[]), "acme", Some("id=channel-1"))
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let bad = Arc::new(Probe {
            bad: true,
            ..Probe::default()
        });
        let state = EcommerceHttpState::new(vec![target(bad)]).unwrap();
        let response = state.catalog(&headers(&[]), "acme", None).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    });
}

#[test]
fn channel_command_forwards_trimmed_exact_input_and_rejects_malformed_variants() {
    sqlx::test_block_on(async {
        let probe = Arc::new(Probe::default());
        let state = EcommerceHttpState::new(vec![target(probe.clone())]).unwrap();
        let response = state
            .create_channel(&headers(&[]), "acme", None, &channel_body())
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let (_, value) = reply(response).await;
        assert_eq!(
            value["data"],
            json!({"id":"channel-1","status":"draft","version":1})
        );
        assert_eq!(
            probe.seen.lock().unwrap().as_slice(),
            [
                "channel|acme.example.test|acme|ecommerce-command-0001|Storefront|web|{\"storefront\":\"https://shop.example\"}|__Host-erpx-product=session"
            ]
        );

        let state = EcommerceHttpState::new(vec![target(Arc::new(Probe::default()))]).unwrap();
        for (headers, body) in [
            (headers(&[("content-type", "text/plain")]), channel_body()),
            (headers(&[("idempotency-key", "")]), channel_body()),
            (
                headers(&[]),
                br#"{"name":"Storefront","channelType":"web","configuration":{},"extra":1}"#
                    .to_vec(),
            ),
            (
                headers(&[]),
                br#"{"name":"Storefront","channelType":"store","configuration":{}}"#.to_vec(),
            ),
            (
                headers(&[]),
                br#"{"name":"Storefront","channelType":"web","configuration":[]}"#.to_vec(),
            ),
        ] {
            let response = state.create_channel(&headers, "acme", None, &body).await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
        let duplicated = {
            let mut headers = headers(&[]);
            headers.append(
                "idempotency-key",
                HeaderValue::from_static("ecommerce-command-0002"),
            );
            headers
        };
        let response = state
            .create_channel(&duplicated, "acme", None, &channel_body())
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = state
            .create_channel(&headers(&[]), "acme", Some("x=1"), &channel_body())
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    });
}

#[test]
fn review_command_forwards_exact_transition_and_maps_owner_failures() {
    sqlx::test_block_on(async {
        let probe = Arc::new(Probe::default());
        let state = EcommerceHttpState::new(vec![target(probe.clone())]).unwrap();
        let response = state
            .review(&headers(&[]), "acme", None, &review_body())
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let (_, value) = reply(response).await;
        assert_eq!(
            value["data"],
            json!({"id":"channel-1","status":"active","version":4})
        );
        assert_eq!(
            probe.seen.lock().unwrap().as_slice(),
            [
                "review|acme.example.test|acme|ecommerce-command-0001|channel-1|3|Independent review of current terms"
            ]
        );

        let state = EcommerceHttpState::new(vec![target(Arc::new(Probe::default()))]).unwrap();
        for body in [
            br#"{"kind":"listing","id":"channel-1","action":"activate","expectedVersion":3,"reason":"Independent review"}"#.to_vec(),
            br#"{"kind":"channel","id":"channel-1","action":"publish","expectedVersion":3,"reason":"Independent review"}"#.to_vec(),
            br#"{"kind":"channel","id":"channel-1","action":"activate","expectedVersion":3,"reason":"no"}"#.to_vec(),
            br#"{"kind":"channel","id":"channel-1","action":"activate","expectedVersion":3,"reason":"Independent review","extra":1}"#.to_vec(),
        ] {
            let response = state.review(&headers(&[]), "acme", None, &body).await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }

        let conflict = Arc::new(Probe {
            fail: Some(BusinessError::new(
                FailureKind::Conflict,
                "ECOMMERCE_SOURCE_CHANGED",
                "Ecommerce record changed. Reload and try again.",
            )),
            ..Probe::default()
        });
        let state = EcommerceHttpState::new(vec![target(conflict)]).unwrap();
        let response = state
            .review(&headers(&[]), "acme", None, &review_body())
            .await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["error"]["code"], "ECOMMERCE_SOURCE_CHANGED");
    });
}

#[test]
fn bad_command_receipts_and_invalid_configuration_fail_closed() {
    sqlx::test_block_on(async {
        let bad = Arc::new(Probe {
            bad: true,
            ..Probe::default()
        });
        let state = EcommerceHttpState::new(vec![target(bad)]).unwrap();
        let response = state
            .create_channel(&headers(&[]), "acme", None, &channel_body())
            .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let response = state
            .review(&headers(&[]), "acme", None, &review_body())
            .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    });

    let backend = Arc::new(Probe::default());
    assert!(ecommerce_router(vec![target(backend.clone())]).is_ok());
    assert!(ecommerce_router(vec![target(backend.clone()), target(backend)]).is_err());
    for invalid in [
        EcommerceHttpTarget {
            host: "".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe::default()),
        },
        EcommerceHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "ACME".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe::default()),
        },
        EcommerceHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "http://acme.example.test".into(),
            backend: Arc::new(Probe::default()),
        },
        EcommerceHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test/path".into(),
            backend: Arc::new(Probe::default()),
        },
    ] {
        assert!(ecommerce_router(vec![invalid]).is_err());
    }
}

#[test]
fn channel_command_forwards_js_trimmed_name_at_utf16_boundary() {
    sqlx::test_block_on(async {
        let probe = Arc::new(Probe::default());
        let state = EcommerceHttpState::new(vec![target(probe.clone())]).unwrap();
        // ZWNBSP (U+FEFF) JSON escapes: JS trim removes them, so the backend
        // must receive the trimmed "Storefront", exactly as Zod would send it.
        let body =
            br#"{"name":"\uFEFFStorefront\uFEFF","channelType":"web","configuration":{}}"#.to_vec();
        let response = state
            .create_channel(&headers(&[]), "acme", None, &body)
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            probe.seen.lock().unwrap().as_slice(),
            [
                "channel|acme.example.test|acme|ecommerce-command-0001|Storefront|web|{}|__Host-erpx-product=session"
            ]
        );

        // 160 Thai scalars are 160 UTF-16 units (allowed, 480 UTF-8 bytes).
        let thai_name = "ก".repeat(160);
        let thai_body =
            format!("{{\"name\":\"{thai_name}\",\"channelType\":\"web\",\"configuration\":{{}}}}")
                .into_bytes();
        let probe = Arc::new(Probe::default());
        let state = EcommerceHttpState::new(vec![target(probe.clone())]).unwrap();
        let response = state
            .create_channel(&headers(&[]), "acme", None, &thai_body)
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            probe.seen.lock().unwrap().as_slice(),
            [format!(
                "channel|acme.example.test|acme|ecommerce-command-0001|{thai_name}|web|{{}}|__Host-erpx-product=session"
            )]
        );

        // 161 Thai scalars exceed the 160 UTF-16-unit Zod maximum.
        let oversized = format!(
            "{{\"name\":\"{}\",\"channelType\":\"web\",\"configuration\":{{}}}}",
            "ก".repeat(161)
        )
        .into_bytes();
        let response = state
            .create_channel(&headers(&[]), "acme", None, &oversized)
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    });
}
