//! Native Quality active-product read: parity and fail-closed coverage for the
//! owner-local successor of `adapters/quality-product.ts`.

use erpx_commerce_server::public::quality_product_read::{
    QualityProductFacts, QualityProductRead, QualityProductReadFuture, QualityProductReadTarget,
    quality_product_read_router,
};
use erpx_platform_runtime_server::public::http_support::serve;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Probe {
    active: BTreeMap<(String, String), QualityProductFacts>,
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

impl QualityProductRead for Probe {
    fn current<'a>(
        &'a self,
        company: &'a str,
        product: &'a str,
    ) -> QualityProductReadFuture<'a, Option<QualityProductFacts>> {
        let seen = self.seen.clone();
        let active = self.active.clone();
        let company = company.to_owned();
        let product = product.to_owned();
        Box::pin(async move {
            seen.lock().unwrap().push((company.clone(), product.clone()));
            Ok(active.get(&(company, product)).cloned())
        })
    }
}

fn target() -> QualityProductReadTarget {
    QualityProductReadTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
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
    let router = quality_product_read_router(probe, vec![target()]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    (addr.port(), server)
}

fn request(_port: u16, host: &str, company: &str, product: &str) -> String {
    format!(
        "GET /api/v1/holdings/acme/commerce/quality-product HTTP/1.1\r\n\
         Host: {host}\r\n\
         Accept: application/json\r\n\
         Connection: close\r\n\
         x-erpx-context-company: {company}\r\n\
         x-erpx-context-product: {product}\r\n\r\n"
    )
}

#[tokio::test]
async fn active_product_parity() {
    let mut active = BTreeMap::new();
    active.insert(
        ("company".to_owned(), "product".to_owned()),
        QualityProductFacts {
            id: "product".into(),
            version: 7,
        },
    );
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (port, server) = serve_probe(Arc::new(Probe { active, seen: seen.clone() })).await;

    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request(port, "acme.example.test", "company", "product"),
    )
    .await;
    server.abort();
    assert_eq!(status, 200);
    assert_eq!(body["product"]["id"], "product");
    assert_eq!(body["product"]["version"], 7);
    assert_eq!(*seen.lock().unwrap(), vec![("company".to_owned(), "product".to_owned())]);
}

#[tokio::test]
async fn missing_product_is_null_not_authority() {
    let (port, server) = serve_probe(Arc::new(Probe {
        active: BTreeMap::new(),
        seen: Arc::new(Mutex::new(Vec::new())),
    }))
    .await;
    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request(port, "acme.example.test", "company", "missing"),
    )
    .await;
    server.abort();
    assert_eq!(status, 200);
    assert!(body["product"].is_null());
}

#[tokio::test]
async fn wrong_host_fails_closed() {
    let (port, server) = serve_probe(Arc::new(Probe {
        active: BTreeMap::new(),
        seen: Arc::new(Mutex::new(Vec::new())),
    }))
    .await;
    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request(port, "attacker.example.test", "company", "product"),
    )
    .await;
    server.abort();
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn invalid_company_fails_closed() {
    let (port, server) = serve_probe(Arc::new(Probe {
        active: BTreeMap::new(),
        seen: Arc::new(Mutex::new(Vec::new())),
    }))
    .await;
    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request(port, "acme.example.test", "bad company id!", "product"),
    )
    .await;
    server.abort();
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "QUALITY_PRODUCT_READ_REQUEST_INVALID");
}

#[test]
fn configuration_requires_exact_distinct_holdings() {
    let probe = Arc::new(Probe {
        active: BTreeMap::new(),
        seen: Arc::new(Mutex::new(Vec::new())),
    });
    assert!(quality_product_read_router(probe.clone(), vec![target()]).is_ok());
    assert!(quality_product_read_router(probe.clone(), vec![target(), target()]).is_err());
    assert!(quality_product_read_router(probe.clone(), vec![]).is_err());
}
