//! Owner-local axum router for the lane-7 legacy people clock. Target, Host,
//! Company and clock headers are isolated exactly as in the Commerce native
//! read companion; any failure fails closed without a TypeScript fallback.

use std::collections::BTreeMap;
use std::sync::Arc;

use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Request, Response, Router,
    State, StatusCode, header, json, routing::get,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};

use super::application::{
    self, AuthorityPersonLookup, AuthorityQuery, LegacyPeopleClock, TransitionFuture,
    WorkspacePeoplePayments,
};
use super::domain::{
    self, AUTHORITY_READ_KIND, AUTHORITY_ROUTE, AuthorityPerson, AuthorityPersonQuery,
    PAYMENTS_READ_KIND, PAYMENTS_ROUTE, READ_KIND, ROUTE, WorkspacePaymentRow,
};
use super::postgres::{
    PgAuthorityPersonLookup, PgLegacyPeopleClock, PgWorkspacePeoplePayments,
};

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
    payments: BTreeMap<(String, String), Arc<dyn WorkspacePeoplePayments>>,
    authority: BTreeMap<(String, String), Arc<dyn AuthorityPersonLookup>>,
}

struct UnavailableAuthority;
impl AuthorityPersonLookup for UnavailableAuthority {
    fn lookup<'a>(
        &'a self,
        _company: &'a str,
        _query: AuthorityQuery,
    ) -> TransitionFuture<'a, AuthorityPerson> {
        Box::pin(async { Err(domain::unavailable()) })
    }
}

/// Test/alternative-host reader: the payments route fails closed instead of
/// borrowing another Holding's pool.
struct UnavailablePayments;
impl WorkspacePeoplePayments for UnavailablePayments {
    fn outstanding<'a>(
        &'a self,
        _company: &'a str,
    ) -> TransitionFuture<'a, Vec<WorkspacePaymentRow>> {
        Box::pin(async { Err(domain::unavailable()) })
    }
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
    factory: impl Fn(P) -> (
        Arc<dyn LegacyPeopleClock>,
        Arc<dyn WorkspacePeoplePayments>,
        Arc<dyn AuthorityPersonLookup>,
    ),
) -> Result<Arc<Lane7State>, BusinessError> {
    let mut clocks = BTreeMap::new();
    let mut payments = BTreeMap::new();
    let mut authority = BTreeMap::new();
    for (target, source) in targets {
        if target.host.is_empty() || !valid_slug(&target.holding_slug) {
            return Err(domain::configuration_invalid());
        }
        let key = (target.host.clone(), target.holding_slug.clone());
        let (clock, payment_reader, authority_reader) = factory(source);
        if clocks.insert(key.clone(), clock).is_some()
            || payments.insert(key.clone(), payment_reader).is_some()
            || authority.insert(key, authority_reader).is_some()
        {
            return Err(domain::configuration_invalid());
        }
    }
    if clocks.is_empty() {
        return Err(domain::configuration_invalid());
    }
    Ok(Arc::new(Lane7State {
        targets: clocks,
        payments,
        authority,
    }))
}

fn router_from_state(state: Arc<Lane7State>) -> Router {
    Router::new()
        .route(ROUTE, get(next_transition))
        .route(PAYMENTS_ROUTE, get(payments))
        .route(AUTHORITY_ROUTE, get(authority_person))
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

fn payments_reader_for(
    state: &Lane7State,
    host: &str,
    slug: &str,
) -> Result<Arc<dyn WorkspacePeoplePayments>, BusinessError> {
    state
        .payments
        .get(&(host.to_owned(), slug.to_owned()))
        .cloned()
        .ok_or_else(missing)
}

fn authority_reader_for(
    state: &Lane7State,
    host: &str,
    slug: &str,
) -> Result<Arc<dyn AuthorityPersonLookup>, BusinessError> {
    state
        .authority
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
        |reader| {
            (
                reader,
                Arc::new(UnavailablePayments) as Arc<dyn WorkspacePeoplePayments>,
                Arc::new(UnavailableAuthority) as Arc<dyn AuthorityPersonLookup>,
            )
        },
    )?;
    Ok(router_from_state(state))
}

/// Build the native router over one shared owner pool. The lead mounts it; this
/// lane never edits composition or another owner's router.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let state = build_state(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
        |pool| {
            (
                Arc::new(PgLegacyPeopleClock::new(pool.clone())) as Arc<dyn LegacyPeopleClock>,
                Arc::new(PgWorkspacePeoplePayments::new(pool.clone()))
                    as Arc<dyn WorkspacePeoplePayments>,
                Arc::new(PgAuthorityPersonLookup::new(pool)) as Arc<dyn AuthorityPersonLookup>,
            )
        },
    )?;
    Ok(router_from_state(state))
}

/// Build the native router with one owner reader per registered Holding pool.
/// Every read family gets its own per-Holding reader, so a request can never
/// cross a Holding boundary. Composition keeps `targets[i]` aligned with
/// `pools[i + 1]`.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = build_state(targets, |pool| {
        (
            Arc::new(PgLegacyPeopleClock::new(pool.clone())) as Arc<dyn LegacyPeopleClock>,
            Arc::new(PgWorkspacePeoplePayments::new(pool.clone()))
                as Arc<dyn WorkspacePeoplePayments>,
            Arc::new(PgAuthorityPersonLookup::new(pool)) as Arc<dyn AuthorityPersonLookup>,
        )
    })?;
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

async fn payments(
    State(state): State<Arc<Lane7State>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = payments_reader_for(&state, host, &slug)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| domain::valid_id(value))
            .ok_or_else(domain::invalid_company)?;
        let rows = application::read_workspace_people_payments(reader.as_ref(), company).await?;
        Ok::<_, BusinessError>(json!({
            "kind": PAYMENTS_READ_KIND,
            "payments": rows
        }))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn authority_person(
    State(state): State<Arc<Lane7State>>,
    Path(slug): Path<String>,
    Query(query): Query<AuthorityPersonQuery>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = authority_reader_for(&state, host, &slug)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| domain::valid_id(value))
            .ok_or_else(domain::invalid_company)?;
        let query = AuthorityQuery::try_from(&query)?;
        let person = application::read_authority_person(reader.as_ref(), company, query).await?;
        Ok::<_, BusinessError>(json!({
            "kind": AUTHORITY_READ_KIND,
            "person": {"id": person.id}
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

    struct TaggedPayments(&'static str);
    impl WorkspacePeoplePayments for TaggedPayments {
        fn outstanding<'a>(
            &'a self,
            _company: &'a str,
        ) -> TransitionFuture<'a, Vec<WorkspacePaymentRow>> {
            let tag = self.0;
            Box::pin(async move {
                Ok(vec![WorkspacePaymentRow {
                    id: format!("{tag}-batch"),
                    source_type: "payroll".to_owned(),
                    source_id: "run-1".to_owned(),
                    currency: "THB".to_owned(),
                    status: "approved".to_owned(),
                    due_date: None,
                    amount_minor: if tag == "acme" { 11 } else { 22 },
                }])
            })
        }
    }

    struct TaggedAuthority(&'static str);
    impl AuthorityPersonLookup for TaggedAuthority {
        fn lookup<'a>(
            &'a self,
            _company: &'a str,
            _query: AuthorityQuery,
        ) -> TransitionFuture<'a, AuthorityPerson> {
            let tag = self.0;
            Box::pin(async move {
                Ok(AuthorityPerson {
                    id: Some(format!("{tag}-person")),
                })
            })
        }
    }

    #[tokio::test]
    async fn authority_route_projects_only_the_bounded_id() {
        let state = build_state(vec![(target(), "acme")], |tag| {
            (
                Arc::new(TaggedClock(tag)) as Arc<dyn LegacyPeopleClock>,
                Arc::new(TaggedPayments(tag)) as Arc<dyn WorkspacePeoplePayments>,
                Arc::new(TaggedAuthority(tag)) as Arc<dyn AuthorityPersonLookup>,
            )
        })
        .expect("state");
        let request = Request::builder()
            .header(header::HOST, "acme.jesadakorn.com")
            .header(CONTEXT_COMPANY, "acme")
            .body(erpx_server_runtime::public::http_support::Body::empty())
            .expect("request");
        let response = authority_person(
            State(state),
            Path("acme".to_owned()),
            Query(AuthorityPersonQuery {
                kind: super::super::domain::AuthorityKind::Actor,
                user_id: "usr_1".to_owned(),
                person_id: String::new(),
                position_id: String::new(),
                appointment_kind: String::new(),
                scope_id: String::new(),
                self_ceo: false,
            }),
            request,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = erpx_server_runtime::public::http_support::body::to_bytes(
            response.into_body(),
            usize::MAX,
        )
        .await
        .expect("body");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).expect("json"),
            serde_json::json!({"kind": AUTHORITY_READ_KIND, "person": {"id": "acme-person"}})
        );
    }

    #[tokio::test]
    async fn authority_route_rejects_unbounded_queries() {
        let state = build_state(vec![(target(), "acme")], |_tag| {
            (
                Arc::new(BusyClock) as Arc<dyn LegacyPeopleClock>,
                Arc::new(UnavailablePayments) as Arc<dyn WorkspacePeoplePayments>,
                Arc::new(UnavailableAuthority) as Arc<dyn AuthorityPersonLookup>,
            )
        })
        .expect("state");
        let request = Request::builder()
            .header(header::HOST, "acme.jesadakorn.com")
            .header(CONTEXT_COMPANY, "acme")
            .body(erpx_server_runtime::public::http_support::Body::empty())
            .expect("request");
        let response = authority_person(
            State(state),
            Path("acme".to_owned()),
            Query(AuthorityPersonQuery {
                kind: super::super::domain::AuthorityKind::Appointee,
                user_id: String::new(),
                person_id: String::new(),
                position_id: String::new(),
                appointment_kind: "head".to_owned(),
                scope_id: "dept_1".to_owned(),
                self_ceo: false,
            }),
            request,
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
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
            (
                Arc::new(TaggedClock(tag)) as Arc<dyn LegacyPeopleClock>,
                Arc::new(TaggedPayments(tag)) as Arc<dyn WorkspacePeoplePayments>,
                Arc::new(TaggedAuthority(tag)) as Arc<dyn AuthorityPersonLookup>,
            )
        })
        .expect("state");
        let a = reader_for(&state, "acme.jesadakorn.com", "acme").expect("acme reader");
        let b =
            reader_for(&state, "northstar.jesadakorn.com", "northstar").expect("northstar reader");
        assert!(!Arc::ptr_eq(&a, &b));
        let pa = payments_reader_for(&state, "acme.jesadakorn.com", "acme").expect("acme payments");
        let pb = payments_reader_for(&state, "northstar.jesadakorn.com", "northstar")
            .expect("northstar payments");
        assert!(!Arc::ptr_eq(&pa, &pb));
        assert!(
            reader_for(&state, "unknown.jesadakorn.com", "acme").is_err(),
            "an unregistered host must fail closed"
        );
        assert!(
            payments_reader_for(&state, "unknown.jesadakorn.com", "acme").is_err(),
            "payments must fail closed for an unregistered host too"
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
        assert_eq!(
            application::read_workspace_people_payments(pa.as_ref(), "acme")
                .await
                .expect("acme payments read")[0]
                .amount_minor,
            11
        );
        assert_eq!(
            application::read_workspace_people_payments(pb.as_ref(), "northstar")
                .await
                .expect("northstar payments read")[0]
                .amount_minor,
            22
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
