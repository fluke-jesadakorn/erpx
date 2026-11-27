//! Native Quality receiving-origin read: parity and fail-closed coverage for
//! the owner-local successor of the receipt half of
//! `adapters/quality-purchase-sources.ts`.

use erpx_server_commerce::public::quality_receipt_origin_read::{
    QualityReceiptOriginFacts, QualityReceiptOriginRead, QualityReceiptOriginReadFuture,
    QualityReceiptOriginReadTarget, quality_receipt_origin_read_router,
    quality_receipt_origin_read_router_for_targets,
};
use erpx_server_runtime::public::http_support::serve;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

type Key = (String, Option<String>, String);

#[derive(Clone)]
struct Probe {
    origins: BTreeMap<Key, QualityReceiptOriginFacts>,
    seen: Arc<Mutex<Vec<Key>>>,
}

impl QualityReceiptOriginRead for Probe {
    fn inspect<'a>(
        &'a self,
        company: &'a str,
        branch: Option<&'a str>,
        line: &'a str,
    ) -> QualityReceiptOriginReadFuture<'a, Option<QualityReceiptOriginFacts>> {
        let seen = self.seen.clone();
        let origins = self.origins.clone();
        let key = (
            company.to_owned(),
            branch.map(str::to_owned),
            line.to_owned(),
        );
        Box::pin(async move {
            seen.lock().unwrap().push(key.clone());
            Ok(origins.get(&key).cloned())
        })
    }
}

fn target() -> QualityReceiptOriginReadTarget {
    QualityReceiptOriginReadTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
    }
}

fn facts() -> QualityReceiptOriginFacts {
    QualityReceiptOriginFacts {
        order_id: "order".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        warehouse_id: "warehouse".into(),
        product_id: "product".into(),
        fulfillment_line_id: "line".into(),
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
    (status, serde_json::from_str(body).unwrap_or(serde_json::Value::Null))
}

async fn serve_probe(
    probe: Arc<Probe>,
) -> (u16, tokio::task::JoinHandle<()>) {
    let router = quality_receipt_origin_read_router(probe, vec![target()]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    (addr.port(), server)
}

fn request(host: &str, company: &str, branch: Option<&str>, line: &str) -> String {
    request_for(host, "acme", company, branch, line)
}

fn request_for(
    host: &str,
    slug: &str,
    company: &str,
    branch: Option<&str>,
    line: &str,
) -> String {
    let branch_line = branch
        .map(|value| format!("x-erpx-context-branch: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "GET /api/v1/holdings/{slug}/commerce/quality-receipt-origin HTTP/1.1\r\n\
         Host: {host}\r\n\
         Accept: application/json\r\n\
         Connection: close\r\n\
         x-erpx-context-company: {company}\r\n\
         {branch_line}\
         x-erpx-context-fulfillment-line: {line}\r\n\r\n"
    )
}

#[tokio::test]
async fn origin_parity() {
    let mut origins = BTreeMap::new();
    origins.insert(
        (
            "company".to_owned(),
            Some("branch".to_owned()),
            "line".to_owned(),
        ),
        facts(),
    );
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (port, server) = serve_probe(Arc::new(Probe {
        origins,
        seen: seen.clone(),
    }))
    .await;

    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request("acme.example.test", "company", Some("branch"), "line"),
    )
    .await;
    server.abort();
    assert_eq!(status, 200);
    assert_eq!(body["origin"]["orderId"], "order");
    assert_eq!(body["origin"]["companyId"], "company");
    assert_eq!(body["origin"]["branchId"], "branch");
    assert_eq!(body["origin"]["warehouseId"], "warehouse");
    assert_eq!(body["origin"]["productId"], "product");
    assert_eq!(body["origin"]["fulfillmentLineId"], "line");
    assert_eq!(
        *seen.lock().unwrap(),
        vec![(
            "company".to_owned(),
            Some("branch".to_owned()),
            "line".to_owned()
        )]
    );
}

#[tokio::test]
async fn missing_origin_is_null_not_authority() {
    let (port, server) = serve_probe(Arc::new(Probe {
        origins: BTreeMap::new(),
        seen: Arc::new(Mutex::new(Vec::new())),
    }))
    .await;
    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request("acme.example.test", "company", None, "missing"),
    )
    .await;
    server.abort();
    assert_eq!(status, 200);
    assert!(body["origin"].is_null());
}

#[tokio::test]
async fn wrong_host_fails_closed() {
    let (port, server) = serve_probe(Arc::new(Probe {
        origins: BTreeMap::new(),
        seen: Arc::new(Mutex::new(Vec::new())),
    }))
    .await;
    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request("attacker.example.test", "company", None, "line"),
    )
    .await;
    server.abort();
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn invalid_company_fails_closed() {
    let (port, server) = serve_probe(Arc::new(Probe {
        origins: BTreeMap::new(),
        seen: Arc::new(Mutex::new(Vec::new())),
    }))
    .await;
    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request("acme.example.test", "bad company id!", None, "line"),
    )
    .await;
    server.abort();
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "QUALITY_RECEIPT_ORIGIN_READ_REQUEST_INVALID");
}

#[tokio::test]
async fn invalid_line_fails_closed() {
    let (port, server) = serve_probe(Arc::new(Probe {
        origins: BTreeMap::new(),
        seen: Arc::new(Mutex::new(Vec::new())),
    }))
    .await;
    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request("acme.example.test", "company", None, "bad line id!"),
    )
    .await;
    server.abort();
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "QUALITY_RECEIPT_ORIGIN_READ_REQUEST_INVALID");
}

#[tokio::test]
async fn same_path_on_distinct_holding_hosts_uses_distinct_readers() {
    let key = (
        "company".to_owned(),
        Some("branch".to_owned()),
        "line".to_owned(),
    );

    let acme_seen = Arc::new(Mutex::new(Vec::new()));
    let mut acme_origins = BTreeMap::new();
    let mut acme_facts = facts();
    acme_facts.order_id = "order-acme".into();
    acme_origins.insert(key.clone(), acme_facts);
    let acme: Arc<dyn QualityReceiptOriginRead> = Arc::new(Probe {
        origins: acme_origins,
        seen: acme_seen.clone(),
    });

    let northstar_seen = Arc::new(Mutex::new(Vec::new()));
    let mut northstar_origins = BTreeMap::new();
    let mut northstar_facts = facts();
    northstar_facts.order_id = "order-northstar".into();
    northstar_origins.insert(key.clone(), northstar_facts);
    let northstar: Arc<dyn QualityReceiptOriginRead> = Arc::new(Probe {
        origins: northstar_origins,
        seen: northstar_seen.clone(),
    });

    let target = |host: &str| QualityReceiptOriginReadTarget {
        host: host.into(),
        holding_slug: "acme".into(),
    };
    let router = quality_receipt_origin_read_router_for_targets(vec![
        (target("acme.example.test"), acme),
        (target("northstar.example.test"), northstar),
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], listener.local_addr().unwrap().port()));
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body) = http(
        addr,
        request("acme.example.test", "company", Some("branch"), "line"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["origin"]["orderId"], "order-acme");

    let (status, body) = http(
        addr,
        request("northstar.example.test", "company", Some("branch"), "line"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["origin"]["orderId"], "order-northstar");

    assert_eq!(*acme_seen.lock().unwrap(), vec![key.clone()]);
    assert_eq!(*northstar_seen.lock().unwrap(), vec![key]);

    let (status, body) = http(
        addr,
        request("attacker.example.test", "company", Some("branch"), "line"),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body) = http(
        addr,
        request_for(
            "acme.example.test",
            "other",
            "company",
            Some("branch"),
            "line",
        ),
    )
    .await;
    server.abort();
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[test]
fn configuration_requires_exact_distinct_holdings() {
    let probe = Arc::new(Probe {
        origins: BTreeMap::new(),
        seen: Arc::new(Mutex::new(Vec::new())),
    });
    assert!(quality_receipt_origin_read_router(probe.clone(), vec![target()]).is_ok());
    assert!(quality_receipt_origin_read_router(probe.clone(), vec![target(), target()]).is_err());
    assert!(quality_receipt_origin_read_router(probe.clone(), vec![]).is_err());
    assert!(quality_receipt_origin_read_router_for_targets(vec![(target(), probe.clone())]).is_ok());
    assert!(quality_receipt_origin_read_router_for_targets(vec![
        (target(), probe.clone()),
        (target(), probe.clone()),
    ])
    .is_err());
    assert!(quality_receipt_origin_read_router_for_targets(vec![
        (target(), probe.clone()),
        (
            QualityReceiptOriginReadTarget {
                host: "northstar.example.test".into(),
                holding_slug: "acme".into(),
            },
            probe.clone(),
        ),
    ])
    .is_ok());
    assert!(quality_receipt_origin_read_router_for_targets(vec![]).is_err());
}
