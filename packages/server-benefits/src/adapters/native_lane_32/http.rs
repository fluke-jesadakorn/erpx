//! Owner-local axum router for the lane-32 Benefits payroll-input read.
//!
//! The router transports bounded identities only. It enforces registered
//! target/Host/Origin isolation, requires a well-formed Company scope and maps
//! owner errors to the shared BusinessError response shape. No SQL is present
//! in this module; the owner reader executes the statement.

use super::application::{PayrollInputRead, ReadPayrollInputs};
use super::domain::{
    LaneTarget, PayrollInputsRequest, invalid, unavailable, valid_id,
};
use super::postgres::PgPayrollInputRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
    State, StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_ENGAGEMENT: HeaderName = HeaderName::from_static("x-erpx-benefits-engagement");
const CONTEXT_CURRENCY: HeaderName = HeaderName::from_static("x-erpx-benefits-currency");
const CONTEXT_PERIOD_END: HeaderName = HeaderName::from_static("x-erpx-benefits-period-end");
const CONTEXT_PERIOD_START: HeaderName = HeaderName::from_static("x-erpx-benefits-period-start");

type Handler = ReadPayrollInputs<Arc<dyn PayrollInputRead>>;

/// One registered Holding: its target identity and its own injected reader.
/// The reader owns its pool, so Holding A can never borrow Holding B's pool.
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
        "BENEFIT_PAYROLL_INPUT_READ_CONFIG_INVALID",
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

/// Build the owner read router. The lead mounts it; no other module is edited.
/// Validate the registered pairs and build the per-Holding state. Each target
/// keeps its own reader; no reader is shared across a `(host, slug)` boundary.
fn build_state(
    targets: Vec<(LaneTarget, Arc<dyn PayrollInputRead>)>,
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
                        handler: Arc::new(ReadPayrollInputs::new(reader)),
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
            "/api/v1/holdings/{holding_slug}/benefits/payroll-inputs",
            get(read),
        )
        .with_state(state)
}

/// Backward-compatible single-pool constructor: one Pg reader shared by every
/// registered target. Kept for the existing mounted-route shape and tests.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn PayrollInputRead> = Arc::new(PgPayrollInputRead::new(pool));
    let injected = targets
        .into_iter()
        .map(|target| (target, reader.clone()))
        .collect();
    Ok(into_router(build_state(injected)?))
}

/// Reader-injection constructor used by unit tests. Each target carries its
/// own `Arc<dyn PayrollInputRead>`, so distinct readers/pools are observable.
pub fn router_with_reader(
    targets: Vec<(LaneTarget, Arc<dyn PayrollInputRead>)>,
) -> Result<Router, BusinessError> {
    Ok(into_router(build_state(targets)?))
}

/// Per-Holding constructor. Composition supplies one `(LaneTarget, DatabasePool)`
/// pair per Holding; this builds one `PgPayrollInputRead` per pair and inserts
/// it keyed by `(host, holding_slug)`, so every Holding reads its own database.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let injected = targets
        .into_iter()
        .map(|(target, pool)| {
            let reader: Arc<dyn PayrollInputRead> = Arc::new(PgPayrollInputRead::new(pool));
            (target, reader)
        })
        .collect();
    Ok(into_router(build_state(injected)?))
}

fn required_header(
    headers: &HeaderMap,
    name: HeaderName,
) -> Result<String, BusinessError> {
    unique_header(headers, name)?
        .filter(|value| valid_id(value))
        .map(str::to_owned)
        .ok_or_else(invalid)
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
        let company = required_header(&headers, CONTEXT_COMPANY)?;
        let engagement_id = required_header(&headers, CONTEXT_ENGAGEMENT)?;
        let currency = required_header(&headers, CONTEXT_CURRENCY)?;
        let period_end = required_header(&headers, CONTEXT_PERIOD_END)?;
        let period_start = required_header(&headers, CONTEXT_PERIOD_START)?;
        let body = lane
            .handler
            .execute(PayrollInputsRequest {
                company_id: company,
                engagement_id,
                currency,
                period_end,
                period_start,
            })
            .await?;
        Ok::<_, BusinessError>(serde_json::to_value(body).map_err(|_| unavailable())?)
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
    use super::super::application::PayrollInputReadFuture;
    use super::super::domain::PayrollInputFact;

    #[test]
    fn rejects_duplicate_headers() {
        let mut headers = HeaderMap::new();
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-1"));
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-2"));
        assert!(unique_header(&headers, CONTEXT_COMPANY).is_err());
    }

    #[test]
    fn rejects_unregistered_origin_and_slug() {
        assert!(!valid_origin("http://acme.example.com"));
        assert!(!valid_origin("https://acme.example.com/evil"));
        assert!(valid_origin("https://acme.example.com"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug(""));
        assert!(valid_slug("acme-1"));
    }

    #[test]
    fn maps_failure_kinds_to_the_shared_status_contract() {
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
    }

    #[test]
    fn error_response_pins_the_error_shape() {
        let response = error_response(unavailable());
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    struct TaggedRead(&'static str);

    impl PayrollInputRead for TaggedRead {
        fn released<'a>(
            &'a self,
            _request: &'a PayrollInputsRequest,
        ) -> PayrollInputReadFuture<'a, Vec<PayrollInputFact>> {
            let plan = self.0.to_owned();
            Box::pin(async move {
                Ok(vec![PayrollInputFact {
                    id: format!("input-{plan}"),
                    plan_name: plan,
                    employee_deduction_minor: 100,
                    employer_cost_minor: 200,
                    source_election_version: 3,
                }])
            })
        }
    }

    #[tokio::test]
    async fn reader_injection_routes_two_holdings_to_two_distinct_readers() {
        let acme: Arc<dyn PayrollInputRead> = Arc::new(TaggedRead("acme"));
        let northstar: Arc<dyn PayrollInputRead> = Arc::new(TaggedRead("northstar"));
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
        // The state built by `router_with_reader` keeps one reader per key.
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

        let request = PayrollInputsRequest {
            company_id: "company-1".into(),
            engagement_id: "engagement-1".into(),
            currency: "THB".into(),
            period_end: "2026-12-31".into(),
            period_start: "2026-01-01".into(),
        };
        let from_a = lane_a.handler.execute(request.clone()).await.unwrap();
        let from_b = lane_b.handler.execute(request).await.unwrap();
        assert_eq!(from_a.inputs[0].plan_name, "acme");
        assert_eq!(from_b.inputs[0].plan_name, "northstar");
    }
}
