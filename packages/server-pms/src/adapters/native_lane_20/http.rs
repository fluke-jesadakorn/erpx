//! Owner-local axum router for the native PMS property-readiness read.
//!
//! The route is isolated by exact Host + registered Holding slug, requires a
//! single Company and Property identity header, and returns only the bounded
//! boolean facts computed by the owner reader. There is no TS SQL fallback: a
//! missing/malformed native reply fails closed in the TS client with 503.

use super::application::{PropertyReadinessRead, PropertyReadinessFuture};
use super::domain::{
    PropertyReadinessFacts, configuration_unavailable, invalid_configuration, invalid_request,
    valid_id, valid_slug,
};
use super::postgres::PgPropertyReadinessRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_PROPERTY: HeaderName = HeaderName::from_static("x-erpx-context-property");

/// Registered Holding target. Both fields are exact: the Host header and the
/// path slug must match one configured target or the request is a 404.
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

/// Deterministic reader used by unit tests; production uses the Pg reader.
pub struct ProbePropertyReadinessRead<F> {
    load: F,
}

impl<F> ProbePropertyReadinessRead<F>
where
    F: Fn(&str, &str) -> PropertyReadinessFuture<'static, Option<PropertyReadinessFacts>>
        + Send
        + Sync,
{
    pub fn new(load: F) -> Self {
        Self { load }
    }
}

impl<F> PropertyReadinessRead for ProbePropertyReadinessRead<F>
where
    F: Fn(&str, &str) -> PropertyReadinessFuture<'static, Option<PropertyReadinessFacts>>
        + Send
        + Sync,
{
    fn load<'a>(
        &'a self,
        company: &'a str,
        property: &'a str,
    ) -> PropertyReadinessFuture<'a, Option<PropertyReadinessFacts>> {
        (self.load)(company, property)
    }
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn PropertyReadinessRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
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

fn state(
    reader: Arc<dyn PropertyReadinessRead>,
    targets: Vec<LaneTarget>,
) -> Result<Arc<LaneState>, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    reader.clone(),
                )
                .is_some()
        {
            return Err(invalid_configuration());
        }
    }
    if configured.is_empty() {
        return Err(invalid_configuration());
    }
    Ok(Arc::new(LaneState { targets: configured }))
}

/// Composition-facing constructor: the owner crate owns the pool, the SQL and
/// the projection. The lead mounts the returned router; no composition edit is
/// needed inside this lane.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgPropertyReadinessRead::new(pool)), targets)
}

/// Test-facing constructor: injects a deterministic reader so parity, isolation
/// and fail-closed behaviour are exercised without a live database.
pub fn router_with_reader(
    reader: Arc<dyn PropertyReadinessRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pms/native-lane-20/property-readiness",
            get(readiness),
        )
        .with_state(state(reader, targets)?))
}

async fn readiness(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
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
        let property = unique_header(request.headers(), CONTEXT_PROPERTY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid_request)?;
        let facts = reader.load(company, property).await?;
        let facts = facts.map(|facts| {
            serde_json::json!({
                "hasRoomTypes": facts.has_room_types,
                "hasActiveRooms": facts.has_active_rooms,
                "hasActiveRates": facts.has_active_rates,
                "hasStays": facts.has_stays,
            })
        });
        Ok::<_, BusinessError>(serde_json::json!({"facts": facts}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

// Keep the availability error reachable from the router's public surface so the
// TS transport and the reader agree on the same fail-closed code.
pub fn unavailable() -> BusinessError {
    configuration_unavailable()
}
