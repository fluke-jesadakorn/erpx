//! Additive native R3 read for the commerce-order-by-source fact consumed by
//! `GET /api/v1/holdings/{holdingSlug}/commerce/{...path}` (L03).
//!
//! The retained catch-all's orders branch reaches this fact through the R2
//! command-plan: Rust plans `RD_ORDER_BY_SOURCE_ID` and the TypeScript bridge
//! prepares/binds/executes it on the caller's unit. This module removes that
//! hop with a real in-owner SQLx read: it executes the owner-owned SELECT,
//! scopes it to the request Company and returns the exact
//! `{data:{results:[...]}}` envelope and `Cache-Control: private, no-store`
//! header the TypeScript read returned.
//!
//! Deliberately bounded to this one owner fact. The rest of the catch-all
//! (overview, products, parties, warehouses, posting setups, pricing,
//! reorder policies) still executes owner SQL in TypeScript and is reported as
//! the route blocker. New and unique module; the lead owns the mount line.

use erpx_platform_runtime_server::public::http_support::{
    header, routing::get, HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request,
    Response, Router, State, StatusCode, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{PgPool, Row};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

// ---------------------------------------------------------------------------
// domain
// ---------------------------------------------------------------------------

/// Native mount requested for the owner fact.
pub const SOURCE_PATH: &str =
    "/api/v1/holdings/{holding_slug}/commerce/orders/by-source/{source_id}";

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_COMMERCE_ORDER_SOURCE_REQUEST_INVALID",
        "Use one bounded native order-source read.",
    )
}

fn configuration_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_COMMERCE_READ_UNAVAILABLE",
        "The native Commerce reader is unavailable.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "ORDER_NOT_FOUND", "Order not found.")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderSourceQuery {
    pub company_id: String,
    pub source_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderSourceRow {
    pub id: String,
    pub number: String,
}

impl OrderSourceRow {
    pub fn value(&self) -> serde_json::Value {
        serde_json::json!({ "id": self.id, "number": self.number })
    }
}

/// Exact `{data:{results:[...]}}` envelope returned by the TS read.
pub fn envelope(rows: Vec<OrderSourceRow>) -> serde_json::Value {
    serde_json::json!({
        "data": { "results": rows.into_iter().map(|row| row.value()).collect::<Vec<_>>() }
    })
}

// ---------------------------------------------------------------------------
// application: query port, no SQL
// ---------------------------------------------------------------------------

pub type OrderSourceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<OrderSourceRow>, BusinessError>> + Send + 'a>>;

pub trait OrderSourceRead: Send + Sync {
    fn load<'a>(&'a self, query: &'a OrderSourceQuery) -> OrderSourceFuture<'a>;
}

// ---------------------------------------------------------------------------
// adapters: SQLx/Postgres execution
// ---------------------------------------------------------------------------

/// Owner-owned literal; `?` translated only to `$n`, with tenant scope added.
pub const ORDER_SOURCE_SQL: &str =
    "SELECT id::text,number::text FROM commerce_orders WHERE source_id=$1 AND company_id=$2 ORDER BY id LIMIT 250";

pub struct PgOrderSourceRead {
    pool: PgPool,
}

impl PgOrderSourceRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool: pool.raw().clone(),
        }
    }
}

impl OrderSourceRead for PgOrderSourceRead {
    fn load<'a>(&'a self, query: &'a OrderSourceQuery) -> OrderSourceFuture<'a> {
        let pool = self.pool.clone();
        let source_id = query.source_id.clone();
        let company = query.company_id.clone();
        Box::pin(async move {
            let rows = sqlx::query(ORDER_SOURCE_SQL)
                .bind(source_id)
                .bind(company)
                .fetch_all(&pool)
                .await
                .map_err(|_| configuration_unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(OrderSourceRow {
                        id: row.try_get("id").map_err(|_| configuration_unavailable())?,
                        number: row.try_get("number").map_err(|_| configuration_unavailable())?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
    }
}

// ---------------------------------------------------------------------------
// public: DTO + router constructor
// ---------------------------------------------------------------------------

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

pub struct OrderSourceTarget {
    pub host: String,
    pub holding_slug: String,
}

pub struct ProbeOrderSourceRead<F> {
    load: F,
}

impl<F> ProbeOrderSourceRead<F>
where
    F: for<'a> Fn(&'a OrderSourceQuery) -> OrderSourceFuture<'a> + Send + Sync,
{
    pub fn new(load: F) -> Self {
        Self { load }
    }
}

impl<F> OrderSourceRead for ProbeOrderSourceRead<F>
where
    F: for<'a> Fn(&'a OrderSourceQuery) -> OrderSourceFuture<'a> + Send + Sync,
{
    fn load<'a>(&'a self, query: &'a OrderSourceQuery) -> OrderSourceFuture<'a> {
        (self.load)(query)
    }
}

struct OrderSourceState {
    readers: BTreeMap<(String, String), Arc<dyn OrderSourceRead>>,
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

fn unique_header<'a>(headers: &'a HeaderMap, name: HeaderName) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

async fn source(
    State(state): State<Arc<OrderSourceState>>,
    Path((slug, source_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        if !valid_slug(&slug) || !valid_id(&source_id) {
            return Err(invalid());
        }
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .readers
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?
            .to_owned();
        let query = OrderSourceQuery {
            company_id: company,
            source_id,
        };
        Ok(envelope(reader.load(&query).await?))
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

fn router_state(
    targets: Vec<(OrderSourceTarget, Arc<dyn OrderSourceRead>)>,
) -> Result<Arc<OrderSourceState>, BusinessError> {
    let mut readers = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || readers
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(invalid());
        }
    }
    if readers.is_empty() {
        return Err(invalid());
    }
    Ok(Arc::new(OrderSourceState { readers }))
}

/// Composition-facing constructor: one shared pool for all targets.
pub fn router(pool: DatabasePool, targets: Vec<OrderSourceTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

/// Composition-facing constructor: one pool-backed reader per Holding.
pub fn router_for_holdings(
    targets: Vec<(OrderSourceTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = router_state(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    Arc::new(PgOrderSourceRead::new(pool)) as Arc<dyn OrderSourceRead>,
                )
            })
            .collect(),
    )?;
    Ok(Router::new()
        .route(SOURCE_PATH, get(source))
        .with_state(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> OrderSourceRow {
        OrderSourceRow {
            id: "order-1".into(),
            number: "SO-1".into(),
        }
    }

    #[test]
    fn bounded_values_only() {
        assert!(valid_slug("acme"));
        assert!(!valid_slug("Acme"));
        assert!(valid_id("SO-1"));
        assert!(!valid_id("SO 1"));
    }

    #[test]
    fn envelope_matches_ts_results_shape() {
        let value = envelope(vec![row()]);
        assert_eq!(value["data"]["results"][0]["id"], "order-1");
        assert_eq!(value["data"]["results"][0]["number"], "SO-1");
        assert_eq!(value.as_object().unwrap().len(), 1);
    }

    #[test]
    fn owner_sql_is_tenant_scoped_and_bounded() {
        assert!(ORDER_SOURCE_SQL.contains("source_id=$1"));
        assert!(ORDER_SOURCE_SQL.contains("company_id=$2"));
        assert!(ORDER_SOURCE_SQL.contains("LIMIT 250"));
    }

    #[test]
    fn duplicate_context_headers_are_rejected() {
        let mut headers = HeaderMap::new();
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-1"));
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-2"));
        assert!(unique_header(&headers, CONTEXT_COMPANY).is_err());
    }
}
