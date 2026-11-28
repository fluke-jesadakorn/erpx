//! Lane 39 Ecommerce owner-local axum ingress for the workspace list family.
//!
//! Transport facts only: exact Host/Holding targets with one dedicated pool
//! each, an authorized Company scope and optional Branch/Department selectors.
//! The owner reader executes both SELECTs, applies the scope and projects the
//! rows. No SQL or relation name is accepted from the wire; every failure maps
//! to a named error and the TypeScript client fails closed.

use crate::adapters::native_lane_39::application::{WorkspaceListRead, read_workspace_lists};
use crate::adapters::native_lane_39::domain::WorkspaceScope;
use crate::adapters::native_lane_39::postgres::PgWorkspaceListRead;
use crate::adapters::native_lane_39::workspace_rows::{PgWorkspaceRowRead, WorkspaceRowRead, read_workspace_rows};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const WORKSPACE_BRANCH: HeaderName = HeaderName::from_static("x-erpx-workspace-branch");
const WORKSPACE_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-workspace-department");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceListTarget {
    pub host: String,
    pub holding_slug: String,
}

/// Integration name for the same bounded target (see lane 39 Brand module).
pub type LaneTarget = WorkspaceListTarget;

struct AppState {
    targets: BTreeMap<(String, String), Arc<dyn WorkspaceListRead>>,
    rows: BTreeMap<(String, String), Arc<dyn WorkspaceRowRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ECOMMERCE_WORKSPACE_LIST_REQUEST_INVALID",
        "Use one bounded Ecommerce workspace list read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "ECOMMERCE_WORKSPACE_LIST_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
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

/// Build the router with one shared pool.
pub fn router(
    pool: DatabasePool,
    targets: Vec<WorkspaceListTarget>,
) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

/// Build the router with one dedicated reader/pool per Holding, keyed by
/// `(host, holding_slug)`. Duplicate or malformed targets fail closed.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    let mut rows = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty() || !valid_slug(&target.holding_slug) {
            return Err(configuration_invalid());
        }
        let key = (target.host.clone(), target.holding_slug.clone());
        if configured
            .insert(
                key.clone(),
                Arc::new(PgWorkspaceListRead::new(pool.clone())) as Arc<dyn WorkspaceListRead>,
            )
            .is_some()
        {
            return Err(configuration_invalid());
        }
        rows.insert(
            key,
            Arc::new(PgWorkspaceRowRead::new(pool)) as Arc<dyn WorkspaceRowRead>,
        );
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/ecommerce/lane39-workspace-lists",
            post(read),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/ecommerce/lane39-workspace-rows",
            post(read_rows),
        )
        .with_state(Arc::new(AppState { targets: configured, rows })))
}

async fn read(State(state): State<Arc<AppState>>, Path(slug): Path<String>, request: Request) -> Response {
    // Own the transport facts before awaiting: a borrow of `Request<Body>`
    // across an await would make the handler future !Send.
    let headers = request.headers().clone();
    let result = async move {
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?
            .to_owned();
        let branch = unique_header(&headers, WORKSPACE_BRANCH)?
            .filter(|value| valid_id(value))
            .map(str::to_owned);
        let department = unique_header(&headers, WORKSPACE_DEPARTMENT)?
            .filter(|value| valid_id(value))
            .map(str::to_owned);
        let value = read_workspace_lists(reader.as_ref(), company, branch, department).await?;
        Ok::<_, BusinessError>(value)
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn read_rows(State(state): State<Arc<AppState>>, request: Request) -> Response {
    // Own the transport facts before awaiting: a borrow of `Request<Body>`
    // across an await would make the handler future !Send.
    let (parts, _) = request.into_parts();
    let headers = parts.headers.clone();
    let slug = parts
        .uri
        .path()
        .split('/')
        .nth(4)
        .filter(|value| valid_slug(value))
        .map(str::to_owned)
        .ok_or_else(invalid);
    let result = async move {
        let slug = slug?;
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .rows
            .get(&(host.to_owned(), slug))
            .ok_or_else(missing)?;
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let branch = unique_header(&headers, WORKSPACE_BRANCH)?
            .filter(|value| valid_id(value));
        let department = unique_header(&headers, WORKSPACE_DEPARTMENT)?
            .filter(|value| valid_id(value));
        let scope = WorkspaceScope {
            company_id: company,
            branch_id: branch,
            department_id: department,
        };
        Ok::<_, BusinessError>(read_workspace_rows(reader.as_ref(), scope).await?)
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
    fn validation_helpers_reject_bad_targets_and_identities() {
        assert!(valid_slug("acme"));
        assert!(!valid_slug("Acme"));
        assert!(valid_id("co-1"));
        assert!(!valid_id(""));
        assert!(!valid_id("bad id"));
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
    }
}
