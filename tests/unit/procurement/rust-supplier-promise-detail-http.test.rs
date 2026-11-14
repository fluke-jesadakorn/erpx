//! HTTP mapping tests for the native Procurement supplier-promise detail read
//! on the retained Supply route. These prove bounded transport facts only;
//! the owner projection is tested in `rust-supplier-promise-detail.test.rs`.
use erpx_server_procurement::public::supplier_promise_command_http::{
    SupplierPromiseCommandHttpBackend, SupplierPromiseCommandHttpFuture,
    SupplierPromiseCommandHttpRequest, SupplierPromiseCommandHttpResponse,
    SupplierPromiseCommandHttpTarget, SupplierPromiseDetailHttpRequest,
    SupplierPromiseDetailHttpResponse, supplier_promise_command_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

fn detail() -> serde_json::Value {
    serde_json::json!({
        "order": {
            "id": "order-1",
            "company_id": "company",
            "branch_id": "branch",
            "department_id": null,
            "kind": "purchase",
            "number": "PO-1",
            "party_id": "supplier",
            "warehouse_id": "warehouse",
            "currency": "THB",
            "order_date": "2026-10-20",
            "due_date": "2026-11-10",
            "note": "",
            "state": "confirmed",
            "subtotal_minor": 15000,
            "tax_minor": 1050,
            "source_id": null,
            "version": 7,
            "created_by": "owner",
            "approved_by": "approver",
            "created_at": 10,
            "updated_at": 20,
            "price_list_id": null,
            "price_list_version": null,
            "discount_reason": null,
            "discount_approval_required": 0,
            "discount_approved_by": null,
            "discount_approved_at": null,
            "supplier_name": "Bounded supplier"
        },
        "lines": [{
            "id": "order-line-a",
            "order_id": "order-1",
            "product_id": "product",
            "description": "Bounded part",
            "quantity_milli": 10000,
            "fulfilled_milli": 4000,
            "price_minor": 100,
            "tax_bps": 700,
            "subtotal_minor": 10000,
            "tax_minor": 700,
            "ordinal": 0,
            "list_price_minor": null,
            "discount_bps": 0,
            "item_type": "stock",
            "sku": "SKU-A",
            "outstanding_milli": 6000,
            "unconfirmed_milli": 2000
        }],
        "history": [{
            "id": "promise-2",
            "order_id": "order-1",
            "company_id": "company",
            "order_version": 2,
            "supplier_id": "supplier",
            "currency": "THB",
            "required_date": "2026-11-10",
            "kind": "confirmed",
            "confirmation_date": "2026-11-01",
            "supplier_reference": "SUP-REF-1",
            "evidence": "Supplier confirmation evidence",
            "recorded_by": "owner",
            "created_at": 20
        }],
        "schedule": [{
            "id": "line-a1",
            "promise_id": "promise-2",
            "order_line_id": "order-line-a",
            "ordinal": 0,
            "quantity_milli": 4000,
            "fulfilled_snapshot_milli": 4000,
            "promised_date": "2026-11-08"
        }],
        "open": [],
        "canRecord": true,
        "purchase": {
            "newWork": true,
            "settleExisting": true,
            "history": true,
            "revision": "sha256:7"
        }
    })
}

#[derive(Default)]
struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: Option<BusinessError>,
    without_purchase: bool,
}

impl Probe {
    fn detail(&self) -> serde_json::Value {
        let mut value = detail();
        if self.without_purchase {
            value.as_object_mut().unwrap().remove("purchase");
        }
        value
    }
}

impl SupplierPromiseCommandHttpBackend for Probe {
    fn record<'a>(
        &'a self,
        _: SupplierPromiseCommandHttpRequest<'a>,
    ) -> SupplierPromiseCommandHttpFuture<'a, SupplierPromiseCommandHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }

    fn detail<'a>(
        &'a self,
        request: SupplierPromiseDetailHttpRequest<'a>,
    ) -> SupplierPromiseCommandHttpFuture<'a, SupplierPromiseDetailHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(request.order_id.into());
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(SupplierPromiseDetailHttpResponse {
                data: self.detail(),
                context: serde_json::json!({"userId": "user", "companyId": "company"}),
            })
        })
    }
}

const PATH: &str = "/api/v1/holdings/acme/supplier-promises/order-1";

async fn start(
    probes: Vec<(&str, Arc<Probe>)>,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let targets = probes
        .into_iter()
        .map(|(host, backend)| SupplierPromiseCommandHttpTarget {
            host: format!("{host}.example.test"),
            holding_slug: host.into(),
            origin: format!("https://{host}.example.test"),
            backend,
        })
        .collect();
    let router = supplier_promise_command_http_router(targets).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    (addr, server)
}

async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, serde_json::Value) {
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
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    (
        status,
        head.to_ascii_lowercase(),
        serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
    )
}

fn get(path: &str, host: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: {host}.example.test\r\nConnection: close\r\n\r\n")
}

#[tokio::test]
async fn detail_route_returns_the_retained_shape_without_reshaping() {
    let probe = Arc::new(Probe::default());
    let (addr, server) = start(vec![("acme", probe.clone())]).await;
    let (status, head, body) = http(addr, get(PATH, "acme")).await;
    assert_eq!(status, 200, "{body}");
    assert!(head.contains("cache-control: private, no-store"));
    assert!(head.contains("referrer-policy: no-referrer"));
    assert!(head.contains("x-content-type-options: nosniff"));
    assert_eq!(body["data"], detail());
    let mut keys = body["data"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["canRecord", "history", "lines", "open", "order", "purchase", "schedule"]
    );
    assert!(body["data"].get("order_id").is_none());
    assert_eq!(body["data"]["lines"][0]["outstanding_milli"], 6000);
    assert_eq!(body["data"]["lines"][0]["unconfirmed_milli"], 2000);
    assert_eq!(body["data"]["canRecord"], true);
    assert_eq!(body["data"]["purchase"]["settleExisting"], true);
    assert_eq!(body["context"]["companyId"], "company");
    assert_eq!(probe.seen.lock().unwrap().clone(), vec!["order-1"]);
    server.abort();
}

#[tokio::test]
async fn detail_route_omits_purchase_when_composition_has_no_package_overlay() {
    let probe = Arc::new(Probe {
        without_purchase: true,
        ..Probe::default()
    });
    let (addr, server) = start(vec![("acme", probe)]).await;
    let (status, _, body) = http(addr, get(PATH, "acme")).await;
    assert_eq!(status, 200, "{body}");
    assert!(body["data"].get("purchase").is_none());
    assert_eq!(body["data"]["canRecord"], true);
    server.abort();
}

#[tokio::test]
async fn detail_route_maps_owner_failures_and_bounds_the_request() {
    let failing = Arc::new(Probe {
        fail: Some(BusinessError::new(
            FailureKind::Conflict,
            "SUPPLIER_PROMISE_HISTORY_CHANGED",
            "Synthetic changed supplier history.",
        )),
        ..Probe::default()
    });
    let (addr, server) = start(vec![("acme", failing)]).await;

    let (status, _, body) = http(addr, get(&format!("{PATH}?view=all"), "acme")).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["error"]["code"], "SUPPLIER_PROMISE_COMMAND_INVALID");

    let (status, _, body) = http(addr, get(PATH, "acme")).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["error"]["code"], "SUPPLIER_PROMISE_HISTORY_CHANGED");
    server.abort();
}
