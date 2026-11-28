//! Native lane 40 Brand owner-local axum ingress.
//!
//! Transport facts only: an exact registered Host/Holding target, a bounded
//! guard id and the Company scope header. The owner reader executes the SQL,
//! applies the Company scope and projects the response on the private Holding
//! pool. No SQL, bind array or caller relation name is accepted from the wire;
//! every failure maps to a named error and the TypeScript client fails closed.

use crate::adapters::native_lane_40::application::{
    read_active_branch, read_company_exists, BranchGuardQuery, CompanyGuardQuery,
    ManagementGuardRead,
};
use crate::adapters::native_lane_40::postgres::PgManagementGuardRead;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Arc;

const BODY_LIMIT: usize = 8 * 1024;
const COMPANY_HEADER: &str = "x-erpx-context-company";

/// The exact private target mounted by composition. The Holding owns the host;
/// the slug selects the branded workspace inside it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagementGuardTarget {
    pub host: String,
    pub holding_slug: String,
}

struct AppState {
    targets: BTreeMap<(String, String), Arc<dyn ManagementGuardRead>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GuardBody {
    id: String,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "BRAND_MANAGEMENT_REQUEST_INVALID",
        "Use one bounded Brand management read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "BRAND_MANAGEMENT_CONFIG_INVALID",
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

/// Build the owner guard router with one shared pool. Composition mounts it
/// alongside the other Brand owner routers; no composition or shared module is
/// edited here.
pub fn router(
    pool: DatabasePool,
    targets: Vec<ManagementGuardTarget>,
) -> Result<Router, BusinessError> {
    router_for_holdings(targets.into_iter().map(|target| (target, pool.clone())).collect())
}

/// Build the owner guard router with one dedicated pool per Holding. Targets
/// are keyed by `(host, holding_slug)` so a request can never reach a different
/// Holding's reader; duplicate or malformed targets fail closed.
pub fn router_for_holdings(
    targets: Vec<(ManagementGuardTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    Arc::new(PgManagementGuardRead::new(pool)) as Arc<dyn ManagementGuardRead>,
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/branding/lane40-management-branch",
            post(branch),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/branding/lane40-management-company",
            post(company),
        )
        .with_state(Arc::new(AppState { targets: configured })))
}

async fn branch(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let company_id = unique_header(
            request.headers(),
            HeaderName::from_static(COMPANY_HEADER),
        )?
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid)?
        .to_owned();
        let reader = state
            .targets
            .get(&(host, slug.clone()))
            .ok_or_else(missing)?;
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let body: GuardBody = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let query = BranchGuardQuery {
            id: body.id,
            company_id,
        };
        read_active_branch(reader.as_ref(), query).await
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn company(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let reader = state
            .targets
            .get(&(host, slug.clone()))
            .ok_or_else(missing)?;
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let body: GuardBody = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let query = CompanyGuardQuery { id: body.id };
        read_company_exists(reader.as_ref(), query).await
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
    fn route_helpers_validate_slug_and_status() {
        assert!(valid_slug("acme"));
        assert!(valid_slug("erpx-internal_1"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug("bad/slug"));
        assert!(!valid_slug(""));
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
    }
}
