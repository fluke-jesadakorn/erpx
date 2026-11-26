//! Owner-local axum ingress for the production accounting-profiles read.
//!
//! Target/host/company isolation and error mapping follow the lane-28 owner
//! routes (`native_lane_28/http.rs`). The handler never accepts SQL and never
//! falls back to a TypeScript reader: the injected owner reader executes the
//! single Manufacturing-owned SELECT and returns the complete projection.

use crate::application::production_profiles::{
    AccountingProfilesHandler, AccountingProfilesQuery, AccountingProfilesRead,
};
use crate::domain::production_profiles::validate_company;
use crate::adapters::production_profiles_postgres::PgProductionProfilesRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// Exact registered Holding target for this owner route.
pub struct ProductionProfilesTarget {
    pub host: String,
    pub holding_slug: String,
}

struct TargetHandlers {
    profiles: AccountingProfilesHandler,
}

struct ProductionProfilesState {
    targets: BTreeMap<(String, String), TargetHandlers>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_PRODUCTION_PROFILES_REQUEST_INVALID",
        "Use one bounded Manufacturing accounting-profiles read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_PRODUCTION_PROFILES_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
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
            "/api/v1/holdings/{holding_slug}/production/accounting-profiles",
            get(accounting_profiles),
        )
        .with_state(Arc::new(ProductionProfilesState { targets })))
}

fn handlers(reader: Arc<dyn AccountingProfilesRead>) -> TargetHandlers {
    TargetHandlers {
        profiles: AccountingProfilesHandler::new(reader),
    }
}

/// Build from one owner pool shared by all targets.
pub fn router(pool: DatabasePool, targets: Vec<ProductionProfilesTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(targets.into_iter().map(|target| (target, pool.clone())).collect())
}

/// Build with one owner reader per target/pool, keyed by `(host, holding_slug)`.
pub fn router_for_holdings(
    targets: Vec<(ProductionProfilesTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    handlers(Arc::new(PgProductionProfilesRead::new(pool))),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    assemble(configured)
}

/// Reader-injected variant used by unit tests and by a composition that already
/// owns the pooled reader instance.
pub fn router_with_reader(
    reader: Arc<dyn AccountingProfilesRead>,
    targets: Vec<ProductionProfilesTarget>,
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
    state: &'a ProductionProfilesState,
    slug: &str,
    headers: &HeaderMap,
) -> Result<&'a TargetHandlers, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|host| !host.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

async fn accounting_profiles(
    State(state): State<Arc<ProductionProfilesState>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let handlers = resolve_target(&state, &slug, &headers)?;
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| validate_company(value).is_ok())
            .ok_or_else(invalid)?;
        let query = AccountingProfilesQuery::new(company)?;
        let rows = handlers.profiles.handle(query).await?;
        let body = serde_json::to_value(&rows).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "MANUFACTURING_PRODUCTION_PROFILES_UNAVAILABLE",
                "The native Manufacturing accounting-profiles read is unavailable.",
            )
        })?;
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
    use crate::domain::production_profiles::AccountingProfileRow;
    use erpx_server_runtime::public::http_support::serve;

    #[derive(Clone)]
    struct Probe;

    impl AccountingProfilesRead for Probe {
        fn profiles<'a>(
            &'a self,
            company_id: &'a str,
        ) -> crate::application::production_profiles::AccountingProfilesReadFuture<'a, Vec<AccountingProfileRow>>
        {
            let valid = validate_company(company_id).is_ok();
            Box::pin(async move {
                if !valid {
                    return Ok(vec![]);
                }
                Ok(vec![AccountingProfileRow {
                    id: "profile-1".into(),
                    status: "approved".into(),
                    version: 2,
                    currency: "THB".into(),
                    chart_id: "chart-1".into(),
                    inventory_account_id: "inv".into(),
                    wip_account_id: "wip".into(),
                    labor_account_id: "labor".into(),
                    overhead_account_id: "overhead".into(),
                }])
            })
        }
    }

    fn target() -> ProductionProfilesTarget {
        ProductionProfilesTarget {
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

    async fn serve_probe() -> (u16, tokio::task::JoinHandle<()>) {
        let router = router_with_reader(Arc::new(Probe), vec![target()]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { let _ = serve(listener, router).await; });
        (addr.port(), server)
    }

    fn request(path: &str, host: &str, company: &str) -> String {
        format!(
            "GET /api/v1/holdings/acme/production/{path} HTTP/1.1\r\n\
             Host: {host}\r\nAccept: application/json\r\nConnection: close\r\n\
             x-erpx-context-company: {company}\r\n\r\n"
        )
    }

    #[tokio::test]
    async fn accounting_profiles_payload_parity() {
        let (port, server) = serve_probe().await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("accounting-profiles", "acme.example.test", "company"),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["data"][0]["id"], "profile-1");
        assert_eq!(body["data"][0]["status"], "approved");
        assert_eq!(body["data"][0]["version"], 2);
        assert_eq!(body["data"][0]["currency"], "THB");
        assert_eq!(body["data"][0]["chart_id"], "chart-1");
        assert_eq!(body["data"][0]["inventory_account_id"], "inv");
        assert_eq!(body["data"][0]["wip_account_id"], "wip");
        assert_eq!(body["data"][0]["labor_account_id"], "labor");
        assert_eq!(body["data"][0]["overhead_account_id"], "overhead");
    }

    #[tokio::test]
    async fn wrong_host_fails_closed() {
        let (port, server) = serve_probe().await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("accounting-profiles", "attacker.example.test", "company"),
        )
        .await;
        server.abort();
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn invalid_company_fails_closed() {
        let (port, server) = serve_probe().await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("accounting-profiles", "acme.example.test", "bad company!"),
        )
        .await;
        server.abort();
        assert_eq!(status, 422);
        assert_eq!(
            body["error"]["code"],
            "MANUFACTURING_PRODUCTION_PROFILES_REQUEST_INVALID"
        );
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let probe = Arc::new(Probe);
        assert!(router_with_reader(probe.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(probe.clone(), vec![target(), target()]).is_err());
        assert!(router_with_reader(probe, vec![]).is_err());
    }
}
