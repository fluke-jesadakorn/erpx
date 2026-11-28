//! Owner-local axum ingress for the native Finance reference-option reads.
//!
//! Follows `server-commerce::quality_product_read`: exact registered Holding
//! host + slug targets, a single bounded Company header, no caller SQL, and a
//! fail-closed error envelope. Composition mounts this router; this lane never
//! edits composition.

use super::application::{FinanceReferenceRead, read_guard, read_reference};
use super::domain::{
    GuardKind, ReferenceKind, configuration_invalid, invalid, missing, unique_header, valid_id,
    valid_slug,
};
use super::postgres::PgFinanceReferenceRead;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, get, header, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_KIND: HeaderName = HeaderName::from_static("x-erpx-lane4-kind");
const CONTEXT_ID: HeaderName = HeaderName::from_static("x-erpx-lane4-id");
const CONTEXT_CHART: HeaderName = HeaderName::from_static("x-erpx-lane4-chart");
const CONTEXT_CURRENCY: HeaderName = HeaderName::from_static("x-erpx-lane4-currency");

/// A Holding host and slug pair. Both must be exact and distinct; no wildcard
/// or suffix matching is performed.
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn FinanceReferenceRead>>,
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

/// Resolve one read after target, kind and Company validation. Split out from
/// the extractor handler so the isolation contract is directly testable.
async fn read_value(
    state: &LaneState,
    host: Option<&str>,
    slug: &str,
    kind: &str,
    company: Option<&str>,
) -> Result<serde_json::Value, BusinessError> {
    let host = host.filter(|value| !value.is_empty()).ok_or_else(missing)?;
    let reader = state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?;
    let kind = ReferenceKind::parse(kind).ok_or_else(invalid)?;
    let company = company.filter(|value| valid_id(value)).ok_or_else(invalid)?;
    read_reference(reader.as_ref(), kind, company).await
}

/// Resolve and run one lock-free posting guard probe after the same target and
/// Company validation as the reference reads.
async fn read_guard_value(
    state: &LaneState,
    host: Option<&str>,
    slug: &str,
    kind: &str,
    company: Option<&str>,
    id: Option<&str>,
    chart: Option<&str>,
    currency: Option<&str>,
) -> Result<serde_json::Value, BusinessError> {
    let host = host.filter(|value| !value.is_empty()).ok_or_else(missing)?;
    let reader = state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?;
    let kind = GuardKind::parse(kind).ok_or_else(invalid)?;
    let company = company.filter(|value| valid_id(value)).ok_or_else(invalid)?;
    let id = id.filter(|value| valid_id(value)).ok_or_else(invalid)?;
    let currency = currency
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 16
                && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
        .ok_or_else(invalid)?;
    let chart = match kind {
        GuardKind::AccountLiability => {
            Some(chart.filter(|value| valid_id(value)).ok_or_else(invalid)?)
        }
        _ => chart.filter(|value| valid_id(value)),
    };
    read_guard(reader.as_ref(), kind, company, id, chart, currency).await
}

const READ_PATH: &str = "/api/v1/holdings/{holding_slug}/finance/native-lane-4/reference-options";
const GUARD_PATH: &str = "/api/v1/holdings/{holding_slug}/finance/native-lane-4/posting-guard";

/// Validate target uniqueness and build the shared state map. Every path into
/// the router uses this seam, so per-Holding pools cannot weaken isolation.
fn configure_entries(
    entries: Vec<(LaneTarget, Arc<dyn FinanceReferenceRead>)>,
) -> Result<Arc<LaneState>, BusinessError> {
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
    Ok(Arc::new(LaneState { targets: configured }))
}

fn into_router(state: Arc<LaneState>) -> Router {
    Router::new()
        .route(READ_PATH, get(read))
        .route(GUARD_PATH, get(posting_guard))
        .with_state(state)
}

/// Build the owner router with one pool for every target. The lead mounts it
/// on the private native API; the lane does not touch composition.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn FinanceReferenceRead> = Arc::new(PgFinanceReferenceRead::new(pool));
    router_with_reader(reader, targets)
}

/// Testable construction seam: one injected reader shared by all targets.
pub fn router_with_reader(
    reader: Arc<dyn FinanceReferenceRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let state = configure_entries(
        targets
            .into_iter()
            .map(|target| (target, Arc::clone(&reader)))
            .collect(),
    )?;
    Ok(into_router(state))
}

/// Composition seam: one reader per registered Holding and its own pool. Each
/// `(host, holding_slug)` key resolves only to that Holding's reader, so no
/// Holding can observe another Holding's database.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = configure_entries(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn FinanceReferenceRead> =
                    Arc::new(PgFinanceReferenceRead::new(pool));
                (target, reader)
            })
            .collect(),
    )?;
    Ok(into_router(state))
}

async fn read(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?;
        let kind = unique_header(request.headers(), CONTEXT_KIND)?
            .filter(|value| !value.is_empty())
            .ok_or_else(invalid)?;
        let data = read_value(&state, host, &slug, kind, company).await?;
        Ok::<_, BusinessError>(serde_json::json!({"data": data}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn posting_guard(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?;
        let kind = unique_header(request.headers(), CONTEXT_KIND)?
            .filter(|value| !value.is_empty())
            .ok_or_else(invalid)?;
        let id = unique_header(request.headers(), CONTEXT_ID)?;
        let chart = unique_header(request.headers(), CONTEXT_CHART)?;
        let currency = unique_header(request.headers(), CONTEXT_CURRENCY)?;
        let data = read_guard_value(&state, host, &slug, kind, company, id, chart, currency).await?;
        Ok::<_, BusinessError>(data)
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
    use crate::adapters::native_lane_4::domain::{GuardRow, ReferenceRow};
    use std::future::Future;
    use std::pin::Pin;

    struct FakeReader;

    impl FinanceReferenceRead for FakeReader {
        fn read<'a>(
            &'a self,
            _kind: ReferenceKind,
            _company: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<ReferenceRow>, BusinessError>> + Send + 'a>>
        {
            Box::pin(async {
                Ok(vec![ReferenceRow {
                    id: "acct-1".to_owned(),
                    name: "Cash".to_owned(),
                    code: Some("100".to_owned()),
                    chart_id: Some("chart-1".to_owned()),
                    category: Some("asset".to_owned()),
                }])
            })
        }
        fn guard<'a>(
            &'a self,
            _kind: GuardKind,
            _company: &'a str,
            id: &'a str,
            _chart: Option<&'a str>,
            _currency: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<Option<GuardRow>, BusinessError>> + Send + 'a>> {
            let id = id.to_owned();
            Box::pin(async move { Ok(Some(GuardRow { id })) })
        }
    }

    struct LabeledReader {
        label: &'static str,
    }

    impl FinanceReferenceRead for LabeledReader {
        fn read<'a>(
            &'a self,
            _kind: ReferenceKind,
            _company: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<ReferenceRow>, BusinessError>> + Send + 'a>>
        {
            let label = self.label;
            Box::pin(async move {
                Ok(vec![ReferenceRow {
                    id: format!("{label}-acct"),
                    name: "Cash".to_owned(),
                    code: Some("100".to_owned()),
                    chart_id: Some("chart-1".to_owned()),
                    category: Some("asset".to_owned()),
                }])
            })
        }
    }

    fn state() -> LaneState {
        let mut targets = BTreeMap::new();
        targets.insert(
            ("acme.jesadakorn.com".to_owned(), "acme".to_owned()),
            Arc::new(FakeReader) as Arc<dyn FinanceReferenceRead>,
        );
        LaneState { targets }
    }

    #[tokio::test]
    async fn read_value_enforces_target_kind_and_company() {
        let state = state();
        let ok = read_value(
            &state,
            Some("acme.jesadakorn.com"),
            "acme",
            "accounts",
            Some("company-1"),
        )
        .await
        .unwrap();
        assert_eq!(
            ok,
            serde_json::json!([{
                "id": "acct-1", "name": "Cash", "code": "100",
                "chart_id": "chart-1", "category": "asset"
            }])
        );
        assert_eq!(
            read_value(&state, Some("wrong.example"), "acme", "accounts", Some("company-1"))
                .await
                .unwrap_err(),
            missing()
        );
        assert_eq!(
            read_value(&state, Some("acme.jesadakorn.com"), "acme", "SELECT 1", Some("company-1"))
                .await
                .unwrap_err(),
            invalid()
        );
        assert_eq!(
            read_value(&state, Some("acme.jesadakorn.com"), "acme", "accounts", Some("bad/id"))
                .await
                .unwrap_err(),
            invalid()
        );
        assert_eq!(
            read_value(&state, None, "acme", "accounts", Some("company-1"))
                .await
                .unwrap_err(),
            missing()
        );
    }

    #[tokio::test]
    async fn read_guard_value_validates_target_kind_and_guard_args() {
        let state = state();
        let ok = read_guard_value(
            &state,
            Some("acme.jesadakorn.com"),
            "acme",
            "chart_active",
            Some("company-1"),
            Some("chart-1"),
            None,
            Some("THB"),
        )
        .await
        .unwrap();
        assert_eq!(ok, serde_json::json!({"data": {"id": "chart-1"}}));
        assert_eq!(
            read_guard_value(
                &state,
                Some("acme.jesadakorn.com"),
                "acme",
                "account_liability",
                Some("company-1"),
                Some("acct-1"),
                None,
                Some("THB"),
            )
            .await
            .unwrap_err(),
            invalid()
        );
        assert_eq!(
            read_guard_value(
                &state,
                Some("acme.jesadakorn.com"),
                "acme",
                "party_customer",
                Some("company-1"),
                Some("party-1"),
                None,
                Some("BAD-CURRENCY"),
            )
            .await
            .unwrap_err(),
            invalid()
        );
        assert_eq!(
            read_guard_value(
                &state,
                Some("wrong.example"),
                "acme",
                "chart_active",
                Some("company-1"),
                Some("chart-1"),
                None,
                Some("THB"),
            )
            .await
            .unwrap_err(),
            missing()
        );
    }

    #[tokio::test]
    async fn two_targets_route_to_two_distinct_readers() {
        let first: Arc<dyn FinanceReferenceRead> = Arc::new(LabeledReader { label: "a" });
        let second: Arc<dyn FinanceReferenceRead> = Arc::new(LabeledReader { label: "b" });
        let state = configure_entries(vec![
            (
                LaneTarget {
                    host: "acme.jesadakorn.com".to_owned(),
                    holding_slug: "acme".to_owned(),
                },
                Arc::clone(&first),
            ),
            (
                LaneTarget {
                    host: "northstar.jesadakorn.com".to_owned(),
                    holding_slug: "northstar".to_owned(),
                },
                Arc::clone(&second),
            ),
        ])
        .unwrap();
        assert_eq!(state.targets.len(), 2);
        let acme = state
            .targets
            .get(&("acme.jesadakorn.com".to_owned(), "acme".to_owned()))
            .unwrap();
        let northstar = state
            .targets
            .get(&("northstar.jesadakorn.com".to_owned(), "northstar".to_owned()))
            .unwrap();
        assert!(!Arc::ptr_eq(acme, northstar));

        let acme_rows = read_value(
            &state,
            Some("acme.jesadakorn.com"),
            "acme",
            "accounts",
            Some("company-1"),
        )
        .await
        .unwrap();
        let northstar_rows = read_value(
            &state,
            Some("northstar.jesadakorn.com"),
            "northstar",
            "accounts",
            Some("company-1"),
        )
        .await
        .unwrap();
        assert_eq!(acme_rows[0]["id"].as_str(), Some("a-acct"));
        assert_eq!(northstar_rows[0]["id"].as_str(), Some("b-acct"));
        assert_eq!(
            read_value(
                &state,
                Some("northstar.jesadakorn.com"),
                "acme",
                "accounts",
                Some("company-1")
            )
            .await
            .unwrap_err(),
            missing()
        );
    }

    #[test]
    fn router_for_holdings_rejects_empty_targets() {
        assert!(matches!(
            router_for_holdings(vec![]),
            Err(error) if error == configuration_invalid()
        ));
    }

    #[test]
    fn public_surface_exposes_router_for_holdings() {
        // The already-wired public glob must re-export the composition seam.
        let _constructor = crate::public::native_lane_4::router_for_holdings;
    }

    #[test]
    fn router_config_rejects_empty_duplicate_and_invalid_targets() {
        assert!(matches!(
            router_with_reader(Arc::new(FakeReader), vec![]),
            Err(error) if error == configuration_invalid()
        ));
        assert!(matches!(
            router_with_reader(
                Arc::new(FakeReader),
                vec![LaneTarget {
                    host: "acme.jesadakorn.com".to_owned(),
                    holding_slug: "ACME".to_owned()
                }]
            ),
            Err(error) if error == configuration_invalid()
        ));
        let duplicate = vec![
            LaneTarget {
                host: "acme.jesadakorn.com".to_owned(),
                holding_slug: "acme".to_owned(),
            },
            LaneTarget {
                host: "acme.jesadakorn.com".to_owned(),
                holding_slug: "acme".to_owned(),
            },
        ];
        assert!(matches!(
            router_with_reader(Arc::new(FakeReader), duplicate),
            Err(error) if error == configuration_invalid()
        ));
        assert!(router_with_reader(
            Arc::new(FakeReader),
            vec![LaneTarget {
                host: "acme.jesadakorn.com".to_owned(),
                holding_slug: "acme".to_owned()
            }]
        )
        .is_ok());
    }

    #[test]
    fn error_mapping_is_fail_closed() {
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn duplicate_headers_are_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTEXT_COMPANY, HeaderValue::from_static("company-1"));
        assert_eq!(
            unique_header(&headers, CONTEXT_COMPANY).unwrap(),
            Some("company-1")
        );
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-2"));
        assert_eq!(
            unique_header(&headers, CONTEXT_COMPANY).unwrap_err(),
            invalid()
        );
    }
}
