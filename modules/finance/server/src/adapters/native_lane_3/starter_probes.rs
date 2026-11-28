//! Native lane 3, wave 2: Finance starter setup probes.
//!
//! Owner-local successor of the two company-scoped existence probes in
//! `adapters/finance-starter.ts`. The axum router owns the statements, the
//! company scope and the boolean response; the TypeScript adapter is a strict,
//! fail-closed multi-Holding transport with no SQL fallback.

use super::domain::valid_id;
use super::http::LaneTarget;
use erpx_platform_runtime_server::public::http_support::{
    body, header, serde_json, HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request,
    Response, Router, State, StatusCode, post,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

/// Statement bytes inherited from the retired TypeScript probes.
pub const SQL_EXISTING_CHART: &str = "SELECT 1 FROM finance_charts WHERE company_id=?";
/// Bind order is preserved from the TS caller: company, end, start.
pub const SQL_OVERLAPPING_PERIOD: &str =
    "SELECT 1 FROM finance_periods WHERE company_id=? AND start_date<=? AND end_date>=?";

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const NATIVE_HOST: HeaderName = HeaderName::from_static("x-erpx-native-host");
const MAX_BODY: usize = 4 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StarterProbes {
    pub existing_chart: bool,
    pub overlapping_period: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ProbesResponse {
    pub data: ProbesData,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ProbesData {
    pub probes: StarterProbes,
}

impl ProbesResponse {
    pub fn new(probes: StarterProbes) -> Self {
        Self { data: ProbesData { probes } }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeRequest {
    pub start: String,
    pub end: String,
}

impl ProbeRequest {
    fn validate(&self) -> Result<(), BusinessError> {
        // ISO date bounds: YYYY-MM-DD, bounded and lexically ordered.
        let iso = |v: &str| {
            v.len() == 10
                && v.as_bytes()[4] == b'-'
                && v.as_bytes()[7] == b'-'
                && v.bytes().enumerate().all(|(i, b)| {
                    i == 4 || i == 7 || b.is_ascii_digit()
                })
        };
        if !iso(&self.start) || !iso(&self.end) || self.start > self.end {
            return Err(invalid());
        }
        Ok(())
    }
}

pub type StarterProbeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<StarterProbes, BusinessError>> + Send + 'a>>;

pub trait StarterProbeRead: Send + Sync {
    fn probes<'a>(
        &'a self,
        company: &'a str,
        start: &'a str,
        end: &'a str,
    ) -> StarterProbeFuture<'a>;
}

pub async fn read_probes(
    reader: &dyn StarterProbeRead,
    company: &str,
    start: &str,
    end: &str,
) -> Result<StarterProbes, BusinessError> {
    if !valid_id(company) || !valid_id(start) || !valid_id(end) || start > end {
        return Err(invalid());
    }
    reader.probes(company, start, end).await
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE3_STARTER_PROBES_INPUT_INVALID",
        "Use one bounded Company and ISO date range.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE3_STARTER_PROBES_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LANE3_STARTER_PROBES_UNAVAILABLE",
        "The native Finance starter service is unavailable.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes.as_ref());
    format!("{:x}", hash.finalize())
}

pub fn sql_oracles() -> [String; 2] {
    [hex(SQL_EXISTING_CHART), hex(SQL_OVERLAPPING_PERIOD)]
}

pub fn result_oracle(probes: &StarterProbes) -> String {
    let value = serde_json::to_value(probes).expect("probes serialize");
    hex(serde_json::to_string(&value).expect("value serializes"))
}

pub struct PgStarterProbeRead {
    pool: DatabasePool,
}

impl PgStarterProbeRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl StarterProbeRead for PgStarterProbeRead {
    fn probes<'a>(
        &'a self,
        company: &'a str,
        start: &'a str,
        end: &'a str,
    ) -> StarterProbeFuture<'a> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let start = start.to_owned();
        let end = end.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let start = start.clone();
                let end = end.clone();
                Box::pin(async move {
                    let existing_chart = sqlx::query_scalar::<_, i32>(SQL_EXISTING_CHART)
                        .bind(company.as_str())
                        .fetch_optional(&mut *connection)
                        .await?
                        .is_some();
                    let overlapping_period = sqlx::query_scalar::<_, i32>(SQL_OVERLAPPING_PERIOD)
                        .bind(company.as_str())
                        .bind(end.as_str())
                        .bind(start.as_str())
                        .fetch_optional(&mut *connection)
                        .await?
                        .is_some();
                    Ok(StarterProbes { existing_chart, overlapping_period })
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

struct ProbeState {
    targets: BTreeMap<(String, String), Arc<dyn StarterProbeRead>>,
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

fn bounded(body: impl Serialize) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response.headers_mut().insert(name, HeaderValue::from_static(value));
    }
    response
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

fn build_state(
    targets: Vec<(LaneTarget, Arc<dyn StarterProbeRead>)>,
) -> Result<Arc<ProbeState>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
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
    Ok(Arc::new(ProbeState { targets: configured }))
}

fn into_router(state: Arc<ProbeState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/lane3/starter-probes",
            post(read),
        )
        .with_state(state)
}

pub fn starter_router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn StarterProbeRead> = Arc::new(PgStarterProbeRead::new(pool));
    let injected = targets
        .into_iter()
        .map(|target| (target, reader.clone()))
        .collect();
    Ok(into_router(build_state(injected)?))
}

pub fn starter_router_with_reader(
    targets: Vec<(LaneTarget, Arc<dyn StarterProbeRead>)>,
) -> Result<Router, BusinessError> {
    Ok(into_router(build_state(targets)?))
}

pub fn starter_router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let injected = targets
        .into_iter()
        .map(|(target, pool)| {
            let reader: Arc<dyn StarterProbeRead> = Arc::new(PgStarterProbeRead::new(pool));
            (target, reader)
        })
        .collect();
    Ok(into_router(build_state(injected)?))
}

async fn read(
    State(state): State<Arc<ProbeState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), NATIVE_HOST)?
            .or(unique_header(request.headers(), header::HOST)?)
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let reader = state
            .targets
            .get(&(host, slug.clone()))
            .ok_or_else(missing)?
            .clone();
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?
            .to_owned();
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let bytes = body::to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|_| invalid())?;
        let payload: ProbeRequest = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        payload.validate()?;
        let probes = read_probes(reader.as_ref(), &company, &payload.start, &payload.end).await?;
        Ok::<_, BusinessError>(ProbesResponse::new(probes))
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

    fn fixture() -> StarterProbes {
        StarterProbes { existing_chart: true, overlapping_period: false }
    }

    fn target(host: &str, slug: &str) -> LaneTarget {
        LaneTarget { host: host.into(), holding_slug: slug.into() }
    }

    struct Fake(StarterProbes);
    impl StarterProbeRead for Fake {
        fn probes<'a>(
            &'a self,
            company: &'a str,
            _start: &'a str,
            _end: &'a str,
        ) -> StarterProbeFuture<'a> {
            let probes = self.0.clone();
            let company = company.to_owned();
            Box::pin(async move {
                if company != "company-1" {
                    return Err(invalid());
                }
                Ok(probes)
            })
        }
    }

    fn reader() -> Arc<dyn StarterProbeRead> {
        Arc::new(Fake(fixture()))
    }

    #[test]
    fn statement_bytes_and_oracles_are_stable() {
        assert_eq!(SQL_EXISTING_CHART, "SELECT 1 FROM finance_charts WHERE company_id=?");
        assert_eq!(
            SQL_OVERLAPPING_PERIOD,
            "SELECT 1 FROM finance_periods WHERE company_id=? AND start_date<=? AND end_date>=?"
        );
        assert_eq!(sql_oracles(), [hex(SQL_EXISTING_CHART), hex(SQL_OVERLAPPING_PERIOD)]);
    }

    #[test]
    fn result_oracle_is_stable() {
        assert_eq!(result_oracle(&fixture()), "3479f16cfe2b1389cb0bc790bc8e3ef7d9068459fb420a7d2d934d2c159e4d67");
    }

    #[test]
    fn router_rejects_empty_duplicate_and_invalid_targets() {
        assert_eq!(
            starter_router_with_reader(vec![]).err().expect("empty").code,
            "LANE3_STARTER_PROBES_CONFIG_INVALID"
        );
        assert_eq!(
            starter_router_with_reader(vec![
                (target("a.example.com", "a"), reader()),
                (target("a.example.com", "a"), reader()),
            ])
            .err()
            .expect("duplicate")
            .code,
            "LANE3_STARTER_PROBES_CONFIG_INVALID"
        );
    }

    #[test]
    fn date_bounds_are_validated() {
        assert!(ProbeRequest { start: "2026-01-01".into(), end: "2026-12-31".into() }
            .validate()
            .is_ok());
        assert!(ProbeRequest { start: "2026-12-31".into(), end: "2026-01-01".into() }
            .validate()
            .is_err());
        assert!(ProbeRequest { start: "2026/01/01".into(), end: "2026-12-31".into() }
            .validate()
            .is_err());
    }

    #[tokio::test]
    async fn two_targets_route_to_two_distinct_readers() {
        let a: Arc<dyn StarterProbeRead> = Arc::new(Fake(StarterProbes { existing_chart: true, overlapping_period: false }));
        let b: Arc<dyn StarterProbeRead> = Arc::new(Fake(StarterProbes { existing_chart: false, overlapping_period: true }));
        assert!(!Arc::ptr_eq(&a, &b));
        let state = Arc::new(ProbeState {
            targets: BTreeMap::from([
                (("a.example.com".into(), "acme".into()), a),
                (("b.example.com".into(), "northstar".into()), b),
            ]),
        });
        let request = |host: &str, slug: &str| {
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/holdings/{slug}/finance/lane3/starter-probes"))
                .header("host", host)
                .header("x-erpx-native-host", host)
                .header("x-erpx-context-company", "company-1")
                .body(body::Body::from(
                    serde_json::to_vec(&serde_json::json!({"start":"2026-01-01","end":"2026-12-31"}))
                        .expect("body"),
                ))
                .expect("request")
        };
        for (host, slug, expected) in [
            ("a.example.com", "acme", true),
            ("b.example.com", "northstar", false),
        ] {
            let response = read(State(state.clone()), Path(slug.into()), request(host, slug)).await;
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = body::to_bytes(response.into_body(), 16 * 1024).await.expect("body");
            let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
            assert_eq!(value["data"]["probes"]["existingChart"], expected);
        }
    }

    #[tokio::test]
    async fn unavailable_maps_to_503() {
        let response = error_response(unavailable());
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = body::to_bytes(response.into_body(), 1024).await.expect("body");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(value["error"]["code"], "LANE3_STARTER_PROBES_UNAVAILABLE");
    }
}
