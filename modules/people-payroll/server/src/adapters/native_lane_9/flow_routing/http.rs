//! Owner-local axum ingress for the payroll payment-batch reads. Host/Holding
//! targets, Company scope and error mapping follow the lane-9 native-read
//! reference; no TS SQL or business fallback remains.

use super::application::PaymentBatchRead;
use super::domain::{invalid, valid_id};
use super::postgres::PgPaymentBatchRead;
use crate::adapters::native_lane_9::LaneTarget;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_SOURCE: HeaderName = HeaderName::from_static("x-erpx-context-source");

struct FlowState {
    targets: BTreeMap<(String, String), Arc<dyn PaymentBatchRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE9_FLOW_CONFIG_INVALID",
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

fn reader(headers: &HeaderMap, slug: &str, state: &FlowState) -> Result<Arc<dyn PaymentBatchRead>, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|host| !host.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .cloned()
        .ok_or_else(missing)
}

fn company(headers: &HeaderMap) -> Result<&str, BusinessError> {
    unique_header(headers, CONTEXT_COMPANY)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)
}

fn configure_readers(
    entries: impl IntoIterator<Item = (LaneTarget, Arc<dyn PaymentBatchRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn PaymentBatchRead>>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
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
    Ok(configured)
}

fn finish(
    configured: BTreeMap<(String, String), Arc<dyn PaymentBatchRead>>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/people-payroll/payment-batches",
            get(batches),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people-payroll/payment-batch",
            get(by_source),
        )
        .with_state(Arc::new(FlowState { targets: configured })))
}

/// Public router contract for a single pool.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgPaymentBatchRead::new(pool)), targets)
}

/// Per-Holding constructor: one Pg reader per registered `(LaneTarget, pool)`.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    finish(configure_readers(targets.into_iter().map(|(target, pool)| {
        (
            target,
            Arc::new(PgPaymentBatchRead::new(pool)) as Arc<dyn PaymentBatchRead>,
        )
    }))?)
}

pub fn router_with_reader(
    reader: Arc<dyn PaymentBatchRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

pub fn router_with_readers(
    entries: Vec<(LaneTarget, Arc<dyn PaymentBatchRead>)>,
) -> Result<Router, BusinessError> {
    finish(configure_readers(entries)?)
}

async fn batches(
    State(state): State<Arc<FlowState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let reader = reader(request.headers(), &slug, &state)?;
        let company = company(request.headers())?;
        let rows = reader.batches(company).await?;
        let items = rows
            .into_iter()
            .map(|row| {
                serde_json::json!({
                    "source_id": row.source_id,
                    "id": row.id,
                    "status": row.status,
                    "created_by": row.created_by,
                })
            })
            .collect::<Vec<_>>();
        Ok::<_, BusinessError>(serde_json::json!({"batches": items}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn by_source(
    State(state): State<Arc<FlowState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let reader = reader(request.headers(), &slug, &state)?;
        let company = company(request.headers())?;
        let source = unique_header(request.headers(), CONTEXT_SOURCE)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let row = reader.by_source(company, source).await?;
        let batch = row.map(|row| {
            serde_json::json!({
                "id": row.id,
                "status": row.status,
                "created_by": row.created_by,
            })
        });
        Ok::<_, BusinessError>(serde_json::json!({"batch": batch}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}
