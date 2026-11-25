//! Owner-local axum router for the lane 37 Commercial seat-history read.
//!
//! Target registration binds one exact Host + Holding slug + Holding identity.
//! Every request must repeat that identity in a unique header; a malformed,
//! duplicated or mismatched identity fails closed without touching the pool.

use std::{collections::BTreeMap, sync::Arc};

use erpx_server_runtime::public::http_support::{
    header, routing::get, serde_json, HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path,
    Query, Request, Response, Router, State, StatusCode,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;

use crate::adapters::native_lane_37::application::{handler, ReadRequest, SeatHistoryRead};
use crate::adapters::native_lane_37::domain::{valid_id, valid_slug};
use crate::adapters::native_lane_37::postgres::PgSeatHistoryRead;

const CONTEXT_HOLDING: HeaderName = HeaderName::from_static("x-erpx-context-holding");
const SEAT_CUTOFF_DEPENDENCY: HeaderName =
    HeaderName::from_static("x-erpx-lane37-cutoff-dependency");

/// One exact registered Holding target. The lead supplies these from the
/// identity-owned Holding registry; no wildcard host is accepted.
pub struct Lane37Target {
    pub host: String,
    pub holding_slug: String,
    pub holding_id: String,
}

#[derive(Deserialize)]
struct WindowQuery {
    start: Option<String>,
    end: Option<String>,
}

struct Lane37State {
    targets: BTreeMap<(String, String), (String, Arc<dyn SeatHistoryRead>)>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE37_REQUEST_INVALID",
        "Use one bounded Commercial seat-history read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE37_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
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

fn parse_i64(value: Option<String>) -> Result<i64, BusinessError> {
    value
        .as_deref()
        .ok_or_else(invalid)?
        .parse::<i64>()
        .map_err(|_| invalid())
}

/// Build the native seat-history router from an injected reader.
pub fn router_with_reader(
    reader: Arc<dyn SeatHistoryRead>,
    targets: Vec<Lane37Target>,
) -> Result<Router, BusinessError> {
    let mut configured: BTreeMap<(String, String), (String, Arc<dyn SeatHistoryRead>)> =
        BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_id(&target.holding_id)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    (target.holding_id.clone(), reader.clone()),
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
            "/api/v1/holdings/{holding_slug}/commercial/seat-history",
            get(history),
        )
        .with_state(Arc::new(Lane37State {
            targets: configured,
        })))
}

/// Build the native seat-history router with the owner's sqlx-backed reader.
pub fn router(pool: DatabasePool, targets: Vec<Lane37Target>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgSeatHistoryRead::new(pool)), targets)
}

async fn history(
    State(state): State<Arc<Lane37State>>,
    Path(slug): Path<String>,
    Query(window): Query<WindowQuery>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let (expected_holding, reader) = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let holding = unique_header(request.headers(), CONTEXT_HOLDING)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        if holding != expected_holding {
            return Err(invalid());
        }
        let cutoff = unique_header(request.headers(), SEAT_CUTOFF_DEPENDENCY)?
            .filter(|value| matches!(*value, "true" | "false"))
            .ok_or_else(invalid)?
            == "true";
        let start = parse_i64(window.start)?;
        let end = parse_i64(window.end)?;
        let value = handler(
            reader.clone(),
            ReadRequest {
                start,
                end,
                has_cutoff_dependency: cutoff,
            },
        )
        .await?;
        Ok::<_, BusinessError>(serde_json::json!(value))
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
    use crate::adapters::native_lane_37::application::EmptySeatHistoryRead;

    #[test]
    fn configuration_requires_exact_distinct_targets() {
        assert!(router_with_reader(
            Arc::new(EmptySeatHistoryRead),
            vec![Lane37Target {
                host: "acme.jesadakorn.com".to_owned(),
                holding_slug: "acme".to_owned(),
                holding_id: "holding-1".to_owned(),
            }],
        )
        .is_ok());
        assert_eq!(
            router_with_reader(Arc::new(EmptySeatHistoryRead), vec![])
                .unwrap_err()
                .code,
            "LANE37_CONFIG_INVALID"
        );
        let duplicate = vec![
            Lane37Target {
                host: "acme.jesadakorn.com".to_owned(),
                holding_slug: "acme".to_owned(),
                holding_id: "holding-1".to_owned(),
            },
            Lane37Target {
                host: "acme.jesadakorn.com".to_owned(),
                holding_slug: "acme".to_owned(),
                holding_id: "holding-2".to_owned(),
            },
        ];
        assert_eq!(
            router_with_reader(Arc::new(EmptySeatHistoryRead), duplicate)
                .unwrap_err()
                .code,
            "LANE37_CONFIG_INVALID"
        );
    }

    #[test]
    fn error_status_mapping_is_closed() {
        assert_eq!(
            status(FailureKind::InvalidInput),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            status(FailureKind::Unavailable),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
    }
}
