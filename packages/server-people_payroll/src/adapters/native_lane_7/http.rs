//! Owner-local axum router for the lane-7 legacy people clock. Target, Host,
//! Company and clock headers are isolated exactly as in the Commerce native
//! read companion; any failure fails closed without a TypeScript fallback.

use std::collections::BTreeMap;
use std::sync::Arc;

use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, json, routing::get,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};

use super::application::{self, LegacyPeopleClock};
use super::domain::{self, READ_KIND, ROUTE};
use super::postgres::PgLegacyPeopleClock;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_OBSERVED_AT: HeaderName = HeaderName::from_static("x-erpx-observed-at");

/// A registered Holding target. The lead owns composition and mounts this
/// router; no composition file is edited here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct Lane7State {
    targets: BTreeMap<(String, String), Arc<dyn LegacyPeopleClock>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE7_REQUEST_INVALID",
        "Use one bounded legacy people clock read.",
    )
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

/// Build the target map from a source per registered Holding. Every source gets
/// its own reader, so a Holding never observes another Holding's pool.
fn build_state<P>(
    targets: Vec<(LaneTarget, P)>,
    factory: impl Fn(P) -> Arc<dyn LegacyPeopleClock>,
) -> Result<Arc<Lane7State>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, source) in targets {
        if target.host.is_empty() || !valid_slug(&target.holding_slug) {
            return Err(domain::configuration_invalid());
        }
        if configured
            .insert(
                (target.host.clone(), target.holding_slug.clone()),
                factory(source),
            )
            .is_some()
        {
            return Err(domain::configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(domain::configuration_invalid());
    }
    Ok(Arc::new(Lane7State {
        targets: configured,
    }))
}

fn router_from_state(state: Arc<Lane7State>) -> Router {
    Router::new()
        .route(ROUTE, get(next_transition))
        .with_state(state)
}

fn reader_for(
    state: &Lane7State,
    host: &str,
    slug: &str,
) -> Result<Arc<dyn LegacyPeopleClock>, BusinessError> {
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .cloned()
        .ok_or_else(missing)
}

fn build(
    reader: Arc<dyn LegacyPeopleClock>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let state = build_state(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
        |reader| reader,
    )?;
    Ok(router_from_state(state))
}

/// Build the native router over one shared owner pool. The lead mounts it; this
/// lane never edits composition or another owner's router.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    build(Arc::new(PgLegacyPeopleClock::new(pool)), targets)
}

/// Build the native router with one owner reader per registered Holding pool.
/// Each `(host, holding_slug)` key resolves to that Holding's own reader, so a
/// request can never cross a Holding boundary. Composition keeps `targets[i]`
/// aligned with `pools[i + 1]`.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = build_state(targets, |pool| Arc::new(PgLegacyPeopleClock::new(pool)))?;
    Ok(router_from_state(state))
}

/// Injectable constructor used by owner-local tests and by alternative hosts.
pub fn router_with_reader(
    reader: Arc<dyn LegacyPeopleClock>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    build(reader, targets)
}

async fn next_transition(
    State(state): State<Arc<Lane7State>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let clock = reader_for(&state, host, &slug)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| domain::valid_id(value))
            .ok_or_else(domain::invalid_company)?;
        let raw = unique_header(request.headers(), CONTEXT_OBSERVED_AT)?
            .ok_or_else(domain::invalid_observed_at)?;
        let observed_at_ms = domain::parse_observed_at(raw)?;
        let facts =
            application::read_next_transition(clock.as_ref(), company, observed_at_ms).await?;
        Ok::<_, BusinessError>(json!({
            "kind": READ_KIND,
            "transition": {"deadline": facts.deadline}
        }))
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
    use erpx_server_shared::public::FailureKind;

    struct BusyClock;
    impl LegacyPeopleClock for BusyClock {
        fn next_transition<'a>(
            &'a self,
            _query: super::super::application::TransitionQuery,
        ) -> super::super::application::TransitionFuture<
            'a,
            super::super::domain::LegacyPeopleTransition,
        > {
            Box::pin(async { Err(domain::unavailable()) })
        }
    }

    fn target() -> LaneTarget {
        LaneTarget {
            host: "acme.jesadakorn.com".to_owned(),
            holding_slug: "acme".to_owned(),
        }
    }

    #[test]
    fn rejects_empty_duplicate_and_invalid_targets() {
        assert!(router_with_reader(Arc::new(BusyClock), vec![]).is_err());
        assert!(
            router_with_reader(
                Arc::new(BusyClock),
                vec![LaneTarget {
                    host: String::new(),
                    holding_slug: "acme".to_owned()
                }]
            )
            .is_err()
        );
        assert!(
            router_with_reader(
                Arc::new(BusyClock),
                vec![LaneTarget {
                    host: "acme.jesadakorn.com".to_owned(),
                    holding_slug: "../bad".to_owned()
                }]
            )
            .is_err()
        );
        assert!(router_with_reader(Arc::new(BusyClock), vec![target(), target()]).is_err());
    }

    #[test]
    fn accepts_distinct_registered_targets() {
        assert!(router_with_reader(Arc::new(BusyClock), vec![target()]).is_ok());
        assert!(
            router_with_reader(
                Arc::new(BusyClock),
                vec![
                    target(),
                    LaneTarget {
                        host: "northstar.jesadakorn.com".to_owned(),
                        holding_slug: "northstar".to_owned()
                    }
                ]
            )
            .is_ok()
        );
    }

    struct TaggedClock(&'static str);
    impl LegacyPeopleClock for TaggedClock {
        fn next_transition<'a>(
            &'a self,
            _query: super::super::application::TransitionQuery,
        ) -> super::super::application::TransitionFuture<
            'a,
            super::super::domain::LegacyPeopleTransition,
        > {
            let tag = self.0;
            Box::pin(async move {
                Ok(super::super::domain::LegacyPeopleTransition {
                    deadline: if tag == "acme" { Some(11) } else { Some(22) },
                })
            })
        }
    }

    #[tokio::test]
    async fn two_holdings_route_to_distinct_readers() {
        let first = LaneTarget {
            host: "acme.jesadakorn.com".to_owned(),
            holding_slug: "acme".to_owned(),
        };
        let second = LaneTarget {
            host: "northstar.jesadakorn.com".to_owned(),
            holding_slug: "northstar".to_owned(),
        };
        let state = build_state(vec![(first, "acme"), (second, "northstar")], |tag| {
            Arc::new(TaggedClock(tag)) as Arc<dyn LegacyPeopleClock>
        })
        .expect("state");
        let a = reader_for(&state, "acme.jesadakorn.com", "acme").expect("acme reader");
        let b =
            reader_for(&state, "northstar.jesadakorn.com", "northstar").expect("northstar reader");
        assert!(!Arc::ptr_eq(&a, &b));
        assert!(
            reader_for(&state, "unknown.jesadakorn.com", "acme").is_err(),
            "an unregistered host must fail closed"
        );
        assert_eq!(
            application::read_next_transition(a.as_ref(), "acme", 1)
                .await
                .expect("acme read")
                .deadline,
            Some(11)
        );
        assert_eq!(
            application::read_next_transition(b.as_ref(), "northstar", 1)
                .await
                .expect("northstar read")
                .deadline,
            Some(22)
        );
    }

    #[test]
    fn maps_failure_kinds_to_fail_closed_statuses() {
        assert_eq!(
            status(FailureKind::InvalidInput),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(
            status(FailureKind::Unavailable),
            StatusCode::SERVICE_UNAVAILABLE
        );
        let response = error_response(domain::unavailable());
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn detects_repeated_bounded_headers() {
        let mut headers = HeaderMap::new();
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("acme"));
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("other"));
        assert!(unique_header(&headers, CONTEXT_COMPANY).is_err());
    }
}
