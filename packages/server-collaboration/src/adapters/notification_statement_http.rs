//! Native notification statement HTTP ingress for caller-owned transactions.
use crate::adapters::notification_statement::{
    notification_statement, NotificationStatement, NotificationStatementInput,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes,
    header,
    routing::post,
    serde_json,
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
pub struct StatementTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn StatementBackend>,
}
struct AppState { targets: BTreeMap<(String, String), StatementTarget> }
fn malformed() -> BusinessError {
    BusinessError::invalid("NOTIFICATION_STATEMENT_INVALID", "Use one notification statement request.")
}
fn target<'a>(state: &'a AppState, headers: &HeaderMap, slug: &str) -> Result<&'a StatementTarget, BusinessError> {
    let mut hosts = headers.get_all(header::HOST).iter();
    let host = hosts.next().and_then(|v| v.to_str().ok()).unwrap_or_default().split(':').next().unwrap_or_default().to_owned();
    if hosts.next().is_some() { return Err(malformed()); }
    let t = state.targets.get(&(host, slug.to_owned()))
        .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
    let mut origins = headers.get_all(header::ORIGIN).iter();
    let first = origins.next().and_then(|v| v.to_str().ok());
    if origins.next().is_some() || first != Some(t.origin.as_str()) {
        return Err(BusinessError::new(FailureKind::Forbidden, "ORIGIN_FORBIDDEN", "Use the application origin."));
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
    (status, Json(serde_json::json!({"error":{"code":error.code,"message":error.message}}))).into_response()
}
async fn statement(State(state): State<Arc<AppState>>, Path(slug): Path<String>, request: Request) -> Response {
    let result = async move {
        let t = target(&state, request.headers(), &slug)?;
        let headers = request.headers().clone();
        let bytes = to_bytes(request.into_body(), BODY_LIMIT).await.map_err(|_| malformed())?;
        let input: NotificationStatementInput = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
        t.backend.statement(StatementRequest { host: &t.host, headers: &headers, input }).await
    }.await;
    match result {
        Ok(s) => (StatusCode::OK, Json(serde_json::json!({"data":s}))).into_response(),
        Err(e) => error(e),
    }
}
pub fn notification_statement_router(targets: Vec<StatementTarget>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for t in targets {
        if t.host.is_empty() || t.holding_slug.is_empty() || t.origin.is_empty()
            || configured.insert((t.host.clone(), t.holding_slug.clone()), t).is_some() {
            return Err(BusinessError::invalid("NOTIFICATION_STATEMENT_CONFIG_INVALID", "Use exact distinct targets."));
        }
    }
    Ok(Router::new()
        .route("/api/v1/holdings/{holding_slug}/collaboration/notification-statement", post(statement))
        .with_state(Arc::new(AppState { targets: configured })))
}
// keep imports used
#[allow(dead_code)]
fn _company_marker(_: CompanyId, _: HeaderName) {}
