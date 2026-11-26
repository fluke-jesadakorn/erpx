//! Owner-local axum ingress for the Records reference-search read family.
//!
//! Transport facts only: exact registered Host/Holding targets, a strict `q`
//! query, the signed scope headers injected by composition, and owner error
//! mapping. The handler and the owner SQL adapter below execute the read. No
//! TypeScript prepares or executes SQL on this path.

use crate::adapters::record_search::PgRecordSearch;
use crate::application::record_search::{
    RecordSearchHandler, RecordSearchSource, SearchAdmission, SearchFuture,
};
use crate::domain::record_search::{
    PrincipalHit, RecordHit, SearchActor, SearchQuery, SearchTarget, configuration_invalid,
    request_invalid, valid_id, valid_slug,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

const USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const ASSIGNMENT: HeaderName = HeaderName::from_static("x-erpx-context-assignment");
const COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const BRANCH: HeaderName = HeaderName::from_static("x-erpx-workspace-branch");
const DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-workspace-department");
const MODULES: HeaderName = HeaderName::from_static("x-erpx-context-modules");
const QUERY_LIMIT: usize = 4_096;

struct SharedRecordSource(Arc<dyn RecordSearchSource>);

impl RecordSearchSource for SharedRecordSource {
    fn records<'a>(
        &'a self,
        actor: &'a SearchActor,
        query: &'a SearchQuery,
    ) -> SearchFuture<'a, Vec<RecordHit>> {
        self.0.records(actor, query)
    }
}

struct SharedAdmission(Arc<dyn SearchAdmission>);

impl SearchAdmission for SharedAdmission {
    fn visible<'a>(
        &'a self,
        actor: &'a SearchActor,
        hit: &'a RecordHit,
    ) -> SearchFuture<'a, bool> {
        self.0.visible(actor, hit)
    }

    fn principal_hits<'a>(
        &'a self,
        needle: &'a str,
        now: i64,
    ) -> SearchFuture<'a, Vec<PrincipalHit>> {
        self.0.principal_hits(needle, now)
    }
}

type Handler = RecordSearchHandler<SharedRecordSource, SharedAdmission>;

struct SearchHttpState {
    targets: BTreeMap<(String, String), Arc<Handler>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

fn required(headers: &HeaderMap, name: HeaderName) -> Result<String, BusinessError> {
    unique_header(headers, name)?
        .filter(|value| valid_id(value))
        .map(str::to_owned)
        .ok_or_else(request_invalid)
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

fn bounded(body: Value) -> Response {
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

fn target<'a>(
    state: &'a SearchHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a Arc<Handler>, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn valid_target(target: &SearchTarget) -> bool {
    !target.host.is_empty() && valid_slug(&target.holding_slug)
}

/// Pure target check, exercised without a live pool.
pub fn validate_targets(targets: &[SearchTarget]) -> Result<(), BusinessError> {
    let mut seen = BTreeMap::new();
    for target in targets {
        if !valid_target(target)
            || seen
                .insert((&target.host, &target.holding_slug), ())
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if targets.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(())
}

fn percent_decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' => {
                let high = bytes
                    .get(index + 1)
                    .and_then(|value| (*value as char).to_digit(16))
                    .ok_or_else(request_invalid)?;
                let low = bytes
                    .get(index + 2)
                    .and_then(|value| (*value as char).to_digit(16))
                    .ok_or_else(request_invalid)?;
                out.push((high * 16 + low) as u8);
                index += 3;
            }
            value => {
                out.push(value);
                index += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| request_invalid())
}

/// Accept only `?q=` with one value and no other query keys.
fn query_term(query: Option<&str>) -> Result<String, BusinessError> {
    let raw = query.unwrap_or_default();
    if raw.len() > QUERY_LIMIT {
        return Err(request_invalid());
    }
    let mut term = None;
    for pair in raw.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').ok_or_else(request_invalid)?;
        if percent_decode(key)? != "q" || term.is_some() {
            return Err(request_invalid());
        }
        term = Some(percent_decode(value)?);
    }
    Ok(term.unwrap_or_default())
}

fn actor(headers: &HeaderMap) -> Result<SearchActor, BusinessError> {
    let branch = unique_header(headers, BRANCH)?
        .filter(|value| valid_id(value))
        .map(str::to_owned);
    let department = unique_header(headers, DEPARTMENT)?
        .filter(|value| valid_id(value))
        .map(str::to_owned);
    let modules = unique_header(headers, MODULES)?
        .unwrap_or_default()
        .split(',')
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let actor = SearchActor {
        user_id: required(headers, USER)?,
        assignment_id: required(headers, ASSIGNMENT)?,
        company_id: required(headers, COMPANY)?,
        branch_id: branch,
        department_id: department,
        modules,
    };
    actor.validate()?;
    Ok(actor)
}

fn now() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(i64::MAX)
}

fn build_handlers(
    targets: Vec<(
        SearchTarget,
        Arc<dyn RecordSearchSource>,
        Arc<dyn SearchAdmission>,
    )>,
) -> Result<BTreeMap<(String, String), Arc<Handler>>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, source, admission) in targets {
        if !valid_target(&target)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    Arc::new(RecordSearchHandler::new(
                        SharedRecordSource(source),
                        SharedAdmission(admission),
                    )),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(configured)
}

fn router_from(configured: BTreeMap<(String, String), Arc<Handler>>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/records/native-search",
            get(read_search),
        )
        .with_state(Arc::new(SearchHttpState { targets: configured }))
}

/// Reader/admission injection constructor used by tests.
pub fn router_with_sources(
    targets: Vec<(
        SearchTarget,
        Arc<dyn RecordSearchSource>,
        Arc<dyn SearchAdmission>,
    )>,
) -> Result<Router, BusinessError> {
    validate_targets(
        &targets
            .iter()
            .map(|(target, _, _)| target.clone())
            .collect::<Vec<_>>(),
    )?;
    Ok(router_from(build_handlers(targets)?))
}

/// Per-Holding constructor: composition pairs one owner pool per registered
/// Holding and supplies one composition-owned admission port.
pub fn router_for_holdings(
    targets: Vec<(SearchTarget, DatabasePool, Arc<dyn SearchAdmission>)>,
) -> Result<Router, BusinessError> {
    validate_targets(
        &targets
            .iter()
            .map(|(target, _, _)| target.clone())
            .collect::<Vec<_>>(),
    )?;
    let configured = build_handlers(
        targets
            .into_iter()
            .map(|(target, pool, admission)| {
                (
                    target,
                    Arc::new(PgRecordSearch::new(pool)) as Arc<dyn RecordSearchSource>,
                    admission,
                )
            })
            .collect(),
    )?;
    Ok(router_from(configured))
}

async fn ensure_empty_body(request: Request) -> Result<(), BusinessError> {
    let bytes = to_bytes(request.into_body(), 0)
        .await
        .map_err(|_| request_invalid())?;
    if bytes.is_empty() {
        Ok(())
    } else {
        Err(request_invalid())
    }
}

async fn read_search(
    State(state): State<Arc<SearchHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let query = request.uri().query().map(str::to_owned);
    let result = async move {
        let handler = target(&state, &headers, &slug)?;
        let term = query_term(query.as_deref())?;
        ensure_empty_body(request).await?;
        if term.is_empty() {
            return Ok(serde_json::json!({"data": []}));
        }
        let query = SearchQuery::new(&term)?;
        let actor = actor(&headers)?;
        Ok::<_, BusinessError>(handler.execute(&actor, &query, now()).await?.value())
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

    #[test]
    fn target_configuration_requires_exact_distinct_hosts() {
        let good = SearchTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
        };
        assert!(validate_targets(&[good.clone()]).is_ok());
        let bad = SearchTarget {
            host: good.host.clone(),
            holding_slug: "BAD SLUG".into(),
        };
        assert_eq!(
            validate_targets(&[bad]).unwrap_err().code,
            "NATIVE_RECORDS_SEARCH_CONFIG_INVALID"
        );
        assert!(validate_targets(&[good.clone(), good]).is_err());
        assert!(validate_targets(&[]).is_err());
    }

    #[test]
    fn query_accepts_only_one_bounded_q() {
        assert_eq!(query_term(Some("q=needle")).unwrap(), "needle");
        assert_eq!(query_term(Some("q=needle+review")).unwrap(), "needle review");
        assert!(query_term(Some("q=a&q=b")).is_err());
        assert!(query_term(Some("other=a")).is_err());
        assert!(query_term(Some("q")).is_err());
    }
}
