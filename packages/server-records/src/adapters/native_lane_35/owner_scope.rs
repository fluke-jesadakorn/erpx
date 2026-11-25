//! Wave 2 lane-35 family: Records legacy attachment owner-scope read.
//!
//! Owner-local, unpinned by the dated read-tail dedupe evidence. The Rust side
//! owns the SQL, the kind admission, the Company scope result and the response
//! shape. The Files consumer gets a strict multi-holding transport only.

use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::collections::BTreeMap;
use std::sync::Arc;

use super::application::LaneFuture;
use super::domain::{valid_id, valid_slug};

/// Byte-identical HEAD literal, including the descriptor interpolation sites.
pub const LEGACY_OWNER_SCOPE_SQL: &str =
    "SELECT company_id,${branch} branch_id,${department} department_id,version FROM ${table} WHERE id=?";
/// Exact rendered statement for the only admitted descriptor (`employee_request`).
pub const RENDERED_OWNER_SCOPE_SQL: &str =
    "SELECT company_id,branch_id branch_id,department_id department_id,version FROM employee_requests WHERE id=?";
/// Owner statement: redundant aliases and the TS descriptor are gone; the id is bound.
pub const OWNER_SCOPE_SQL: &str =
    "SELECT company_id,branch_id,department_id,version FROM employee_requests WHERE id=$1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerScopeTarget {
    pub host: String,
    pub holding_slug: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct OwnerScopeQuery {
    pub kind: String,
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerScope {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerScopeRow {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub version: i64,
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_RECORDS_OWNER_SCOPE_UNAVAILABLE",
        "The native Records owner-scope read is unavailable.",
    )
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_RECORDS_OWNER_SCOPE_REQUEST_INVALID",
        "Use one bounded Records owner-scope read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_RECORDS_OWNER_SCOPE_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

/// Only the descriptor the legacy TS switch actually served.
pub fn kind_admitted(kind: &str) -> bool {
    kind == "employee_request"
}

pub fn project(row: OwnerScopeRow) -> OwnerScope {
    OwnerScope {
        company_id: row.company_id,
        branch_id: row.branch_id,
        department_id: row.department_id,
        version: row.version,
    }
}

pub trait OwnerScopeSource: Send + Sync {
    fn scope<'a>(&'a self, id: &'a str) -> LaneFuture<'a, Option<OwnerScopeRow>>;
}

/// Execute the owner statement on a pooled Holding connection.
pub struct PgOwnerScopeSource {
    pool: DatabasePool,
}

impl PgOwnerScopeSource {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl OwnerScopeSource for PgOwnerScopeSource {
    fn scope<'a>(&'a self, id: &'a str) -> LaneFuture<'a, Option<OwnerScopeRow>> {
        let id = id.to_owned();
        Box::pin(async move {
            let mut connection = self.pool.raw().acquire().await.map_err(|_| unavailable())?;
            let row = sqlx::query(OWNER_SCOPE_SQL)
                .bind(id.as_str())
                .fetch_optional(&mut *connection)
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(OwnerScopeRow {
                    company_id: row.try_get("company_id").map_err(|_| unavailable())?,
                    branch_id: row.try_get("branch_id").map_err(|_| unavailable())?,
                    department_id: row.try_get("department_id").map_err(|_| unavailable())?,
                    version: row.try_get("version").map_err(|_| unavailable())?,
                })
            })
            .transpose()
        })
    }
}

/// Owner validation and projection. An unadmitted kind returns `None`, exactly
/// as the legacy switch did without touching SQL.
pub async fn read_scope<S: OwnerScopeSource + ?Sized>(
    source: &S,
    query: &OwnerScopeQuery,
) -> Result<Option<OwnerScope>, BusinessError> {
    if !kind_admitted(&query.kind) {
        return Ok(None);
    }
    if !valid_id(&query.id) {
        return Err(invalid());
    }
    Ok(source.scope(&query.id).await?.map(project))
}

struct SharedSource(Arc<dyn OwnerScopeSource>);

impl OwnerScopeSource for SharedSource {
    fn scope<'a>(&'a self, id: &'a str) -> LaneFuture<'a, Option<OwnerScopeRow>> {
        self.0.scope(id)
    }
}

struct OwnerScopeState {
    targets: BTreeMap<(String, String), Arc<dyn OwnerScopeSource>>,
}

const COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const MAX_BODY: usize = 16 * 1024;

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first.map(|value| value.to_str().map_err(|_| invalid())).transpose()
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

fn bounded(body: Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response.headers_mut().insert(name, HeaderValue::from_static(value));
    }
    response
}

fn build_state(
    targets: Vec<(OwnerScopeTarget, Arc<dyn OwnerScopeSource>)>,
) -> Result<Arc<OwnerScopeState>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, source) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), source)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Arc::new(OwnerScopeState { targets: configured }))
}

fn router_from(state: Arc<OwnerScopeState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/records/native-lane-35/attachment-owner-scope",
            post(read_owner_scope),
        )
        .with_state(state)
}

/// One owner reader per Holding pool; there is no shared reader across targets.
pub fn owner_scope_router_for_holdings(
    targets: Vec<(OwnerScopeTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = build_state(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    Arc::new(PgOwnerScopeSource::new(pool)) as Arc<dyn OwnerScopeSource>,
                )
            })
            .collect(),
    )?;
    Ok(router_from(state))
}

/// Reader-injection path retained for tests.
pub fn owner_scope_router_with_reader(
    reader: Arc<dyn OwnerScopeSource>,
    targets: Vec<OwnerScopeTarget>,
) -> Result<Router, BusinessError> {
    let state = build_state(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )?;
    Ok(router_from(state))
}

pub fn owner_scope_router(
    pool: DatabasePool,
    targets: Vec<OwnerScopeTarget>,
) -> Result<Router, BusinessError> {
    owner_scope_router_with_reader(Arc::new(PgOwnerScopeSource::new(pool)), targets)
}

pub fn validate_targets(targets: &[OwnerScopeTarget]) -> Result<(), BusinessError> {
    let mut seen = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || seen.insert((&target.host, &target.holding_slug), ()).is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if targets.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(())
}

async fn read_owner_scope(
    State(state): State<Arc<OwnerScopeState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let source = state
            .targets
            .get(&(host, slug))
            .ok_or_else(missing)?
            .clone();
        if let Some(company) = unique_header(request.headers(), COMPANY)? {
            if !valid_id(company) {
                return Err(invalid());
            }
        }
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|_| invalid())?;
        let query: OwnerScopeQuery = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let scope = read_scope(&SharedSource(source), &query).await?;
        Ok::<_, BusinessError>(serde_json::json!({"data": {"scope": scope}}))
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

    #[derive(Clone, Copy)]
    struct Tagged(&'static str);

    impl OwnerScopeSource for Tagged {
        fn scope<'a>(&'a self, id: &'a str) -> LaneFuture<'a, Option<OwnerScopeRow>> {
            let company = if self.0 == "alpha" { "acme" } else { "northstar" };
            let row = OwnerScopeRow {
                company_id: company.to_owned(),
                branch_id: None,
                department_id: None,
                version: id.len() as i64,
            };
            Box::pin(async move { Ok(Some(row)) })
        }
    }

    #[test]
    fn sql_oracles_are_byte_identical() {
        assert_eq!(
            RENDERED_OWNER_SCOPE_SQL,
            "SELECT company_id,branch_id branch_id,department_id department_id,version FROM employee_requests WHERE id=?"
        );
        assert_eq!(RENDERED_OWNER_SCOPE_SQL.matches("SELECT").count(), 1);
        assert!(OWNER_SCOPE_SQL.contains("id=$1"));
        assert!(!OWNER_SCOPE_SQL.contains("branch_id branch_id"));
    }

    #[tokio::test]
    async fn two_holdings_route_to_two_distinct_readers() {
        let alpha: Arc<dyn OwnerScopeSource> = Arc::new(Tagged("alpha"));
        let beta: Arc<dyn OwnerScopeSource> = Arc::new(Tagged("beta"));
        assert!(!Arc::ptr_eq(&alpha, &beta));

        let state = build_state(vec![
            (
                OwnerScopeTarget {
                    host: "alpha.jesadakorn.com".to_owned(),
                    holding_slug: "acme".to_owned(),
                },
                alpha,
            ),
            (
                OwnerScopeTarget {
                    host: "beta.jesadakorn.com".to_owned(),
                    holding_slug: "northstar".to_owned(),
                },
                beta,
            ),
        ])
        .unwrap();
        assert_eq!(state.targets.len(), 2);
        let a = state.targets.get(&("alpha.jesadakorn.com".to_owned(), "acme".to_owned())).unwrap();
        let b = state.targets.get(&("beta.jesadakorn.com".to_owned(), "northstar".to_owned())).unwrap();
        assert!(!Arc::ptr_eq(a, b));

        let query = OwnerScopeQuery {
            kind: "employee_request".to_owned(),
            id: "r1".to_owned(),
        };
        let from_alpha = read_scope(&SharedSource(a.clone()), &query).await.unwrap();
        let from_beta = read_scope(&SharedSource(b.clone()), &query).await.unwrap();
        assert_ne!(from_alpha, from_beta);
        assert_eq!(from_alpha.unwrap().company_id, "acme");
        assert_eq!(from_beta.unwrap().company_id, "northstar");
    }

    #[tokio::test]
    async fn unadmitted_kind_returns_none_without_touching_sql() {
        let source = Tagged("alpha");
        let scope = read_scope(
            &source,
            &OwnerScopeQuery {
                kind: "invoice_payment".to_owned(),
                id: "r1".to_owned(),
            },
        )
        .await
        .unwrap();
        assert!(scope.is_none());
    }

    #[test]
    fn target_validation_rejects_duplicates() {
        let target = OwnerScopeTarget {
            host: "acme.jesadakorn.com".to_owned(),
            holding_slug: "acme".to_owned(),
        };
        assert!(validate_targets(&[target.clone()]).is_ok());
        assert_eq!(
            validate_targets(&[target.clone(), target]).unwrap_err().code,
            "NATIVE_RECORDS_OWNER_SCOPE_CONFIG_INVALID"
        );
    }
}
