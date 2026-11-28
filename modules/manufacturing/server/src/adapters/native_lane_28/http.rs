//! Owner-local axum router for the lane-28 Manufacturing read families:
//! initial-stock history and existing-posting economic references.
//! Target/host/company isolation and error mapping follow
//! `modules/commerce/server/src/adapters/quality_product_read.rs`.

use super::application::{
    Lane28Read, PostingReferencesHandler, StockHistoryHandler,
};
use super::domain::validate_identifier;
use super::postgres::PgLane28Read;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_WAREHOUSE: HeaderName = HeaderName::from_static("x-erpx-context-warehouse");
const CONTEXT_PRODUCT: HeaderName = HeaderName::from_static("x-erpx-context-product");

pub struct Lane28Target {
    pub host: String,
    pub holding_slug: String,
}

struct TargetHandlers {
    stock_history: StockHistoryHandler,
    posting_references: PostingReferencesHandler,
}

struct Lane28State {
    targets: BTreeMap<(String, String), TargetHandlers>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_LANE28_REQUEST_INVALID",
        "Use one bounded Manufacturing read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_LANE28_CONFIG_INVALID",
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

fn assemble(targets: BTreeMap<(String, String), TargetHandlers>) -> Result<Router, BusinessError> {
    if targets.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/manufacturing/lane28/initial-stock-history",
            get(stock_history),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/manufacturing/lane28/existing-posting-references",
            get(posting_references),
        )
        .with_state(Arc::new(Lane28State { targets })))
}

fn handlers(reader: Arc<dyn Lane28Read>) -> TargetHandlers {
    TargetHandlers {
        stock_history: StockHistoryHandler::new(reader.clone()),
        posting_references: PostingReferencesHandler::new(reader),
    }
}

/// Build from one owner pool shared by all targets.
pub fn router(pool: DatabasePool, targets: Vec<Lane28Target>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgLane28Read::new(pool)), targets)
}

/// Build with one owner reader per target/pool, keyed by `(host, holding_slug)`.
pub fn router_for_holdings(
    targets: Vec<(Lane28Target, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    handlers(Arc::new(PgLane28Read::new(pool))),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    assemble(configured)
}

/// Reader-injected variant used by unit tests.
pub fn router_with_reader(
    reader: Arc<dyn Lane28Read>,
    targets: Vec<Lane28Target>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    handlers(reader.clone()),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    assemble(configured)
}

fn resolve_target<'a>(
    state: &'a Lane28State,
    slug: &str,
    request: &Request,
) -> Result<&'a TargetHandlers, BusinessError> {
    let host = unique_header(request.headers(), header::HOST)?
        .filter(|host| !host.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

async fn stock_history(
    State(state): State<Arc<Lane28State>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let handlers = resolve_target(&state, &slug, &request)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| validate_identifier(value).is_ok())
            .ok_or_else(invalid)?;
        let warehouse = unique_header(request.headers(), CONTEXT_WAREHOUSE)?
            .filter(|value| validate_identifier(value).is_ok())
            .ok_or_else(invalid)?;
        let product = unique_header(request.headers(), CONTEXT_PRODUCT)?
            .filter(|value| validate_identifier(value).is_ok())
            .ok_or_else(invalid)?;
        let facts = handlers.stock_history.history(company, warehouse, product).await?;
        let body = serde_json::to_value(&facts).map_err(|_| BusinessError::new(
            FailureKind::Unavailable,
            "MANUFACTURING_LANE28_READ_UNAVAILABLE",
            "The native Manufacturing lane-28 read service is unavailable.",
        ))?;
        Ok::<_, BusinessError>(serde_json::json!({"data": body}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn posting_references(
    State(state): State<Arc<Lane28State>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let handlers = resolve_target(&state, &slug, &request)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| validate_identifier(value).is_ok())
            .ok_or_else(invalid)?;
        let facts = handlers.posting_references.references(company).await?;
        let body = serde_json::to_value(&facts).map_err(|_| BusinessError::new(
            FailureKind::Unavailable,
            "MANUFACTURING_LANE28_READ_UNAVAILABLE",
            "The native Manufacturing lane-28 read service is unavailable.",
        ))?;
        Ok::<_, BusinessError>(serde_json::json!({"data": body}))
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
    use super::super::application::{
        PostingReferencesRead, PostingReferencesReadFuture, StockHistoryRead, StockHistoryReadFuture,
    };
    use super::super::domain::{PostingReferencesFacts, StockHistoryFacts};
    use erpx_platform_runtime_server::public::http_support::serve;

    #[derive(Clone)]
    struct Probe;

    impl StockHistoryRead for Probe {
        fn history<'a>(
            &'a self,
            company: &'a str,
            warehouse: &'a str,
            product: &'a str,
        ) -> StockHistoryReadFuture<'a, StockHistoryFacts> {
            let present = company == "company" && warehouse == "wh" && product == "product";
            Box::pin(async move { Ok(StockHistoryFacts { history: present }) })
        }
    }

    impl PostingReferencesRead for Probe {
        fn references<'a>(
            &'a self,
            company: &'a str,
        ) -> PostingReferencesReadFuture<'a, PostingReferencesFacts> {
            let valid = validate_identifier(company).is_ok();
            Box::pin(async move {
                Ok(PostingReferencesFacts {
                    journal_ids: if valid { vec!["j1".into()] } else { vec![] },
                    order_ids: if valid { vec!["o1".into()] } else { vec![] },
                    invalid: false,
                    image: "[]".into(),
                })
            })
        }
    }

    fn target() -> Lane28Target {
        Lane28Target { host: "acme.example.test".into(), holding_slug: "acme".into() }
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let status = text.lines().next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok()).unwrap();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        (status, serde_json::from_str(body).unwrap_or(serde_json::Value::Null))
    }

    async fn serve_probe() -> (u16, tokio::task::JoinHandle<()>) {
        let router = router_with_reader(Arc::new(Probe), vec![target()]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { let _ = serve(listener, router).await; });
        (addr.port(), server)
    }

    fn request(path: &str, host: &str, company: &str) -> String {
        format!(
            "GET /api/v1/holdings/acme/manufacturing/lane28/{path} HTTP/1.1\r\n\
             Host: {host}\r\nAccept: application/json\r\nConnection: close\r\n\
             x-erpx-context-company: {company}\r\n\r\n"
        )
    }

    #[tokio::test]
    async fn stock_history_parity() {
        let (port, server) = serve_probe().await;
        let mut raw = request("initial-stock-history", "acme.example.test", "company");
        raw = raw.replace("x-erpx-context-company: company\r\n",
            "x-erpx-context-company: company\r\nx-erpx-context-warehouse: wh\r\nx-erpx-context-product: product\r\n");
        let (status, body) = http(std::net::SocketAddr::from(([127, 0, 0, 1], port)), raw).await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["data"]["history"], true);
    }

    #[tokio::test]
    async fn posting_references_parity() {
        let (port, server) = serve_probe().await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("existing-posting-references", "acme.example.test", "company"),
        ).await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["data"]["journalIds"][0], "j1");
        assert_eq!(body["data"]["orderIds"][0], "o1");
        assert_eq!(body["data"]["invalid"], false);
        assert_eq!(body["data"]["image"], "[]");
    }

    #[tokio::test]
    async fn posting_references_wrong_host_fails_closed() {
        let (port, server) = serve_probe().await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("existing-posting-references", "attacker.example.test", "company"),
        ).await;
        server.abort();
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn posting_references_invalid_company_fails_closed() {
        let (port, server) = serve_probe().await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("existing-posting-references", "acme.example.test", "bad company!"),
        ).await;
        server.abort();
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "MANUFACTURING_LANE28_REQUEST_INVALID");
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let probe = Arc::new(Probe);
        assert!(router_with_reader(probe.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(probe.clone(), vec![target(), target()]).is_err());
        assert!(router_with_reader(probe, vec![]).is_err());
    }
}
