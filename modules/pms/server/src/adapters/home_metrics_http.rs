//! Owner-local axum read router for the native PMS home hotel-metrics read.
//!
//! Rust owns the retained `hotelMetrics` statement, the scoped binds, the
//! ordering guard and the response shape. The handler executes no SQL itself:
//! it calls the application use case, which delegates to the sqlx adapter. The
//! TypeScript caller is a strict transport only and fails closed with 503.
//!
//! This is the sample bounded family cut out of the `/pms` catch-all. The
//! catch-all must be split into smaller families before its full deletion.

use std::{collections::BTreeMap, sync::Arc};

use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::json;

use crate::adapters::home_metrics_postgres::PgPmsHotelMetricsRead;
use crate::application::home_metrics::{
    PmsHotelMetricsFuture, pms_hotel_metrics,
};
use crate::domain::home_metrics::{PmsHotelMetric, PmsHotelMetricsFacts, ensure_ordered};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");
const CONTEXT_PROPERTY: HeaderName = HeaderName::from_static("x-erpx-context-property");

/// Exact registered Holding target for the mounted owner router.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HomeMetricsTarget {
    pub host: String,
    pub holding_slug: String,
}

/// Bounded owner reader injected into the router. The handler receives facts,
/// never SQL or a connection, so a test can prove the response shape without a
/// database. Production supplies the sqlx adapter below.
pub trait HomeMetricsRead: Send + Sync {
    fn hotel_metrics<'a>(
        &'a self,
        facts: &'a PmsHotelMetricsFacts,
    ) -> PmsHotelMetricsFuture<'a, Vec<PmsHotelMetric>, BusinessError>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_HOME_METRICS_UNAVAILABLE",
        "The native hotel metrics read is unavailable.",
    )
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_HOME_METRICS_REQUEST_INVALID",
        "Use one bounded hotel metrics read.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_HOME_METRICS_CONFIG_INVALID",
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

fn valid_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| !byte.is_ascii_control() && byte != b'?' && byte != b'#' && byte != b'\\')
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
        Json(json!({"error":{"code":error.code,"message":error.message}})),
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

/// One retained `hotelMetrics` row, rendered with its original snake_case
/// column names so the payload is byte-parity with the retired TypeScript read.
#[derive(Debug, serde::Serialize)]
struct HotelMetricRow {
    id: String,
    name: String,
    branch_id: String,
    business_date: String,
    state: String,
    currency: Option<String>,
    room_count: i64,
    inspected: i64,
    capacity: i64,
    sold: i64,
    arrivals: i64,
    departures: i64,
}

impl From<PmsHotelMetric> for HotelMetricRow {
    fn from(metric: PmsHotelMetric) -> Self {
        Self {
            id: metric.id,
            name: metric.name,
            branch_id: metric.branch_id,
            business_date: metric.business_date,
            state: metric.state,
            currency: metric.currency,
            room_count: metric.room_count,
            inspected: metric.inspected,
            capacity: metric.capacity,
            sold: metric.sold,
            arrivals: metric.arrivals,
            departures: metric.departures,
        }
    }
}

struct MetricsState {
    targets: BTreeMap<(String, String), Arc<dyn HomeMetricsRead>>,
}

/// Validate exact registered targets and index them by `(host, holding_slug)`.
/// Every constructor funnels through this one check so isolation and duplicate
/// rejection cannot drift between them.
fn configured_targets(
    entries: impl IntoIterator<Item = (HomeMetricsTarget, Arc<dyn HomeMetricsRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn HomeMetricsRead>>, BusinessError> {
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

fn read_router_from(
    configured: BTreeMap<(String, String), Arc<dyn HomeMetricsRead>>,
) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pms/home-metrics",
            get(home_metrics),
        )
        .with_state(Arc::new(MetricsState { targets: configured }))
}

/// Reader-injection path for tests: one distinct reader per Holding, so routing
/// and the complete payload can be proven without a database.
pub fn router_with_readers(
    entries: Vec<(HomeMetricsTarget, Arc<dyn HomeMetricsRead>)>,
) -> Result<Router, BusinessError> {
    Ok(read_router_from(configured_targets(entries)?))
}

/// Mount entry point with a single shared injected reader.
pub fn router_with_reader(
    reader: Arc<dyn HomeMetricsRead>,
    targets: Vec<HomeMetricsTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Mount entry point with one shared Pg pool.
pub fn router(pool: DatabasePool, targets: Vec<HomeMetricsTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgHomeMetricsRead::new(pool)), targets)
}

/// Mount entry point with one Pg reader per Holding pool. `pools[i]` belongs to
/// `targets[i]`, so no Holding can reach another's data.
pub fn router_for_holdings(
    targets: Vec<(HomeMetricsTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn HomeMetricsRead> =
                    Arc::new(PgHomeMetricsRead::new(pool));
                (target, reader)
            })
            .collect(),
    )
}

async fn home_metrics(
    State(state): State<Arc<MetricsState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(not_found)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(not_found)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_value(value))
            .ok_or_else(invalid)?
            .to_owned();
        let branch = match unique_header(request.headers(), CONTEXT_BRANCH)? {
            Some("") | None => None,
            Some(value) if valid_value(value) => Some(value.to_owned()),
            Some(_) => Err(invalid())?,
        };
        let property = match unique_header(request.headers(), CONTEXT_PROPERTY)? {
            Some("") | None => None,
            Some(value) if valid_value(value) => Some(value.to_owned()),
            Some(_) => Err(invalid())?,
        };
        let facts = PmsHotelMetricsFacts {
            company_id: company,
            branch_id: branch,
            property_id: property,
        };
        let rows = reader.hotel_metrics(&facts).await?;
        ensure_ordered(&rows)?;
        let hotels: Vec<HotelMetricRow> = rows.into_iter().map(HotelMetricRow::from).collect();
        Ok::<_, BusinessError>(json!({"hotels": hotels}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

/// sqlx-backed owner reader. Executes the retained statement on the caller's
/// owner pool inside a bounded unit; no other owner's table is read.
pub struct PgHomeMetricsRead {
    pool: DatabasePool,
}

impl PgHomeMetricsRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl HomeMetricsRead for PgHomeMetricsRead {
    fn hotel_metrics<'a>(
        &'a self,
        facts: &'a PmsHotelMetricsFacts,
    ) -> PmsHotelMetricsFuture<'a, Vec<PmsHotelMetric>, BusinessError> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let facts = facts.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let facts = facts.clone();
                Box::pin(async move {
                    let mut reader = PgPmsHotelMetricsRead::new(connection);
                    // The use case over the Pg adapter already returns
                    // Result<_, PgTransactionError<BusinessError>>, which is
                    // exactly PgWorkFuture's output. Remapping a BusinessError
                    // here would change the error channel and break `run`.
                    pms_hotel_metrics(&facts, &mut reader).await
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::body::{self, Body};

    struct FakeReader(Vec<PmsHotelMetric>);

    impl HomeMetricsRead for FakeReader {
        fn hotel_metrics<'a>(
            &'a self,
            _facts: &'a PmsHotelMetricsFacts,
        ) -> PmsHotelMetricsFuture<'a, Vec<PmsHotelMetric>, BusinessError> {
            let rows = self.0.clone();
            Box::pin(async move { Ok(rows) })
        }
    }

    fn metric(id: &str, name: &str) -> PmsHotelMetric {
        PmsHotelMetric {
            id: id.into(),
            name: name.into(),
            branch_id: "north".into(),
            business_date: "2026-11-04".into(),
            state: "open".into(),
            currency: Some("THB".into()),
            room_count: 3,
            inspected: 1,
            capacity: 2,
            sold: 2,
            arrivals: 1,
            departures: 1,
        }
    }

    fn target(host: &str, slug: &str) -> HomeMetricsTarget {
        HomeMetricsTarget {
            host: host.into(),
            holding_slug: slug.into(),
        }
    }

    fn state(targets: Vec<(HomeMetricsTarget, Arc<dyn HomeMetricsRead>)>) -> Arc<MetricsState> {
        Arc::new(MetricsState {
            targets: configured_targets(targets).expect("targets"),
        })
    }

    async fn call(
        state: Arc<MetricsState>,
        host: &str,
        slug: &str,
        company: Option<&str>,
    ) -> Response {
        let mut builder = Request::builder()
            .header(header::HOST, host)
            .uri(format!("/api/v1/holdings/{slug}/pms/home-metrics"));
        if let Some(company) = company {
            builder = builder.header(CONTEXT_COMPANY, company);
        }
        let request = builder.body(Body::empty()).expect("request");
        home_metrics(State(state), Path(slug.to_owned()), request).await
    }

    #[test]
    fn rejects_invalid_target_configuration() {
        assert_eq!(
            router_with_reader(Arc::new(FakeReader(vec![])), vec![])
                .unwrap_err()
                .code,
            "PMS_HOME_METRICS_CONFIG_INVALID"
        );
        assert_eq!(
            router_with_reader(
                Arc::new(FakeReader(vec![])),
                vec![target("acme.jesadakorn.com", "Acme")]
            )
            .unwrap_err()
            .code,
            "PMS_HOME_METRICS_CONFIG_INVALID"
        );
        assert_eq!(
            router_with_reader(
                Arc::new(FakeReader(vec![])),
                vec![target("acme.jesadakorn.com", "acme"), target("acme.jesadakorn.com", "acme")]
            )
            .unwrap_err()
            .code,
            "PMS_HOME_METRICS_CONFIG_INVALID"
        );
        assert!(router_with_reader(
            Arc::new(FakeReader(vec![])),
            vec![target("acme.jesadakorn.com", "acme")]
        )
        .is_ok());
    }

    #[test]
    fn maps_failure_kinds_and_bounds_success() {
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
        let response = bounded(json!({"hotels":[]}));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("private, no-store")
        );
        assert_eq!(error_response(invalid()).status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn full_payload_is_byte_parity_with_the_retired_read() {
        let state = state(vec![(
            target("acme.jesadakorn.com", "acme"),
            Arc::new(FakeReader(vec![metric("p1", "alpha")])) as Arc<dyn HomeMetricsRead>,
        )]);
        let request = Request::builder()
            .header(header::HOST, "acme.jesadakorn.com")
            .header(CONTEXT_COMPANY, "company-1")
            .header(CONTEXT_BRANCH, "north")
            .header(CONTEXT_PROPERTY, "p1")
            .uri("/api/v1/holdings/acme/pms/home-metrics")
            .body(Body::empty())
            .expect("request");
        let response = home_metrics(State(state), Path("acme".to_owned()), request).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        assert_eq!(
            String::from_utf8(bytes.to_vec()).expect("utf8"),
            r#"{"hotels":[{"id":"p1","name":"alpha","branch_id":"north","business_date":"2026-11-04","state":"open","currency":"THB","room_count":3,"inspected":1,"capacity":2,"sold":2,"arrivals":1,"departures":1}]}"#
        );
    }

    #[tokio::test]
    async fn each_holding_routes_to_its_own_reader_and_fails_closed() {
        let acme: Arc<dyn HomeMetricsRead> = Arc::new(FakeReader(vec![metric("acme-1", "alpha")]));
        let north: Arc<dyn HomeMetricsRead> = Arc::new(FakeReader(vec![metric("north-1", "bravo")]));
        let state = state(vec![
            (target("acme.example.com", "acme"), acme),
            (target("north.example.com", "northstar"), north),
        ]);

        for (host, slug, expected_id) in [
            ("acme.example.com", "acme", "acme-1"),
            ("north.example.com", "northstar", "north-1"),
        ] {
            let response = call(state.clone(), host, slug, Some("company-1")).await;
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = body::to_bytes(response.into_body(), 1024 * 1024)
                .await
                .expect("body");
            let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
            assert_eq!(value["hotels"][0]["id"], expected_id);
        }

        // Unknown target and missing company both fail closed.
        let unknown = call(state.clone(), "evil.example.com", "acme", Some("company-1")).await;
        assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
        let missing = call(state, "acme.example.com", "acme", None).await;
        assert_eq!(missing.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
