//! Actual owner-local HTTP router tests for the native Commerce order slice.
//! The exact owner dispatch is exercised over loopback without a shared
//! composition backend or PostgreSQL-backed request.
use erpx_commerce_server::public::order_http::{
    CommerceOrderHttpBackend, CommerceOrderHttpCommand, CommerceOrderHttpFuture,
    CommerceOrderHttpRequest, CommerceOrderHttpResponse, CommerceOrderHttpState,
    CommerceOrderHttpTarget, commerce_order_http_router,
};
use erpx_platform_runtime_server::public::http_support::{serde_json, serve};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
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

/// Pinned owner document used to prove the native order-detail payload is what
/// the TypeScript client accepts; no plan or `include_str!` fragment is used.
#[derive(Clone, Debug)]
struct DocumentProbe {
    document: serde_json::Value,
    context: serde_json::Value,
}

impl CommerceOrderHttpBackend for DocumentProbe {
    fn execute<'a>(
        &'a self,
        request: CommerceOrderHttpRequest<'a>,
    ) -> CommerceOrderHttpFuture<'a, CommerceOrderHttpResponse> {
        Box::pin(async move {
            assert!(matches!(&request.command, CommerceOrderHttpCommand::Detail { .. }));
            Ok(CommerceOrderHttpResponse {
                data: self.document.clone(),
                context: self.context.clone(),
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
            erpx_commerce_server::public::order_http::CommerceOriginPolicy::NonProduction
        )
        .is_ok()
    );
    let mut bad_slug = target("acme.example.test", "Acme!", seen, None, false);
    bad_slug.holding_slug = "Acme!".into();
    assert!(CommerceOrderHttpState::new(vec![bad_slug]).is_err());
}

#[tokio::test]
async fn ingress_preserves_untrusted_selection_for_backend_and_classifies_writes() {
    use erpx_commerce_server::public::order_http::CommerceSelectionOperation;
    use erpx_platform_runtime_server::public::http_support::{HeaderMap, HeaderValue, StatusCode};
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
    use erpx_platform_runtime_server::public::http_support::{HeaderMap, HeaderValue, StatusCode};
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
    use erpx_commerce_server::public::order_http::CommerceOriginPolicy;
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
    let module = root.join("modules/commerce/ui/src/public/native-commerce-orders.ts");
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

#[tokio::test]
async fn command_plan_route_returns_owner_fixed_sql_and_fails_closed() {
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
    let path = "/api/v1/holdings/acme/commerce/command-plan";

    let (status, body, headers) = http(
        addr,
        post(
            path,
            "acme.example.test",
            None,
            r#"{"kind":"commerce_rd_pms_warehouse_active","values":["company-1"]}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body["data"]["statements"][0]["sql"],
        "SELECT 1 FROM commerce_warehouses WHERE company_id=? AND status='active' LIMIT 1"
    );
    assert_eq!(body["data"]["statements"][0]["values"][0], "company-1");
    assert!(headers.contains("cache-control: private, no-store"));
    // The command-plan route must not reach the order backend.
    assert!(seen.lock().unwrap().is_empty());

    let (status, _, _) = http(
        addr,
        post(
            path,
            "acme.example.test",
            None,
            r#"{"kind":"SELECT 1 FROM commerce_settings","values":[]}"#,
        ),
    )
    .await;
    assert_ne!(status, 200, "unregistered kind must fail closed");

    let (status, _, _) = http(
        addr,
        post(
            path,
            "acme.example.test",
            None,
            r#"{"kind":"commerce_rd_pms_settings_exists","values":["c1","c2"]}"#,
        ),
    )
    .await;
    assert_ne!(status, 200, "wrong bind count must fail closed");

    for (kind, values, expected_sql, expected_values) in [
        (
            "commerce_rd_pos_startup_warehouses",
            r#"["company-1","branch-1","branch-1"]"#,
            "SELECT id,branch_id,code,name,status,version FROM commerce_warehouses WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY id",
            r#"["company-1","branch-1","branch-1"]"#,
        ),
        (
            "commerce_rd_pos_startup_products",
            r#"["company-1"]"#,
            "SELECT id,sku,name,item_type,tracking_mode,unit,price_minor,tax_bps,status,version FROM commerce_products WHERE company_id=? ORDER BY id",
            r#"["company-1"]"#,
        ),
        (
            "commerce_rd_settings_by_company",
            r#"["company-1"]"#,
            "SELECT * FROM commerce_settings WHERE company_id=?",
            r#"["company-1"]"#,
        ),
        (
            "commerce_rd_currency_by_company",
            r#"["company-1"]"#,
            "SELECT currency FROM commerce_settings WHERE company_id=?",
            r#"["company-1"]"#,
        ),
        (
            "commerce_rd_posting_mapping_state",
            r#"["company-1"]"#,
            "SELECT chart_id \"chartId\",currency,receivable_account_id \"receivableAccountId\",payable_account_id \"payableAccountId\",revenue_account_id \"revenueAccountId\",inventory_account_id \"inventoryAccountId\",clearing_account_id \"clearingAccountId\",cogs_account_id \"cogsAccountId\",input_tax_account_id \"inputTaxAccountId\",output_tax_account_id \"outputTaxAccountId\",cash_account_id \"cashAccountId\" FROM commerce_settings WHERE company_id=?",
            r#"["company-1"]"#,
        ),
        (
            "commerce_rd_product_page_rows",
            r#"["company-1"]"#,
            "SELECT *,sku AS page_key FROM commerce_products WHERE company_id=?",
            r#"["company-1"]"#,
        ),
        (
            "commerce_rd_product_master_rows",
            r#"["company-1"]"#,
            "SELECT * FROM commerce_products WHERE company_id=? ORDER BY sku",
            r#"["company-1"]"#,
        ),
        (
            "commerce_rd_product_active",
            r#"["product-1","company-1"]"#,
            "SELECT * FROM commerce_products WHERE id=? AND company_id=? AND status='active'",
            r#"["product-1","company-1"]"#,
        ),
        (
            "commerce_rd_product_active_options",
            r#"["company-1"]"#,
            "SELECT id,name FROM commerce_products WHERE company_id=? AND status='active' ORDER BY name",
            r#"["company-1"]"#,
        ),
        (
            "commerce_rd_product_master_detail",
            r#"["product-1","company-1"]"#,
            "SELECT * FROM commerce_products WHERE id=? AND company_id=?",
            r#"["product-1","company-1"]"#,
        ),
        (
            "commerce_rd_product_point",
            r#"["product-1","company-1"]"#,
            "SELECT id FROM commerce_products WHERE id=? AND company_id=? AND status='active'",
            r#"["product-1","company-1"]"#,
        ),
        (
            "commerce_rd_warehouse_page_rows",
            r#"["company-1","branch-1","branch-2"]"#,
            "SELECT *,name AS page_key FROM commerce_warehouses WHERE company_id=? AND (?::text IS NULL OR branch_id=?)",
            r#"["company-1","branch-1","branch-2"]"#,
        ),
        (
            "commerce_rd_warehouse_master_rows",
            r#"["company-1","branch-1","branch-2"]"#,
            "SELECT * FROM commerce_warehouses WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name",
            r#"["company-1","branch-1","branch-2"]"#,
        ),
        (
            "commerce_rd_warehouse_active",
            r#"["warehouse-1","company-1","branch-1","branch-2"]"#,
            "SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)",
            r#"["warehouse-1","company-1","branch-1","branch-2"]"#,
        ),
        (
            "commerce_rd_warehouses_active",
            r#"["company-1","branch-1","branch-2"]"#,
            "SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name",
            r#"["company-1","branch-1","branch-2"]"#,
        ),
        (
            "commerce_rd_warehouse_branch_active",
            r#"["company-1","branch-1"]"#,
            "SELECT id,name FROM commerce_warehouses WHERE company_id=? AND branch_id=? AND status='active'",
            r#"["company-1","branch-1"]"#,
        ),
    ] {
        let request = format!(r#"{{"kind":"{kind}","values":{values}}}"#);
        let (status, response, _) = http(addr, post(path, "acme.example.test", None, &request)).await;
        assert_eq!(status, 200, "{kind} must resolve on the command-plan route");
        assert_eq!(response["data"]["statements"][0]["sql"], expected_sql);
        assert_eq!(
            response["data"]["statements"][0]["values"],
            serde_json::from_str::<serde_json::Value>(expected_values).unwrap(),
            "{kind} values pass through verbatim"
        );
    }
    // Planned owner reads never reach the order backend.
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn catchall_order_detail_native_payload_is_accepted_by_typescript_client() {
    // Same fixture as tests/unit/commerce/native-commerce-orders-transport.test.ts.
    let document = serde_json::json!({
        "order": {
            "id":"order-1","company_id":"company","branch_id":null,"department_id":null,
            "kind":"quote","number":"QT-1","party_id":"party-1","warehouse_id":null,"currency":"THB",
            "order_date":"2026-11-01","due_date":"2026-11-08","note":"","state":"draft",
            "subtotal_minor":10000,"tax_minor":700,"source_id":null,"version":1,"created_by":"user",
            "approved_by":null,"created_at":1,"updated_at":1,"price_list_id":null,"price_list_version":null,
            "discount_reason":null,"discount_approval_required":0,"discount_approved_by":null,
            "discount_approved_at":null
        },
        "lines": [{
            "id":"line-1","order_id":"order-1","product_id":"product-1","description":"Service",
            "quantity_milli":1000,"fulfilled_milli":0,"price_minor":10000,"tax_bps":700,
            "subtotal_minor":10000,"tax_minor":700,"ordinal":0,"list_price_minor":null,
            "discount_bps":0,"item_type":"service","sku":"SVC-1","unit":"each","tracking_mode":"none"
        }]
    });
    let context = serde_json::json!({
        "userId":"user","holdingId":"h001","companyId":"company","branchId":null,
        "departmentId":null,"branchSlug":null,"departmentSlug":null,"assignmentId":"assignment",
        "assignmentVersion":3,"revision":7,"role":"owner","assignmentScopeKind":"holding",
        "modules":["sales"],"permissions":["commerce.read","commerce.sales","commerce.approve"],
        "expiresAt":9_007_199_254_740_991_i64
    });
    let router = commerce_order_http_router(vec![CommerceOrderHttpTarget {
        host: "acme.example.com".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.com".into(),
        backend: Arc::new(DocumentProbe {
            document: document.clone(),
            context: context.clone(),
        }),
    }])
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
            "acme.example.com",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body, serde_json::json!({"data": document, "context": context}));
    assert!(headers.contains("cache-control: private, no-store"));

    // Actual TypeScript client validation of the payload emitted by the Axum
    // handler; this is transport through the real native client, not a plan.
    let script = r#"const m=await import(process.argv[1]);const body=JSON.parse(process.argv[2]);const actor={userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},modules:['sales'],iam:{permissions:['commerce.read','commerce.sales','commerce.approve']}};const input={holdingSlug:'acme',expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};const transport=async()=>new Response(JSON.stringify(body),{status:200,headers:{'content-type':'application/json'}});const cfg=m.nativeCommerceOrdersConfiguration(JSON.stringify({privateBaseUrl:'http://127.0.0.1:44993',timeoutMs:1000,holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}]}));const value=await m.createNativeCommerceOrdersClient(cfg,transport).detail({get:name=>name==='host'?'acme.example.com':null},actor,'order-1',input);if(value.order.id!=='order-1'||value.lines.length!==1)throw Error('PARITY_FAILED');process.stdout.write('PARITY_OK');"#;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let module = root.join("modules/commerce/ui/src/public/native-commerce-orders.ts");
    let output = std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
        .current_dir(root)
        .args(["--conditions=react-server", "-e", script])
        .arg(module)
        .arg(serde_json::to_string(&body).unwrap())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "TypeScript native client rejected the Rust payload: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "PARITY_OK");
}
