//! Native Commerce currency read: parity and fail-closed coverage for the
//! owner-local successor of `adapters/records-request-page-owned-facts.ts`.

use erpx_server_commerce::public::records_request_page_source::{
    CommerceCurrencyFacts, CommerceCurrencyRead, CommerceCurrencyReadFuture,
    CommerceCurrencyReadTarget, commerce_currency_read_router,
};
use erpx_server_runtime::public::http_support::serve;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Probe {
    currency: BTreeMap<String, CommerceCurrencyFacts>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl CommerceCurrencyRead for Probe {
    fn currency<'a>(
        &'a self,
        company: &'a str,
    ) -> CommerceCurrencyReadFuture<'a, Option<CommerceCurrencyFacts>> {
        let seen = self.seen.clone();
        let currency = self.currency.clone();
        let company = company.to_owned();
        Box::pin(async move {
            seen.lock().unwrap().push(company.clone());
            Ok(currency.get(&company).cloned())
        })
    }
}

fn target() -> CommerceCurrencyReadTarget {
    CommerceCurrencyReadTarget {
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

async fn serve_probe(probe: Arc<Probe>) -> (u16, tokio::task::JoinHandle<()>) {
    let router = commerce_currency_read_router(probe, vec![target()]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    (addr.port(), server)
}

fn request(host: &str, company: &str) -> String {
    format!(
        "GET /api/v1/holdings/acme/commerce/request-page-currency HTTP/1.1\r\n\
         Host: {host}\r\n\
         Accept: application/json\r\n\
         Connection: close\r\n\
         x-erpx-context-company: {company}\r\n\r\n"
    )
}

#[tokio::test]
async fn currency_parity() {
    let mut currency = BTreeMap::new();
    currency.insert(
        "company".to_owned(),
        CommerceCurrencyFacts {
            currency: "THB".into(),
        },
    );
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (port, server) = serve_probe(Arc::new(Probe {
        currency,
        seen: seen.clone(),
    }))
    .await;
    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request("acme.example.test", "company"),
    )
    .await;
    server.abort();
    assert_eq!(status, 200);
    assert_eq!(body["currency"], "THB");
    assert_eq!(*seen.lock().unwrap(), vec!["company".to_owned()]);
}

#[tokio::test]
async fn missing_company_is_null_not_authority() {
    let (port, server) = serve_probe(Arc::new(Probe {
        currency: BTreeMap::new(),
        seen: Arc::new(Mutex::new(Vec::new())),
    }))
    .await;
    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request("acme.example.test", "company"),
    )
    .await;
    server.abort();
    assert_eq!(status, 200);
    assert!(body["currency"].is_null());
}

#[tokio::test]
async fn wrong_host_fails_closed() {
    let (port, server) = serve_probe(Arc::new(Probe {
        currency: BTreeMap::new(),
        seen: Arc::new(Mutex::new(Vec::new())),
    }))
    .await;
    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request("attacker.example.test", "company"),
    )
    .await;
    server.abort();
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn invalid_company_fails_closed() {
    let (port, server) = serve_probe(Arc::new(Probe {
        currency: BTreeMap::new(),
        seen: Arc::new(Mutex::new(Vec::new())),
    }))
    .await;
    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request("acme.example.test", "bad company id!"),
    )
    .await;
    server.abort();
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "COMMERCE_CURRENCY_READ_REQUEST_INVALID");
}

#[test]
fn configuration_requires_exact_distinct_holdings() {
    let probe = Arc::new(Probe {
        currency: BTreeMap::new(),
        seen: Arc::new(Mutex::new(Vec::new())),
    });
    assert!(commerce_currency_read_router(probe.clone(), vec![target()]).is_ok());
    assert!(commerce_currency_read_router(probe.clone(), vec![target(), target()]).is_err());
    assert!(commerce_currency_read_router(probe.clone(), vec![]).is_err());
}
