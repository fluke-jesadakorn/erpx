//! Native R3 read for the `/hospitality/setup` properties fact (L16).
//!
//! The TypeScript `hospitalitySetup` adapter currently obtains this fact via
//! the R2 command-plan: Rust plans `SQL_HOSPITALITY_SETUP_PROPERTIES` and the
//! TypeScript bridge prepares/binds/executes it on the caller's unit. This
//! module replaces that hop with a real in-owner sqlx read: the owner executes
//! the exact SELECT, scopes it to the request Company, projects the four text
//! columns, and returns the same `{data:{properties:[...]}}` envelope and
//! `Cache-Control: private, no-store` header the TypeScript read returned.
//!
//! This is deliberately bounded to the `properties` fact. The other setup
//! facts (branches, commerce, settings, warehouses, roles, finance, POS, ...)
//! still execute owner SQL in TypeScript and are reported as the route blocker.

use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Request, Response, Router,
    State, StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Deserialize;
use sqlx::Row;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

// ---------------------------------------------------------------------------
// domain: pure types and invariants
// ---------------------------------------------------------------------------

/// One `pms_properties` row as projected by the retained TypeScript read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupProperty {
    pub id: String,
    pub name: String,
    pub state: String,
    pub business_date: String,
}

impl SetupProperty {
    pub fn value(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "name": self.name,
            "state": self.state,
            "business_date": self.business_date,
        })
    }
}

/// Bounded identity: same space the sibling native PMS lane accepts for
/// Company/branch routing values. No whitespace, control chars or delimiters.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

pub fn invalid_request() -> BusinessError {
    BusinessError::invalid(
        "PMS_SETUP_PROPERTIES_REQUEST_INVALID",
        "Use a valid Company and branch.",
    )
}

pub fn configuration_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_NATIVE_READ_UNAVAILABLE",
        "The native PMS read service is unavailable.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

// ---------------------------------------------------------------------------
// application: query port, no SQL
// ---------------------------------------------------------------------------

pub type SetupPropertiesFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader port. Composition injects the Pg implementation; tests inject a
/// deterministic probe. Company and branch are already-validated identities;
/// no caller SQL reaches this port.
pub trait SetupPropertiesRead: Send + Sync {
    fn load<'a>(
        &'a self,
        company: &'a str,
        branch: &'a str,
    ) -> SetupPropertiesFuture<'a, Vec<SetupProperty>>;
}

// ---------------------------------------------------------------------------
// adapters: SQLx/Postgres execution, row mapping
// ---------------------------------------------------------------------------

/// Byte-for-byte the retired TypeScript literal, with `?` translated only to
/// `$n`. No predicate or projection changed.
pub const SETUP_PROPERTIES_SQL: &str =
    "SELECT id,name,state,business_date FROM pms_properties WHERE company_id=$1 AND branch_id=$2 ORDER BY name";

pub struct PgSetupPropertiesRead {
    pool: DatabasePool,
}

impl PgSetupPropertiesRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl SetupPropertiesRead for PgSetupPropertiesRead {
    fn load<'a>(
        &'a self,
        company: &'a str,
        branch: &'a str,
    ) -> SetupPropertiesFuture<'a, Vec<SetupProperty>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let branch = branch.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let branch = branch.clone();
                Box::pin(async move {
                    let rows = sqlx::query(SETUP_PROPERTIES_SQL)
                        .bind(company.as_str())
                        .bind(branch.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| configuration_unavailable())?;
                    let unavailable = configuration_unavailable();
                    rows.into_iter()
                        .map(|row| {
                            Ok(SetupProperty {
                                id: row.try_get::<String, _>("id").map_err(|_| unavailable.clone())?,
                                name: row.try_get::<String, _>("name").map_err(|_| unavailable.clone())?,
                                state: row.try_get::<String, _>("state").map_err(|_| unavailable.clone())?,
                                business_date: row
                                    .try_get::<String, _>("business_date")
                                    .map_err(|_| unavailable.clone())?,
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => configuration_unavailable(),
            })
        })
    }
}

// ---------------------------------------------------------------------------
// public: DTO + router constructor
// ---------------------------------------------------------------------------

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// Registered Holding target. Both fields are exact: the Host header and path
/// slug must match one configured target or the request is a 404.
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

/// Deterministic reader used by unit tests; production uses the Pg reader.
pub struct ProbeSetupPropertiesRead<F> {
    load: F,
}

impl<F> ProbeSetupPropertiesRead<F>
where
    F: for<'a> Fn(&'a str, &'a str) -> SetupPropertiesFuture<'a, Vec<SetupProperty>>
        + Send
        + Sync,
{
    pub fn new(load: F) -> Self {
        Self { load }
    }
}

impl<F> SetupPropertiesRead for ProbeSetupPropertiesRead<F>
where
    F: for<'a> Fn(&'a str, &'a str) -> SetupPropertiesFuture<'a, Vec<SetupProperty>>
        + Send
        + Sync,
{
    fn load<'a>(
        &'a self,
        company: &'a str,
        branch: &'a str,
    ) -> SetupPropertiesFuture<'a, Vec<SetupProperty>> {
        (self.load)(company, branch)
    }
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn SetupPropertiesRead>>,
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

/// Exact envelope of the TypeScript read: `Response.json(canonicalDestinationLinks({data}))`.
/// `Cache-Control: private, no-store` is the TS header; the two extra security
/// headers match the sibling native lane routers and do not alter the payload.
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

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid_request());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid_request()))
        .transpose()
}

#[derive(Debug, Deserialize)]
pub struct PropertiesParams {
    pub branch: String,
}

fn reader_map(
    targets: Vec<(LaneTarget, Arc<dyn SetupPropertiesRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn SetupPropertiesRead>>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(invalid_request());
        }
    }
    if configured.is_empty() {
        return Err(invalid_request());
    }
    Ok(configured)
}

fn route(state: Arc<LaneState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/hospitality/setup/properties",
            get(properties),
        )
        .with_state(state)
}

/// Composition-facing constructor for one pool shared by all targets.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

/// Composition-facing constructor: one reader per registered Holding, keyed by
/// exact `(host, holding_slug)`.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let readers = targets
        .into_iter()
        .map(|(target, pool)| {
            (
                target,
                Arc::new(PgSetupPropertiesRead::new(pool)) as Arc<dyn SetupPropertiesRead>,
            )
        })
        .collect();
    Ok(route(Arc::new(LaneState {
        targets: reader_map(readers)?,
    })))
}

/// Test-facing constructor: inject one deterministic reader per target.
pub fn router_with_readers(
    targets: Vec<(LaneTarget, Arc<dyn SetupPropertiesRead>)>,
) -> Result<Router, BusinessError> {
    Ok(route(Arc::new(LaneState {
        targets: reader_map(targets)?,
    })))
}

async fn properties(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    Query(params): Query<PropertiesParams>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid_request)?;
        if !valid_id(&params.branch) {
            return Err(invalid_request());
        }
        let rows = reader.load(company, &params.branch).await?;
        let properties = rows.into_iter().map(|row| row.value()).collect::<Vec<_>>();
        Ok::<_, BusinessError>(serde_json::json!({"data":{"properties":properties}}))
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
    use erpx_platform_runtime_server::public::http_support::serve;
    use sha2::{Digest, Sha256};
    use std::sync::Mutex;

    const HOST: &str = "acme.example.test";

    fn sha(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn row() -> SetupProperty {
        SetupProperty {
            id: "hotel-1".to_owned(),
            name: "Grand Hotel".to_owned(),
            state: "open".to_owned(),
            business_date: "2026-11-28".to_owned(),
        }
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, serde_json::Value) {
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
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let cache = head
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("cache-control:"))
            .map(|line| line.split_once(':').unwrap().1.trim().to_owned())
            .unwrap_or_default();
        (
            status,
            cache,
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        )
    }

    async fn serve_probe(
        seen: Arc<Mutex<Vec<(String, String)>>>,
        answer: Vec<SetupProperty>,
    ) -> (u16, tokio::task::JoinHandle<()>) {
        let reader = Arc::new(ProbeSetupPropertiesRead::new(move |company, branch| {
            let company = company.to_owned();
            let branch = branch.to_owned();
            let seen = seen.clone();
            let answer = answer.clone();
            Box::pin(async move {
                seen.lock().unwrap().push((company, branch));
                Ok::<_, BusinessError>(answer)
            })
        }));
        let router = router_with_readers(vec![(
            LaneTarget {
                host: HOST.to_owned(),
                holding_slug: "acme".to_owned(),
            },
            reader,
        )])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        (addr.port(), server)
    }

    fn request(host: &str, company: &str, branch: &str) -> String {
        format!(
            "GET /api/v1/holdings/acme/hospitality/setup/properties?branch={branch} HTTP/1.1\r\n\
             Host: {host}\r\n\
             Accept: application/json\r\n\
             Connection: close\r\n\
             x-erpx-context-company: {company}\r\n\r\n"
        )
    }

    #[tokio::test]
    async fn properties_envelope_headers_and_scope() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (port, server) = serve_probe(seen.clone(), vec![row()]).await;
        let (status, cache, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request(HOST, "company-1", "branch-1"),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(cache, "private, no-store");
        assert_eq!(body["data"]["properties"][0]["id"], "hotel-1");
        assert_eq!(body["data"]["properties"][0]["name"], "Grand Hotel");
        assert_eq!(body["data"]["properties"][0]["state"], "open");
        assert_eq!(body["data"]["properties"][0]["business_date"], "2026-11-28");
        assert_eq!(
            *seen.lock().unwrap(),
            vec![("company-1".to_owned(), "branch-1".to_owned())]
        );
    }

    #[tokio::test]
    async fn wrong_host_fails_closed() {
        let (port, server) = serve_probe(Arc::new(Mutex::new(Vec::new())), vec![row()]).await;
        let (status, _, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("attacker.example.test", "company-1", "branch-1"),
        )
        .await;
        server.abort();
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn invalid_identity_fails_closed() {
        let (port, server) = serve_probe(Arc::new(Mutex::new(Vec::new())), vec![row()]).await;
        let (status, _, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request(HOST, "bad company id!", "branch-1"),
        )
        .await;
        server.abort();
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "PMS_SETUP_PROPERTIES_REQUEST_INVALID");
    }

    #[test]
    fn configuration_requires_exact_distinct_targets() {
        let reader = Arc::new(ProbeSetupPropertiesRead::new(|_, _| {
            Box::pin(async { Ok::<_, BusinessError>(vec![]) })
        }));
        let target = || LaneTarget {
            host: HOST.to_owned(),
            holding_slug: "acme".to_owned(),
        };
        assert!(router_with_readers(vec![(target(), reader.clone())]).is_ok());
        assert!(router_with_readers(vec![
            (target(), reader.clone()),
            (target(), reader.clone())
        ])
        .is_err());
        assert!(router_with_readers(vec![]).is_err());
    }

    #[test]
    fn sql_and_result_oracles() {
        let retired = "SELECT id,name,state,business_date FROM pms_properties WHERE company_id=? AND branch_id=? ORDER BY name";
        let translated = retired
            .replacen("?", "$1", 1)
            .replacen("?", "$2", 1);
        assert_eq!(SETUP_PROPERTIES_SQL, translated);
        assert_eq!(
            sha(SETUP_PROPERTIES_SQL),
            "45520efd0bfde61384c2721dba2f4a6c6e1441ccea8893185db867d526e84ff3"
        );
        let result = serde_json::json!({
            "data":{"properties":[{"id":"hotel-1","name":"Grand Hotel","state":"open","business_date":"2026-11-28"}]}
        })
        .to_string();
        assert_eq!(
            sha(&result),
            "85548943b71e719b597de9b106cdecf2f0e0ee451251d17e72dc55984e4ac61d"
        );
    }
}
