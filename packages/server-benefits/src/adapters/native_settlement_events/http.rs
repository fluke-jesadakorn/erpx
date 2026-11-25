//! Owner-local axum router for the Benefits settlement event-history read.

use super::application::{ReadSettlementEvents, SettlementEventRead};
use super::domain::{LaneTarget, SettlementEventsRequest, invalid, unavailable, valid_id};
use super::postgres::PgSettlementEventRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
    State, StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

struct LaneState {
    target: LaneTarget,
    handler: Arc<ReadSettlementEvents<Arc<dyn SettlementEventRead>>>,
}

struct ReadState {
    lanes: BTreeMap<(String, String), LaneState>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "BENEFIT_SETTLEMENT_EVENTS_READ_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

pub(crate) fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

pub(crate) fn valid_origin(origin: &str) -> bool {
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
    targets: Vec<(LaneTarget, Arc<dyn SettlementEventRead>)>,
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
                        handler: Arc::new(ReadSettlementEvents::new(reader)),
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
            "/api/v1/holdings/{holding_slug}/benefits/settlement-events",
            get(read),
        )
        .with_state(state)
}

/// Reader injection for unit tests: one independent reader per Holding.
pub fn router_with_reader(
    targets: Vec<(LaneTarget, Arc<dyn SettlementEventRead>)>,
) -> Result<Router, BusinessError> {
    Ok(into_router(build_state(targets)?))
}

/// Composition supplies one `(LaneTarget, DatabasePool)` pair per Holding.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let injected = targets
        .into_iter()
        .map(|(target, pool)| {
            let reader: Arc<dyn SettlementEventRead> = Arc::new(PgSettlementEventRead::new(pool));
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
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?
            .to_owned();
        let body = lane
            .handler
            .execute(SettlementEventsRequest { company_id: company })
            .await?;
        serde_json::to_value(body).map_err(|_| unavailable())
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
    use super::super::application::SettlementEventsFuture;
    use super::super::domain::SettlementEventFact;

    struct TaggedRead(&'static str);

    impl SettlementEventRead for TaggedRead {
        fn history<'a>(
            &'a self,
            _request: &'a SettlementEventsRequest,
        ) -> SettlementEventsFuture<'a, Vec<SettlementEventFact>> {
            let tag = self.0.to_owned();
            Box::pin(async move {
                Ok(vec![SettlementEventFact {
                    kind: "plan".into(),
                    image: tag,
                }])
            })
        }
    }

    #[test]
    fn rejects_duplicate_headers_and_invalid_targets() {
        let mut headers = HeaderMap::new();
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-1"));
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-2"));
        assert!(unique_header(&headers, CONTEXT_COMPANY).is_err());
        assert!(!valid_slug("Acme"));
        assert!(valid_slug("acme-1"));
        assert!(!valid_origin("http://acme.example.com"));
        assert!(valid_origin("https://acme.example.com"));
    }

    #[test]
    fn status_contract_maps_all_failure_kinds() {
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn distinct_holdings_keep_distinct_readers() {
        let a: Arc<dyn SettlementEventRead> = Arc::new(TaggedRead("acme"));
        let b: Arc<dyn SettlementEventRead> = Arc::new(TaggedRead("northstar"));
        let targets = vec![
            (LaneTarget { host: "acme.example.com".into(), holding_slug: "acme".into(), origin: "https://acme.example.com".into() }, a.clone()),
            (LaneTarget { host: "northstar.example.com".into(), holding_slug: "northstar".into(), origin: "https://northstar.example.com".into() }, b.clone()),
        ];
        assert!(router_with_reader(targets.iter().map(|(t, r)| (t.clone(), r.clone())).collect()).is_ok());
        let state = build_state(targets).unwrap();
        let lane_a = state.lanes.get(&("acme.example.com".into(), "acme".into())).unwrap();
        let lane_b = state.lanes.get(&("northstar.example.com".into(), "northstar".into())).unwrap();
        assert!(Arc::ptr_eq(lane_a.handler.reader(), &a));
        assert!(Arc::ptr_eq(lane_b.handler.reader(), &b));
        assert!(!Arc::ptr_eq(lane_a.handler.reader(), lane_b.handler.reader()));
    }
}
