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

use crate::adapters::native_lane_37::application::{
    handler, projection_facts_handler, EmptyProjectionFactsRead, ProjectionFactsRead, ReadRequest,
    SeatHistoryRead,
};
use crate::adapters::native_lane_37::domain::{valid_id, valid_month, valid_slug};
use crate::adapters::native_lane_37::postgres::{PgProjectionFactsRead, PgSeatHistoryRead};

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

/// Composition-side name for the exact registered Holding target.
pub type LaneTarget = Lane37Target;

#[derive(Deserialize)]
struct WindowQuery {
    start: Option<String>,
    end: Option<String>,
}

#[derive(Deserialize)]
struct MonthQuery {
    month: Option<String>,
}

struct TargetReaders {
    holding_id: String,
    seat: Arc<dyn SeatHistoryRead>,
    projection: Arc<dyn ProjectionFactsRead>,
}

struct Lane37State {
    targets: BTreeMap<(String, String), TargetReaders>,
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

/// Validate and index both owner readers per exact (Host, Holding slug) target.
fn configured_state(
    entries: Vec<(
        LaneTarget,
        Arc<dyn SeatHistoryRead>,
        Arc<dyn ProjectionFactsRead>,
    )>,
) -> Result<Arc<Lane37State>, BusinessError> {
    let mut configured: BTreeMap<(String, String), TargetReaders> = BTreeMap::new();
    for (target, seat, projection) in entries {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_id(&target.holding_id)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    TargetReaders {
                        holding_id: target.holding_id.clone(),
                        seat,
                        projection,
                    },
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Arc::new(Lane37State {
        targets: configured,
    }))
}

fn router_from_state(state: Arc<Lane37State>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/commercial/seat-history",
            get(history),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/commercial/usage-projection-facts",
            get(projection_facts),
        )
        .with_state(state)
}

/// Reader-injection path: one distinct reader per target, used by tests and by
/// the per-Holding pool constructor below. The projection reader defaults to a
/// no-op so the original seat-only constructors keep their behaviour.
pub fn router_with_readers(
    entries: Vec<(LaneTarget, Arc<dyn SeatHistoryRead>)>,
) -> Result<Router, BusinessError> {
    let projection: Arc<dyn ProjectionFactsRead> = Arc::new(EmptyProjectionFactsRead);
    Ok(router_from_state(configured_state(
        entries
            .into_iter()
            .map(|(target, seat)| (target, seat, projection.clone()))
            .collect(),
    )?))
}

/// Build the native seat-history router from one injected reader.
pub fn router_with_reader(
    reader: Arc<dyn SeatHistoryRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Build the native seat-history router with one owner sqlx-backed reader.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgSeatHistoryRead::new(pool)), targets)
}

/// Build the router with one `Pg...Read` per Holding/pool pair. Each target is
/// keyed by `(host, holding_slug)` and keeps its own reader, so a request for
/// one Holding can only ever execute on that Holding's database pool.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let entries = targets
        .into_iter()
        .map(|(target, pool)| {
            let seat: Arc<dyn SeatHistoryRead> = Arc::new(PgSeatHistoryRead::new(pool.clone()));
            let projection: Arc<dyn ProjectionFactsRead> =
                Arc::new(PgProjectionFactsRead::new(pool));
            (target, seat, projection)
        })
        .collect();
    Ok(router_from_state(configured_state(entries)?))
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
        let target_readers = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let expected_holding = &target_readers.holding_id;
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
            target_readers.seat.clone(),
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

async fn projection_facts(
    State(state): State<Arc<Lane37State>>,
    Path(slug): Path<String>,
    Query(query): Query<MonthQuery>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let target_readers = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let holding = unique_header(request.headers(), CONTEXT_HOLDING)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        if holding != target_readers.holding_id {
            return Err(invalid());
        }
        let month = query.month.as_deref().ok_or_else(invalid)?;
        if !valid_month(month) {
            return Err(invalid());
        }
        let value = projection_facts_handler(target_readers.projection.clone(), month).await?;
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

    fn target(host: &str, slug: &str, holding_id: &str) -> LaneTarget {
        LaneTarget {
            host: host.to_owned(),
            holding_slug: slug.to_owned(),
            holding_id: holding_id.to_owned(),
        }
    }

    #[test]
    fn two_holdings_route_to_two_distinct_readers() {
        let acme: Arc<dyn SeatHistoryRead> = Arc::new(EmptySeatHistoryRead);
        let northstar: Arc<dyn SeatHistoryRead> = Arc::new(EmptySeatHistoryRead);
        assert!(!Arc::ptr_eq(&acme, &northstar));
        let acme_projection: Arc<dyn ProjectionFactsRead> = Arc::new(EmptyProjectionFactsRead);
        let northstar_projection: Arc<dyn ProjectionFactsRead> = Arc::new(EmptyProjectionFactsRead);
        let entries = || {
            vec![
                (
                    target("acme.jesadakorn.com", "acme", "holding-1"),
                    acme.clone(),
                    acme_projection.clone(),
                ),
                (
                    target("northstar.jesadakorn.com", "northstar", "holding-2"),
                    northstar.clone(),
                    northstar_projection.clone(),
                ),
            ]
        };
        assert!(configured_state(entries()).is_ok());
        let state = configured_state(entries()).expect("state");
        assert_eq!(state.targets.len(), 2);
        let acme_target = state
            .targets
            .get(&("acme.jesadakorn.com".to_owned(), "acme".to_owned()))
            .expect("acme target");
        let northstar_target = state
            .targets
            .get(&(
                "northstar.jesadakorn.com".to_owned(),
                "northstar".to_owned(),
            ))
            .expect("northstar target");
        assert_eq!(acme_target.holding_id, "holding-1");
        assert_eq!(northstar_target.holding_id, "holding-2");
        assert!(Arc::ptr_eq(&acme_target.seat, &acme));
        assert!(Arc::ptr_eq(&northstar_target.seat, &northstar));
        assert!(!Arc::ptr_eq(&acme_target.seat, &northstar_target.seat));
        assert!(Arc::ptr_eq(&acme_target.projection, &acme_projection));
        assert!(Arc::ptr_eq(
            &northstar_target.projection,
            &northstar_projection
        ));
        assert!(!Arc::ptr_eq(
            &acme_target.projection,
            &northstar_target.projection
        ));
    }

    #[test]
    fn seat_only_constructors_keep_a_no_op_projection_reader() {
        let router = router_with_reader(
            Arc::new(EmptySeatHistoryRead),
            vec![target("acme.jesadakorn.com", "acme", "holding-1")],
        )
        .expect("router");
        let _ = router;
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
