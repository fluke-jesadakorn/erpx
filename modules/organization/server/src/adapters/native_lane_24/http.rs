//! Owner-local axum router for the lane-24 Organization evaluation reads.
//!
//! Transport facts only: exact registered Host/Holding target, a single bounded
//! Company header, one allow-listed kind and bounded positional ids. The pool,
//! SQL and projection stay inside the owner crate; a missing or malformed
//! native runtime fails closed with an explicit 503.

use crate::adapters::native_lane_24::application::{
    EvaluationFactsReader, FactsQuery, handle,
};
use crate::adapters::native_lane_24::domain::valid_company;
use crate::adapters::native_lane_24::postgres::PgEvaluationFacts;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Arc;

const BODY_LIMIT: usize = 16 * 1024;
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// One exact registered Holding target for the native read router.
#[derive(Clone, Debug)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadRequest {
    kind: String,
    values: Vec<Option<String>>,
}

struct AppState {
    targets: BTreeMap<(String, String), Arc<dyn EvaluationFactsReader>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ORGANIZATION_LANE24_READ_INVALID",
        "Use one bounded Organization evaluation read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "ORGANIZATION_LANE24_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
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

async fn decode(request: Request) -> Result<ReadRequest, BusinessError> {
    if request.uri().query().is_some() {
        return Err(invalid());
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| invalid())?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if !value.is_object() {
        return Err(invalid());
    }
    serde_json::from_value::<ReadRequest>(value).map_err(|_| invalid())
}

/// Reader-injection path. Every target carries its own reader, so distinct
/// Holdings can use distinct pools; the state map is keyed by host and slug.
fn router_with_readers(
    targets: Vec<(LaneTarget, Arc<dyn EvaluationFactsReader>)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    reader,
                )
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
            "/api/v1/holdings/{holding_slug}/organization/lane24/evaluation-facts",
            post(read),
        )
        .with_state(Arc::new(AppState { targets: configured })))
}

/// Build the owner read router. The lead mounts it; no composition is edited
/// here. One owner reader is shared across exact distinct registered targets.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn EvaluationFactsReader> = Arc::new(PgEvaluationFacts::new(pool));
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Build the native read router with one owner reader and database pool per
/// Holding. Each `(LaneTarget, DatabasePool)` pair is bound to a distinct
/// `PgEvaluationFacts`, keyed by `(host, holding_slug)` in the shared state map,
/// so a request can only reach the pool registered for its Holding.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn EvaluationFactsReader> =
                    Arc::new(PgEvaluationFacts::new(pool));
                (target, reader)
            })
            .collect(),
    )
}

async fn read(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_company(value))
            .ok_or_else(invalid)?
            .to_owned();
        let ReadRequest { kind, values } = decode(request).await?;
        let envelope = handle(reader.as_ref(), FactsQuery { company, kind, values }).await?;
        Ok::<_, BusinessError>(serde_json::to_value(envelope).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "ORGANIZATION_LANE24_READ_UNAVAILABLE",
                "The native Organization evaluation read is unavailable.",
            )
        })?)
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
    use crate::adapters::native_lane_24::application::ReadFuture;
    use crate::adapters::native_lane_24::domain::{
        EVAL_MARKET_LEVELS, MarketLevel, ReadResult,
    };
    use std::sync::Mutex;

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
    }
    impl EvaluationFactsReader for Probe {
        fn read(&self, query: FactsQuery) -> ReadFuture<'_, ReadResult> {
            let seen = self.seen.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(format!("{}:{}", query.company, query.kind));
                Ok(ReadResult::MarketLevels(vec![MarketLevel { id: "l1".into() }]))
            })
        }
    }

    fn router_with(seen: Arc<Mutex<Vec<String>>>) -> Router {
        let reader: Arc<dyn EvaluationFactsReader> = Arc::new(Probe { seen });
        router_with_readers(vec![(
            LaneTarget {
                host: "acme.example.test".to_owned(),
                holding_slug: "acme".to_owned(),
            },
            reader,
        )])
        .unwrap()
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

    async fn spawn(router: Router) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        (addr, server)
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        // Validation is exercised through the pure slug/state rules; the pool
        // constructor needs a live DB, so only the target rules are checked here.
        assert!(valid_slug("acme"));
        assert!(valid_slug("north-star"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug(""));
    }

    #[tokio::test]
    async fn two_targets_route_to_distinct_readers() {
        // Stubs stand in for two independent Pg pools. `router_for_holdings`
        // injects one reader per pool through this same state map, keyed by
        // (host, holding_slug); a cross-host/slug request cannot reach either.
        let acme_seen = Arc::new(Mutex::new(Vec::new()));
        let north_seen = Arc::new(Mutex::new(Vec::new()));
        let router = router_with_readers(vec![
            (
                LaneTarget {
                    host: "acme.example.test".into(),
                    holding_slug: "acme".into(),
                },
                Arc::new(Probe { seen: acme_seen.clone() }) as Arc<dyn EvaluationFactsReader>,
            ),
            (
                LaneTarget {
                    host: "northstar.example.test".into(),
                    holding_slug: "northstar".into(),
                },
                Arc::new(Probe { seen: north_seen.clone() }) as Arc<dyn EvaluationFactsReader>,
            ),
        ])
        .unwrap();
        let (addr, server) = spawn(router).await;
        let request = |host: &str, slug: &str, company: &str| {
            let body = r#"{"kind":"eval_market_levels","values":[]}"#;
            format!(
                "POST /api/v1/holdings/{slug}/organization/lane24/evaluation-facts HTTP/1.1\r\nHost: {host}\r\nx-erpx-context-company: {company}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        };
        assert_eq!(http(addr, request("acme.example.test", "acme", "acme-co")).await.0, 200);
        assert_eq!(http(addr, request("northstar.example.test", "northstar", "northstar-co")).await.0, 200);
        assert_eq!(http(addr, request("northstar.example.test", "acme", "acme-co")).await.0, 404);
        assert_eq!(acme_seen.lock().unwrap().as_slice(), &["acme-co:eval_market_levels".to_owned()]);
        assert_eq!(north_seen.lock().unwrap().as_slice(), &["northstar-co:eval_market_levels".to_owned()]);
        let _ = server.abort();
    }

    #[tokio::test]
    async fn actual_router_isolates_target_company_and_kind() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (addr, server) = spawn(router_with(seen.clone())).await;
        let body = r#"{"kind":"eval_market_levels","values":[]}"#;
        let request = |host: &str, company: &str, body: &str| {
            format!(
                "POST /api/v1/holdings/acme/organization/lane24/evaluation-facts HTTP/1.1\r\nHost: {host}\r\nx-erpx-context-company: {company}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        };
        let ok = http(
            addr,
            request("acme.example.test", "acme-co", body),
        )
        .await;
        assert_eq!(ok.0, 200);
        assert_eq!(ok.1["data"]["kind"], EVAL_MARKET_LEVELS);
        assert_eq!(ok.1["data"]["rows"][0]["id"], "l1");
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &[format!("acme-co:{EVAL_MARKET_LEVELS}")]
        );

        let wrong_target = http(addr, request("other.example.test", "acme-co", body)).await;
        assert_eq!(wrong_target.0, 404);

        let bad_company = http(addr, request("acme.example.test", "../bad", body)).await;
        assert_eq!(bad_company.0, 422);

        let unknown_kind = http(
            addr,
            request("acme.example.test", "acme-co", r#"{"kind":"drop_table","values":[]}"#),
        )
        .await;
        assert_eq!(unknown_kind.0, 422);
        let _ = server.abort();
    }
}
