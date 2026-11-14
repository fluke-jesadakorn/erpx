//! Owner-local HTTP adapter tests for the native POS workspace/day read.
//! The exact owner dispatch is exercised without opening a socket; mounting the
//! router into the shared composition backend remains an integration step.
use erpx_server_pos::public::read::POS_PERMISSION_NAMES;
use erpx_server_pos::public::read_http::{
    PosWorkspaceHttpBackend, PosWorkspaceHttpFuture, PosWorkspaceHttpRequest,
    PosWorkspaceHttpResponse, PosWorkspaceHttpState, PosWorkspaceHttpTarget, pos_workspace_router,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, StatusCode, body::to_bytes, header, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl PosWorkspaceHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: PosWorkspaceHttpRequest<'a>,
    ) -> PosWorkspaceHttpFuture<'a, PosWorkspaceHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
}

#[derive(Default)]
struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: Option<BusinessError>,
    broken: bool,
}

fn permissions() -> serde_json::Value {
    serde_json::json!(
        POS_PERMISSION_NAMES
            .iter()
            .map(|name| ((*name).to_owned(), serde_json::json!(name == &"pos.read")))
            .collect::<serde_json::Map<String, serde_json::Value>>()
    )
}

fn data() -> serde_json::Value {
    serde_json::json!({
        "businessDate": "2027-01-15",
        "registers": [],
        "settings": null,
        "shifts": [],
        "cashMovements": [],
        "cashSummary": {
            "pendingCount": 0,
            "pendingMinor": 0,
            "approvedCount": 0,
            "approvedMinor": 0
        },
        "day": {"businessDate": "2027-01-15", "state": "open", "close": null},
        "sales": [],
        "sale": null,
        "lines": [],
        "payments": [],
        "permissions": permissions(),
        "entitlements": {"packageEnforced": false, "roomChargeEnabled": true},
        "purchases": null,
        "userId": "cashier",
        "companyId": "company",
        "branchId": "branch"
    })
}

impl PosWorkspaceHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: PosWorkspaceHttpRequest<'a>,
    ) -> PosWorkspaceHttpFuture<'a, PosWorkspaceHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.query.tab.map_or("-", |tab| tab.as_str()),
                request.query.sale_id.as_deref().unwrap_or("-"),
                request.query.search,
                request.query.limit,
                request
                    .headers
                    .get("host")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
            ));
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            let mut data = data();
            if self.broken {
                data["purchases"] = serde_json::json!({});
                data.as_object_mut().unwrap().remove("entitlements");
            }
            Ok(PosWorkspaceHttpResponse {
                data,
                context: serde_json::json!({"userId": "cashier", "companyId": "company"}),
            })
        })
    }
}

fn target(backend: Arc<dyn PosWorkspaceHttpBackend>) -> PosWorkspaceHttpTarget {
    PosWorkspaceHttpTarget {
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
        ("origin", "https://acme.example.test"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    for (name, value) in overrides {
        headers.insert(*name, HeaderValue::from_static(value));
    }
    headers
}

async fn reply(
    state: &PosWorkspaceHttpState,
    headers: &HeaderMap,
    query: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let response = state.read(headers, "acme", query).await;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap();
    (status, value)
}

#[test]
fn target_configuration_requires_distinct_exact_https_holdings() {
    let backend = Arc::new(Noop);
    assert!(pos_workspace_router(vec![target(backend.clone())]).is_ok());
    assert!(
        pos_workspace_router(vec![PosWorkspaceHttpTarget {
            origin: "https://acme.example.test:3443".into(),
            ..target(backend.clone())
        }])
        .is_ok()
    );
    assert!(pos_workspace_router(vec![target(backend.clone()), target(backend.clone())]).is_err());
    for origin in [
        "http://acme.example.test",
        "https://acme.example.test:0",
        "https://acme.example.test:65536",
        "https://acme.example.test:abc",
        "https://acme.example.test:3443:1",
    ] {
        assert!(
            pos_workspace_router(vec![PosWorkspaceHttpTarget {
                origin: origin.into(),
                ..target(backend.clone())
            }])
            .is_err()
        );
    }
    for slug in ["", "Bad Slug", "ACME"] {
        assert!(
            pos_workspace_router(vec![PosWorkspaceHttpTarget {
                holding_slug: slug.into(),
                ..target(backend.clone())
            }])
            .is_err()
        );
    }
}

#[test]
fn forwards_the_exact_bounded_query_and_returns_private_json() {
    sqlx::test_block_on(async {
        let probe = Arc::new(Probe::default());
        let state = PosWorkspaceHttpState::new(vec![target(probe.clone())]).unwrap();
        let response = state
            .read(
                &headers(&[]),
                "acme",
                Some("tab=shifts&limit=2&q=Coffee+Shop&sale=sale-1"),
            )
            .await;
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
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["data"], data());
        assert_eq!(value["context"]["userId"], "cashier");
        assert_eq!(
            probe.seen.lock().unwrap().as_slice(),
            ["acme.example.test|acme|shifts|sale-1|Coffee Shop|2|acme.example.test"]
        );

        let (status, value) = reply(&state, &headers(&[]), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["businessDate"], "2027-01-15");
        assert_eq!(
            probe.seen.lock().unwrap().last().unwrap(),
            "acme.example.test|acme|-|-||150|acme.example.test"
        );
    });
}

#[test]
fn rejects_unknown_duplicate_and_unbounded_queries() {
    sqlx::test_block_on(async {
        let state = PosWorkspaceHttpState::new(vec![target(Arc::new(Probe::default()))]).unwrap();
        for query in [
            "cursor=page2",
            "finance=1",
            "tab=catalog&cursor=x",
            "tab=unknown",
            "tab=sale&tab=shifts",
            "limit=0",
            "limit=151",
            "limit=abc",
            "limit=",
            "sale=sale-1&sale=sale-2",
            "q=a&q=b",
            "q=%zz",
            "q=%e0%a4",
            "flag",
            "=sale",
        ] {
            let (status, value) = reply(&state, &headers(&[]), Some(query)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}");
            assert_eq!(value["error"]["code"], "POS_REQUEST_INVALID", "{query}");
        }
        let long = format!("q={}", "x".repeat(101));
        let (status, value) = reply(&state, &headers(&[]), Some(&long)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(value["error"]["code"], "POS_REQUEST_INVALID");

        let huge = format!("q={}", "x".repeat(4_096));
        let (status, _) = reply(&state, &headers(&[]), Some(&huge)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

        for accepted in [
            "q=",
            "tab=",
            "sale=",
            "limit=1",
            "limit=150",
            "tab=sale&q=POS%2D0001",
        ] {
            let (status, _) = reply(&state, &headers(&[]), Some(accepted)).await;
            assert_eq!(status, StatusCode::OK, "{accepted}");
        }
    });
}

#[test]
fn missing_target_origin_and_backend_failures_map_to_http() {
    sqlx::test_block_on(async {
        let state = PosWorkspaceHttpState::new(vec![target(Arc::new(Noop))]).unwrap();
        for (view, expected, code) in [
            (
                headers(&[("host", "evil.example.test")]),
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
            ),
            (
                headers(&[("origin", "https://evil.example.test")]),
                StatusCode::FORBIDDEN,
                "ORIGIN_FORBIDDEN",
            ),
        ] {
            let (status, value) = reply(&state, &view, None).await;
            assert_eq!(status, expected);
            assert_eq!(value["error"]["code"], code);
        }
        let (status, _) = reply(&state, &headers(&[]), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let mut duplicate = headers(&[]);
        duplicate.append("host", HeaderValue::from_static("second.example.test"));
        let (status, _) = reply(&state, &duplicate, None).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

        let failing = Arc::new(Probe {
            fail: Some(BusinessError::new(
                FailureKind::Conflict,
                "POS_DAY_CHANGED",
                "Synthetic changed POS source.",
            )),
            ..Probe::default()
        });
        let state = PosWorkspaceHttpState::new(vec![target(failing)]).unwrap();
        let (status, value) = reply(&state, &headers(&[]), None).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(value["error"]["code"], "POS_DAY_CHANGED");
    });
}

#[test]
fn partial_or_extended_backend_payload_is_rejected() {
    sqlx::test_block_on(async {
        let broken = Arc::new(Probe {
            broken: true,
            ..Probe::default()
        });
        let state = PosWorkspaceHttpState::new(vec![target(broken)]).unwrap();
        let (status, value) = reply(&state, &headers(&[]), None).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(value["error"]["code"], "POS_SOURCE_INVALID");
    });
}
