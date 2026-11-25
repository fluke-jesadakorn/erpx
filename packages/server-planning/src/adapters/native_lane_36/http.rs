//! Owner-local axum router for the Planning lane-36 scoped workspace reads.
//!
//! Transport facts only: an exact registered Host/Holding target, one bounded
//! Company header and an optional bounded branch query. The owner reader owns
//! the SQL, the projection and the scope. No TypeScript fallback and no caller
//! SQL crosses this port.

use super::application;
use super::domain::{
    KIND_EXECUTIVE_RECOMMENDATION, KIND_EXECUTIVE_SHORTAGE, LaneScope,
};
use super::postgres::PgLaneRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// Exact registered Holding target. Duplicate host/slug pairs are rejected.
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    /// One reader per registered (host, holding_slug). `router_for_holdings`
    /// gives each Holding its own DatabasePool; `router_with_reader` shares a
    /// single injected reader for tests and single-pool deployments.
    readers: BTreeMap<(String, String), Arc<dyn application::LaneRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PLANNING_LANE36_REQUEST_INVALID",
        "Use one unambiguous Planning scoped read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PLANNING_LANE36_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
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

/// Read one optional bounded query value. Values are already restricted to the
/// safe identifier alphabet, so no percent-decoding is required or admitted.
fn query_value(query: Option<&str>, key: &str) -> Result<Option<String>, BusinessError> {
    let Some(query) = query else {
        return Ok(None);
    };
    let mut found: Option<String> = None;
    for pair in query.split('&') {
        let Some((name, value)) = pair.split_once('=') else {
            return Err(invalid());
        };
        if name != key {
            continue;
        }
        if found.is_some() || !super::domain::valid_scope(value) {
            return Err(invalid());
        }
        found = Some(value.to_owned());
    }
    Ok(found)
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

impl LaneState {
    /// Exact handler used by both the axum route and the owner tests.
    pub async fn read(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        kind: &str,
        query: Option<&str>,
    ) -> Response {
        match self.read_inner(headers, holding_slug, kind, query).await {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn read_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        kind: &str,
        query: Option<&str>,
    ) -> Result<serde_json::Value, BusinessError> {
        let host = unique_header(headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = self
            .readers
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        let company = unique_header(headers, CONTEXT_COMPANY)?
            .filter(|value| super::domain::valid_id(value))
            .ok_or_else(invalid)?;
        let branch = query_value(query, "branch")?;
        let id = query_value(query, "id")?;
        let comparison = query_value(query, "comparisonScenarioId")?;
        let specification = query_value(query, "specificationId")?;
        let from = query_value(query, "from")?;
        let to = query_value(query, "to")?;
        let eligibility = query_value(query, "eligibility")?;
        // Reject unrelated query keys so a caller cannot smuggle a predicate.
        if let Some(query) = query {
            for pair in query.split('&') {
                let name = pair.split_once('=').map(|(name, _)| name).unwrap_or(pair);
                if !matches!(
                    name,
                    "branch"
                        | "id"
                        | "comparisonScenarioId"
                        | "specificationId"
                        | "from"
                        | "to"
                        | "eligibility"
                ) {
                    return Err(invalid());
                }
            }
        }
        let scope = if matches!(kind, KIND_EXECUTIVE_SHORTAGE | KIND_EXECUTIVE_RECOMMENDATION) {
            LaneScope::new_executive(
                kind,
                company,
                branch.as_deref(),
                id.as_deref(),
                from.as_deref(),
                to.as_deref(),
                eligibility.as_deref(),
            )?
        } else {
            LaneScope::new(
                kind,
                company,
                branch.as_deref(),
                id.as_deref(),
                comparison.as_deref(),
                specification.as_deref(),
            )?
        };
        application::handle(reader.as_ref(), scope).await
    }
}

/// Insert one reader per exact registered Holding target. Duplicate host/slug
/// pairs and empty configurations are rejected before any state is built.
fn state_with_readers(
    targets: Vec<(LaneTarget, Arc<dyn application::LaneRead>)>,
) -> Result<LaneState, BusinessError> {
    let mut readers = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || readers
                .insert((target.host, target.holding_slug), reader)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if readers.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(LaneState { readers })
}

fn new_router(state: LaneState) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/planning/lane-36/{kind}",
            get(current),
        )
        .with_state(Arc::new(state))
}

/// Build the native read router with one injected reader shared by every
/// target. Retained for tests and single-pool deployments.
pub fn router_with_reader(
    reader: Arc<dyn application::LaneRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let state = state_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )?;
    Ok(new_router(state))
}

/// Build the native read router with a single pool shared by all targets.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgLaneRead::new(pool)), targets)
}

/// Build the native read router with one distinct `PgLaneRead`, and therefore
/// one distinct `DatabasePool`, per registered Holding. Composition pairs each
/// entry of `config.holdings` with its own pool; no cross-Holding pool reuse is
/// possible because the map is keyed by `(host, holding_slug)`.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = state_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    Arc::new(PgLaneRead::new(pool)) as Arc<dyn application::LaneRead>,
                )
            })
            .collect(),
    )?;
    Ok(new_router(state))
}

async fn current(
    State(state): State<Arc<LaneState>>,
    Path((slug, kind)): Path<(String, String)>,
    request: Request,
) -> Response {
    state
        .read(request.headers(), &slug, &kind, request.uri().query())
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_shared::public::BusinessError as Error;
    use serde_json::json;

    struct Fake;

    impl application::LaneRead for Fake {
        fn read<'a>(
            &'a self,
            scope: &'a LaneScope,
        ) -> application::LaneReadFuture<'a, super::super::domain::LaneData> {
            let scope = scope.clone();
            Box::pin(async move {
                Ok(super::super::domain::LaneData::List(vec![json!({
                    "kind": scope.kind,
                    "companyId": scope.company_id,
                    "branchId": scope.branch,
                })]))
            })
        }
    }

    fn state() -> LaneState {
        state_with_readers(vec![(
            LaneTarget {
                host: "acme.example.com".to_owned(),
                holding_slug: "acme".to_owned(),
            },
            Arc::new(Fake) as Arc<dyn application::LaneRead>,
        )])
        .unwrap()
    }

    fn headers(company: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("acme.example.com"));
        headers.insert(CONTEXT_COMPANY, HeaderValue::from_str(company).unwrap());
        headers
    }

    #[tokio::test]
    async fn exact_target_and_company_are_required() {
        let state = state();
        let response = state.read(&headers("company-1"), "acme", "resources", None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        let wrong_host = state.read(&headers("company-1"), "other", "resources", None).await;
        assert_eq!(wrong_host.status(), StatusCode::NOT_FOUND);
        let mut without_company = headers("company-1");
        without_company.remove(CONTEXT_COMPANY);
        let response = state
            .read(&without_company, "acme", "resources", None)
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn executive_reads_require_id_window_and_closed_eligibility() {
        let state = state();
        let shortage = state
            .read(
                &headers("company-1"),
                "acme",
                KIND_EXECUTIVE_SHORTAGE,
                Some("id=scenario-1&from=2026-01-01&to=2026-12-31"),
            )
            .await;
        assert_eq!(shortage.status(), StatusCode::OK);
        let proposed = state
            .read(
                &headers("company-1"),
                "acme",
                KIND_EXECUTIVE_RECOMMENDATION,
                Some("id=recommendation-1&from=2026-01-01&to=2026-12-31&eligibility=proposed"),
            )
            .await;
        assert_eq!(proposed.status(), StatusCode::OK);
        for (kind, query) in [
            (KIND_EXECUTIVE_RECOMMENDATION, "id=recommendation-1&from=2026-01-01&to=2026-12-31"),
            (KIND_EXECUTIVE_RECOMMENDATION, "id=recommendation-1&from=2026-01-01&to=2026-12-31&eligibility=approved"),
            (KIND_EXECUTIVE_SHORTAGE, "id=scenario-1&from=2026-01-01&to=2026-12-31&eligibility=capacity"),
            (KIND_EXECUTIVE_SHORTAGE, "id=scenario-1&from=2026-01-01"),
            (KIND_EXECUTIVE_SHORTAGE, "id=scenario-1&from=2026-01-01&to=2026/12/31"),
        ] {
            let response = state
                .read(&headers("company-1"), "acme", kind, Some(query))
                .await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{query}");
        }
    }

    #[tokio::test]
    async fn rejects_unknown_kind_and_unbounded_branch() {
        let state = state();
        let response = state
            .read(&headers("company-1"), "acme", "executive-bogus", None)
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = state
            .read(
                &headers("company-1"),
                "acme",
                "resources",
                Some("branch=x/y"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = state
            .read(
                &headers("company-1"),
                "acme",
                "resources",
                Some("branch=b1&company_id=other"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn row_reads_require_closed_identity_query() {
        let state = state();
        assert_eq!(
            state
                .read(
                    &headers("company-1"),
                    "acme",
                    "scenario",
                    Some("id=s1&branch=b1"),
                )
                .await
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            state
                .read(&headers("company-1"), "acme", "scenario", None)
                .await
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            state
                .read(
                    &headers("company-1"),
                    "acme",
                    "comparison",
                    Some("comparisonScenarioId=other&specificationId=spec&branch=b1"),
                )
                .await
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            state
                .read(
                    &headers("company-1"),
                    "acme",
                    "scenario",
                    Some("id=s1&where=x"),
                )
                .await
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[tokio::test]
    async fn duplicate_or_empty_targets_are_rejected() {
        // Configuration validation is covered through the shared builder below.
        let duplicate = vec![
            LaneTarget {
                host: "acme.example.com".into(),
                holding_slug: "acme".into(),
            },
            LaneTarget {
                host: "acme.example.com".into(),
                holding_slug: "acme".into(),
            },
        ];
        assert!(validate_targets(&duplicate).is_err());
        assert!(validate_targets(&[]).is_err());
        let valid = vec![LaneTarget {
            host: "acme.example.com".into(),
            holding_slug: "acme".into(),
        }];
        assert!(validate_targets(&valid).is_ok());
    }

    /// Exercises the real target-validation path through injected readers.
    fn validate_targets(targets: &[LaneTarget]) -> Result<(), Error> {
        state_with_readers(
            targets
                .iter()
                .map(|target| {
                    (
                        LaneTarget {
                            host: target.host.clone(),
                            holding_slug: target.holding_slug.clone(),
                        },
                        Arc::new(Fake) as Arc<dyn application::LaneRead>,
                    )
                })
                .collect(),
        )
        .map(|_| ())
    }

    /// Distinct injected readers stand in for distinct per-Holding pools:
    /// `router_for_holdings` builds one `PgLaneRead` per entry through the same
    /// state map and the same lookup used by this test.
    struct Marked(&'static str);

    impl application::LaneRead for Marked {
        fn read<'a>(
            &'a self,
            _scope: &'a LaneScope,
        ) -> application::LaneReadFuture<'a, super::super::domain::LaneData> {
            let marker = self.0;
            Box::pin(async move {
                Ok(super::super::domain::LaneData::List(vec![json!({
                    "reader": marker,
                })]))
            })
        }
    }

    async fn injected_marker(state: &LaneState, headers: &HeaderMap, slug: &str) -> String {
        let response = state.read(headers, slug, "resources", None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = erpx_server_runtime::public::http_support::body::to_bytes(
            response.into_body(),
            usize::MAX,
        )
        .await
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value["data"][0]["reader"].as_str().unwrap().to_owned()
    }

    #[tokio::test]
    async fn two_holdings_route_to_two_distinct_readers() {
        let state = state_with_readers(vec![
            (
                LaneTarget {
                    host: "acme.example.com".to_owned(),
                    holding_slug: "acme".to_owned(),
                },
                Arc::new(Marked("acme-pool")) as Arc<dyn application::LaneRead>,
            ),
            (
                LaneTarget {
                    host: "north.example.com".to_owned(),
                    holding_slug: "north".to_owned(),
                },
                Arc::new(Marked("north-pool")) as Arc<dyn application::LaneRead>,
            ),
        ])
        .unwrap();
        assert_eq!(state.readers.len(), 2);

        let acme = headers("company-1");
        let mut north = headers("company-1");
        north.insert(
            header::HOST,
            HeaderValue::from_static("north.example.com"),
        );
        assert_eq!(injected_marker(&state, &acme, "acme").await, "acme-pool");
        assert_eq!(injected_marker(&state, &north, "north").await, "north-pool");
        // A crossed host/slug pair must not fall back to the other Holding's pool.
        assert_eq!(
            state
                .read(&north, "acme", "resources", None)
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn state_trait_is_object_safe() {
        fn assert_object_safe(_: &dyn application::LaneRead) {}
        assert_object_safe(&Fake);
        fn assert_future(_: application::LaneReadFuture<'_, ()>) {}
        let _ = assert_future;
    }
}
