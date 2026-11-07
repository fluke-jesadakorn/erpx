#[allow(
    dead_code,
    reason = "only the shared Account policy is needed by provider fixtures"
)]
#[path = "../../support/rust-account.rs"]
mod fixture;
use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use erpx_server_identity_access::public::{account_mail::*, *};
use std::{
    collections::{BTreeMap, VecDeque},
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    task::JoinHandle,
};

fn message() -> AccountEmailMessage {
    AccountMailPolicy::new(fixture::policy())
        .unwrap()
        .render(
            EmailAddress::new("staff@example.test").unwrap(),
            &OpenedAccountMail {
                kind: CredentialProofKind::PasswordReset,
                account_origin: "https://account.example.test".into(),
                return_path: "/reset-password".into(),
                token: OpaqueSecret::new("a".repeat(64)).unwrap(),
            },
            "synthetic-provider-outbox",
        )
        .unwrap()
}
struct HttpState {
    responses: Mutex<VecDeque<(u16, serde_json::Value)>>,
    calls: AtomicUsize,
    escaped: AtomicUsize,
    delivered: Mutex<BTreeMap<String, String>>,
}
struct HttpSink {
    address: SocketAddr,
    state: Arc<HttpState>,
    task: JoinHandle<()>,
}
impl Drop for HttpSink {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn http_sink(responses: Vec<(u16, serde_json::Value)>) -> HttpSink {
    let state = Arc::new(HttpState {
        responses: Mutex::new(responses.into()),
        calls: AtomicUsize::new(0),
        escaped: AtomicUsize::new(0),
        delivered: Mutex::new(BTreeMap::new()),
    });
    async fn receive(
        State(state): State<Arc<HttpState>>,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        state.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(
            headers["authorization"],
            "Bearer public-synthetic-provider-key"
        );
        let key = headers["idempotency-key"].to_str().unwrap().to_owned();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["from"], "ERPX <noreply@example.test>");
        assert_eq!(parsed["to"], serde_json::json!(["staff@example.test"]));
        assert_eq!(parsed["subject"], "Reset your ERPX password");
        assert!(
            parsed["text"]
                .as_str()
                .unwrap()
                .contains("/api/auth/reset-password/")
        );
        let mut delivered = state.delivered.lock().unwrap();
        if let Some(prior) = delivered.get(&key) {
            assert_eq!(prior, &parsed.to_string());
            return axum::Json(serde_json::json!({"id":"00000000-0000-4000-8000-000000000001"}))
                .into_response();
        }
        let (status, value) = state.responses.lock().unwrap().pop_front().unwrap_or((
            200,
            serde_json::json!({"id":"00000000-0000-4000-8000-000000000001"}),
        ));
        if status == 307 {
            return (
                StatusCode::TEMPORARY_REDIRECT,
                [("location", "/escaped")],
                "redirect",
            )
                .into_response();
        }
        if status == 200 && value.get("id").is_some() {
            delivered.insert(key, parsed.to_string());
        }
        (StatusCode::from_u16(status).unwrap(), axum::Json(value)).into_response()
    }
    async fn escaped(State(state): State<Arc<HttpState>>) -> StatusCode {
        state.escaped.fetch_add(1, Ordering::SeqCst);
        StatusCode::OK
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Router::new()
        .route("/emails", post(receive))
        .route("/escaped", post(escaped))
        .with_state(state.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    HttpSink {
        address,
        state,
        task,
    }
}
fn resend(sink: &HttpSink, key: &str) -> ResendAccountMail {
    ResendAccountMail::for_local_test_sink(
        "synthetic-profile",
        "ERPX <noreply@example.test>".into(),
        key.into(),
        sink.address,
    )
    .unwrap()
}

#[tokio::test]
async fn real_resend_adapter_sends_exact_request_and_reuses_original_provider_receipt() {
    let sink = http_sink(vec![]).await;
    let provider = resend(&sink, "public-synthetic-provider-key");
    let message = message();
    for _ in 0..2 {
        assert!(
            matches!(provider.send(&message).await,AccountMailSendOutcome::Accepted{provider_id} if provider_id=="00000000-0000-4000-8000-000000000001")
        );
    }
    assert_eq!(sink.state.calls.load(Ordering::SeqCst), 2);
    assert_eq!(sink.state.delivered.lock().unwrap().len(), 1);
    let changed = resend(&sink, "different-public-synthetic-key");
    assert_ne!(
        provider.profile().profile_id,
        changed.profile().profile_id,
        "provider key changes cannot silently reuse the original dispatch identity"
    );
}
#[tokio::test]
async fn real_resend_adapter_never_treats_missing_receipt_error_or_redirect_as_success() {
    let sink = http_sink(vec![
        (200, serde_json::json!({})),
        (429, serde_json::json!({"message":"synthetic limit"})),
        (500, serde_json::json!({"message":"synthetic unknown"})),
        (403, serde_json::json!({"message":"synthetic denied"})),
        (307, serde_json::json!({})),
    ])
    .await;
    let provider = resend(&sink, "public-synthetic-provider-key");
    let message = message();
    assert!(matches!(
        provider.send(&message).await,
        AccountMailSendOutcome::Uncertain(_)
    ));
    assert!(matches!(
        provider.send(&message).await,
        AccountMailSendOutcome::Retryable(AccountMailFailure::ProviderRateLimited)
    ));
    assert!(matches!(
        provider.send(&message).await,
        AccountMailSendOutcome::Uncertain(_)
    ));
    assert!(matches!(
        provider.send(&message).await,
        AccountMailSendOutcome::Rejected(_)
    ));
    assert!(matches!(
        provider.send(&message).await,
        AccountMailSendOutcome::Uncertain(_)
    ));
    assert_eq!(sink.state.calls.load(Ordering::SeqCst), 5);
    assert_eq!(sink.state.escaped.load(Ordering::SeqCst), 0);
}
struct SmtpRecord {
    commands: Vec<String>,
    messages: usize,
    body: Option<String>,
}
struct SmtpSink {
    address: SocketAddr,
    record: Arc<Mutex<SmtpRecord>>,
    task: JoinHandle<()>,
}
impl Drop for SmtpSink {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn smtp_sink(drop_after_data: bool) -> SmtpSink {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let record = Arc::new(Mutex::new(SmtpRecord {
        commands: vec![],
        messages: 0,
        body: None,
    }));
    let capture = record.clone();
    let task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = BufReader::new(socket);
        socket
            .get_mut()
            .write_all(b"220 synthetic.test ESMTP\r\n")
            .await
            .unwrap();
        let mut data = false;
        let mut body = String::new();
        loop {
            let mut line = String::new();
            if socket.read_line(&mut line).await.unwrap_or(0) == 0 {
                break;
            }
            if data {
                if line == ".\r\n" {
                    {
                        let mut record = capture.lock().unwrap();
                        record.messages += 1;
                        record.body = Some(body.clone());
                    }
                    if drop_after_data {
                        break;
                    }
                    socket
                        .get_mut()
                        .write_all(b"250 2.0.0 synthetic-accepted\r\n")
                        .await
                        .unwrap();
                    data = false;
                } else {
                    body.push_str(&line);
                    assert!(body.len() < 32_768);
                }
                continue;
            }
            capture.lock().unwrap().commands.push(line.trim().into());
            let response = if line.starts_with("EHLO ") {
                b"250-synthetic.test\r\n250 8BITMIME\r\n".as_slice()
            } else if line.starts_with("MAIL FROM:") || line.starts_with("RCPT TO:") {
                b"250 2.1.0 Ok\r\n".as_slice()
            } else if line.starts_with("DATA") {
                data = true;
                b"354 End data\r\n".as_slice()
            } else if line.starts_with("QUIT") {
                socket.get_mut().write_all(b"221 Bye\r\n").await.unwrap();
                break;
            } else {
                b"250 Ok\r\n".as_slice()
            };
            socket.get_mut().write_all(response).await.unwrap();
        }
    });
    SmtpSink {
        address,
        record,
        task,
    }
}
#[tokio::test]
async fn real_smtp_adapter_requires_positive_server_acceptance_and_marks_dropped_data_uncertain() {
    for drop_after in [false, true] {
        let sink = smtp_sink(drop_after).await;
        let provider = SmtpAccountMail::for_local_test_sink(
            "synthetic-smtp",
            "ERPX <noreply@example.test>".into(),
            sink.address,
        )
        .unwrap();
        let outcome = provider.send(&message()).await;
        assert_eq!(
            matches!(outcome, AccountMailSendOutcome::Accepted { .. }),
            !drop_after
        );
        if drop_after {
            assert!(matches!(outcome, AccountMailSendOutcome::Uncertain(_)));
        }
        let record = sink.record.lock().unwrap();
        assert_eq!(record.messages, 1);
        assert!(
            record
                .commands
                .iter()
                .any(|c| c == "RCPT TO:<staff@example.test>")
        );
        assert!(
            record
                .body
                .as_ref()
                .unwrap()
                .contains("Subject: Reset your ERPX password")
        );
    }
}
#[tokio::test]
async fn required_starttls_never_downgrades_or_exposes_auth_to_a_plaintext_sink() {
    let sink = smtp_sink(false).await;
    let provider = SmtpAccountMail::new(
        "synthetic-tls",
        "ERPX <noreply@example.test>".into(),
        "127.0.0.1".into(),
        sink.address.port(),
        AccountSmtpTls::RequiredStartTls,
        "synthetic-user".into(),
        "public-synthetic-password".into(),
    )
    .unwrap();
    assert!(!matches!(
        provider.send(&message()).await,
        AccountMailSendOutcome::Accepted { .. }
    ));
    let record = sink.record.lock().unwrap();
    assert_eq!(record.messages, 0);
    assert!(
        !record
            .commands
            .iter()
            .any(|c| c.starts_with("AUTH ") || c.starts_with("MAIL FROM:"))
    );
}
#[test]
fn local_sink_constructors_reject_external_and_protected_endpoints_and_header_injection() {
    for address in [
        "203.0.113.10:2525",
        "127.0.0.1:3000",
        "127.0.0.1:3101",
        "127.0.0.1:5432",
    ] {
        let address = address.parse().unwrap();
        assert!(
            ResendAccountMail::for_local_test_sink(
                "test",
                "ERPX <noreply@example.test>".into(),
                "public-synthetic-provider-key".into(),
                address
            )
            .is_err()
        );
        assert!(
            SmtpAccountMail::for_local_test_sink(
                "test",
                "ERPX <noreply@example.test>".into(),
                address
            )
            .is_err()
        );
    }
    assert!(
        ResendAccountMail::new(
            "test",
            "ERPX\r\nBcc: injected@example.test".into(),
            "public-synthetic-provider-key".into()
        )
        .is_err()
    );
}
