//! Owner-local axum router for the lane 27 Projects actuals read. It follows
//! the Commerce quality-product target/header isolation contract: one exact
//! registered Holding host + slug, one Company identity, no caller SQL.

use super::application::{ProjectActualsRead, read_cost_allocations, read_work_logs};
use super::domain::{LaneTarget, invalid, scope, valid_slug};
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

fn targets_map(targets: Vec<LaneTarget>, reader: Arc<dyn ProjectActualsRead>) -> Result<BTreeMap<(String, String), Arc<dyn ProjectActualsRead>>, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader.clone())
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

/// Build the native read router. The lead mounts it; no composition or shared
/// module is edited here.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgProjectActualsRead::new(pool)), targets)
}

/// Same contract with an injected owner reader; used by module tests that need
/// no live database.
pub fn router_with_reader(
    reader: Arc<dyn ProjectActualsRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/projects/actuals/work-logs",
            get(work_logs),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/projects/actuals/cost-allocations",
            get(cost_allocations),
        )
        .with_state(Arc::new(LaneState {
            targets: targets_map(targets, reader)?,
        })))
}

fn request_scope(
    state: &LaneState,
    request: &Request,
    slug: &str,
) -> Result<(Arc<dyn ProjectActualsRead>, super::domain::ActualsScope), BusinessError> {
    let host = unique_header(request.headers(), header::HOST)?
        .filter(|host| !host.is_empty())
        .ok_or_else(missing)?;
    let reader = state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?
        .clone();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_27::application::ActualsFuture;
    use crate::adapters::native_lane_27::domain::{ActualsScope, CostAllocationRow, WorkLogRow};

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
}
