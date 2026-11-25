//! Owner-local axum router for the lane 27 Projects actuals read. It follows
//! the Commerce quality-product target/header isolation contract: one exact
//! registered Holding host + slug, one Company identity, no caller SQL.

use super::application::{
    ProjectActualsRead, read_cost_allocations, read_handoff_project, read_handoff_work,
    read_handoffs, read_work_logs,
};
use super::domain::{HandoffScope, LaneTarget, handoff_scope, invalid, scope, valid_slug};
use super::postgres::PgProjectActualsRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const CONTEXT_PROJECT: HeaderName = HeaderName::from_static("x-erpx-context-project");
const CONTEXT_HANDOFF: HeaderName = HeaderName::from_static("x-erpx-context-handoff");

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn ProjectActualsRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROJECT_ACTUALS_LANE27_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
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

/// The reader-injection path: every target carries its own reader, so distinct
/// Holdings can be backed by distinct pools. Host + slug remain the exact map
/// key and duplicates are refused before any router is built.
fn state_from_readers(
    targets: Vec<(LaneTarget, Arc<dyn ProjectActualsRead>)>,
) -> Result<LaneState, BusinessError> {
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
    Ok(LaneState { targets: configured })
}

fn router_from_state(state: LaneState) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/projects/actuals/work-logs",
            get(work_logs),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/projects/actuals/cost-allocations",
            get(cost_allocations),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/projects/handoffs",
            get(handoffs),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/projects/handoff-work",
            get(handoff_work),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/projects/handoff-project",
            get(handoff_project),
        )
        .with_state(Arc::new(state))
}

/// Build the native read router with one shared pool. The lead mounts it; no
/// composition or shared module is edited here.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgProjectActualsRead::new(pool)), targets)
}

/// Per-Holding constructor: one `PgProjectActualsRead`, and therefore one
/// database pool, is created for each target. Composition pairs each configured
/// Holding with `pools[i + 1].clone()`; no Holding can observe another's pool.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let readers = targets
        .into_iter()
        .map(|(target, pool)| {
            let reader: Arc<dyn ProjectActualsRead> = Arc::new(PgProjectActualsRead::new(pool));
            (target, reader)
        })
        .collect();
    router_with_readers(readers)
}

/// Same contract with one injected owner reader reused by every target; kept
/// for module tests that need no live database.
pub fn router_with_reader(
    reader: Arc<dyn ProjectActualsRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Reader-injection path used by both constructors and by the isolation test.
pub(crate) fn router_with_readers(
    targets: Vec<(LaneTarget, Arc<dyn ProjectActualsRead>)>,
) -> Result<Router, BusinessError> {
    Ok(router_from_state(state_from_readers(targets)?))
}

fn target_reader(
    state: &LaneState,
    request: &Request,
    slug: &str,
) -> Result<Arc<dyn ProjectActualsRead>, BusinessError> {
    let host = unique_header(request.headers(), header::HOST)?
        .filter(|host| !host.is_empty())
        .ok_or_else(missing)?;
    Ok(state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?
        .clone())
}

fn request_scope(
    state: &LaneState,
    request: &Request,
    slug: &str,
) -> Result<(Arc<dyn ProjectActualsRead>, super::domain::ActualsScope), BusinessError> {
    let reader = target_reader(state, request, slug)?;
    let company = unique_header(request.headers(), CONTEXT_COMPANY)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?;
    let user = unique_header(request.headers(), CONTEXT_USER)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?;
    let project = unique_header(request.headers(), CONTEXT_PROJECT)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?;
    let scope = scope(company, user, project)?;
    Ok((reader, scope))
}

fn required_context(headers: &HeaderMap, name: HeaderName) -> Result<String, BusinessError> {
    let value = unique_header(headers, name)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?;
    Ok(value.to_owned())
}

fn optional_context(headers: &HeaderMap, name: HeaderName) -> Result<Option<String>, BusinessError> {
    match unique_header(headers, name)? {
        None => Ok(None),
        Some(value) => {
            if !valid_id(value) {
                return Err(invalid());
            }
            Ok(Some(value.to_owned()))
        }
    }
}

fn request_handoff_scope(
    state: &LaneState,
    request: &Request,
    slug: &str,
    with_project: bool,
    with_handoff: bool,
) -> Result<(Arc<dyn ProjectActualsRead>, HandoffScope), BusinessError> {
    let reader = target_reader(state, request, slug)?;
    let company = required_context(request.headers(), CONTEXT_COMPANY)?;
    let user = required_context(request.headers(), CONTEXT_USER)?;
    let project = if with_project {
        required_context(request.headers(), CONTEXT_PROJECT)?
    } else {
        optional_context(request.headers(), CONTEXT_PROJECT)?.unwrap_or_default()
    };
    let handoff = if with_handoff {
        required_context(request.headers(), CONTEXT_HANDOFF)?
    } else {
        optional_context(request.headers(), CONTEXT_HANDOFF)?.unwrap_or_default()
    };
    Ok((reader, handoff_scope(&company, &user, &project, &handoff)?))
}

async fn work_logs(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let (reader, scope) = request_scope(&state, &request, &slug)?;
        let rows = read_work_logs(reader.as_ref(), &scope).await?;
        Ok::<_, BusinessError>(serde_json::json!({"workLogs": rows}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn cost_allocations(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let (reader, scope) = request_scope(&state, &request, &slug)?;
        let rows = read_cost_allocations(reader.as_ref(), &scope).await?;
        Ok::<_, BusinessError>(serde_json::json!({"costAllocations": rows}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn handoffs(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let (reader, scope) = request_handoff_scope(&state, &request, &slug, true, false)?;
        let rows = read_handoffs(reader.as_ref(), &scope).await?;
        Ok::<_, BusinessError>(serde_json::json!({"handoffs": rows}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn handoff_work(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let (reader, scope) = request_handoff_scope(&state, &request, &slug, false, false)?;
        let rows = read_handoff_work(reader.as_ref(), &scope).await?;
        Ok::<_, BusinessError>(serde_json::json!({"handoffs": rows}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn handoff_project(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let (reader, scope) = request_handoff_scope(&state, &request, &slug, false, true)?;
        let project = read_handoff_project(reader.as_ref(), &scope).await?;
        Ok::<_, BusinessError>(serde_json::json!({"handoff": project}))
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
    use crate::adapters::native_lane_27::application::ActualsFuture;
    use crate::adapters::native_lane_27::domain::{
        ActualsScope, CostAllocationRow, HandoffProject, HandoffRow, HandoffScope, HandoffWorkRow,
        WorkLogRow,
    };
    use erpx_server_runtime::public::http_support::Body;

    struct Empty;
    impl ProjectActualsRead for Empty {
        fn work_logs<'a>(&'a self, _: &'a ActualsScope) -> ActualsFuture<'a, Vec<WorkLogRow>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn cost_allocations<'a>(
            &'a self,
            _: &'a ActualsScope,
        ) -> ActualsFuture<'a, Vec<CostAllocationRow>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn handoffs<'a>(
            &'a self,
            _: &'a HandoffScope,
        ) -> ActualsFuture<'a, Vec<HandoffRow>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn handoff_project<'a>(
            &'a self,
            _: &'a HandoffScope,
        ) -> ActualsFuture<'a, Option<HandoffProject>> {
            Box::pin(async { Ok(None) })
        }
        fn handoff_work<'a>(
            &'a self,
            _: &'a HandoffScope,
        ) -> ActualsFuture<'a, Vec<HandoffWorkRow>> {
            Box::pin(async { Ok(vec![]) })
        }
    }

    #[test]
    fn duplicate_targets_are_refused() {
        let reader: Arc<dyn ProjectActualsRead> = Arc::new(Empty);
        let targets = vec![
            LaneTarget { host: "acme.jesadakorn.com".into(), holding_slug: "acme".into() },
            LaneTarget { host: "acme.jesadakorn.com".into(), holding_slug: "acme".into() },
        ];
        assert_eq!(
            router_with_reader(reader, targets).err().unwrap().code,
            "PROJECT_ACTUALS_LANE27_CONFIG_INVALID"
        );
    }

    #[test]
    fn no_targets_are_refused() {
        let reader: Arc<dyn ProjectActualsRead> = Arc::new(Empty);
        assert!(router_with_reader(reader, vec![]).is_err());
    }

    #[test]
    fn error_mapping_is_fail_closed() {
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// Proves the per-target map routes each Holding to its own injected reader
    /// (the same path `router_for_holdings` uses with one pool per Holding).
    #[test]
    fn two_targets_route_to_two_distinct_readers() {
        let acme: Arc<dyn ProjectActualsRead> = Arc::new(Empty);
        let northstar: Arc<dyn ProjectActualsRead> = Arc::new(Empty);
        assert!(!Arc::ptr_eq(&acme, &northstar));
        let pairs = vec![
            (
                LaneTarget { host: "acme.jesadakorn.com".into(), holding_slug: "acme".into() },
                acme.clone(),
            ),
            (
                LaneTarget { host: "northstar.jesadakorn.com".into(), holding_slug: "northstar".into() },
                northstar.clone(),
            ),
        ];
        let state = state_from_readers(pairs.clone()).unwrap();
        assert_eq!(state.targets.len(), 2);
        assert!(router_with_readers(pairs).is_ok());

        let request = |host: &'static str| {
            Request::builder()
                .header(header::HOST, host)
                .header(CONTEXT_COMPANY, "company-1")
                .header(CONTEXT_USER, "user-1")
                .header(CONTEXT_PROJECT, "project-1")
                .body(Body::empty())
                .unwrap()
        };
        let (routed_acme, _) = request_scope(
            &state,
            &request("acme.jesadakorn.com"),
            "acme",
        )
        .unwrap();
        let (routed_northstar, _) = request_scope(
            &state,
            &request("northstar.jesadakorn.com"),
            "northstar",
        )
        .unwrap();
        assert!(Arc::ptr_eq(&routed_acme, &acme));
        assert!(Arc::ptr_eq(&routed_northstar, &northstar));
        assert!(!Arc::ptr_eq(&routed_acme, &routed_northstar));
    }

    #[test]
    fn handoff_routes_require_their_own_bounded_identity() {
        let reader: Arc<dyn ProjectActualsRead> = Arc::new(Empty);
        let state = state_from_readers(vec![(
            LaneTarget { host: "acme.jesadakorn.com".into(), holding_slug: "acme".into() },
            reader,
        )])
        .unwrap();
        let request = |handoff: bool| {
            let mut builder = Request::builder()
                .header(header::HOST, "acme.jesadakorn.com")
                .header(CONTEXT_COMPANY, "company-1")
                .header(CONTEXT_USER, "user-1");
            if handoff {
                builder = builder.header(CONTEXT_HANDOFF, "handoff-1");
            }
            builder.body(Body::empty()).unwrap()
        };
        assert!(request_handoff_scope(&state, &request(false), "acme", false, false).is_ok());
        assert!(request_handoff_scope(&state, &request(false), "acme", true, false).is_err());
        assert!(request_handoff_scope(&state, &request(false), "acme", false, true).is_err());
        let scoped = request_handoff_scope(&state, &request(true), "acme", false, true).unwrap();
        assert_eq!(scoped.1.handoff, "handoff-1");
        assert_eq!(scoped.1.company, "company-1");
    }
}
