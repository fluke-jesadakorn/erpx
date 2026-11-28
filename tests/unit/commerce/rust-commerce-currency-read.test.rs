//! Native Commerce currency read: parity and fail-closed coverage for the
//! owner-local successor of `adapters/records-request-page-owned-facts.ts`.

use erpx_commerce_server::public::records_request_page_source::{
    CommerceCurrencyFacts, CommerceCurrencyRead, CommerceCurrencyReadFuture,
    CommerceCurrencyReadTarget, commerce_currency_read_router,
    commerce_currency_read_router_for_targets,
};
use erpx_platform_runtime_server::public::http_support::serve;
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
    request_for(host, "acme", company)
}

fn request_for(host: &str, slug: &str, company: &str) -> String {
    format!(
        "GET /api/v1/holdings/{slug}/commerce/request-page-currency HTTP/1.1\r\n\
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

#[tokio::test]
async fn same_path_on_distinct_holding_hosts_uses_distinct_readers() {
    let acme_seen = Arc::new(Mutex::new(Vec::new()));
    let mut acme_currency = BTreeMap::new();
    acme_currency.insert(
        "company".to_owned(),
        CommerceCurrencyFacts {
            currency: "THB".into(),
        },
    );
    let acme: Arc<dyn CommerceCurrencyRead> = Arc::new(Probe {
        currency: acme_currency,
        seen: acme_seen.clone(),
    });

    let northstar_seen = Arc::new(Mutex::new(Vec::new()));
    let mut northstar_currency = BTreeMap::new();
    northstar_currency.insert(
        "company".to_owned(),
        CommerceCurrencyFacts {
            currency: "USD".into(),
        },
    );
    let northstar: Arc<dyn CommerceCurrencyRead> = Arc::new(Probe {
        currency: northstar_currency,
        seen: northstar_seen.clone(),
    });

    let target = |host: &str| CommerceCurrencyReadTarget {
        host: host.into(),
        holding_slug: "acme".into(),
    };
    let router = commerce_currency_read_router_for_targets(vec![
        (target("acme.example.test"), acme),
        (target("northstar.example.test"), northstar),
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], listener.local_addr().unwrap().port()));
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body) = http(addr, request("acme.example.test", "company")).await;
    assert_eq!(status, 200);
    assert_eq!(body["currency"], "THB");

    let (status, body) = http(addr, request("northstar.example.test", "company")).await;
    assert_eq!(status, 200);
    assert_eq!(body["currency"], "USD");

    assert_eq!(*acme_seen.lock().unwrap(), vec!["company".to_owned()]);
    assert_eq!(*northstar_seen.lock().unwrap(), vec!["company".to_owned()]);

    let (status, body) = http(addr, request("attacker.example.test", "company")).await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body) = http(addr, request_for("acme.example.test", "other", "company")).await;
    server.abort();
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
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
    assert!(commerce_currency_read_router_for_targets(vec![(target(), probe.clone())]).is_ok());
    assert!(commerce_currency_read_router_for_targets(vec![
        (target(), probe.clone()),
        (target(), probe.clone()),
    ])
    .is_err());
    assert!(commerce_currency_read_router_for_targets(vec![
        (target(), probe.clone()),
        (
            CommerceCurrencyReadTarget {
                host: "northstar.example.test".into(),
                holding_slug: "acme".into(),
            },
            probe.clone(),
        ),
    ])
    .is_ok());
    assert!(commerce_currency_read_router_for_targets(vec![]).is_err());
}
