//! Additive native R3 read for the authoritative PMS inventory-command fact
//! used by `GET/POST /api/v1/holdings/{holdingSlug}/pms` (L03).
//!
//! The retained `/pms` route falls back to `pmsWorkspace`/`pmsCommand`, whose
//! `readPmsInventoryCommand` obtains this fact through the R2 command-plan:
//! Rust plans `PMS_RD_INVENTORY_COMMAND` and the TypeScript bridge
//! prepares/binds/executes it on the caller's unit. This module removes that
//! hop with a real in-owner SQLx read: it executes the byte-retained SELECT,
//! decodes each row exactly like `decodePmsRecord`, and returns the same
//! `{data:{results:[...]}}` envelope and `Cache-Control: private, no-store`
//! header the TypeScript read returned.
//!
//! Deliberately bounded to this one owner fact. The full `/pms` workspace
//! envelope (rooms, housekeeping, front-desk, dashboard, operations, ...) and
//! the POST command ledger remain TypeScript and are reported as blockers.
//! New and unique module; the lead owns the composition mount line.

use erpx_platform_runtime_server::public::http_support::{
    header, routing::get, HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query,
    Request, Response, Router, State, StatusCode, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Deserialize;
use sqlx::{PgPool, Row};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

// ---------------------------------------------------------------------------
// domain
// ---------------------------------------------------------------------------

pub const INVENTORY_COMMAND_PATH: &str =
    "/api/v1/holdings/{holding_slug}/pms/inventory-command/{property_id}";

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
        "PMS_INVENTORY_COMMAND_REQUEST_INVALID",
        "Use one bounded native PMS inventory read.",
    )
}

fn configuration_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_NATIVE_READ_UNAVAILABLE",
        "The native PMS read service is unavailable.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryCommandQuery {
    pub property_id: String,
}

/// `decodePmsRecord` projection: `{id, kind, data, version, updatedAt}`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredRecord {
    pub id: String,
    pub kind: String,
    pub data: serde_json::Value,
    pub version: i64,
    pub updated_at: i64,
}

impl StoredRecord {
    pub fn value(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "kind": self.kind,
            "data": self.data,
            "version": self.version,
            "updatedAt": self.updated_at,
        })
    }
}

/// Exact `{data:{results:[...]}}` envelope of the TS native read.
pub fn envelope(rows: Vec<StoredRecord>) -> serde_json::Value {
    serde_json::json!({
        "data": { "results": rows.into_iter().map(|row| row.value()).collect::<Vec<_>>() }
    })
}

// ---------------------------------------------------------------------------
// application: query port, no SQL
// ---------------------------------------------------------------------------

pub type InventoryCommandFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<StoredRecord>, BusinessError>> + Send + 'a>>;

pub trait InventoryCommandRead: Send + Sync {
    fn load<'a>(&'a self, query: &'a InventoryCommandQuery) -> InventoryCommandFuture<'a>;
}

// ---------------------------------------------------------------------------
// adapters: SQLx/Postgres execution
// ---------------------------------------------------------------------------

/// Byte-for-byte the retired literal from `native_read_sql.rs`, with `?`
/// translated only to `$n`. Tenant isolation is the registered Holding pool
/// plus the validated property id, matching the TypeScript caller's unit.
pub const INVENTORY_COMMAND_SQL: &str = "SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=$1 AND kind IN ('room_type','room','rate','block','item','credit') UNION ALL SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=$2 AND kind='stay' AND data_json::jsonb->>'state' IN ('reserved','in_house') ORDER BY id";

pub struct PgInventoryCommandRead {
    pool: PgPool,
}

impl PgInventoryCommandRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool: pool.raw().clone(),
        }
    }
}

impl InventoryCommandRead for PgInventoryCommandRead {
    fn load<'a>(&'a self, query: &'a InventoryCommandQuery) -> InventoryCommandFuture<'a> {
        let pool = self.pool.clone();
        let property = query.property_id.clone();
        Box::pin(async move {
            let rows = sqlx::query(INVENTORY_COMMAND_SQL)
                .bind(property.as_str())
                .bind(property.as_str())
                .fetch_all(&pool)
                .await
                .map_err(|_| configuration_unavailable())?;
            rows.into_iter()
                .map(|row| {
                    let raw: String = row.try_get("data_json").map_err(|_| configuration_unavailable())?;
                    let data: serde_json::Value =
                        serde_json::from_str(&raw).map_err(|_| configuration_unavailable())?;
                    Ok(StoredRecord {
                        id: row.try_get("id").map_err(|_| configuration_unavailable())?,
                        kind: row.try_get("kind").map_err(|_| configuration_unavailable())?,
                        data,
                        version: row.try_get("version").map_err(|_| configuration_unavailable())?,
                        updated_at: row.try_get("updated_at").map_err(|_| configuration_unavailable())?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
    }
}

// ---------------------------------------------------------------------------
// public: DTO + router constructor
// ---------------------------------------------------------------------------

pub struct InventoryCommandTarget {
    pub host: String,
    pub holding_slug: String,
}

pub struct ProbeInventoryCommandRead<F> {
    load: F,
}

impl<F> ProbeInventoryCommandRead<F>
where
    F: for<'a> Fn(&'a InventoryCommandQuery) -> InventoryCommandFuture<'a> + Send + Sync,
{
    pub fn new(load: F) -> Self {
        Self { load }
    }
}

impl<F> InventoryCommandRead for ProbeInventoryCommandRead<F>
where
    F: for<'a> Fn(&'a InventoryCommandQuery) -> InventoryCommandFuture<'a> + Send + Sync,
{
    fn load<'a>(&'a self, query: &'a InventoryCommandQuery) -> InventoryCommandFuture<'a> {
        (self.load)(query)
    }
}

struct InventoryCommandState {
    readers: BTreeMap<(String, String), Arc<dyn InventoryCommandRead>>,
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

#[derive(Debug, Deserialize)]
pub struct EmptyQuery {}

async fn inventory_command(
    State(state): State<Arc<InventoryCommandState>>,
    Path((slug, property_id)): Path<(String, String)>,
    Query(_): Query<EmptyQuery>,
    request: Request,
) -> Response {
    let result = async move {
        if !valid_slug(&slug) || !valid_id(&property_id) {
            return Err(invalid());
        }
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .readers
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let query = InventoryCommandQuery {
            property_id,
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
    targets: Vec<(InventoryCommandTarget, Arc<dyn InventoryCommandRead>)>,
) -> Result<Arc<InventoryCommandState>, BusinessError> {
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
    Ok(Arc::new(InventoryCommandState { readers }))
}

/// Composition-facing constructor: one shared pool for all targets.
pub fn router(
    pool: DatabasePool,
    targets: Vec<InventoryCommandTarget>,
) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

/// Composition-facing constructor: one pool-backed reader per Holding.
pub fn router_for_holdings(
    targets: Vec<(InventoryCommandTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = router_state(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    Arc::new(PgInventoryCommandRead::new(pool)) as Arc<dyn InventoryCommandRead>,
                )
            })
            .collect(),
    )?;
    Ok(Router::new()
        .route(INVENTORY_COMMAND_PATH, get(inventory_command))
        .with_state(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> StoredRecord {
        StoredRecord {
            id: "room-1".into(),
            kind: "room".into(),
            data: serde_json::json!({"number":"101"}),
            version: 2,
            updated_at: 100,
        }
    }

    #[test]
    fn bounded_values_only() {
        assert!(valid_slug("acme"));
        assert!(!valid_slug("Acme"));
        assert!(valid_id("property-1"));
        assert!(!valid_id("property 1"));
    }

    #[test]
    fn envelope_matches_ts_results_shape() {
        let value = envelope(vec![row()]);
        assert_eq!(value["data"]["results"][0]["id"], "room-1");
        assert_eq!(value["data"]["results"][0]["data"]["number"], "101");
        assert_eq!(value["data"]["results"][0]["updatedAt"], 100);
        assert_eq!(value.as_object().unwrap().len(), 1);
    }

    #[test]
    fn retained_sql_is_byte_bounded() {
        assert!(INVENTORY_COMMAND_SQL.contains("kind IN ('room_type','room','rate','block','item','credit')"));
        assert!(INVENTORY_COMMAND_SQL.contains("data_json::jsonb->>'state' IN ('reserved','in_house')"));
        assert!(!INVENTORY_COMMAND_SQL.contains('?'));
    }

    #[test]
    fn duplicate_headers_are_rejected() {
        let host = HeaderName::from_static("x-erpx-context-user");
        let mut headers = HeaderMap::new();
        headers.append(host.clone(), HeaderValue::from_static("user-1"));
        headers.append(host.clone(), HeaderValue::from_static("user-2"));
        assert!(unique_header(&headers, host).is_err());
    }
}
