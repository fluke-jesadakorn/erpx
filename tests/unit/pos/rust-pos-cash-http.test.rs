//! Owner-local HTTP adapter tests for the native POS cash-movement command.
//! The exact owner dispatch is exercised without opening a socket; mounting the
//! router into the shared composition backend remains an integration step.
use erpx_server_pos::public::cash_movement_http::{
    CashMovementHttpBackend, CashMovementHttpFuture, CashMovementHttpRequest,
    CashMovementHttpResponse, CashMovementHttpState, CashMovementHttpTarget, cash_movement_router,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, StatusCode, body::to_bytes, header, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

struct BadReceipt;

impl CashMovementHttpBackend for BadReceipt {
    fn create<'a>(
        &'a self,
        _: CashMovementHttpRequest<'a>,
    ) -> CashMovementHttpFuture<'a, CashMovementHttpResponse> {
        Box::pin(async {
            Ok(CashMovementHttpResponse {
                data: serde_json::json!({"id":"not-a-uuid"}),
                context: serde_json::json!({"userId":"user"}),
            })
        })
    }
}

impl CashMovementHttpBackend for Noop {
    fn create<'a>(
        &'a self,
        _: CashMovementHttpRequest<'a>,
    ) -> CashMovementHttpFuture<'a, CashMovementHttpResponse> {
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
}

impl CashMovementHttpBackend for Probe {
    fn create<'a>(
        &'a self,
        request: CashMovementHttpRequest<'a>,
    ) -> CashMovementHttpFuture<'a, CashMovementHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.key,
                request.shift_id,
                request.expected_version,
                request.amount_minor
            ));
            assert_eq!(request.reason, "Opening float");
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(CashMovementHttpResponse {
                data: serde_json::json!({
                    "id": "0f8fad5b-d9cb-469f-a165-70867728950e"
                }),
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

fn target(backend: Arc<dyn CashMovementHttpBackend>) -> CashMovementHttpTarget {
    CashMovementHttpTarget {
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
        ("idempotency-key", "cash-command-0001"),
        ("origin", "https://acme.example.test"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    for (name, value) in overrides {
        headers.insert(*name, HeaderValue::from_static(value));
    }
    headers
}

fn body(value: &str) -> Vec<u8> {
    value.as_bytes().to_vec()
}

fn request_body() -> Vec<u8> {
    body(r#"{"id":"shift-1","expectedVersion":3,"amountMinor":50000,"reason":"Opening float"}"#)
}

async fn reply(
    state: &CashMovementHttpState,
    headers: &HeaderMap,
    query: Option<&str>,
    body: &[u8],
) -> (StatusCode, serde_json::Value) {
    let response = state.create(headers, "acme", query, body).await;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap();
    (status, value)
}

#[test]
fn forwards_the_exact_command_and_returns_private_bounded_json() {
    sqlx::test_block_on(async {
        let probe = Arc::new(Probe::default());
        let state = CashMovementHttpState::new(vec![target(probe.clone())]).unwrap();
        let response = state
            .create(&headers(&[]), "acme", None, &request_body())
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
        assert_eq!(
            value,
            serde_json::json!({
                "data": {"id": "0f8fad5b-d9cb-469f-a165-70867728950e"},
                "context": {"userId": "user"}
            })
        );
        assert_eq!(
            probe.seen.lock().unwrap().as_slice(),
            ["acme.example.test|acme|cash-command-0001|shift-1|3|50000"]
        );
    });
}

#[test]
fn rejects_unknown_targets_origins_queries_headers_and_exact_body_variants() {
    sqlx::test_block_on(async {
        let state = CashMovementHttpState::new(vec![target(Arc::new(Noop))]).unwrap();
        for (view, expected) in [
            (
                headers(&[("host", "evil.example.test")]),
                StatusCode::NOT_FOUND,
            ),
            (
                headers(&[("origin", "https://evil.example.test")]),
                StatusCode::FORBIDDEN,
            ),
        ] {
            let (status, _) = reply(&state, &view, None, &request_body()).await;
            assert_eq!(status, expected);
        }

        let state = CashMovementHttpState::new(vec![target(Arc::new(Probe::default()))]).unwrap();
        for query in [Some("next=1"), Some(""), None] {
            let (status, _) = reply(&state, &headers(&[]), query, &request_body()).await;
            assert_eq!(
                status,
                if query == Some("next=1") {
                    StatusCode::UNPROCESSABLE_ENTITY
                } else {
                    StatusCode::OK
                }
            );
        }

        for overrides in [
            [("content-type", "text/plain")],
            [("idempotency-key", "short")],
            [("idempotency-key", "bad key with spaces")],
        ] {
            let (status, _) = reply(&state, &headers(&overrides), None, &request_body()).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        }

        let mut duplicate = headers(&[]);
        duplicate.append("host", HeaderValue::from_static("second.example.test"));
        let (status, _) = reply(&state, &duplicate, None, &request_body()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

        for value in [
            r#"{"expectedVersion":3,"amountMinor":50000,"reason":"Opening float"}"#,
            r#"{"id":"shift-1","amountMinor":50000,"reason":"Opening float"}"#,
            r#"{"id":"shift-1","expectedVersion":3,"reason":"Opening float"}"#,
            r#"{"id":"shift-1","expectedVersion":3,"amountMinor":50000}"#,
            r#"{"id":"shift-1","expectedVersion":3,"amountMinor":50000,"reason":"Opening float","countedMinor":1}"#,
            r#"{"id":1,"expectedVersion":3,"amountMinor":50000,"reason":"Opening float"}"#,
            r#"{"id":"shift-1","expectedVersion":3.5,"amountMinor":50000,"reason":"Opening float"}"#,
            r#"{"id":"shift-1","expectedVersion":3,"amountMinor":"50000","reason":"Opening float"}"#,
            r#"[]"#,
            r#"{"id":"shift-1","expectedVersion":3,"amountMinor":50000,"reason":"Opening float""#,
        ] {
            let (status, value) = reply(&state, &headers(&[]), None, &body(value)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{value}");
            assert_eq!(value["error"]["code"], "POS_CASH_REQUEST_INVALID");
        }

        let oversized = format!(
            r#"{{"id":"shift-1","expectedVersion":3,"amountMinor":50000,"reason":"{}"}}"#,
            "x".repeat(9 * 1024)
        );
        let (status, _) = reply(&state, &headers(&[]), None, &body(&oversized)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    });
}

#[test]
fn maps_owner_failures_to_bounded_statuses() {
    sqlx::test_block_on(async {
        for (failure, expected) in [
            (
                BusinessError::new(FailureKind::Unauthenticated, "UNAUTHENTICATED", "Sign in."),
                StatusCode::UNAUTHORIZED,
            ),
            (
                BusinessError::new(FailureKind::Forbidden, "ORIGIN_FORBIDDEN", "Origin."),
                StatusCode::FORBIDDEN,
            ),
            (
                BusinessError::new(FailureKind::Conflict, "POS_VERSION_CONFLICT", "Changed."),
                StatusCode::CONFLICT,
            ),
            (
                BusinessError::new(
                    FailureKind::Unavailable,
                    "POS_CASH_SOURCE_INVALID",
                    "Source.",
                ),
                StatusCode::SERVICE_UNAVAILABLE,
            ),
        ] {
            let probe = Probe {
                seen: Arc::new(Mutex::new(Vec::new())),
                fail: Some(failure.clone()),
            };
            let state = CashMovementHttpState::new(vec![target(Arc::new(probe))]).unwrap();
            let (status, value) = reply(&state, &headers(&[]), None, &request_body()).await;
            assert_eq!(status, expected);
            assert_eq!(value["error"]["code"], failure.code);
            assert_eq!(value["error"]["message"], failure.message);
        }
    });
}

#[test]
fn requires_exact_distinct_https_targets() {
    let backend = Arc::new(Noop);
    assert!(cash_movement_router(vec![target(backend.clone())]).is_ok());
    assert!(cash_movement_router(vec![target(backend.clone()), target(backend.clone())]).is_err());
    assert!(
        cash_movement_router(vec![CashMovementHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target(backend.clone())
        }])
        .is_err()
    );
    assert!(
        cash_movement_router(vec![CashMovementHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target(backend.clone())
        }])
        .is_err()
    );
    assert!(
        cash_movement_router(vec![CashMovementHttpTarget {
            holding_slug: "Acme".into(),
            ..target(backend)
        }])
        .is_err()
    );
}

#[test]
fn missing_holder_and_backend_errors_do_not_leak_owner_details() {
    sqlx::test_block_on(async {
        let state = CashMovementHttpState::new(vec![target(Arc::new(Noop))]).unwrap();
        let (status, value) = reply(&state, &headers(&[]), None, &request_body()).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(
            value,
            serde_json::json!({"error":{"code":"NOT_FOUND","message":"Not found."}})
        );

        let state = CashMovementHttpState::new(vec![target(Arc::new(BadReceipt))]).unwrap();
        let (status, value) = reply(&state, &headers(&[]), None, &request_body()).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(value["error"]["code"], "POS_CASH_SOURCE_INVALID");
    });
}
