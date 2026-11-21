//! Owner-local HTTP ingress for native POS commands.
use erpx_server_pos::public::commands_http::{
    PosCommandHttpBackend, PosCommandHttpFuture, PosCommandHttpRequest, PosCommandHttpResponse,
    PosCommandHttpState, PosCommandHttpTarget, pos_command_router,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, StatusCode, body::to_bytes, header, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;
struct BadReceipt;
#[derive(Default)]
struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: Option<BusinessError>,
}
impl PosCommandHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: PosCommandHttpRequest<'a>,
    ) -> PosCommandHttpFuture<'a, PosCommandHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.key,
                request.command.operation()
            ));
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(PosCommandHttpResponse {
                data: serde_json::json!({"id":"0f8fad5b-d9cb-469f-a165-70867728950e"}),
                context: serde_json::json!({"userId":"user"}),
            })
        })
    }
}
impl PosCommandHttpBackend for BadReceipt {
    fn execute<'a>(
        &'a self,
        _: PosCommandHttpRequest<'a>,
    ) -> PosCommandHttpFuture<'a, PosCommandHttpResponse> {
        Box::pin(async {
            Ok(PosCommandHttpResponse {
                data: serde_json::json!({"id":"not-a-uuid"}),
                context: serde_json::json!({"userId":"user"}),
            })
        })
    }
}
impl PosCommandHttpBackend for Noop {
    fn execute<'a>(
        &'a self,
        _: PosCommandHttpRequest<'a>,
    ) -> PosCommandHttpFuture<'a, PosCommandHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
}

fn target(backend: Arc<dyn PosCommandHttpBackend>) -> PosCommandHttpTarget {
    PosCommandHttpTarget {
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
        ("idempotency-key", "pos-command-0001"),
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
    body(r#"{"registerId":"register","openingMinor":1000}"#)
}
async fn reply(
    state: &PosCommandHttpState,
    headers: &HeaderMap,
    operation: &str,
    body: &[u8],
) -> (StatusCode, serde_json::Value) {
    let response = state.execute(headers, "acme", operation, body).await;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[test]
fn forwards_exact_operation_key_and_body_and_returns_private_json() {
    sqlx::test_block_on(async {
        let probe = Arc::new(Probe::default());
        let state = PosCommandHttpState::new(vec![target(probe.clone())]).unwrap();
        let response = state
            .execute(&headers(&[]), "acme", "shift.open", &request_body())
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["data"]["id"], "0f8fad5b-d9cb-469f-a165-70867728950e");
        assert_eq!(value["context"]["userId"], "user");
        assert_eq!(
            probe.seen.lock().unwrap().as_slice(),
            ["acme.example.test|acme|pos-command-0001|shift.open"]
        );
    });
}

#[test]
fn rejects_unknown_targets_origins_operations_headers_and_exact_body_variants() {
    sqlx::test_block_on(async {
        let state = PosCommandHttpState::new(vec![target(Arc::new(Probe::default()))]).unwrap();
        for (view, operation, expected) in [
            (
                headers(&[("host", "evil.example.test")]),
                "shift.open",
                StatusCode::NOT_FOUND,
            ),
            (
                headers(&[("origin", "https://evil.example.test")]),
                "shift.open",
                StatusCode::FORBIDDEN,
            ),
            (headers(&[]), "sale.unknown", StatusCode::NOT_FOUND),
            (
                headers(&[("content-type", "text/plain")]),
                "shift.open",
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                headers(&[("idempotency-key", "short")]),
                "shift.open",
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
        ] {
            let (status, _) = reply(&state, &view, operation, &request_body()).await;
            assert_eq!(status, expected, "{operation}");
        }
        for value in [
            r#"{"registerId":"register","openingMinor":1000,"extra":1}"#,
            r#"{"registerId":"register"}"#,
            r#"{"registerId":"","openingMinor":1000}"#,
            r#"{"registerId":"register","openingMinor":-1}"#,
            r#"{"registerId":"register","openingMinor":100000000000}"#,
            r#"[]"#,
            r#"{"registerId":"register","openingMinor":1000"#,
            r#"{"id":"shift","expectedVersion":1,"countedMinor":1000,"denominations":{"500":1}}"#,
        ] {
            let operation = if value.contains("denominations") {
                "shift.close"
            } else {
                "shift.open"
            };
            let (status, response) = reply(&state, &headers(&[]), operation, &body(value)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{value}");
            let expected = if value.contains("denominations") {
                "POS_CASH_COUNT"
            } else {
                "POS_COMMAND_INVALID"
            };
            assert_eq!(response["error"]["code"], expected);
        }
        let oversized = format!(
            r#"{{"registerId":"register","openingMinor":1000,"reason":"{}"}}"#,
            "x".repeat(70 * 1024)
        );
        let (status, _) = reply(&state, &headers(&[]), "shift.open", &body(&oversized)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    });
}

#[test]
fn maps_owner_failures_and_rejects_a_bad_receipt() {
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
                BusinessError::new(FailureKind::Unavailable, "POS_SOURCE_INVALID", "Source."),
                StatusCode::SERVICE_UNAVAILABLE,
            ),
        ] {
            let probe = Probe {
                seen: Arc::new(Mutex::new(Vec::new())),
                fail: Some(failure.clone()),
            };
            let state = PosCommandHttpState::new(vec![target(Arc::new(probe))]).unwrap();
            let (status, value) = reply(&state, &headers(&[]), "shift.open", &request_body()).await;
            assert_eq!(status, expected);
            assert_eq!(value["error"]["code"], failure.code);
        }
        let state = PosCommandHttpState::new(vec![target(Arc::new(BadReceipt))]).unwrap();
        let (status, value) = reply(&state, &headers(&[]), "shift.open", &request_body()).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(value["error"]["code"], "POS_SOURCE_INVALID");
    });
}

#[test]
fn requires_exact_distinct_https_targets() {
    let backend = Arc::new(Noop);
    assert!(pos_command_router(vec![target(backend.clone())]).is_ok());
    assert!(pos_command_router(vec![target(backend.clone()), target(backend.clone())]).is_err());
    assert!(
        pos_command_router(vec![PosCommandHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target(backend)
        }])
        .is_err()
    );
}
