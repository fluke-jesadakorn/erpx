//! Owner-local axum ingress for the native Sales price-list workspace read.

use super::pricing_application::{PriceListRead, read_workspace};
use super::pricing_domain::{PriceListQuery, PriceListWorkspace};
use super::pricing_postgres::PgPriceListRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

pub struct PriceListTarget {
    pub host: String,
    pub holding_slug: String,
}

struct PriceListState {
    targets: BTreeMap<(String, String), Arc<dyn PriceListRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "SALES_PRICE_LIST_WORKSPACE_REQUEST_INVALID",
        "Use one bounded price-list workspace read.",
    )
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "SALES_PRICE_LIST_WORKSPACE_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}
fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}
fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}
fn error_response(error: BusinessError) -> Response {
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}
fn bounded(body: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

fn build_with(
    targets: Vec<(PriceListTarget, Arc<dyn PriceListRead>)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/sales/price-list-workspace",
            get(current),
        )
        .with_state(Arc::new(PriceListState {
            targets: configured,
        })))
}

fn with_readers(
    targets: Vec<(PriceListTarget, Arc<dyn PriceListRead>)>,
) -> Result<Router, BusinessError> {
    build_with(targets)
}

pub fn router(pool: DatabasePool, targets: Vec<PriceListTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

/// One `PgPriceListRead` per registered Holding's own pool.
pub fn router_for_holdings(
    targets: Vec<(PriceListTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn PriceListRead> = Arc::new(PgPriceListRead::new(pool));
                (target, reader)
            })
            .collect(),
    )
}

pub fn router_with_reader(
    reader: Arc<dyn PriceListRead>,
    targets: Vec<PriceListTarget>,
) -> Result<Router, BusinessError> {
    with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

async fn current(
    State(state): State<Arc<PriceListState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| !value.is_empty())
            .ok_or_else(invalid)?;
        let query = PriceListQuery {
            company: company.to_owned(),
        };
        let workspace: PriceListWorkspace = read_workspace(reader.as_ref(), &query).await?;
        Ok::<_, BusinessError>(workspace.into_value())
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_30::pricing_application::PriceListReadFuture;
    use std::sync::Mutex;

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
        failing: bool,
    }
    impl PriceListRead for Probe {
        fn workspace<'a>(
            &'a self,
            query: &'a PriceListQuery,
        ) -> PriceListReadFuture<'a, PriceListWorkspace> {
            let seen = self.seen.clone();
            let company = query.company.clone();
            let failing = self.failing;
            Box::pin(async move {
                seen.lock().unwrap().push(company);
                if failing {
                    Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "SALES_PRICE_LIST_WORKSPACE_UNAVAILABLE",
                        "The native Sales price-list workspace read is unavailable.",
                    ))
                } else {
                    Ok(PriceListWorkspace::default())
                }
            })
        }
    }

    fn target(host: &str, slug: &str) -> PriceListTarget {
        PriceListTarget {
            host: host.into(),
            holding_slug: slug.into(),
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
        (
            status,
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        )
    }

    async fn serve(router: Router) -> (u16, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        (addr.port(), server)
    }

    fn request(host: &str, slug: &str, company: &str) -> String {
        format!(
            "GET /api/v1/holdings/{slug}/sales/price-list-workspace HTTP/1.1\r\nHost: {host}\r\nx-erpx-context-company: {company}\r\nConnection: close\r\n\r\n"
        )
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let reader: Arc<dyn PriceListRead> = Arc::new(Probe {
            seen: Arc::new(Mutex::new(Vec::new())),
            failing: false,
        });
        assert!(router_with_reader(reader.clone(), vec![target("a.test", "acme")]).is_ok());
        assert!(router_with_reader(reader.clone(), vec![target("a.test", "acme"), target("a.test", "acme")]).is_err());
        assert!(router_with_reader(reader.clone(), vec![]).is_err());
        assert!(router_with_reader(reader, vec![target("a.test", "BAD")]).is_err());
    }

    #[tokio::test]
    async fn two_holdings_route_to_distinct_readers() {
        let first_seen = Arc::new(Mutex::new(Vec::new()));
        let second_seen = Arc::new(Mutex::new(Vec::new()));
        let first: Arc<dyn PriceListRead> = Arc::new(Probe { seen: first_seen.clone(), failing: false });
        let second: Arc<dyn PriceListRead> = Arc::new(Probe { seen: second_seen.clone(), failing: false });
        let router = with_readers(vec![
            (target("acme.test", "acme"), first),
            (target("other.test", "other"), second),
        ])
        .unwrap();
        let (port, server) = serve(router).await;
        let addr = format!("127.0.0.1:{port}").parse().unwrap();
        let acme = http(addr, request("acme.test", "acme", "acme-co")).await;
        let other = http(addr, request("other.test", "other", "other-co")).await;
        assert_eq!(acme.0, 200);
        assert_eq!(other.0, 200);
        assert_eq!(acme.1, serde_json::json!({"lists":[],"items":[]}));
        assert_eq!(*first_seen.lock().unwrap(), vec!["acme-co".to_owned()]);
        assert_eq!(*second_seen.lock().unwrap(), vec!["other-co".to_owned()]);
        server.abort();
    }

    #[tokio::test]
    async fn unknown_target_and_unavailable_reader_fail_closed() {
        let reader: Arc<dyn PriceListRead> = Arc::new(Probe {
            seen: Arc::new(Mutex::new(Vec::new())),
            failing: true,
        });
        let router = router_with_reader(reader, vec![target("acme.test", "acme")]).unwrap();
        let (port, server) = serve(router).await;
        let addr = format!("127.0.0.1:{port}").parse().unwrap();
        let unknown = http(addr, request("other.test", "acme", "acme-co")).await;
        assert_eq!(unknown.0, 404);
        let unavailable = http(addr, request("acme.test", "acme", "acme-co")).await;
        assert_eq!(unavailable.0, 503);
        assert_eq!(unavailable.1["error"]["code"], "SALES_PRICE_LIST_WORKSPACE_UNAVAILABLE");
        server.abort();
    }
}
