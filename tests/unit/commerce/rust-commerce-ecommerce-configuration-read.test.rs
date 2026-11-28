//! Native Commerce Ecommerce configuration read: parity and fail-closed
//! coverage for the owner-local successor of `adapters/ecommerce-sources.ts`.
//! The read shares the already-mounted Commerce currency route.

use erpx_commerce_server::public::records_request_page_source::{
    CommerceConfigurationFacts, CommerceCurrencyFacts, CommerceCurrencyRead,
    CommerceCurrencyReadFuture, CommerceCurrencyReadTarget, commerce_currency_read_router,
};
use erpx_platform_runtime_server::public::http_support::serve;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Probe {
    configuration: BTreeMap<String, CommerceConfigurationFacts>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl CommerceCurrencyRead for Probe {
    fn currency<'a>(
        &'a self,
        _company: &'a str,
    ) -> CommerceCurrencyReadFuture<'a, Option<CommerceCurrencyFacts>> {
        Box::pin(async { Ok(None) })
    }

    fn configuration<'a>(
        &'a self,
        company: &'a str,
    ) -> CommerceCurrencyReadFuture<'a, Option<CommerceConfigurationFacts>> {
        let seen = self.seen.clone();
        let configuration = self.configuration.clone();
        let company = company.to_owned();
        Box::pin(async move {
            seen.lock().unwrap().push(company.clone());
            Ok(configuration.get(&company).cloned())
        })
    }
}

/// Implementor that does not override `configuration`; exercises the default
/// fail-closed arm of the public read port.
#[derive(Clone)]
struct CurrencyOnly;

impl CommerceCurrencyRead for CurrencyOnly {
    fn currency<'a>(
        &'a self,
        _company: &'a str,
    ) -> CommerceCurrencyReadFuture<'a, Option<CommerceCurrencyFacts>> {
        Box::pin(async { Ok(None) })
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

async fn serve_probe<R: CommerceCurrencyRead + 'static>(
    reader: R,
) -> (u16, tokio::task::JoinHandle<()>) {
    let router = commerce_currency_read_router(Arc::new(reader), vec![target()]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    (addr.port(), server)
}

fn request(host: &str, company: &str, configuration: bool) -> String {
    let header = if configuration {
        "x-erpx-ecommerce-configuration: 1\r\n"
    } else {
        ""
    };
    format!(
        "GET /api/v1/holdings/acme/commerce/request-page-currency HTTP/1.1\r\n\
         Host: {host}\r\n\
         Accept: application/json\r\n\
         Connection: close\r\n\
         x-erpx-context-company: {company}\r\n\
         {header}\r\n"
    )
}

#[tokio::test]
async fn configuration_parity() {
    let mut configuration = BTreeMap::new();
    configuration.insert(
        "company".to_owned(),
        CommerceConfigurationFacts {
            currency: "THB".into(),
            base_currency: "USD".into(),
        },
    );
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (port, server) = serve_probe(Probe {
        configuration,
        seen: seen.clone(),
    })
    .await;
    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request("acme.example.test", "company", true),
    )
    .await;
    server.abort();
    assert_eq!(status, 200);
    assert_eq!(body["configuration"]["currency"], "THB");
    assert_eq!(body["configuration"]["base_currency"], "USD");
    assert_eq!(*seen.lock().unwrap(), vec!["company".to_owned()]);
}

#[tokio::test]
async fn unconfigured_company_is_null() {
    let (port, server) = serve_probe(Probe {
        configuration: BTreeMap::new(),
        seen: Arc::new(Mutex::new(Vec::new())),
    })
    .await;
    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request("acme.example.test", "company", true),
    )
    .await;
    server.abort();
    assert_eq!(status, 200);
    assert!(body["configuration"].is_null());
}

#[tokio::test]
async fn configuration_without_owner_override_fails_closed() {
    let (port, server) = serve_probe(CurrencyOnly).await;
    let (status, body) = http(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        request("acme.example.test", "company", true),
    )
    .await;
    server.abort();
    assert_eq!(status, 503);
    assert_eq!(body["error"]["code"], "COMMERCE_CURRENCY_READ_UNAVAILABLE");
}
