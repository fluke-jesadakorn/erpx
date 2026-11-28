//! Owner-local axum ingress for the native Collaboration beta-facts read family.
//!
//! Composition supplies the sqlx pool and the exact registered Holding targets;
//! this router accepts no SQL and no caller-supplied predicate. Host and slug
//! must both match a registered target, and Company-scoped reads require the
//! `x-erpx-context-company` header to agree with the bounded request body.
//! Every failure maps to the owner error envelope so the TS client fails closed.

use crate::adapters::native_lane_15::{
    application::{load, BetaFactsQuery},
    domain::{
        configuration_invalid, invalid_request, missing, valid_host, valid_slug, BetaFactRequest,
    },
    postgres::PgBetaFactsQuery,
};
use erpx_platform_runtime_server::public::{
    http_support::{
        body::to_bytes, header, routing::post, serde_json, HeaderMap, HeaderName, HeaderValue,
        IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    },
    postgres::DatabasePool,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const BODY_LIMIT: usize = 16 * 1024;
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// A registered Holding host/slug pair. Both the Host header and the path slug
/// must match exactly; no suffix or fallback match is made.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    /// One reader per registered (host, holding_slug). `router_for_holdings`
    /// gives each Holding its own DatabasePool; `router_with_reader` shares a
    /// single injected reader for tests and single-pool deployments.
    targets: BTreeMap<(String, String), Arc<dyn BetaFactsQuery>>,
}

impl LaneState {
    async fn read(&self, headers: &HeaderMap, slug: &str, request: Request) -> Response {
        let result = async {
            let host = unique_header(headers, header::HOST)?
                .filter(|host| valid_host(host))
                .ok_or_else(missing)?;
            let reader = self
                .targets
                .get(&(host.to_owned(), slug.to_owned()))
                .ok_or_else(missing)?;
            let company = unique_header(headers, CONTEXT_COMPANY)?.map(str::to_owned);
            let body = bounded_body(request).await?;
            let data = load(reader.as_ref(), &body, company.as_deref()).await?;
            Ok::<_, BusinessError>(serde_json::json!({"data": data}))
        }
        .await;
        match result {
            Ok(body) => bounded(body),
            Err(error) => error_response(error),
        }
    }
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

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid_request());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid_request()))
        .transpose()
}

async fn bounded_body(request: Request) -> Result<BetaFactRequest, BusinessError> {
    if request.uri().query().is_some() {
        return Err(invalid_request());
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| invalid_request())?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid_request())?;
    if !value.is_object() {
        return Err(invalid_request());
    }
    serde_json::from_value(value).map_err(|_| invalid_request())
}

/// Pure configuration guard: exact distinct registered host/slug pairs only.
/// Kept pool-free so it is testable without a database connection.
fn validate_targets(targets: &[LaneTarget]) -> Result<(), BusinessError> {
    if targets.is_empty() {
        return Err(configuration_invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    for target in targets {
        if !valid_host(&target.host)
            || !valid_slug(&target.holding_slug)
            || !seen.insert((target.host.clone(), target.holding_slug.clone()))
        {
            return Err(configuration_invalid());
        }
    }
    Ok(())
}

/// Insert one reader per exact registered Holding target. Duplicate host/slug
/// pairs and empty configurations are rejected before any state is built.
fn state_with_readers(
    targets: Vec<(LaneTarget, Arc<dyn BetaFactsQuery>)>,
) -> Result<LaneState, BusinessError> {
    let registered: Vec<LaneTarget> = targets.iter().map(|(target, _)| target.clone()).collect();
    validate_targets(&registered)?;
    let mut readers: BTreeMap<(String, String), Arc<dyn BetaFactsQuery>> = BTreeMap::new();
    for (target, reader) in targets {
        readers.insert((target.host, target.holding_slug), reader);
    }
    Ok(LaneState { targets: readers })
}

fn new_router(state: LaneState) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/collaboration/native-lane-15/beta-facts",
            post(beta_facts),
        )
        .with_state(Arc::new(state))
}

/// Build the beta-facts router with one injected reader shared by every target.
/// Retained for tests and single-pool deployments.
pub fn router_with_reader(
    reader: Arc<dyn BetaFactsQuery>,
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

/// Build the beta-facts router with a single pool shared by all targets.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgBetaFactsQuery::new(pool)), targets)
}

/// Build the beta-facts router with one distinct `PgBetaFactsQuery`, and
/// therefore one distinct `DatabasePool`, per registered Holding. Composition
/// pairs each entry of `config.holdings` with its own pool; no cross-Holding
/// pool reuse is possible because the map is keyed by `(host, holding_slug)`.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = state_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    Arc::new(PgBetaFactsQuery::new(pool)) as Arc<dyn BetaFactsQuery>,
                )
            })
            .collect(),
    )?;
    Ok(new_router(state))
}

async fn beta_facts(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    state.read(&headers, &slug, request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_15::application::BetaFactsFuture;
    use crate::adapters::native_lane_15::domain::{PostDraft, PostReport, TaskOrigin};
    use erpx_platform_runtime_server::public::http_support::Body;

    #[test]
    fn rejects_duplicate_and_empty_targets() {
        let invalid = [
            vec![],
            vec![LaneTarget {
                host: "h".into(),
                holding_slug: "A".into(),
            }],
            vec![LaneTarget {
                host: "".into(),
                holding_slug: "a".into(),
            }],
            vec![
                LaneTarget {
                    host: "h".into(),
                    holding_slug: "a".into(),
                },
                LaneTarget {
                    host: "h".into(),
                    holding_slug: "a".into(),
                },
            ],
        ];
        for targets in invalid {
            assert_eq!(validate_targets(&targets), Err(configuration_invalid()));
        }
    }

    #[test]
    fn accepts_a_valid_target() {
        assert!(validate_targets(&[LaneTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
        }])
        .is_ok());
    }

    #[test]
    fn maps_all_failure_kinds_to_the_owner_statuses() {
        assert_eq!(
            status(FailureKind::InvalidInput),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            status(FailureKind::Unauthenticated),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status(FailureKind::Conflict), StatusCode::CONFLICT);
        assert_eq!(
            status(FailureKind::Unavailable),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    /// Distinct injected readers stand in for distinct per-Holding pools:
    /// `router_for_holdings` builds one `PgBetaFactsQuery` per entry through the
    /// same state map and lookup used by this test.
    struct Marked(&'static str);

    impl BetaFactsQuery for Marked {
        fn post_drafts<'a>(
            &'a self,
            _company: &'a str,
            _user: &'a str,
        ) -> BetaFactsFuture<'a, Vec<PostDraft>> {
            Box::pin(async move { Ok(vec![]) })
        }

        fn reports<'a>(&'a self, _company: &'a str) -> BetaFactsFuture<'a, Vec<PostReport>> {
            Box::pin(async move { Ok(vec![]) })
        }

        fn task_origin<'a>(&'a self, _post: &'a str) -> BetaFactsFuture<'a, Option<TaskOrigin>> {
            let marker = self.0;
            Box::pin(async move {
                Ok(Some(TaskOrigin {
                    task_id: marker.to_owned(),
                }))
            })
        }
    }

    async fn injected_marker(state: &LaneState, host: &'static str, slug: &str) -> String {
        let body = serde_json::json!({"kind":"post_task","postId":"p1"});
        let request = Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/holdings/{slug}/collaboration/native-lane-15/beta-facts"
            ))
            .header(header::HOST, host)
            .body(Body::from(body.to_string()))
            .expect("request");
        let headers = request.headers().clone();
        let response = state.read(&headers, slug, request).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), BODY_LIMIT)
            .await
            .expect("body");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        value["data"]["task"]["task_id"]
            .as_str()
            .expect("marker")
            .to_owned()
    }

    #[tokio::test]
    async fn two_holdings_route_to_two_distinct_readers() {
        let state = state_with_readers(vec![
            (
                LaneTarget {
                    host: "acme.example.com".to_owned(),
                    holding_slug: "acme".to_owned(),
                },
                Arc::new(Marked("acme-pool")) as Arc<dyn BetaFactsQuery>,
            ),
            (
                LaneTarget {
                    host: "north.example.com".to_owned(),
                    holding_slug: "north".to_owned(),
                },
                Arc::new(Marked("north-pool")) as Arc<dyn BetaFactsQuery>,
            ),
        ])
        .expect("state");
        assert_eq!(state.targets.len(), 2);
        let acme = state
            .targets
            .get(&("acme.example.com".to_owned(), "acme".to_owned()))
            .expect("acme reader");
        let north = state
            .targets
            .get(&("north.example.com".to_owned(), "north".to_owned()))
            .expect("north reader");
        assert!(!Arc::ptr_eq(acme, north));

        assert_eq!(
            injected_marker(&state, "acme.example.com", "acme").await,
            "acme-pool"
        );
        assert_eq!(
            injected_marker(&state, "north.example.com", "north").await,
            "north-pool"
        );

        // A Holding cannot use another Holding's host/slug pair.
        let body = serde_json::json!({"kind":"post_task","postId":"p1"});
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/collaboration/native-lane-15/beta-facts")
            .header(header::HOST, "north.example.com")
            .body(Body::from(body.to_string()))
            .expect("request");
        let headers = request.headers().clone();
        assert_eq!(
            state.read(&headers, "acme", request).await.status(),
            StatusCode::NOT_FOUND
        );
    }
}
