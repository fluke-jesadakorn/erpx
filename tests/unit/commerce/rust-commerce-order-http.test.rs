//! Actual owner-local HTTP router tests for the native Commerce order slice.
//! The exact owner dispatch is exercised over loopback without a shared
//! composition backend or PostgreSQL-backed request.
use erpx_server_commerce::public::order_http::{
    CommerceOrderHttpBackend, CommerceOrderHttpCommand, CommerceOrderHttpFuture,
    CommerceOrderHttpRequest, CommerceOrderHttpResponse, CommerceOrderHttpState,
    CommerceOrderHttpTarget, commerce_order_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: Option<BusinessError>,
    bad: bool,
}

impl CommerceOrderHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: CommerceOrderHttpRequest<'a>,
    ) -> CommerceOrderHttpFuture<'a, CommerceOrderHttpResponse> {
        Box::pin(async move {
            let seen = match &request.command {
                CommerceOrderHttpCommand::Detail { order_id } => {
                    format!(
                        "detail|{}|{}|{}",
                        request.host, request.holding_slug, order_id
                    )
                }
                CommerceOrderHttpCommand::Transition {
                    order_id,
                    idempotency_key,
                    action,
                    expected_version,
                    reason,
                } => format!(
                    "transition|{}|{}|{}|{}|{}|{}|{}|{}",
                    request.host,
                    request.holding_slug,
                    order_id,
                    idempotency_key,
                    action.as_str(),
                    expected_version,
                    reason.as_deref().unwrap_or(""),
                    request
                        .headers
                        .get("cookie")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("")
                ),
                CommerceOrderHttpCommand::Save {
                    order_id,
                    idempotency_key,
                    input,
                } => format!(
                    "save|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                    request.host,
                    request.holding_slug,
                    order_id.as_deref().unwrap_or(""),
                    idempotency_key,
                    input.kind.as_str(),
                    input.party_id,
                    input.warehouse_id.as_deref().unwrap_or(""),
                    input.order_date,
                    input.due_date,
                    input.lines.len(),
                    input.expected_version.unwrap_or(0),
                ),
                CommerceOrderHttpCommand::Fulfill {
                    order_id,
                    idempotency_key,
                    input,
                } => format!(
                    "fulfill|{}|{}|{}|{}|{}|{}",
                    request.host,
                    request.holding_slug,
                    order_id,
                    idempotency_key,
                    input.expected_version,
                    input.lines.len(),
                ),
            };
            self.seen.lock().unwrap().push(seen);
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(CommerceOrderHttpResponse {
                data: match &request.command {
                    CommerceOrderHttpCommand::Detail { .. } => {
                        if self.bad {
                            serde_json::json!({"order":{},"lines":[]})
                        } else {
                            serde_json::json!({
                                "order":{"id":"order-1","company_id":"company","kind":"quote","state":"draft"},
                                "lines":[{"id":"line-1","order_id":"order-1"}]
                            })
                        }
                    }
                    CommerceOrderHttpCommand::Transition { .. } => {
                        if self.bad {
                            serde_json::json!({"id":"order-1","state":"submitted"})
                        } else {
                            serde_json::json!({
                                "id":"order-1","state":"submitted","version":2,"salesId":null
                            })
                        }
                    }
                    CommerceOrderHttpCommand::Save { order_id, .. } => {
                        if self.bad {
                            serde_json::json!({"id":"order-1"})
                        } else {
                            serde_json::json!({
                                "id":order_id.as_deref().unwrap_or("order-1"),"version":2
                            })
                        }
                    }
                    CommerceOrderHttpCommand::Fulfill { order_id, .. } => {
                        if self.bad {
                            serde_json::json!({"id":order_id})
                        } else {
                            serde_json::json!({
                                "id":order_id,"fulfillmentId":"delivery-1","version":2,"state":"fulfilled"
                            })
                        }
                    }
                },
                context: serde_json::json!({"userId":"user"}),
            })
        })
    }
}

fn target(
    host: &str,
    slug: &str,
    seen: Arc<Mutex<Vec<String>>>,
    fail: Option<BusinessError>,
    bad: bool,
) -> CommerceOrderHttpTarget {
    CommerceOrderHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: format!("https://{host}"),
        backend: Arc::new(Probe { seen, fail, bad }),
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

fn post(path: &str, host: &str, key: Option<&str>, body: &str) -> String {
    let key = key
        .map(|value| format!("Idempotency-Key: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n{key}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[tokio::test]
async fn actual_router_forwards_detail_and_transition_with_private_headers() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = commerce_order_http_router(vec![target(
        "acme.example.test",
        "acme",
        seen.clone(),
        None,
        false,
    )])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, headers) = http(
        addr,
        get(
            "/api/v1/holdings/acme/commerce/orders/order-1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["order"]["id"], "order-1");
    assert_eq!(body["context"]["userId"], "user");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/commerce/orders/order-1/transition",
            "acme.example.test",
            Some("commerce-command-0001"),
            r#"{"action":"submit","expectedVersion":1}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body["data"],
        serde_json::json!({"id":"order-1","state":"submitted","version":2,"salesId":null})
    );
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        [
            "detail|acme.example.test|acme|order-1",
            "transition|acme.example.test|acme|order-1|commerce-command-0001|submit|1||"
        ]
    );
}

#[tokio::test]
async fn actual_router_forwards_one_service_completion_exactly() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = commerce_order_http_router(vec![target(
        "acme.example.test",
        "acme",
        seen.clone(),
        None,
        false,
    )])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    let body = r#"{"completionMode":"service","expectedVersion":3,"date":"2026-11-12","evidenceNote":"Customer accepted the service","lines":[{"lineId":"line-1","quantity":"1"}]}"#;
    let (status, response, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/commerce/orders/order-1/fulfill",
            "acme.example.test",
            Some("commerce-fulfill-0001"),
            body,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        response["data"],
        serde_json::json!({
            "id":"order-1","fulfillmentId":"delivery-1","version":2,"state":"fulfilled"
        })
    );
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        ["fulfill|acme.example.test|acme|order-1|commerce-fulfill-0001|3|1"]
    );
    for (body, expected) in [
        (
            r#"{"completionMode":"goods","expectedVersion":3,"date":"2026-11-12","evidenceNote":"x","lines":[]}"#,
            422,
        ),
        (
            r#"{"completionMode":"service","expectedVersion":3,"date":"2026-11-12","evidenceNote":"x","lines":[{"lineId":"line-1","quantity":"1"}],"evidenceIds":[]}"#,
            422,
        ),
        (
            r#"{"completionMode":"service","expectedVersion":3,"date":"2026-11-12","evidenceNote":"x","lines":[]}"#,
            422,
        ),
    ] {
        let (status, _, _) = http(
            addr,
            post(
                "/api/v1/holdings/acme/commerce/orders/order-1/fulfill",
                "acme.example.test",
                Some("commerce-fulfill-invalid"),
                body,
            ),
        )
        .await;
        assert_eq!(status, expected);
    }
}

#[tokio::test]
async fn actual_router_forwards_save_create_and_update_exactly() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = commerce_order_http_router(vec![target(
        "acme.example.test",
        "acme",
        seen.clone(),
        None,
        false,
    )])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    let body = r#"{"kind":"quote","partyId":"party-1","warehouseId":"","orderDate":"2026-11-10","dueDate":"2026-11-15","note":"","priceListId":"list-1","discountReason":"Reviewed commercial discount","lines":[{"productId":"product-1","quantity":"1","price":"80","taxBps":700}]}"#;
    let (status, response, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/commerce/orders",
            "acme.example.test",
            Some("commerce-save-0001"),
            body,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        response["data"],
        serde_json::json!({"id":"order-1","version":2})
    );
    let (status, response, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/commerce/orders/order-1",
            "acme.example.test",
            Some("commerce-save-0002"),
            body,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        response["data"],
        serde_json::json!({"id":"order-1","version":2})
    );
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        [
            "save|acme.example.test|acme||commerce-save-0001|quote|party-1||2026-11-10|2026-11-15|1|0",
            "save|acme.example.test|acme|order-1|commerce-save-0002|quote|party-1||2026-11-10|2026-11-15|1|0"
        ]
    );

    for (request, expected) in [
        (
            post(
                "/api/v1/holdings/acme/commerce/orders",
                "acme.example.test",
                None,
                body,
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/acme/commerce/orders",
                "acme.example.test",
                Some("commerce-save-0003"),
                r#"{"kind":"purchase","partyId":"party-1","orderDate":"2026-11-10","dueDate":"2026-11-15","lines":[]}"#,
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/acme/commerce/orders",
                "acme.example.test",
                Some("commerce-save-0004"),
                r#"{"kind":"quote","partyId":"party-1","orderDate":"2026-11-10","dueDate":"2026-11-15","lines":[{"productId":"p","quantity":"1","price":"1","taxBps":10001}]}"#,
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/acme/commerce/orders",
                "acme.example.test",
                Some("commerce-save-0005"),
                r#"{"kind":"quote","partyId":"party-1","orderDate":"2026-11-10","dueDate":"2026-11-15","lines":[{"productId":"p","quantity":"1","price":"1","taxBps":0}],"unknown":true}"#,
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/acme/commerce/orders?print=1",
                "acme.example.test",
                Some("commerce-save-0006"),
                body,
            ),
            422,
        ),
    ] {
        let (status, _, _) = http(addr, request).await;
        assert_eq!(status, expected);
    }
}

#[tokio::test]
async fn router_rejects_unknown_hosts_scopes_queries_and_malformed_bodies() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = commerce_order_http_router(vec![
        target("acme.example.test", "acme", seen.clone(), None, false),
        target(
            "fail.example.test",
            "fail",
            seen,
            Some(BusinessError::new(
                FailureKind::Conflict,
                "COMMERCE_ORDER_CHANGED",
                "The order changed. Reload and try again.",
            )),
            false,
        ),
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    for (request, expected) in [
        (
            get(
                "/api/v1/holdings/acme/commerce/orders/order-1",
                "evil.example.test",
            ),
            404,
        ),
        (
            get(
                "/api/v1/holdings/other/commerce/orders/order-1",
                "acme.example.test",
            ),
            404,
        ),
        (
            get(
                "/api/v1/holdings/acme/commerce/orders/order-1?print=1",
                "acme.example.test",
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/acme/commerce/orders/order-1/transition",
                "acme.example.test",
                None,
                r#"{"action":"submit","expectedVersion":1}"#,
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/acme/commerce/orders/order-1/transition",
                "acme.example.test",
                Some("commerce-command-0001"),
                r#"{"action":"unknown","expectedVersion":1}"#,
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/acme/commerce/orders/order-1/transition",
                "acme.example.test",
                Some("commerce-command-0001"),
                r#"{"action":"submit","expectedVersion":1,"extra":true}"#,
            ),
            422,
        ),
        (
            post(
                "/api/v1/holdings/fail/commerce/orders/order-1/transition",
                "fail.example.test",
                Some("commerce-command-0002"),
                r#"{"action":"submit","expectedVersion":1}"#,
            ),
            409,
        ),
    ] {
        let (status, _, _) = http(addr, request).await;
        assert_eq!(status, expected);
    }
}

#[test]
fn configuration_rejects_duplicate_targets_and_bad_origins() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    assert!(
        CommerceOrderHttpState::new(vec![target(
            "acme.example.test",
            "acme",
            seen.clone(),
            None,
            false,
        )])
        .is_ok()
    );
    assert!(
        CommerceOrderHttpState::new(vec![
            target("acme.example.test", "acme", seen.clone(), None, false),
            target("acme.example.test", "acme", seen.clone(), None, false),
        ])
        .is_err()
    );
    for origin in [
        "http://acme.example.test",
        "https://acme.example.test/path",
        "https://acme.example.test:8443",
    ] {
        let mut invalid = target("acme.example.test", "acme", seen.clone(), None, false);
        invalid.origin = origin.into();
        assert!(CommerceOrderHttpState::new(vec![invalid]).is_err());
    }
    let mut development = target(
        "acme.erpx.localhost:3443",
        "acme",
        seen.clone(),
        None,
        false,
    );
    development.origin = "https://acme.erpx.localhost:3443".into();
    assert!(
        CommerceOrderHttpState::new_with_origin_policy(
            vec![development],
            erpx_server_commerce::public::order_http::CommerceOriginPolicy::NonProduction
        )
        .is_ok()
    );
    let mut bad_slug = target("acme.example.test", "Acme!", seen, None, false);
    bad_slug.holding_slug = "Acme!".into();
    assert!(CommerceOrderHttpState::new(vec![bad_slug]).is_err());
}

#[tokio::test]
async fn ingress_preserves_untrusted_selection_for_backend_and_classifies_writes() {
    use erpx_server_commerce::public::order_http::CommerceSelectionOperation;
    use erpx_server_runtime::public::http_support::{HeaderMap, HeaderValue, StatusCode};
    struct DeferredProbe(Arc<Mutex<Vec<(CommerceSelectionOperation, Vec<String>)>>>);
    impl CommerceOrderHttpBackend for DeferredProbe {
        fn execute<'a>(
            &'a self,
            request: CommerceOrderHttpRequest<'a>,
        ) -> CommerceOrderHttpFuture<'a, CommerceOrderHttpResponse> {
            Box::pin(async move {
                let values = request
                    .headers
                    .get_all("x-erpx-context-company")
                    .iter()
                    .map(|v| v.to_str().unwrap().to_owned())
                    .collect();
                self.0
                    .lock()
                    .unwrap()
                    .push((request.selection_operation(), values));
                // Simulate a backend that authenticates before resolving the
                // captured selection; this is not a real Account proof.
                Err(BusinessError::new(
                    FailureKind::Unauthenticated,
                    "UNAUTHENTICATED",
                    "Sign in.",
                ))
            })
        }
    }
    let seen = Arc::new(Mutex::new(Vec::new()));
    let state = CommerceOrderHttpState::new(vec![CommerceOrderHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(DeferredProbe(seen.clone())),
    }])
    .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_static("acme.example.test"));
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    headers.insert(
        "idempotency-key",
        HeaderValue::from_static("commerce-command-0001"),
    );
    headers.append(
        "x-erpx-context-company",
        HeaderValue::from_static("company-a"),
    );
    headers.append(
        "x-erpx-context-company",
        HeaderValue::from_static("company-b"),
    );
    assert_eq!(
        state
            .detail(&headers, "acme", "order-1", None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        state
            .transition(
                &headers,
                "acme",
                "order-1",
                None,
                br#"{"action":"submit","expectedVersion":1}"#
            )
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let save=br#"{"kind":"quote","partyId":"party-1","warehouseId":"","orderDate":"2026-11-10","dueDate":"2026-11-15","note":"","priceListId":"list-1","discountReason":"Reviewed commercial discount","lines":[{"productId":"product-1","quantity":"1","price":"80","taxBps":700}]}"#;
    assert_eq!(
        state
            .save(&headers, "acme", None, None, save)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let expected_values = vec!["company-a".to_owned(), "company-b".to_owned()];
    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            (
                CommerceSelectionOperation::RenderedRead,
                expected_values.clone()
            ),
            (
                CommerceSelectionOperation::Mutation,
                expected_values.clone()
            ),
            (CommerceSelectionOperation::Mutation, expected_values)
        ]
    );
}

#[tokio::test]
async fn origin_check_rejects_foreign_or_duplicate_before_backend_dispatch() {
    use erpx_server_runtime::public::http_support::{HeaderMap, HeaderValue, StatusCode};
    let seen = Arc::new(Mutex::new(Vec::new()));
    let state = CommerceOrderHttpState::new(vec![target(
        "acme.example.test",
        "acme",
        seen.clone(),
        None,
        false,
    )])
    .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_static("acme.example.test"));
    headers.insert(
        "origin",
        HeaderValue::from_static("https://foreign.example.test"),
    );
    assert_eq!(
        state
            .detail(&headers, "acme", "order-1", None)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert!(seen.lock().unwrap().is_empty());
    headers.insert(
        "origin",
        HeaderValue::from_static("https://acme.example.test"),
    );
    headers.append(
        "origin",
        HeaderValue::from_static("https://acme.example.test"),
    );
    assert_eq!(
        state
            .detail(&headers, "acme", "order-1", None)
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(seen.lock().unwrap().is_empty());
    headers.remove("origin");
    assert_eq!(
        state
            .detail(&headers, "acme", "order-1", None)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[test]
fn trusted_origin_policy_matches_real_typescript_url_validation() {
    use erpx_server_commerce::public::order_http::CommerceOriginPolicy;
    let cases = [
        (
            "production",
            "https://acme.example.test",
            "acme.example.test",
        ),
        (
            "development",
            "https://acme.example.test",
            "acme.example.test",
        ),
        (
            "production",
            "https://acme.erpx.localhost:3443",
            "acme.erpx.localhost:3443",
        ),
        (
            "development",
            "https://acme.erpx.localhost:3443",
            "acme.erpx.localhost:3443",
        ),
        (
            "test",
            "https://acme.erpx.localhost:3443",
            "acme.erpx.localhost:3443",
        ),
        (
            "development",
            "https://erpx.localhost:3443",
            "erpx.localhost:3443",
        ),
        (
            "development",
            "https://acme.erpx.localhost:8443",
            "acme.erpx.localhost:8443",
        ),
        (
            "production",
            "https://acme.example.test:443",
            "acme.example.test:443",
        ),
        (
            "development",
            "https://ACME.example.test",
            "ACME.example.test",
        ),
        (
            "production",
            "https://acme.example.test/",
            "acme.example.test",
        ),
        (
            "production",
            "https://user@acme.example.test",
            "acme.example.test",
        ),
        ("production", "https://[::1]", "[::1]"),
        (
            "production",
            "http://acme.example.test",
            "acme.example.test",
        ),
        (
            "production",
            "https://xn--bcher-kva.example",
            "xn--bcher-kva.example",
        ),
    ];
    let script = r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);const cases=JSON.parse(process.argv[2]);const out=[];for(const [environment,origin] of cases){process.env.NODE_ENV=environment;try{const c=m.nativeCommerceOrdersConfiguration(JSON.stringify({privateBaseUrl:'http://127.0.0.1:44993',timeoutMs:1000,holdings:[{origin,holdingId:'h001',holdingSlug:'acme'}]}));m.createNativeCommerceOrdersClient(c);out.push(true)}catch{out.push(false)}}process.stdout.write(JSON.stringify(out));"#;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let module = root.join("packages/feature-commerce/src/public/native-commerce-orders.ts");
    let cases_json = serde_json::to_string(&cases).unwrap();
    let output =
        std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
            .current_dir(root)
            .args(["--conditions=react-server", "-e", script])
            .arg(module)
            .arg(cases_json)
            .output()
            .unwrap();
    assert!(
        output.status.success(),
        "actual TypeScript origin oracle unavailable"
    );
    let expected: Vec<bool> = serde_json::from_slice(&output.stdout).unwrap();
    for (i, (environment, origin, host)) in cases.into_iter().enumerate() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut value = target(host, "acme", seen, None, false);
        value.origin = origin.into();
        let policy = if environment == "production" {
            CommerceOriginPolicy::Production
        } else {
            CommerceOriginPolicy::NonProduction
        };
        let accepted = CommerceOrderHttpState::new_with_origin_policy(vec![value], policy).is_ok();
        assert_eq!(accepted, expected[i], "{environment} {origin}");
    }
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut mismatch = target("other.example.test", "acme", seen, None, false);
    mismatch.origin = "https://acme.example.test".into();
    assert!(
        CommerceOrderHttpState::new_with_origin_policy(
            vec![mismatch],
            CommerceOriginPolicy::NonProduction
        )
        .is_err()
    );
    let seen = Arc::new(Mutex::new(Vec::new()));
    let development = target("acme.erpx.localhost:3443", "acme", seen, None, false);
    assert!(
        CommerceOrderHttpState::new(vec![development]).is_err(),
        "legacy constructor remains production strict"
    );
}
