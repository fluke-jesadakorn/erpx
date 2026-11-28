//! Owner-local axum router for the lane 29 calendar availability reads.
//!
//! Follows the Quality product read contract: exact registered
//! Host/Holding target, exactly one Company header, bounded JSON body, no SQL
//! in this layer and no TypeScript fallback. A persistence failure is an
//! explicit 503, never an empty successful projection.

use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::application::Lane29Service;
use super::domain::{ReadKind, ReadRequest, validate};
use super::postgres::PgLane29Read;

const BODY_LIMIT: usize = 64 * 1024;
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// Exact registered target. The lead mounts this router with the same Holding
/// hosts the rest of the Work owner uses; no wildcard or suffix matching.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct AppState {
    /// One owner reader per exact (Host, Holding) pair. A request can only
    /// reach the reader registered for its own Holding pool.
    readers: BTreeMap<(String, String), Arc<Lane29Service<PgLane29Read>>>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE29_REQUEST_INVALID",
        "Use one bounded native Work availability read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE29_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<String>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values
        .next()
        .map(|value| value.to_str().map(str::to_owned))
        .transpose()
        .map_err(|_| invalid())?;
    if values.next().is_some() {
        return Err(invalid());
    }
    Ok(first)
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

fn reader(
    state: &AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<Arc<Lane29Service<PgLane29Read>>, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|host| !host.is_empty())
        .ok_or_else(missing)?;
    state
        .readers
        .get(&(host, slug.to_owned()))
        .cloned()
        .ok_or_else(missing)
}

/// Build the owner router with a single shared pool for every target. Kept for
/// callers that mount one Holding database; `router_for_holdings` is the
/// multi-Holding form the lead uses when each Holding has its own pool.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

/// Build the owner router with one reader per target/pool. Targets are exact,
/// distinct (Host, Holding) pairs; no suffix or wildcard matching is allowed,
/// and a request can only reach the reader bound to its registered Holding.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut readers = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || readers
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    Arc::new(Lane29Service::new(PgLane29Read::new(pool))),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if readers.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/work/native-lane-29",
            post(read),
        )
        .with_state(Arc::new(AppState { readers })))
}

async fn decode(request: Request) -> Result<ReadRequest, BusinessError> {
    if request.uri().query().is_some() {
        return Err(invalid());
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| invalid())?;
    let request: ReadRequest = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    validate(&request)?;
    Ok(request)
}

async fn read(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let service = reader(&state, &headers, &slug)?;
        let company = unique_header(&headers, CONTEXT_COMPANY)?.ok_or_else(invalid)?;
        let request = decode(request).await?;
        if company != request.company_id {
            return Err(BusinessError::invalid(
                "WORK_CONTEXT_CHANGED",
                "The Company header and body must match.",
            ));
        }
        // Kind is parsed and validated in `decode`; reject an unknown variant
        // before it can reach the owner reader even if serde is extended later.
        match request.kind {
            ReadKind::OccurrenceException
            | ReadKind::MovedOccurrences
            | ReadKind::AvailabilitySources
            | ReadKind::SeriesBatch
            | ReadKind::ConflictProbe
            | ReadKind::AppointmentCandidates
            | ReadKind::AttendeeRows
            | ReadKind::AnnouncementCandidates => {}
        }
        service.read(request).await
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

    #[test]
    fn rejects_invalid_target_configuration() {
        // Router construction needs a pool; validate the target rule directly.
        assert!(valid_slug("acme"));
        assert!(valid_slug("acme-1"));
        assert!(!valid_slug(""));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug("a/b"));
    }

    #[test]
    fn error_status_mapping_is_explicit() {
        assert_eq!(
            status(FailureKind::InvalidInput),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(
            status(FailureKind::Unavailable),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            error_response(unavailable()).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    fn unavailable() -> BusinessError {
        BusinessError::new(
            FailureKind::Unavailable,
            "LANE29_READ_UNAVAILABLE",
            "The native Work availability read is unavailable.",
        )
    }
}
