//! Actual router tests for the public native Customer Portal enrollment.
//!
//! Proves the exact Account-host dispatch, strict bodies, session cookie
//! transport, `{data}` envelope and business-error mapping. The owner rules are
//! proved separately in the enrollment unit/Postgres suites.
use erpx_server_customer_authority::public::portal_enrollment::{
    PortalEnrollmentHttpBackend, PortalEnrollmentHttpCommand, PortalEnrollmentHttpFuture,
    PortalEnrollmentHttpRequest, PortalEnrollmentHttpTarget, portal_enrollment_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
}

impl PortalEnrollmentHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: PortalEnrollmentHttpRequest<'a>,
    ) -> PortalEnrollmentHttpFuture<'a, serde_json::Value> {
        Box::pin(async move {
            let seen = match request.command {
                PortalEnrollmentHttpCommand::Reserve { token } => {
                    assert_eq!(request.host, "account.example.test");
                    format!("reserve|{token}")
                }
                PortalEnrollmentHttpCommand::Release { reservation } => {
                    format!(
                        "release|{}|{}",
                        reservation.invitation_id, reservation.window_start
                    )
                }
                PortalEnrollmentHttpCommand::Accept { token, session } => {
                    format!("accept|{token}|{session}")
                }
            };
            self.seen.lock().unwrap().push(seen);
            Ok(
                serde_json::json!({"bindingId": "binding-1", "holdingSlug": "acme", "accepted": true}),
            )
        })
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

fn post(path: &str, host: &str, cookie: Option<&str>, body: &str) -> String {
    let cookie = cookie
        .map(|value| format!("Cookie: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n{cookie}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[tokio::test]
async fn actual_router_dispatches_reserve_release_and_accept() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = portal_enrollment_http_router(vec![PortalEnrollmentHttpTarget {
        host: "account.example.test".into(),
        backend: Arc::new(Probe { seen: seen.clone() }),
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, headers) = http(
        addr,
        post(
            "/api/customer-portal/enroll/reserve",
            "account.example.test",
            None,
            r#"{"token":"signed.token"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["accepted"], true);
    assert!(headers.contains("cache-control: private, no-store"));

    let (status, _, _) = http(
        addr,
        post(
            "/api/customer-portal/enroll/release",
            "account.example.test",
            None,
            r#"{"reservation":{"holdingSlug":"acme","bindingId":"binding-1","invitationId":"invite-1","windowStart":1800000000000}}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);

    let (status, _, _) = http(
        addr,
        post(
            "/api/customer-portal/enroll/accept",
            "account.example.test",
            Some("erpx-account.session_token=session-token"),
            r#"{"token":"signed.token"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        seen.lock().unwrap().clone(),
        vec![
            "reserve|signed.token".to_string(),
            "release|invite-1|1800000000000".to_string(),
            "accept|signed.token|session-token".to_string(),
        ]
    );
}

#[tokio::test]
async fn actual_router_fails_closed_for_unknown_host_missing_session_and_bad_bodies() {
    let router = portal_enrollment_http_router(vec![PortalEnrollmentHttpTarget {
        host: "account.example.test".into(),
        backend: Arc::new(Probe::default()),
    }])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, _, _) = http(
        addr,
        post(
            "/api/customer-portal/enroll/reserve",
            "unknown.example.test",
            None,
            r#"{"token":"signed.token"}"#,
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, body, _) = http(
        addr,
        post(
            "/api/customer-portal/enroll/accept",
            "account.example.test",
            None,
            r#"{"token":"signed.token"}"#,
        ),
    )
    .await;
    assert_eq!(status, 401);
    assert_eq!(body["error"]["code"], "CUSTOMER_PORTAL_SESSION_REQUIRED");

    let (status, body, _) = http(
        addr,
        post(
            "/api/customer-portal/enroll/release",
            "account.example.test",
            None,
            r#"{"reservation":{"invitationId":"../escape","windowStart":1}}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(
        body["error"]["code"],
        "CUSTOMER_PORTAL_ENROLLMENT_REQUEST_INVALID"
    );

    let (status, _, _) = http(
        addr,
        post(
            "/api/customer-portal/enroll/reserve",
            "account.example.test",
            None,
            r#"{"token":""}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);
    let _ = BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.");
}
