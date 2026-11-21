//! Owner-local HTTP adapter tests for the native Billing invoice lifecycle.
//! The exact owner dispatch is exercised without opening a socket; mounting the
//! router into composition remains a lead integration step.
use erpx_server_billing::public::invoice::BillingInvoiceCommand;
use erpx_server_billing::public::invoice_http::{
    BillingInvoiceHttpBackend, BillingInvoiceHttpFuture, BillingInvoiceHttpRequest,
    BillingInvoiceHttpResponse, BillingInvoiceHttpState, BillingInvoiceHttpTarget,
    billing_invoice_router,
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
    data: Option<serde_json::Value>,
}

fn describe(command: &BillingInvoiceCommand) -> String {
    match command {
        BillingInvoiceCommand::Create(command) => format!(
            "create|{}|{}|{}",
            command.key.as_str(),
            command.period_id.as_str(),
            command.finalize
        ),
        BillingInvoiceCommand::Update(command) => {
            format!(
                "update|{}|{}",
                command.key.as_str(),
                command.invoice_id.as_str()
            )
        }
        BillingInvoiceCommand::Finalize(command) => format!(
            "finalize|{}|{}",
            command.key.as_str(),
            command.invoice_id.as_str()
        ),
        BillingInvoiceCommand::Void(command) => {
            format!(
                "void|{}|{}",
                command.key.as_str(),
                command.invoice_id.as_str()
            )
        }
    }
}

impl BillingInvoiceHttpBackend for Probe {
    fn command<'a>(
        &'a self,
        request: BillingInvoiceHttpRequest<'a>,
    ) -> BillingInvoiceHttpFuture<'a, BillingInvoiceHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.command.expected_version(),
                describe(&request.command)
            ));
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(BillingInvoiceHttpResponse {
                data: self.data.clone().unwrap_or_else(|| json!({"ok":true})),
                context: json!({"userId":"admin"}),
            })
        })
    }
}

fn target(backend: Arc<dyn BillingInvoiceHttpBackend>) -> BillingInvoiceHttpTarget {
    BillingInvoiceHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

fn state(
    fail: Option<BusinessError>,
    data: Option<serde_json::Value>,
) -> (BillingInvoiceHttpState, Arc<Probe>) {
    let probe = Arc::new(Probe {
        seen: Arc::new(Mutex::new(Vec::new())),
        fail,
        data,
    });
    let state = BillingInvoiceHttpState::new(vec![target(probe.clone())]).unwrap();
    (state, probe)
}

fn headers(overrides: &[(&'static str, &'static str)]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("host", "acme.example.test"),
        ("content-type", "application/json"),
        ("idempotency-key", "billing-invoice-key-0001"),
        ("origin", "https://acme.example.test"),
        ("cookie", "__Host-erpx-product=session"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    for (name, value) in overrides {
        if value.is_empty() {
            headers.remove(*name);
        } else {
            headers.insert(*name, HeaderValue::from_static(value));
        }
    }
    headers
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

const CREATE: &[u8] =
    br#"{"action":"approve","version":4,"reason":"Approve the reviewed October costs","periodId":"period-10"}"#;
const CREATE_DRAFT: &[u8] = br#"{"action":"create-invoice","version":4,"reason":"Create the reviewed draft","periodId":"period-10","finalize":false}"#;
const CHANGE: &[u8] = br#"{"version":4,"reason":"Review the current invoice"}"#;

#[test]
fn create_accepts_the_retained_approve_body_and_forwards_the_key() {
    let (state, probe) = state(None, None);
    let response = block_on(state.create(&headers(&[]), "acme", None, CREATE));
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store"
    );
    assert_eq!(
        probe.seen.lock().unwrap().as_slice(),
        ["acme.example.test|acme|4|create|billing-invoice-key-0001|period-10|true"]
    );
    let body = block_on(to_bytes(response.into_body(), 8 * 1024)).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        value,
        json!({"data":{"ok":true},"context":{"userId":"admin"}})
    );
}

#[test]
fn create_can_leave_an_explicit_draft() {
    let (state, probe) = state(None, None);
    let response = block_on(state.create(&headers(&[]), "acme", None, CREATE_DRAFT));
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        probe.seen.lock().unwrap().as_slice(),
        ["acme.example.test|acme|4|create|billing-invoice-key-0001|period-10|false"]
    );
}

#[test]
fn update_finalize_and_void_forward_the_exact_path_invoice() {
    let invoice = "billing-invoice-key-0001";
    for (kind, expected) in [
        ("update", "update"),
        ("finalize", "finalize"),
        ("void", "void"),
    ] {
        let (state, probe) = state(None, None);
        let response = match kind {
            "update" => block_on(state.update(&headers(&[]), "acme", invoice, None, CHANGE)),
            "finalize" => block_on(state.finalize(&headers(&[]), "acme", invoice, None, CHANGE)),
            _ => block_on(state.void(&headers(&[]), "acme", invoice, None, CHANGE)),
        };
        assert_eq!(response.status(), StatusCode::OK, "{kind}");
        assert_eq!(
            probe.seen.lock().unwrap().as_slice(),
            [format!(
                "acme.example.test|acme|4|{expected}|billing-invoice-key-0001|{invoice}"
            )]
        );
    }
}

#[test]
fn body_invoice_id_must_match_the_path() {
    let (state, _) = state(None, None);
    let body =
        br#"{"version":4,"reason":"Review the current invoice","invoiceId":"invoice-other"}"#;
    let response = block_on(state.void(&headers(&[]), "acme", "invoice-1", None, body));
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn rejects_wrong_origin_holding_key_content_type_query_and_action() {
    let (state, _) = state(None, None);
    for (overrides, slug, query, body) in [
        (
            vec![("origin", "https://evil.test")],
            "acme",
            None,
            CREATE,
        ),
        (vec![], "other", None, CREATE),
        (vec![("idempotency-key", "")], "acme", None, CREATE),
        (vec![("content-type", "text/plain")], "acme", None, CREATE),
        (vec![], "acme", Some("force=1"), CREATE),
        (
            vec![],
            "acme",
            None,
            br#"{"action":"enroll","version":4,"reason":"Approve the reviewed October costs","periodId":"period-10"}"#
                as &[u8],
        ),
        (
            vec![],
            "acme",
            None,
            br#"{"version":4,"reason":"Approve the reviewed October costs","periodId":"period-10","extra":true}"#,
        ),
        (
            vec![],
            "acme",
            None,
            br#"{"version":4,"reason":"Create the reviewed draft","periodId":"period-10","finalize":"yes"}"#,
        ),
    ] {
        let response = block_on(state.create(&headers(&overrides), slug, query, body));
        assert!(
            response.status().is_client_error(),
            "{overrides:?} {slug} {query:?}"
        );
    }
}

#[test]
fn rejects_bodies_over_the_transport_bound() {
    let (state, _) = state(None, None);
    let mut body = CREATE.to_vec();
    body.resize(8 * 1024 + 1, b' ');
    let response = block_on(state.create(&headers(&[]), "acme", None, &body));
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn maps_business_failures_to_the_retained_error_envelope() {
    for (kind, expected) in [
        (FailureKind::Conflict, StatusCode::CONFLICT),
        (FailureKind::Forbidden, StatusCode::FORBIDDEN),
        (FailureKind::NotFound, StatusCode::NOT_FOUND),
        (FailureKind::Unavailable, StatusCode::SERVICE_UNAVAILABLE),
    ] {
        let (state, _) = state(
            Some(BusinessError::new(
                kind,
                "INVOICE_TEST",
                "Review the invoice.",
            )),
            None,
        );
        let response = block_on(state.create(&headers(&[]), "acme", None, CREATE));
        assert_eq!(response.status(), expected);
        let body = block_on(to_bytes(response.into_body(), 8 * 1024)).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            value,
            json!({"error":{"code":"INVOICE_TEST","message":"Review the invoice."}})
        );
    }
}

#[test]
fn an_unexpected_backend_payload_never_becomes_a_success() {
    let (state, _) = state(None, Some(json!({"ok": false})));
    let response = block_on(state.create(&headers(&[]), "acme", None, CREATE));
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn router_and_configuration_reject_duplicate_targets() {
    let probe = Arc::new(Probe {
        seen: Arc::new(Mutex::new(Vec::new())),
        fail: None,
        data: None,
    });
    let duplicate = || BillingInvoiceHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: probe.clone(),
    };
    assert!(billing_invoice_router(vec![duplicate(), duplicate()]).is_err());
    assert!(billing_invoice_router(vec![duplicate()]).is_ok());
    assert!(BillingInvoiceHttpState::new(Vec::new()).is_ok());
}
