//! Native lane 3, wave 2: Finance setup reference counts.
//!
//! Owner-local successor of five company-scoped SELECTs in
//! `adapters/finance-workspace.ts`. The axum router owns the statements, the
//! company scope and the response shape; the retained TypeScript adapter is a
//! strict, fail-closed multi-Holding transport with no SQL fallback.

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

/// Statement bytes inherited from the retired TypeScript callbacks.
pub const SQL_ACCOUNT_COUNT: &str =
    "SELECT count(*) count FROM finance_accounts WHERE company_id=? AND status='active'";
pub const SQL_PERIOD_COUNT: &str = "SELECT count(*) count FROM finance_periods WHERE company_id=?";
pub const SQL_TAX_COUNT: &str =
    "SELECT count(*) count FROM finance_tax_codes WHERE company_id=? AND status='active'";
pub const SQL_BANK_COUNT: &str =
    "SELECT count(DISTINCT bank_account) count FROM finance_bank_statements WHERE company_id=?";
pub const SQL_FIRST_CURRENCY: &str =
    "SELECT base_currency FROM finance_charts WHERE company_id=? ORDER BY created_at LIMIT 1";

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
/// Node's fetch overwrites `Host`, so the private transport carries the exact
/// registered Holding host in a dedicated header as well.
const NATIVE_HOST: HeaderName = HeaderName::from_static("x-erpx-native-host");
const MAX_BODY: usize = 4 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SetupCounts {
    pub account_count: i64,
    pub period_count: i64,
    pub tax_count: i64,
    pub bank_count: i64,
    pub first_currency: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SetupResponse {
    pub data: SetupData,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SetupData {
    pub setup: SetupCounts,
}

impl SetupResponse {
    pub fn new(setup: SetupCounts) -> Self {
        Self { data: SetupData { setup } }
    }
}

pub type SetupCountsFuture<'a> =
    Pin<Box<dyn Future<Output = Result<SetupCounts, BusinessError>> + Send + 'a>>;

pub trait SetupCountsRead: Send + Sync {
    fn counts<'a>(&'a self, company: &'a str) -> SetupCountsFuture<'a>;
}

pub async fn read_setup(
    reader: &dyn SetupCountsRead,
    company: &str,
) -> Result<SetupCounts, BusinessError> {
    if !valid_id(company) {
        return Err(invalid());
    }
    reader.counts(company).await
}

fn invalid() -> BusinessError {
    BusinessError::invalid("LANE3_SETUP_COUNTS_INPUT_INVALID", "Use one bounded Company.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE3_SETUP_COUNTS_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LANE3_SETUP_COUNTS_UNAVAILABLE",
        "The native Finance setup-counts service is unavailable.",
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

/// Oracle for the five exact SQL bytes executed by the owner adapter.
pub fn sql_oracles() -> [String; 5] {
    [
        hex(SQL_ACCOUNT_COUNT),
        hex(SQL_PERIOD_COUNT),
        hex(SQL_TAX_COUNT),
        hex(SQL_BANK_COUNT),
        hex(SQL_FIRST_CURRENCY),
    ]
}

/// Oracle for a canonical (sorted-key) result projection.
pub fn result_oracle(counts: &SetupCounts) -> String {
    let value = serde_json::to_value(counts).expect("counts serialize");
    hex(serde_json::to_string(&value).expect("value serializes"))
}

pub struct PgSetupCountsRead {
    pool: DatabasePool,
}

impl PgSetupCountsRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl SetupCountsRead for PgSetupCountsRead {
    fn counts<'a>(&'a self, company: &'a str) -> SetupCountsFuture<'a> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                Box::pin(async move {
                    let account_count: i64 = sqlx::query_scalar(SQL_ACCOUNT_COUNT)
                        .bind(company.as_str())
                        .fetch_one(&mut *connection)
                        .await?;
                    let period_count: i64 = sqlx::query_scalar(SQL_PERIOD_COUNT)
                        .bind(company.as_str())
                        .fetch_one(&mut *connection)
                        .await?;
                    let tax_count: i64 = sqlx::query_scalar(SQL_TAX_COUNT)
                        .bind(company.as_str())
                        .fetch_one(&mut *connection)
                        .await?;
                    let bank_count: i64 = sqlx::query_scalar(SQL_BANK_COUNT)
                        .bind(company.as_str())
                        .fetch_one(&mut *connection)
                        .await?;
                    let first_currency: Option<String> = sqlx::query_scalar(SQL_FIRST_CURRENCY)
                        .bind(company.as_str())
                        .fetch_optional(&mut *connection)
                        .await?;
                    Ok(SetupCounts {
                        account_count,
                        period_count,
                        tax_count,
                        bank_count,
                        first_currency,
                    })
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

struct SetupState {
    targets: BTreeMap<(String, String), Arc<dyn SetupCountsRead>>,
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
    targets: Vec<(LaneTarget, Arc<dyn SetupCountsRead>)>,
) -> Result<Arc<SetupState>, BusinessError> {
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
    Ok(Arc::new(SetupState { targets: configured }))
}

fn into_router(state: Arc<SetupState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/lane3/setup-counts",
            post(read),
        )
        .with_state(state)
}

/// Backward-compatible single-pool constructor.
pub fn setup_router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn SetupCountsRead> = Arc::new(PgSetupCountsRead::new(pool));
    let injected = targets
        .into_iter()
        .map(|target| (target, reader.clone()))
        .collect();
    Ok(into_router(build_state(injected)?))
}

/// Injectable variant used by the owner's own tests.
pub fn setup_router_with_reader(
    targets: Vec<(LaneTarget, Arc<dyn SetupCountsRead>)>,
) -> Result<Router, BusinessError> {
    Ok(into_router(build_state(targets)?))
}

/// Per-Holding constructor: one reader per registered `(target, pool)` pair.
pub fn setup_router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let injected = targets
        .into_iter()
        .map(|(target, pool)| {
            let reader: Arc<dyn SetupCountsRead> = Arc::new(PgSetupCountsRead::new(pool));
            (target, reader)
        })
        .collect();
    Ok(into_router(build_state(injected)?))
}

async fn read(
    State(state): State<Arc<SetupState>>,
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
        // Reject a caller-supplied body or query: this read takes no predicate.
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let bytes = body::to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|_| invalid())?;
        if !bytes.is_empty() {
            return Err(invalid());
        }
        let counts = read_setup(reader.as_ref(), &company).await?;
        Ok::<_, BusinessError>(SetupResponse::new(counts))
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

    pub fn fixture() -> SetupCounts {
        SetupCounts {
            account_count: 12,
            period_count: 3,
            tax_count: 4,
            bank_count: 2,
            first_currency: Some("THB".to_owned()),
        }
    }

    fn target(host: &str, slug: &str) -> LaneTarget {
        LaneTarget { host: host.into(), holding_slug: slug.into() }
    }

    struct Fake(SetupCounts);
    impl SetupCountsRead for Fake {
        fn counts<'a>(&'a self, company: &'a str) -> SetupCountsFuture<'a> {
            let counts = self.0.clone();
            let company = company.to_owned();
            Box::pin(async move {
                if company != "company-1" {
                    return Err(invalid());
                }
                Ok(counts)
            })
        }
    }

    fn reader() -> Arc<dyn SetupCountsRead> {
        Arc::new(Fake(fixture()))
    }

    #[test]
    fn statement_bytes_and_oracles_are_stable() {
        assert_eq!(sql_oracles(), [
            hex(SQL_ACCOUNT_COUNT),
            hex(SQL_PERIOD_COUNT),
            hex(SQL_TAX_COUNT),
            hex(SQL_BANK_COUNT),
            hex(SQL_FIRST_CURRENCY),
        ]);
        assert_eq!(SQL_ACCOUNT_COUNT, "SELECT count(*) count FROM finance_accounts WHERE company_id=? AND status='active'");
        assert_eq!(SQL_PERIOD_COUNT, "SELECT count(*) count FROM finance_periods WHERE company_id=?");
        assert_eq!(SQL_TAX_COUNT, "SELECT count(*) count FROM finance_tax_codes WHERE company_id=? AND status='active'");
        assert_eq!(SQL_BANK_COUNT, "SELECT count(DISTINCT bank_account) count FROM finance_bank_statements WHERE company_id=?");
        assert_eq!(SQL_FIRST_CURRENCY, "SELECT base_currency FROM finance_charts WHERE company_id=? ORDER BY created_at LIMIT 1");
    }

    #[test]
    fn result_oracle_is_stable() {
        assert_eq!(
            result_oracle(&fixture()),
            "9db081ab14783f3cb24e0fdf1a92478bf6f6a05f2b83fa7413c0693dc1bd7ebc"
        );
    }

    #[test]
    fn router_rejects_empty_duplicate_and_invalid_targets() {
        assert_eq!(
            setup_router_with_reader(vec![]).err().expect("empty").code,
            "LANE3_SETUP_COUNTS_CONFIG_INVALID"
        );
        assert_eq!(
            setup_router_with_reader(vec![
                (target("acme.example.com", "acme"), reader()),
                (target("acme.example.com", "acme"), reader()),
            ])
            .err()
            .expect("duplicate")
            .code,
            "LANE3_SETUP_COUNTS_CONFIG_INVALID"
        );
        for bad in [target("", "acme"), target("acme.example.com", "Acme")] {
            assert_eq!(
                setup_router_with_reader(vec![(bad, reader())])
                    .err()
                    .expect("invalid")
                    .code,
                "LANE3_SETUP_COUNTS_CONFIG_INVALID"
            );
        }
    }

    #[tokio::test]
    async fn two_targets_route_to_two_distinct_readers() {
        let a: Arc<dyn SetupCountsRead> = Arc::new(Fake(SetupCounts {
            account_count: 1,
            ..fixture()
        }));
        let b: Arc<dyn SetupCountsRead> = Arc::new(Fake(SetupCounts {
            account_count: 2,
            ..fixture()
        }));
        assert!(!Arc::ptr_eq(&a, &b));
        let state = Arc::new(SetupState {
            targets: BTreeMap::from([
                (("acme.example.com".into(), "acme".into()), a),
                (("northstar.example.com".into(), "northstar".into()), b),
            ]),
        });
        let request = |host: &str, slug: &str| {
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/holdings/{slug}/finance/lane3/setup-counts"))
                .header("host", host)
                .header("x-erpx-native-host", host)
                .header("x-erpx-context-company", "company-1")
                .body(body::Body::empty())
                .expect("request")
        };
        for (host, slug, expected) in [
            ("acme.example.com", "acme", 1),
            ("northstar.example.com", "northstar", 2),
        ] {
            let response = read(State(state.clone()), Path(slug.into()), request(host, slug)).await;
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = body::to_bytes(response.into_body(), 16 * 1024).await.expect("body");
            let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
            assert_eq!(value["data"]["setup"]["accountCount"], expected);
        }
    }

    #[tokio::test]
    async fn unavailable_maps_to_503() {
        let response = error_response(unavailable());
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = body::to_bytes(response.into_body(), 1024).await.expect("body");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(value["error"]["code"], "LANE3_SETUP_COUNTS_UNAVAILABLE");
    }
}
