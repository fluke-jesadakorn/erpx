//! Owner-local axum ingress for the Finance overview-counts read.
//!
//! Composition supplies the sqlx pool and the exact registered Holding targets;
//! this router accepts no SQL and no caller predicate. Holding host, slug and
//! Company identity are isolated per request, and every failure maps to the
//! owner's explicit error envelope so the TypeScript client fails closed.

use crate::adapters::native_lane_2::{
    application::{OverviewCountsQuery, load},
    domain::{configuration_invalid, invalid_request, missing, valid_company_id, valid_slug},
    postgres::PgOverviewCountsQuery,
};
use erpx_server_runtime::public::{
    http_support::{
        HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Response, Router, State,
        StatusCode, header, routing::get, serde_json,
    },
    postgres::DatabasePool,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// A registered Holding host/slug pair. The host header and the path slug must
/// both match an exact configured target; no suffix or fallback match is made.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn OverviewCountsQuery>>,
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

fn valid_host(host: &str) -> bool {
    !host.is_empty() && host.len() <= 512 && !host.chars().any(|c| c.is_control())
}

/// Validate the exact, distinct registered targets without touching the pool.
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

/// Build the target map keyed by the exact `(host, holding_slug)` pair. Every
/// constructor below funnels through here, so the same isolation rules apply.
fn state_for_readers(
    readers: Vec<(LaneTarget, Arc<dyn OverviewCountsQuery>)>,
) -> Result<Arc<LaneState>, BusinessError> {
    let targets: Vec<LaneTarget> = readers.iter().map(|(target, _)| target.clone()).collect();
    validate_targets(&targets)?;
    let mut configured: BTreeMap<(String, String), Arc<dyn OverviewCountsQuery>> = BTreeMap::new();
    for (target, reader) in readers {
        configured.insert((target.host, target.holding_slug), reader);
    }
    Ok(Arc::new(LaneState { targets: configured }))
}

fn mounted(state: Arc<LaneState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/native-lane-2/overview-counts",
            get(counts),
        )
        .with_state(state)
}

/// Build the native read router with one reader shared by every target.
/// Retained for tests and single-pool callers.
pub fn router_with_reader(
    reader: Arc<dyn OverviewCountsQuery>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let state = state_for_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )?;
    Ok(mounted(state))
}

/// Build the native read router from a single pool. The lead mounts it on the
/// private native API; no composition or public module is edited by the lane.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn OverviewCountsQuery> = Arc::new(PgOverviewCountsQuery::new(pool));
    router_with_reader(reader, targets)
}

/// Build the native read router with one independent reader per Holding pool.
/// `pools[0]` is the control plane; entry `i` pairs `config.holdings[i]` with
/// `pools[i + 1]`. A request can only reach the reader registered for its exact
/// `(Host, holding_slug)` target.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = state_for_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn OverviewCountsQuery> =
                    Arc::new(PgOverviewCountsQuery::new(pool));
                (target, reader)
            })
            .collect(),
    )?;
    Ok(mounted(state))
}

async fn counts(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    // `HeaderMap` is a parts extractor and is Sync, unlike `&Request<Body>`;
    // borrowing it across the owner read keeps the handler future Send.
    let result = async {
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| valid_host(host))
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| valid_company_id(value))
            .ok_or_else(invalid_request)?;
        let data = load(reader.as_ref(), company).await?;
        Ok::<_, BusinessError>(json!({"data": data}))
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
    use crate::adapters::native_lane_2::{
        application::OverviewCountsFuture,
        domain::{OverviewCounts, unavailable},
    };
    use erpx_server_runtime::public::http_support::body::to_bytes;

    struct FixedCounts(i64);
    impl OverviewCountsQuery for FixedCounts {
        fn counts<'a>(&'a self, company_id: &'a str) -> OverviewCountsFuture<'a> {
            let journals = self.0;
            let company_id = company_id.to_owned();
            Box::pin(async move {
                if company_id != "acme:co" {
                    return Err(unavailable());
                }
                Ok(OverviewCounts {
                    journals,
                    posting: 0,
                    invoices: 0,
                    payments: 0,
                    receivable: 0,
                    payable: 0,
                    reconcile: 0,
                })
            })
        }
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read body");
        serde_json::from_slice(&bytes).expect("json body")
    }

    async fn get(state: Arc<LaneState>, host: &'static str, slug: &'static str) -> Response {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static(host));
        headers.insert(CONTEXT_COMPANY, HeaderValue::from_static("acme:co"));
        counts(State(state), Path(slug.to_owned()), headers).await
    }

    #[tokio::test]
    async fn two_holdings_route_to_two_distinct_readers() {
        let a: Arc<dyn OverviewCountsQuery> = Arc::new(FixedCounts(11));
        let b: Arc<dyn OverviewCountsQuery> = Arc::new(FixedCounts(22));
        assert!(!Arc::ptr_eq(&a, &b));

        let state = state_for_readers(vec![
            (
                LaneTarget { host: "a.example.com".into(), holding_slug: "acme".into() },
                a.clone(),
            ),
            (
                LaneTarget { host: "b.example.com".into(), holding_slug: "northstar".into() },
                b.clone(),
            ),
        ])
        .expect("two-target state");

        let from_a = body_json(get(state.clone(), "a.example.com", "acme").await).await;
        let from_b = body_json(get(state.clone(), "b.example.com", "northstar").await).await;
        assert_eq!(from_a["data"]["journals"], 11);
        assert_eq!(from_b["data"]["journals"], 22);

        // A valid host with the other Holding's slug must not reach its reader.
        assert_eq!(
            get(state.clone(), "b.example.com", "acme").await.status(),
            StatusCode::NOT_FOUND
        );
        // The single-reader constructor still builds a valid multi-target router.
        let shared = router_with_reader(
            a,
            vec![
                LaneTarget { host: "a.example.com".into(), holding_slug: "acme".into() },
                LaneTarget { host: "b.example.com".into(), holding_slug: "northstar".into() },
            ],
        );
        assert!(shared.is_ok());
    }

    #[test]
    fn rejects_duplicate_and_empty_targets() {
        // A pool is not needed to reject invalid configuration.
        let invalid = [
            vec![],
            vec![LaneTarget { host: "h".into(), holding_slug: "A".into() }],
            vec![LaneTarget { host: "".into(), holding_slug: "a".into() }],
            vec![
                LaneTarget { host: "h".into(), holding_slug: "a".into() },
                LaneTarget { host: "h".into(), holding_slug: "a".into() },
            ],
        ];
        for targets in invalid {
            assert_eq!(validate_targets(&targets), Err(configuration_invalid()));
        }
        assert_eq!(
            validate_targets(&[LaneTarget {
                host: "acme.jesadakorn.com".into(),
                holding_slug: "acme".into(),
            }]),
            Ok(())
        );
    }

    #[test]
    fn maps_all_failure_kinds_to_the_owner_statuses() {
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::Unauthenticated), StatusCode::UNAUTHORIZED);
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status(FailureKind::Conflict), StatusCode::CONFLICT);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn host_validation_is_bounded() {
        assert!(valid_host("acme.jesadakorn.com"));
        assert!(!valid_host(""));
        assert!(!valid_host("bad\nhost"));
        assert!(!valid_host(&"x".repeat(513)));
    }
}
