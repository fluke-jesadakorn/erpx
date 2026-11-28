//! Additive native R3 read for `GET /api/v1/holdings/{holdingSlug}/stock-transit`
//! and its `GET /stock-transit/{id}` detail (L03).
//!
//! The retained Next catch-all serves these reads entirely in TypeScript via
//! `listTransit`/`transitDetail` (`inventory-transit.ts`), which prepare the
//! owner SQL and add IAM-derived authority columns. This module is a genuinely
//! missing *native* owner read: it executes the owner-owned `inventory_transit`
//! / `inventory_transfers` facts with SQLx, scopes them to the request Company
//! and branch, and returns the exact `{data: ...}` envelope and
//! `Cache-Control: private, no-store` header the TS GET returned.
//!
//! Deliberately bounded: the IAM authority columns (`can_dispatch`,
//! `can_receive`, `can_review`, `pending_loss_authors`, ...) and the
//! `requireTransferPurchasedRead` admission are cross-owner and are NOT
//! reproduced here. See the worker report for that exact parity blocker. This
//! module is new and unique; it never edits an existing router and the lead owns
//! the composition mount line.

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
// domain: pure types and invariants
// ---------------------------------------------------------------------------

/// Native mount requested for the transit list GET.
pub const LIST_PATH: &str = "/api/v1/holdings/{holding_slug}/stock-transit";
/// Native mount requested for the transit detail GET.
pub const DETAIL_PATH: &str = "/api/v1/holdings/{holding_slug}/stock-transit/{id}";

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
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':')
        })
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_TRANSIT_READ_REQUEST_INVALID",
        "Use one bounded native transit read.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_INVENTORY_TRANSIT_READ_UNAVAILABLE",
        "The native transport reader is unavailable.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "TRANSIT_NOT_FOUND", "Transport record not found.")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitQuery {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub id: Option<String>,
}

/// Owner-owned transit columns (no IAM-derived authority flags).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitRow {
    pub id: String,
    pub number: String,
    pub status: String,
    pub version: i64,
    pub transfer_id: String,
    pub transfer_number: String,
    pub source_warehouse_id: String,
    pub target_warehouse_id: String,
    pub source_branch_id: Option<String>,
    pub target_branch_id: Option<String>,
    pub arrival_due_date: String,
    pub created_by: String,
    pub prepared_by: String,
    pub receivable_count: i64,
}

impl TransitRow {
    pub fn value(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "number": self.number,
            "status": self.status,
            "version": self.version,
            "transfer_id": self.transfer_id,
            "transfer_number": self.transfer_number,
            "source_warehouse_id": self.source_warehouse_id,
            "target_warehouse_id": self.target_warehouse_id,
            "source_branch_id": self.source_branch_id,
            "target_branch_id": self.target_branch_id,
            "arrival_due_date": self.arrival_due_date,
            "created_by": self.created_by,
            "prepared_by": self.prepared_by,
            "receivable_count": self.receivable_count,
        })
    }
}

/// `{data: ...}` envelope identical in shape to `Response.json(canonicalDestinationLinks({data}))`.
pub fn envelope(rows: Vec<TransitRow>, detail: bool) -> serde_json::Value {
    if detail {
        let value = rows.into_iter().next().map(|row| row.value());
        serde_json::json!({ "data": value })
    } else {
        serde_json::json!({ "data": rows.into_iter().map(|row| row.value()).collect::<Vec<_>>() })
    }
}

// ---------------------------------------------------------------------------
// application: query port, no SQL
// ---------------------------------------------------------------------------

pub type TransitFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<TransitRow>, BusinessError>> + Send + 'a>>;

pub trait TransitRead: Send + Sync {
    fn load<'a>(&'a self, query: &'a TransitQuery) -> TransitFuture<'a>;
}

// ---------------------------------------------------------------------------
// adapters: SQLx/Postgres execution, row mapping
// ---------------------------------------------------------------------------

/// Owner-owned facts only. `?` from the retired literal is translated to `$n`.
/// The IAM authority projection stays a documented cross-owner blocker.
pub const LIST_SQL: &str = r#"SELECT d.id::text,d.number::text,d.status,d.version,d.transfer_id::text,t.number::text transfer_number,d.source_warehouse_id::text,d.target_warehouse_id::text,d.source_branch_id::text,d.target_branch_id::text,d.arrival_due_date::text,d.created_by::text,d.prepared_by::text,(SELECT count(*) FROM inventory_transit_lines l WHERE l.transit_id=d.id AND l.quantity_milli-l.received_milli-l.lost_milli>0)::bigint receivable_count FROM inventory_transit d JOIN inventory_transfers t ON t.id=d.transfer_id WHERE d.company_id=$1 AND ($2::text IS NULL OR d.source_branch_id=$2 OR d.target_branch_id=$2) ORDER BY d.arrival_due_date,d.id LIMIT 250"#;
pub const DETAIL_SQL: &str = r#"SELECT d.id::text,d.number::text,d.status,d.version,d.transfer_id::text,t.number::text transfer_number,d.source_warehouse_id::text,d.target_warehouse_id::text,d.source_branch_id::text,d.target_branch_id::text,d.arrival_due_date::text,d.created_by::text,d.prepared_by::text,(SELECT count(*) FROM inventory_transit_lines l WHERE l.transit_id=d.id AND l.quantity_milli-l.received_milli-l.lost_milli>0)::bigint receivable_count FROM inventory_transit d JOIN inventory_transfers t ON t.id=d.transfer_id WHERE d.id=$1 AND d.company_id=$2 AND ($3::text IS NULL OR d.source_branch_id=$3 OR d.target_branch_id=$3)"#;

fn map(row: sqlx::postgres::PgRow) -> Result<TransitRow, sqlx::Error> {
    Ok(TransitRow {
        id: row.try_get("id")?,
        number: row.try_get("number")?,
        status: row.try_get("status")?,
        version: row.try_get("version")?,
        transfer_id: row.try_get("transfer_id")?,
        transfer_number: row.try_get("transfer_number")?,
        source_warehouse_id: row.try_get("source_warehouse_id")?,
        target_warehouse_id: row.try_get("target_warehouse_id")?,
        source_branch_id: row.try_get("source_branch_id")?,
        target_branch_id: row.try_get("target_branch_id")?,
        arrival_due_date: row.try_get("arrival_due_date")?,
        created_by: row.try_get("created_by")?,
        prepared_by: row.try_get("prepared_by")?,
        receivable_count: row.try_get("receivable_count")?,
    })
}

pub struct PgTransitRead {
    pool: PgPool,
}

impl PgTransitRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool: pool.raw().clone(),
        }
    }
}

impl TransitRead for PgTransitRead {
    fn load<'a>(&'a self, query: &'a TransitQuery) -> TransitFuture<'a> {
        let pool = self.pool.clone();
        let company = query.company_id.clone();
        let branch = query.branch_id.clone();
        let id = query.id.clone();
        Box::pin(async move {
            let rows = if let Some(id) = id {
                sqlx::query(DETAIL_SQL)
                    .bind(id)
                    .bind(company)
                    .bind(branch)
                    .fetch_all(&pool)
                    .await
            } else {
                sqlx::query(LIST_SQL)
                    .bind(company)
                    .bind(branch)
                    .fetch_all(&pool)
                    .await
            }
            .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(map)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| unavailable())
        })
    }
}

// ---------------------------------------------------------------------------
// public: DTO + router constructor
// ---------------------------------------------------------------------------

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-workspace-branch");

pub struct TransitTarget {
    pub host: String,
    pub holding_slug: String,
}

pub struct ProbeTransitRead<F> {
    load: F,
}

impl<F> ProbeTransitRead<F>
where
    F: for<'a> Fn(&'a TransitQuery) -> TransitFuture<'a> + Send + Sync,
{
    pub fn new(load: F) -> Self {
        Self { load }
    }
}

impl<F> TransitRead for ProbeTransitRead<F>
where
    F: for<'a> Fn(&'a TransitQuery) -> TransitFuture<'a> + Send + Sync,
{
    fn load<'a>(&'a self, query: &'a TransitQuery) -> TransitFuture<'a> {
        (self.load)(query)
    }
}

struct TransitState {
    readers: BTreeMap<(String, String), Arc<dyn TransitRead>>,
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
pub struct BranchFilter {
    pub branch: Option<String>,
}

fn reader_map(
    targets: Vec<(TransitTarget, Arc<dyn TransitRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn TransitRead>>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(invalid());
        }
    }
    if configured.is_empty() {
        return Err(invalid());
    }
    Ok(configured)
}

async fn read(
    State(state): State<Arc<TransitState>>,
    Path((slug, id)): Path<(String, String)>,
    Query(filter): Query<BranchFilter>,
    request: Request,
) -> Response {
    let result = async move {
        if !valid_slug(&slug) || !valid_id(&id) {
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
        let branch = filter
            .branch
            .as_deref()
            .filter(|value| !value.is_empty() && valid_id(value))
            .map(str::to_owned);
        let query = TransitQuery {
            company_id: company,
            branch_id: branch,
            id: Some(id),
        };
        let rows = reader.load(&query).await?;
        if rows.is_empty() {
            return Err(missing());
        }
        Ok(envelope(rows, true))
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

async fn list(
    State(state): State<Arc<TransitState>>,
    Path(slug): Path<String>,
    Query(filter): Query<BranchFilter>,
    request: Request,
) -> Response {
    let result = async move {
        if !valid_slug(&slug) {
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
        let branch = filter
            .branch
            .as_deref()
            .filter(|value| !value.is_empty() && valid_id(value))
            .map(str::to_owned);
        let query = TransitQuery {
            company_id: company,
            branch_id: branch,
            id: None,
        };
        Ok(envelope(reader.load(&query).await?, false))
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

fn router_state(
    targets: Vec<(TransitTarget, Arc<dyn TransitRead>)>,
) -> Result<Arc<TransitState>, BusinessError> {
    Ok(Arc::new(TransitState {
        readers: reader_map(targets)?,
    }))
}

/// Composition-facing constructor: one shared pool for all registered targets.
pub fn router(pool: DatabasePool, targets: Vec<TransitTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

/// Composition-facing constructor: one pool-backed reader per Holding.
pub fn router_for_holdings(
    targets: Vec<(TransitTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = router_state(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (target, Arc::new(PgTransitRead::new(pool)) as Arc<dyn TransitRead>)
            })
            .collect(),
    )?;
    Ok(Router::new()
        .route(LIST_PATH, get(list))
        .route(DETAIL_PATH, get(read))
        .with_state(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> TransitRow {
        TransitRow {
            id: "transit-1".into(),
            number: "TR-1".into(),
            status: "in_transit".into(),
            version: 2,
            transfer_id: "transfer-1".into(),
            transfer_number: "TF-1".into(),
            source_warehouse_id: "w1".into(),
            target_warehouse_id: "w2".into(),
            source_branch_id: Some("b1".into()),
            target_branch_id: Some("b2".into()),
            arrival_due_date: "2026-12-01".into(),
            created_by: "user-1".into(),
            prepared_by: "user-1".into(),
            receivable_count: 3,
        }
    }

    #[test]
    fn bounded_values_only() {
        assert!(valid_slug("acme"));
        assert!(!valid_slug("Acme"));
        assert!(valid_id("transit-1"));
        assert!(!valid_id("transit 1"));
    }

    #[test]
    fn list_envelope_uses_data_array() {
        let value = envelope(vec![row()], false);
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), 1);
        assert_eq!(value["data"][0]["id"], "transit-1");
        assert_eq!(value["data"][0]["receivable_count"], 3);
    }

    #[test]
    fn detail_envelope_uses_single_data() {
        let value = envelope(vec![row()], true);
        assert_eq!(value["data"]["number"], "TR-1");
        assert!(value["data"].is_object());
    }

    #[test]
    fn owner_sql_is_scoped_and_bounded() {
        assert!(LIST_SQL.contains("d.company_id=$1"));
        assert!(LIST_SQL.contains("LIMIT 250"));
        assert!(DETAIL_SQL.contains("d.id=$1 AND d.company_id=$2"));
        assert!(!LIST_SQL.contains("iam_"));
    }

    #[test]
    fn duplicate_context_headers_are_rejected() {
        let mut headers = HeaderMap::new();
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-1"));
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-2"));
        assert!(unique_header(&headers, CONTEXT_COMPANY).is_err());
    }
}
