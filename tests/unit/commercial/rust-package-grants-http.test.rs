use erpx_server_commercial::public::purchases::{
    PackageGrantHttpFuture, PackageGrantHttpRequest, PackageGrantHttpResponse,
    PackageGrantsHttpBackend, PackageGrantsHttpTarget, package_grants_router,
};
use erpx_server_shared::public::BusinessError;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Probe {
    seen: Mutex<Vec<String>>,
}

impl PackageGrantsHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: PackageGrantHttpRequest<'a>,
    ) -> PackageGrantHttpFuture<'a, PackageGrantHttpResponse> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap()
                .push(format!("{:?}", request.command));
            Ok(PackageGrantHttpResponse {
                data: serde_json::json!({"echo": format!("{:?}", request.command)}),
                context: serde_json::json!({"userId": "operator", "holdingId": "h001"}),
            })
        })
    }
}

#[derive(Default)]
struct Failing;

impl PackageGrantsHttpBackend for Failing {
    fn execute<'a>(
        &'a self,
        _: PackageGrantHttpRequest<'a>,
    ) -> PackageGrantHttpFuture<'a, PackageGrantHttpResponse> {
        Box::pin(async move {
            Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "PACKAGE_REVIEW_EXPIRED",
                "Preview the current Company again.",
            ))
        })
    }
}

#[derive(Default)]
struct Dependency;

impl PackageGrantsHttpBackend for Dependency {
    fn execute<'a>(
        &'a self,
        _: PackageGrantHttpRequest<'a>,
    ) -> PackageGrantHttpFuture<'a, PackageGrantHttpResponse> {
        Box::pin(async move {
            Err(BusinessError::invalid(
                "PACKAGE_DEPENDENCY_REQUIRED",
                "The package grant change is invalid.",
            ))
        })
    }
}

fn target(backend: Arc<dyn PackageGrantsHttpBackend>) -> PackageGrantsHttpTarget {
    PackageGrantsHttpTarget {
        host: "erpx-internal.example.test".into(),
        origin: "https://erpx-internal.example.test".into(),
        backend,
    }
}

fn grant() -> serde_json::Value {
    serde_json::json!({
        "companyId": "company",
        "packageId": "legal",
        "packageVersion": 1,
        "state": "active",
        "startsAt": 0,
        "endsAt": null,
        "version": 2,
        "reason": "Reviewed change",
        "evidence": "Reviewed owner evidence",
    })
}

fn change() -> serde_json::Value {
    serde_json::json!({
        "grant": grant(),
        "expectedVersion": 1,
        "idempotencyKey": "commercial-http-key-0001",
    })
}

fn batch() -> serde_json::Value {
    serde_json::json!({
        "companyId": "company",
        "expectedRevision": "a".repeat(64),
        "idempotencyKey": "commercial-http-key-0001",
        "changes": [{"expectedVersion": 1, "grant": grant()}],
    })
}

#[test]
fn target_configuration_requires_exact_distinct_https_hosts() {
    let backend: Arc<dyn PackageGrantsHttpBackend> = Arc::new(Probe::default());
    assert!(package_grants_router(vec![target(backend.clone())]).is_ok());
    assert!(package_grants_router(vec![target(backend.clone()), target(backend.clone())]).is_err());
    assert!(
        package_grants_router(vec![PackageGrantsHttpTarget {
            origin: "http://erpx-internal.example.test".into(),
            ..target(backend.clone())
        }])
        .is_err()
    );
    assert!(
        package_grants_router(vec![PackageGrantsHttpTarget {
            host: "other.example.test".into(),
            ..target(backend)
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
    let text = String::from_utf8_lossy(&bytes).into_owned();
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

fn post(host: &str, path: &str, origin: &str, key: Option<&str>, mime: &str, body: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nContent-Type: {mime}\r\n{}Connection: close\r\nContent-Length: {}\r\n\r\n{body}",
        key.map(|key| format!("Idempotency-Key: {key}\r\n"))
            .unwrap_or_default(),
        body.len()
    )
}

#[tokio::test]
async fn actual_router_retains_host_origin_key_body_and_business_errors() {
    let probe = Arc::new(Probe::default());
    let router = package_grants_router(vec![
        target(probe.clone()),
        PackageGrantsHttpTarget {
            host: "fail.example.test".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Failing),
        },
        PackageGrantsHttpTarget {
            host: "dependency.example.test".into(),
            origin: "https://dependency.example.test".into(),
            backend: Arc::new(Dependency),
        },
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
    });

    let preview = serde_json::json!({"holdingId": "h001", "request": change()}).to_string();
    let (status, value, raw) = http(
        addr,
        post(
            "erpx-internal.example.test",
            "/api/v1/platform/commercial/package-grants/preview",
            "https://erpx-internal.example.test",
            None,
            "application/json",
            &preview,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert!(value["data"]["echo"].as_str().unwrap().contains("Preview"));
    assert_eq!(value["context"]["userId"], "operator");
    let headers = raw.to_ascii_lowercase();
    assert_eq!(
        headers.matches("cache-control: private, no-store").count(),
        1
    );
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    let (status, value, _) = http(
        addr,
        post(
            "erpx-internal.example.test",
            "/api/v1/platform/commercial/package-grants/preview",
            "https://evil.example.test",
            None,
            "application/json",
            &preview,
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(value["error"]["code"], "ORIGIN_FORBIDDEN");

    let (status, value, _) = http(
        addr,
        post(
            "erpx-internal.example.test",
            "/api/v1/platform/commercial/package-grants/preview",
            "https://erpx-internal.example.test",
            None,
            "text/plain",
            &preview,
        ),
    )
    .await;
    assert_eq!(status, 415);
    assert_eq!(value["error"]["code"], "JSON_REQUIRED");

    let apply = serde_json::json!({"holdingId": "h001", "request": change()}).to_string();
    let (status, value, _) = http(
        addr,
        post(
            "erpx-internal.example.test",
            "/api/v1/platform/commercial/package-grants/apply",
            "https://erpx-internal.example.test",
            None,
            "application/json",
            &apply,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "IDEMPOTENCY_KEY_REQUIRED");

    let (status, value, _) = http(
        addr,
        post(
            "erpx-internal.example.test",
            "/api/v1/platform/commercial/package-grants/apply",
            "https://erpx-internal.example.test",
            Some("commercial-http-key-0002"),
            "application/json",
            &apply,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "IDEMPOTENCY_KEY_REQUIRED");

    let (status, value, _) = http(
        addr,
        post(
            "erpx-internal.example.test",
            "/api/v1/platform/commercial/package-grants/apply",
            "https://erpx-internal.example.test",
            Some("commercial-http-key-0001"),
            "application/json",
            &apply,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert!(value["data"]["echo"].as_str().unwrap().contains("Apply("));

    let (status, value, _) = http(
        addr,
        post(
            "erpx-internal.example.test",
            "/api/v1/platform/commercial/package-grants/batches/apply",
            "https://erpx-internal.example.test",
            Some("commercial-http-key-0001"),
            "application/json",
            &serde_json::json!({"holdingId": "h001", "request": batch(), "token": "review-token"})
                .to_string(),
        ),
    )
    .await;
    assert_eq!(status, 200);
    let seen = value["data"]["echo"].as_str().unwrap();
    assert!(seen.contains("ApplyBatch"));
    assert!(seen.contains("review-token"));

    let (status, value, _) = http(
        addr,
        post(
            "erpx-internal.example.test",
            "/api/v1/platform/commercial/package-grants/batches/preview",
            "https://erpx-internal.example.test",
            None,
            "application/json",
            &serde_json::json!({"holdingId": "h001", "request": batch(), "extra": true})
                .to_string(),
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "VALIDATION_FAILED");

    let (status, value, _) = http(
        addr,
        post(
            "fail.example.test",
            "/api/v1/platform/commercial/package-grants/batches/apply",
            "https://fail.example.test",
            Some("commercial-http-key-0001"),
            "application/json",
            &serde_json::json!({"holdingId": "h001", "request": batch(), "token": "review-token"})
                .to_string(),
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(value["error"]["code"], "PACKAGE_REVIEW_EXPIRED");

    let (status, value, _) = http(
        addr,
        post(
            "dependency.example.test",
            "/api/v1/platform/commercial/package-grants/batches/preview",
            "https://dependency.example.test",
            None,
            "application/json",
            &serde_json::json!({"holdingId": "h001", "request": batch()}).to_string(),
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(value["error"]["code"], "PACKAGE_DEPENDENCY_REQUIRED");

    let (status, _, _) = http(
        addr,
        post(
            "erpx-internal.example.test",
            "/api/v1/platform/commercial/legacy-grants/preview",
            "https://erpx-internal.example.test",
            None,
            "application/json",
            &preview,
        ),
    )
    .await;
    assert_eq!(status, 404);
}
