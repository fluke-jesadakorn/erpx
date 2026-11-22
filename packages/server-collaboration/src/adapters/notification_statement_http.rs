//! Native notification statement HTTP ingress for caller-owned transactions.
use crate::adapters::notification_statement::{
    NotificationStatement, NotificationStatementInput, notification_statement,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 32 * 1024;
pub type StatementFuture<'a> =
    Pin<Box<dyn Future<Output = Result<NotificationStatement, BusinessError>> + Send + 'a>>;
pub struct StatementRequest<'a> {
    pub host: &'a str,
    pub headers: &'a HeaderMap,
    pub input: NotificationStatementInput,
}
pub trait StatementBackend: Send + Sync {
    fn statement<'a>(&'a self, request: StatementRequest<'a>) -> StatementFuture<'a>;
}
#[derive(Clone)]
pub struct StatementTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn StatementBackend>,
}
struct AppState {
    targets: BTreeMap<(String, String), StatementTarget>,
    company_targets: BTreeMap<String, StatementTarget>,
    holding_targets: BTreeMap<String, StatementTarget>,
    /// A single-target system deployment can serve any dynamic Company id; the
    /// statement backend reads the real company from the request body. Multiple
    /// targets stay fail-closed.
    default_company: Option<StatementTarget>,
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "NOTIFICATION_STATEMENT_INVALID",
        "Use one notification statement request.",
    )
}
fn target<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a StatementTarget, BusinessError> {
    let mut hosts = headers.get_all(header::HOST).iter();
    let host = hosts
        .next()
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .to_owned();
    if hosts.next().is_some() {
        return Err(malformed());
    }
    let t = state
        .targets
        .get(&(host, slug.to_owned()))
        .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
    let mut origins = headers.get_all(header::ORIGIN).iter();
    let first = origins.next().and_then(|v| v.to_str().ok());
    if origins.next().is_some() || first != Some(t.origin.as_str()) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        ));
    }
    Ok(t)
}
fn error(error: BusinessError) -> Response {
    let status = match error.kind {
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::InvalidInput => StatusCode::BAD_REQUEST,
        FailureKind::Conflict => StatusCode::CONFLICT,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    (
        status,
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}
async fn statement(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let t = target(&state, request.headers(), &slug)?;
        let headers = request.headers().clone();
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| malformed())?;
        let input: NotificationStatementInput =
            serde_json::from_slice(&bytes).map_err(|_| malformed())?;
        t.backend
            .statement(StatementRequest {
                host: &t.host,
                headers: &headers,
                input,
            })
            .await
    }
    .await;
    match result {
        Ok(s) => (StatusCode::OK, Json(serde_json::json!({"data":s}))).into_response(),
        Err(e) => error(e),
    }
}
async fn system_statement(
    State(state): State<Arc<AppState>>,
    Path(company_id): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let t = state
            .company_targets
            .get(&company_id)
            .or(state.default_company.as_ref())
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
        let headers = request.headers().clone();
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| malformed())?;
        let input: NotificationStatementInput =
            serde_json::from_slice(&bytes).map_err(|_| malformed())?;
        t.backend
            .statement(StatementRequest {
                host: &t.host,
                headers: &headers,
                input,
            })
            .await
    }
    .await;
    match result {
        Ok(s) => (StatusCode::OK, Json(serde_json::json!({"data":s}))).into_response(),
        Err(e) => error(e),
    }
}

/// Company-keyed system route. No browser Origin/Host checks: this is reachable
/// only on the private native API and is used by system workers that hold a
/// Company id but no browser WorkContext.
pub fn system_notification_statement_router(
    pairs: Vec<(String, StatementTarget)>,
) -> Result<Router, BusinessError> {
    let mut company_targets = BTreeMap::new();
    for (company, target) in pairs {
        if company.is_empty() || company_targets.insert(company, target).is_some() {
            return Err(BusinessError::invalid(
                "NOTIFICATION_STATEMENT_CONFIG_INVALID",
                "Use exact distinct companies.",
            ));
        }
    }
    let default_company = if company_targets.len() == 1 {
        company_targets.values().next().cloned()
    } else {
        None
    };
    Ok(Router::new()
        .route(
            "/api/v1/companies/{company_id}/collaboration/notification-statement",
            post(system_statement),
        )
        .with_state(Arc::new(AppState {
            targets: BTreeMap::new(),
            company_targets,
            holding_targets: BTreeMap::new(),
            default_company,
        })))
}

async fn holding_statement(
    State(state): State<Arc<AppState>>,
    Path(holding_id): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let t = state
            .holding_targets
            .get(&holding_id)
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
        let headers = request.headers().clone();
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| malformed())?;
        let input: NotificationStatementInput =
            serde_json::from_slice(&bytes).map_err(|_| malformed())?;
        t.backend
            .statement(StatementRequest {
                host: &t.host,
                headers: &headers,
                input,
            })
            .await
    }
    .await;
    match result {
        Ok(s) => (StatusCode::OK, Json(serde_json::json!({"data":s}))).into_response(),
        Err(e) => error(e),
    }
}

pub fn holding_notification_statement_router(
    pairs: Vec<(String, StatementTarget)>,
) -> Result<Router, BusinessError> {
    let mut holding_targets = BTreeMap::new();
    for (holding, target) in pairs {
        if holding.is_empty() || holding_targets.insert(holding, target).is_some() {
            return Err(BusinessError::invalid(
                "NOTIFICATION_STATEMENT_CONFIG_INVALID",
                "Use exact distinct holdings.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_id}/collaboration/notification-statement-by-holding",
            post(holding_statement),
        )
        .with_state(Arc::new(AppState {
            targets: BTreeMap::new(),
            company_targets: BTreeMap::new(),
            holding_targets,
            default_company: None,
        })))
}

pub fn notification_statement_router(
    targets: Vec<StatementTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for t in targets {
        if t.host.is_empty()
            || t.holding_slug.is_empty()
            || t.origin.is_empty()
            || configured
                .insert((t.host.clone(), t.holding_slug.clone()), t)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "NOTIFICATION_STATEMENT_CONFIG_INVALID",
                "Use exact distinct targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/collaboration/notification-statement",
            post(statement),
        )
        .with_state(Arc::new(AppState {
            targets: configured,
            company_targets: BTreeMap::new(),
            holding_targets: BTreeMap::new(),
            default_company: None,
        })))
}
// keep imports used
#[allow(dead_code)]
fn _company_marker(_: CompanyId, _: HeaderName) {}
