//! Native payroll run HTTP ingress tests: target selection, exact origin,
//! Idempotency-Key, route dispatch and business-error mapping.
use erpx_server_people_payroll::public::payroll_run_http::{
    PayrollCommandHttpBackend, PayrollCommandHttpFuture, PayrollCommandHttpKind,
    PayrollCommandHttpRequest, PayrollCommandHttpResponse, PayrollCommandHttpTarget,
    people_payroll_command_router,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::sync::{Arc, Mutex};

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl PayrollCommandHttpBackend for Probe {
    fn command<'a>(
        &'a self,
        request: PayrollCommandHttpRequest<'a>,
    ) -> PayrollCommandHttpFuture<'a, PayrollCommandHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "{}:{:?}",
                request.key.map(CommandKey::as_str).unwrap_or("-"),
                request.kind
            ));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "PAYROLL_RUN_CHANGED",
                    "Synthetic changed payroll run.",
                ));
            }
            Ok(PayrollCommandHttpResponse {
                data: serde_json::json!({"id": request.kind.run_id().unwrap_or("run")}),
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let text = String::from_utf8_lossy(&bytes);
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
    )
}

#[tokio::test]
async fn router_retains_target_origin_key_paths_and_error_mapping() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let backend = Arc::new(Probe {
        seen: seen.clone(),
        fail: false,
    });
    let failing = Arc::new(Probe {
        seen: seen.clone(),
        fail: true,
    });
    let router = people_payroll_command_router(vec![
        PayrollCommandHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        },
        PayrollCommandHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: failing,
        },
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
    });

    let body = r#"{"payGroupId":"group","periodStart":"2026-10-01","periodEnd":"2026-10-31","payDate":"2026-10-31","currency":"THB","mode":"preview"}"#;
    let request = |host: &str, origin: &str, key: &str, body: &str| {
        format!(
            "POST /api/v1/holdings/acme/people/payroll/runs HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nIdempotency-Key: {key}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    };
    let (status, response) = http(
        addr,
        request(
            "acme.example.test",
            "https://acme.example.test",
            "command-key-1",
            body,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(response["data"]["id"], "run");
    let (status, response) = http(
        addr,
        request(
            "acme.example.test",
            "https://other.example.test",
            "command-key-2",
            body,
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(response["error"]["code"], "ORIGIN_FORBIDDEN");
    let (status, response) = http(
        addr,
        format!(
            "POST /api/v1/holdings/acme/people/payroll/runs HTTP/1.1\r\nHost: acme.example.test\r\nOrigin: https://acme.example.test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(response["error"]["code"], "COMMAND_KEY_REQUIRED");
    let (status, _) = http(
        addr,
        "GET /api/v1/holdings/acme/people/payroll/runs/run HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert_eq!(status, 200);
    let (status, response) = http(
        addr,
        format!(
            "POST /api/v1/holdings/fail/people/payroll/runs/run/post HTTP/1.1\r\nHost: fail.example.test\r\nOrigin: https://fail.example.test\r\nIdempotency-Key: post-key-1\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(response["error"]["code"], "PAYROLL_RUN_CHANGED");
    let seen = seen.lock().unwrap().clone();
    assert!(seen.iter().any(|value| value == "command-key-1:Create"));
    assert!(
        seen.iter()
            .any(|value| value == "post-key-1:Post { run_id: \"run\" }")
    );
    assert!(
        seen.iter()
            .any(|value| value == "-:Detail { run_id: \"run\" }")
    );
    assert!(matches!(
        PayrollCommandHttpKind::Calculate {
            run_id: "run".into()
        },
        PayrollCommandHttpKind::Calculate { .. }
    ));
    let _ = server.abort();
}
