//! Owner-local axum ingress for lane 6's calendar-availability leave-day read.
//!
//! Transport facts only: an exact registered Host/Holding target, a bounded
//! Company header and one bounded JSON read request. The Postgres reader owns
//! the SQL and the projection; this module maps failures to explicit statuses
//! and adds private, no-store headers. There is no fallback path.

use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::application::{
    CalendarAvailabilityRead, CalendarPeopleRead, CalendarPersonScopeRead,
    UnavailableCalendarPeopleRead, UnavailableCalendarPersonScopeRead,
};
use super::domain::{
    CalendarPeoplePage, CalendarPeopleQuery, LeaveDayQuery, PersonScopeQuery, PersonScopeResult,
    valid_id, validate, validate_calendar_people, validate_person_scope,
};
use super::postgres::{
    PgCalendarAvailabilityRead, PgCalendarPeopleRead, PgCalendarPersonScopeRead,
};

const BODY_LIMIT: usize = 8 * 1024;
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

pub const READ_PATH: &str =
    "/api/v1/holdings/{holding_slug}/people-payroll/lane6/calendar-availability-leave";
pub const PERSON_SCOPE_PATH: &str =
    "/api/v1/holdings/{holding_slug}/people-payroll/lane6/calendar-person-scope";
pub const CALENDAR_PEOPLE_PATH: &str =
    "/api/v1/holdings/{holding_slug}/people-payroll/lane6/calendar-access-people";

/// Exact registered Holding target. No wildcard or caller-supplied host is
/// accepted; the lead mounts this router with the same targets as the owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn CalendarAvailabilityRead>>,
    scope_targets: BTreeMap<(String, String), Arc<dyn CalendarPersonScopeRead>>,
    people_targets: BTreeMap<(String, String), Arc<dyn CalendarPeopleRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE6_REQUEST_INVALID",
        "Use one bounded calendar availability request.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE6_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
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

/// Single shared validation and state construction. Every public constructor
/// funnels through here, so target isolation and error mapping stay identical
/// for all owner-local reads on this router.
fn configure_entries(
    entries: Vec<(
        LaneTarget,
        Arc<dyn CalendarAvailabilityRead>,
        Arc<dyn CalendarPersonScopeRead>,
        Arc<dyn CalendarPeopleRead>,
    )>,
) -> Result<Arc<LaneState>, BusinessError> {
    let mut configured = BTreeMap::new();
    let mut scope_configured = BTreeMap::new();
    let mut people_configured = BTreeMap::new();
    for (target, reader, scope_reader, people_reader) in entries {
        if target.host.is_empty()
            || target.host.len() > 512
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
            || scope_configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    scope_reader,
                )
                .is_some()
            || people_configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    people_reader,
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Arc::new(LaneState {
        targets: configured,
        scope_targets: scope_configured,
        people_targets: people_configured,
    }))
}

fn router_from_state(state: Arc<LaneState>) -> Router {
    Router::new()
        .route(READ_PATH, post(read))
        .route(PERSON_SCOPE_PATH, post(person_scope))
        .route(CALENDAR_PEOPLE_PATH, post(calendar_people))
        .with_state(state)
}

/// Build the native read router. The lead mounts it alongside the other
/// People/Payroll owner routers; no new TypeScript SQL or business rule is
/// introduced here.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn CalendarAvailabilityRead> =
        Arc::new(PgCalendarAvailabilityRead::new(pool.clone()));
    let scope_reader: Arc<dyn CalendarPersonScopeRead> =
        Arc::new(PgCalendarPersonScopeRead::new(pool.clone()));
    let people_reader: Arc<dyn CalendarPeopleRead> =
        Arc::new(PgCalendarPeopleRead::new(pool));
    let state = configure_entries(
        targets
            .into_iter()
            .map(|target| {
                (
                    target,
                    reader.clone(),
                    scope_reader.clone(),
                    people_reader.clone(),
                )
            })
            .collect(),
    )?;
    Ok(router_from_state(state))
}

/// Testable construction seam: composition and unit tests inject the reader;
/// all paths use the exact same transport checks. The other reads fail closed
/// here because no reader was injected for them.
pub fn router_with_reader(
    reader: Arc<dyn CalendarAvailabilityRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let state = configure_entries(
        targets
            .into_iter()
            .map(|target| {
                (
                    target,
                    reader.clone(),
                    Arc::new(UnavailableCalendarPersonScopeRead)
                        as Arc<dyn CalendarPersonScopeRead>,
                    Arc::new(UnavailableCalendarPeopleRead) as Arc<dyn CalendarPeopleRead>,
                )
            })
            .collect(),
    )?;
    Ok(router_from_state(state))
}

/// Per-Holding construction: each registered Holding carries its own database
/// pool, so every reader executes against that Holding's database only. A target
/// (host, holding_slug) can never fall back to another Holding's pool.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = configure_entries(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn CalendarAvailabilityRead> =
                    Arc::new(PgCalendarAvailabilityRead::new(pool.clone()));
                let scope_reader: Arc<dyn CalendarPersonScopeRead> =
                    Arc::new(PgCalendarPersonScopeRead::new(pool.clone()));
                let people_reader: Arc<dyn CalendarPeopleRead> =
                    Arc::new(PgCalendarPeopleRead::new(pool));
                (target, reader, scope_reader, people_reader)
            })
            .collect(),
    )?;
    Ok(router_from_state(state))
}

async fn read(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug))
            .ok_or_else(missing)?
            .clone();
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?
            .to_owned();
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let query: LeaveDayQuery = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        validate(&company, &query)?;
        let leave_days = reader.leave_days(&company, &query).await?;
        Ok::<_, BusinessError>(serde_json::json!({
            "leaveDays": leave_days
                .into_iter()
                .map(|day| serde_json::json!({"userId": day.user_id, "leaveDate": day.leave_date}))
                .collect::<Vec<_>>()
        }))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn person_scope(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .scope_targets
            .get(&(host.to_owned(), slug))
            .ok_or_else(missing)?
            .clone();
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?
            .to_owned();
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let query: PersonScopeQuery = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        validate_person_scope(&company, &query)?;
        let present = reader
            .present(&company, &query.person_id, &query.scope_ids)
            .await?;
        serde_json::to_value(PersonScopeResult::new(present)).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "PEOPLE_PAYROLL_LANE6_RESPONSE_INVALID",
                "The native People Payroll reply could not be rendered.",
            )
        })
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn calendar_people(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .people_targets
            .get(&(host.to_owned(), slug))
            .ok_or_else(missing)?
            .clone();
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?
            .to_owned();
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let query: CalendarPeopleQuery = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        validate_calendar_people(&company, &query)?;
        let people = reader.people(&company, &query).await?;
        serde_json::to_value(CalendarPeoplePage::new(people)).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "PEOPLE_PAYROLL_LANE6_RESPONSE_INVALID",
                "The native People Payroll reply could not be rendered.",
            )
        })
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
    use crate::adapters::native_lane_6::application::LeaveDayFuture;
    use crate::adapters::native_lane_6::domain::LeaveDay;

    struct FakeReader {
        /// Distinguishes injected reader instances in the isolation test.
        tag: &'static str,
        fail: bool,
    }

    impl CalendarAvailabilityRead for FakeReader {
        fn leave_days<'a>(
            &'a self,
            company: &'a str,
            query: &'a LeaveDayQuery,
        ) -> LeaveDayFuture<'a, Vec<LeaveDay>> {
            let rows = vec![LeaveDay {
                user_id: self.tag.to_owned(),
                leave_date: "2026-03-02".into(),
            }];
            let company = company.to_owned();
            let query = query.clone();
            let fail = self.fail;
            Box::pin(async move {
                if fail {
                    return Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "PEOPLE_PAYROLL_LANE6_READ_UNAVAILABLE",
                        "unavailable",
                    ));
                }
                // The fake reader records that it received a bounded scope.
                assert!(company.len() <= 200);
                assert!(!query.user_ids.is_empty());
                Ok(rows)
            })
        }
    }

    struct FakeScopeReader {
        present: bool,
    }

    impl CalendarPersonScopeRead for FakeScopeReader {
        fn present<'a>(
            &'a self,
            company: &'a str,
            person: &'a str,
            scope_ids: &'a str,
        ) -> super::super::application::PersonScopeFuture<'a, bool> {
            assert!(valid_id(company));
            assert!(valid_id(person));
            assert!(scope_ids.starts_with('['));
            let present = self.present;
            Box::pin(async move { Ok(present) })
        }
    }

    fn scope_reader(present: bool) -> Arc<dyn CalendarPersonScopeRead> {
        Arc::new(FakeScopeReader { present })
    }

    fn reader() -> Arc<dyn CalendarAvailabilityRead> {
        tagged_reader("u1")
    }

    fn tagged_reader(tag: &'static str) -> Arc<dyn CalendarAvailabilityRead> {
        Arc::new(FakeReader { tag, fail: false })
    }

    #[test]
    fn rejects_duplicate_or_invalid_targets() {
        let target = LaneTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        };
        assert!(router_with_reader(reader(), vec![target.clone()]).is_ok());
        assert!(router_with_reader(reader(), vec![target.clone(), target]).is_err());
        assert!(router_with_reader(reader(), vec![]).is_err());
        assert!(
            router_with_reader(
                reader(),
                vec![LaneTarget {
                    host: "h".into(),
                    holding_slug: "BAD SLUG".into()
                }]
            )
            .is_err()
        );
    }

    #[test]
    fn two_holdings_route_to_two_distinct_readers() {
        // Reader-injection path mirrors router_for_holdings: each Holding entry
        // carries its own reader (backed by its own pool in production).
        let acme = LaneTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        };
        let northstar = LaneTarget {
            host: "northstar.example.test".into(),
            holding_slug: "northstar".into(),
        };
        let acme_reader = tagged_reader("acme-user");
        let northstar_reader = tagged_reader("northstar-user");
        let state = configure_entries(vec![
            (
                acme.clone(),
                acme_reader.clone(),
                scope_reader(true),
                people_reader(true),
            ),
            (
                northstar.clone(),
                northstar_reader.clone(),
                scope_reader(false),
                people_reader(false),
            ),
        ])
        .expect("two distinct holding targets configure");

        assert_eq!(state.targets.len(), 2);
        let routed_acme = state
            .targets
            .get(&(acme.host.clone(), acme.holding_slug.clone()))
            .expect("acme target routed");
        let routed_northstar = state
            .targets
            .get(&(northstar.host.clone(), northstar.holding_slug.clone()))
            .expect("northstar target routed");
        assert!(
            !Arc::ptr_eq(routed_acme, routed_northstar),
            "each Holding must resolve to its own reader/pool"
        );
        assert!(Arc::ptr_eq(routed_acme, &acme_reader));
        assert!(Arc::ptr_eq(routed_northstar, &northstar_reader));
        // No cross-Holding fallback: an unregistered (host, slug) pair is absent.
        assert!(
            !state
                .targets
                .contains_key(&("other.example.test".into(), "acme".into()))
        );
    }

    #[test]
    fn person_scope_reader_is_isolated_per_holding() {
        let acme = LaneTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        };
        let northstar = LaneTarget {
            host: "northstar.example.test".into(),
            holding_slug: "northstar".into(),
        };
        let acme_scope = scope_reader(true);
        let northstar_scope = scope_reader(false);
        let state = configure_entries(vec![
            (acme.clone(), reader(), acme_scope.clone()),
            (northstar.clone(), reader(), northstar_scope.clone()),
        ])
        .expect("both scope readers configure");
        assert_eq!(state.scope_targets.len(), 2);
        assert!(Arc::ptr_eq(
            state
                .scope_targets
                .get(&(acme.host.clone(), acme.holding_slug.clone()))
                .unwrap(),
            &acme_scope
        ));
        assert!(Arc::ptr_eq(
            state
                .scope_targets
                .get(&(northstar.host.clone(), northstar.holding_slug.clone()))
                .unwrap(),
            &northstar_scope
        ));
    }

    #[test]
    fn error_mapping_preserves_failure_kinds() {
        assert_eq!(
            status(FailureKind::InvalidInput),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            status(FailureKind::Unavailable),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
    }

    #[test]
    fn response_envelope_is_json_and_never_cached() {
        let response = bounded(serde_json::json!({"leaveDays":[]}));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        assert_eq!(
            response.headers().get(header::REFERRER_POLICY).unwrap(),
            "no-referrer"
        );
    }
}
