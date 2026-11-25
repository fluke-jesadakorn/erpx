//! Owner-local axum router for the lane-28 Operations staff-level read family.
//!
//! Composition mounts this router; no TypeScript SQL or business rule is
//! introduced here. Target/host/company isolation and error mapping follow
//! `packages/server-commerce/src/adapters/quality_product_read.rs`.

use super::application::{StaffLevelHandler, StaffLevelRead};
use super::domain::validate_company;
use super::postgres::PgStaffLevelRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_ENGAGEMENT: HeaderName = HeaderName::from_static("x-erpx-context-engagement");

/// Registered Holding target. The host and holding slug must be exact and
/// distinct; an unregistered pair fails closed before any database work.
pub struct Lane28Target {
    pub host: String,
    pub holding_slug: String,
}

struct Lane28State {
    targets: BTreeMap<(String, String), StaffLevelHandler>,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "OPERATIONS_LANE28_READ_UNAVAILABLE",
        "The native Operations staff-level read service is unavailable.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_LANE28_REQUEST_INVALID",
        "Use one bounded Operations staff-level read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_LANE28_CONFIG_INVALID",
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

/// Build the native read router from the owner Postgres pool. The lead mounts
/// it; this crate never edits composition.
pub fn router(pool: DatabasePool, targets: Vec<Lane28Target>) -> Result<Router, BusinessError> {
    let reader = Arc::new(PgStaffLevelRead::new(pool));
    router_with_reader(reader, targets)
}

/// Build the native read router with one owner reader per Holding pool. Each
/// `(Lane28Target, DatabasePool)` pair is keyed by `(host, holding_slug)` so a
/// request can only reach the Postgres pool registered for that exact target.
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
                    StaffLevelHandler::new(Arc::new(PgStaffLevelRead::new(pool))),
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
            "/api/v1/holdings/{holding_slug}/operations/lane28/staff-levels",
            get(staff_levels),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/operations/lane28/staff-level-history",
            get(staff_level_history),
        )
        .with_state(Arc::new(Lane28State { targets: configured })))
}

/// Reader-injected variant used by unit tests and by composition when a shared
/// owner reader already exists.
pub fn router_with_reader(
    reader: Arc<dyn StaffLevelRead>,
    targets: Vec<Lane28Target>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    StaffLevelHandler::new(reader.clone()),
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
            "/api/v1/holdings/{holding_slug}/operations/lane28/staff-levels",
            get(staff_levels),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/operations/lane28/staff-level-history",
            get(staff_level_history),
        )
        .with_state(Arc::new(Lane28State { targets: configured })))
}

async fn staff_levels(
    State(state): State<Arc<Lane28State>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let handler = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| validate_company(value).is_ok())
            .ok_or_else(invalid)?;
        let facts = handler.facts(company).await?;
        let body = serde_json::to_value(&facts).map_err(|_| unavailable())?;
        Ok::<_, BusinessError>(serde_json::json!({"data": body}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn staff_level_history(
    State(state): State<Arc<Lane28State>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let handler = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let engagement = unique_header(request.headers(), CONTEXT_ENGAGEMENT)?
            .filter(|value| super::domain::validate_engagement(value).is_ok())
            .ok_or_else(invalid)?;
        let rows = handler.history(engagement).await?;
        let body = serde_json::to_value(rows).map_err(|_| unavailable())?;
        Ok::<_, BusinessError>(serde_json::json!({"data":{"history": body}}))
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
    use crate::adapters::native_lane_28::application::StaffLevelReadFuture;
    use crate::adapters::native_lane_28::domain::StaffLevel;
    use erpx_server_runtime::public::http_support::serve;
    use std::sync::Mutex;

    #[derive(Clone)]
    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl StaffLevelRead for Probe {
        fn facts<'a>(&'a self, company: &'a str) -> StaffLevelReadFuture<'a, super::super::domain::StaffLevelFacts> {
            let seen = self.seen.clone();
            let company = company.to_owned();
            Box::pin(async move {
                seen.lock().unwrap().push(company.clone());
                Ok(super::super::domain::StaffLevelFacts {
                    levels: vec![
                        StaffLevel { id: format!("{company}-l1"), rank: 1, level_number: 1 },
                        StaffLevel { id: format!("{company}-l2"), rank: 2, level_number: 6 },
                    ],
                    active_count: 2,
                })
            })
        }

        fn history<'a>(&'a self, engagement: &'a str) -> StaffLevelReadFuture<'a, Vec<super::super::domain::LevelHistoryRow>> {
            let engagement = engagement.to_owned();
            Box::pin(async move {
                Ok(vec![super::super::domain::LevelHistoryRow {
                    created_at: 100,
                    reason: format!("{engagement}-promotion"),
                    level_name: Some("Level 2".into()),
                    previous_level_name: Some("Level 1".into()),
                }])
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
        let router = router_with_reader(probe, vec![target()]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        (addr.port(), server)
    }

    fn request(host: &str, company: &str) -> String {
        format!(
            "GET /api/v1/holdings/acme/operations/lane28/staff-levels HTTP/1.1\r\n\
             Host: {host}\r\n\
             Accept: application/json\r\n\
             Connection: close\r\n\
             x-erpx-context-company: {company}\r\n\r\n"
        )
    }

    #[tokio::test]
    async fn staff_levels_parity() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (port, server) = serve_probe(Arc::new(Probe { seen: seen.clone() })).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("acme.example.test", "company"),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["data"]["activeCount"], 2);
        assert_eq!(body["data"]["levels"][0]["id"], "company-l1");
        assert_eq!(body["data"]["levels"][0]["rank"], 1);
        assert_eq!(body["data"]["levels"][1]["levelNumber"], 6);
        assert_eq!(*seen.lock().unwrap(), vec!["company".to_owned()]);
    }

    #[tokio::test]
    async fn wrong_host_fails_closed() {
        let (port, server) = serve_probe(Arc::new(Probe { seen: Arc::new(Mutex::new(Vec::new())) })).await;
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
        let (port, server) = serve_probe(Arc::new(Probe { seen: Arc::new(Mutex::new(Vec::new())) })).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("acme.example.test", "bad company!"),
        )
        .await;
        server.abort();
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "OPERATIONS_LANE28_REQUEST_INVALID");
    }

    #[tokio::test]
    async fn staff_level_history_parity() {
        let (port, server) = serve_probe(Arc::new(Probe { seen: Arc::new(Mutex::new(Vec::new())) })).await;
        let raw = format!(
            "GET /api/v1/holdings/acme/operations/lane28/staff-level-history HTTP/1.1\r\n\
             Host: acme.example.test\r\n\
             Accept: application/json\r\n\
             Connection: close\r\n\
             x-erpx-context-engagement: eng:1\r\n\r\n"
        );
        let (status, body) = http(std::net::SocketAddr::from(([127, 0, 0, 1], port)), raw).await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["data"]["history"][0]["created_at"], 100);
        assert_eq!(body["data"]["history"][0]["level_name"], "Level 2");
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let probe = Arc::new(Probe { seen: Arc::new(Mutex::new(Vec::new())) });
        assert!(router_with_reader(probe.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(probe.clone(), vec![target(), target()]).is_err());
        assert!(router_with_reader(probe, vec![]).is_err());
    }
}
