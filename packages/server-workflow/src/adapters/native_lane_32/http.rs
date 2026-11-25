//! Owner-local axum router for the lane-32 Workflow failed-metrics read.
//!
//! The router transports no predicate and no caller scope. It enforces
//! registered target/Host/Origin isolation and maps owner failures to the
//! shared BusinessError response shape.

use super::application::{FailedMetricsRead, ReadFailedMetrics};
use super::domain::{LaneTarget, invalid, unavailable};
use super::postgres::PgFailedMetricsRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
    State, StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

type Handler = ReadFailedMetrics<Arc<dyn FailedMetricsRead>>;

struct LaneState {
    target: LaneTarget,
    handler: Arc<Handler>,
}

struct ReadState {
    lanes: BTreeMap<(String, String), LaneState>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "WORKFLOW_FAILED_METRICS_READ_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host.bytes().any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
            })
    })
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

pub(crate) fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

pub(crate) fn error_response(error: BusinessError) -> Response {
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

fn build_state(
    targets: Vec<(LaneTarget, Arc<dyn FailedMetricsRead>)>,
) -> Result<Arc<ReadState>, BusinessError> {
    let mut lanes = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || lanes
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    LaneState {
                        target,
                        handler: Arc::new(ReadFailedMetrics::new(reader)),
                    },
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if lanes.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Arc::new(ReadState { lanes }))
}

fn into_router(state: Arc<ReadState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/workflow/inbox-failed-metrics",
            get(read),
        )
        .with_state(state)
}

/// Backward-compatible single-pool constructor.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn FailedMetricsRead> = Arc::new(PgFailedMetricsRead::new(pool));
    let injected = targets
        .into_iter()
        .map(|target| (target, reader.clone()))
        .collect();
    Ok(into_router(build_state(injected)?))
}

/// Reader-injection constructor used by unit tests.
pub fn router_with_reader(
    targets: Vec<(LaneTarget, Arc<dyn FailedMetricsRead>)>,
) -> Result<Router, BusinessError> {
    Ok(into_router(build_state(targets)?))
}

/// One `PgFailedMetricsRead` per Holding pool, keyed by `(host, holding_slug)`.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let injected = targets
        .into_iter()
        .map(|(target, pool)| {
            let reader: Arc<dyn FailedMetricsRead> = Arc::new(PgFailedMetricsRead::new(pool));
            (target, reader)
        })
        .collect();
    Ok(into_router(build_state(injected)?))
}

async fn read(
    State(state): State<Arc<ReadState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let headers = request.headers().clone();
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let lane = state
            .lanes
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        if let Some(origin) = unique_header(&headers, header::ORIGIN)? {
            if origin != lane.target.origin {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "ORIGIN_FORBIDDEN",
                    "Use the registered application origin.",
                ));
            }
        }
        let metrics = lane.handler.execute().await?;
        Ok::<_, BusinessError>(serde_json::to_value(metrics).map_err(|_| unavailable())?)
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::super::application::FailedMetricsReadFuture;
    use super::*;

    struct TaggedRead(i64);

    impl FailedMetricsRead for TaggedRead {
        fn failed<'a>(&'a self) -> FailedMetricsReadFuture<'a, i64> {
            let count = self.0;
            Box::pin(async move { Ok(count) })
        }
    }

    #[test]
    fn rejects_duplicate_headers() {
        let mut headers = HeaderMap::new();
        headers.append(header::HOST, HeaderValue::from_static("acme.example.com"));
        headers.append(header::HOST, HeaderValue::from_static("other.example.com"));
        assert!(unique_header(&headers, header::HOST).is_err());
    }

    #[test]
    fn rejects_invalid_origins_and_slugs() {
        assert!(!valid_origin("http://acme.example.com"));
        assert!(!valid_origin("https://acme.example.com/x"));
        assert!(valid_origin("https://acme.example.com"));
        assert!(!valid_slug("Acme"));
        assert!(valid_slug("acme-1"));
    }

    #[test]
    fn maps_failure_kinds_to_the_shared_status_contract() {
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn reader_injection_routes_two_holdings_to_two_distinct_readers() {
        let acme: Arc<dyn FailedMetricsRead> = Arc::new(TaggedRead(3));
        let northstar: Arc<dyn FailedMetricsRead> = Arc::new(TaggedRead(9));
        let targets = vec![
            (
                LaneTarget {
                    host: "acme.example.com".into(),
                    holding_slug: "acme".into(),
                    origin: "https://acme.example.com".into(),
                },
                acme.clone(),
            ),
            (
                LaneTarget {
                    host: "northstar.example.com".into(),
                    holding_slug: "northstar".into(),
                    origin: "https://northstar.example.com".into(),
                },
                northstar.clone(),
            ),
        ];
        assert!(router_with_reader(
            targets
                .iter()
                .map(|(target, reader)| (target.clone(), reader.clone()))
                .collect(),
        )
        .is_ok());
        let state = build_state(targets).unwrap();
        let lane_a = state
            .lanes
            .get(&("acme.example.com".into(), "acme".into()))
            .expect("acme lane");
        let lane_b = state
            .lanes
            .get(&("northstar.example.com".into(), "northstar".into()))
            .expect("northstar lane");
        assert!(Arc::ptr_eq(lane_a.handler.reader(), &acme));
        assert!(Arc::ptr_eq(lane_b.handler.reader(), &northstar));
        assert!(!Arc::ptr_eq(lane_a.handler.reader(), lane_b.handler.reader()));
        assert_eq!(lane_a.handler.execute().await.unwrap().count, 3);
        assert_eq!(lane_b.handler.execute().await.unwrap().count, 9);
    }
}
