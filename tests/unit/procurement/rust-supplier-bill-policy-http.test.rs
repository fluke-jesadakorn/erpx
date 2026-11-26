//! HTTP transport tests for the native Procurement supplier-invoice policy read.
//! These prove target/origin isolation, the retained response envelope and the
//! closed error mapping; the owner payload is projected in the crate unit tests.
//! Register as a `[[test]]` target in packages/server-procurement/Cargo.toml.
use erpx_server_procurement::public::supplier_bill_policy_read::{
    SupplierBillPolicyHttpBackend, SupplierBillPolicyHttpFuture, SupplierBillPolicyHttpRequest,
    SupplierBillPolicyHttpResponse, SupplierBillPolicyHttpTarget, supplier_bill_policy_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::Arc;

fn policy() -> serde_json::Value {
    serde_json::json!({
        "matchingMode": "required",
        "version": 2,
        "updatedByName": "Approver Name",
        "updatedAt": 50,
        "canManage": true,
        "events": [{
            "id": "policy-event-2",
            "company_id": "company",
            "version": 2,
            "previous_mode": "optional",
            "matching_mode": "required",
            "actor_id": "approver",
            "reason": "Require supplier matches",
            "created_at": 50,
            "actor_name": "Approver Name"
        }],
        "companyId": "company",
        "purchase": {
            "newWork": true,
            "settleExisting": true,
            "history": true,
            "revision": "sha256:7"
        }
    })
}

struct Probe;

impl SupplierBillPolicyHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: SupplierBillPolicyHttpRequest<'a>,
    ) -> SupplierBillPolicyHttpFuture<'a, SupplierBillPolicyHttpResponse> {
        let slug = request.holding_slug.to_owned();
        Box::pin(async move {
            if slug == "fail" {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "PERMISSION_REQUIRED",
                    "Synthetic permission failure.",
                ));
            }
            Ok(SupplierBillPolicyHttpResponse {
                data: policy(),
                context: serde_json::json!({"userId": "user"}),
            })
        })
    }
}

fn target(host: &str, slug: &str) -> SupplierBillPolicyHttpTarget {
    SupplierBillPolicyHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: format!("https://{host}"),
        backend: Arc::new(Probe),
    }
}

#[test]
fn router_requires_exact_distinct_registered_holdings() {
    assert!(supplier_bill_policy_http_router(vec![target("acme.example.test", "acme")]).is_ok());
    assert!(supplier_bill_policy_http_router(vec![
        target("acme.example.test", "acme"),
        target("acme.example.test", "acme"),
    ])
    .is_err());
    let mut bad = target("acme.example.test", "acme");
    bad.origin = "http://acme.example.test".into();
    assert!(supplier_bill_policy_http_router(vec![bad]).is_err());
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
async fn actual_router_serves_the_retained_policy_envelope() {
    let router = supplier_bill_policy_http_router(vec![
        target("acme.example.test", "acme"),
        target("fail.example.test", "fail"),
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body) = http(
        addr,
        "GET /api/v1/holdings/acme/supplier-bills/policy HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n"
            .into(),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["matchingMode"], "required");
    assert_eq!(body["data"]["companyId"], "company");
    assert_eq!(body["data"]["events"][0]["previous_mode"], "optional");
    assert_eq!(body["context"]["userId"], "user");

    let (status, body) = http(
        addr,
        "GET /api/v1/holdings/acme/supplier-bills/policy?x=1 HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n"
            .into(),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "PROCUREMENT_REQUEST_INVALID");

    let (status, body) = http(
        addr,
        "GET /api/v1/holdings/acme/supplier-bills/policy HTTP/1.1\r\nHost: unknown.example.test\r\nConnection: close\r\n\r\n"
            .into(),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body) = http(
        addr,
        "GET /api/v1/holdings/fail/supplier-bills/policy HTTP/1.1\r\nHost: fail.example.test\r\nConnection: close\r\n\r\n"
            .into(),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "PERMISSION_REQUIRED");

    server.abort();
}
