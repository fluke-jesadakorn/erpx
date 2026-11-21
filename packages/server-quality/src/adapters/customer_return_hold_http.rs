//! Native-owned customer-return Quality hold statement HTTP ingress.
//!
//! Returns owner SQL statement descriptors for a caller-owned serializable
//! transaction. The caller supplies its own unit; this endpoint only reads the
//! current release specification and hands back the authoritative statements.
use crate::adapters::customer_return_hold_statement::{
    CustomerReturnHoldInput, CustomerReturnHoldStatements,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes,
    header,
    routing::post,
    serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type HoldStatementFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CustomerReturnHoldStatements, BusinessError>> + Send + 'a>>;

pub struct HoldStatementRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub input: CustomerReturnHoldInput,
}

pub trait HoldStatementBackend: Send + Sync {
    fn statements<'a>(&'a self, request: HoldStatementRequest<'a>) -> HoldStatementFuture<'a>;
}

pub struct HoldStatementTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn HoldStatementBackend>,
}

struct AppState {
    targets: BTreeMap<(String, String), HoldStatementTarget>,
}

fn malformed() -> BusinessError {
    BusinessError::invalid("QUALITY_HOLD_STATEMENT_INVALID", "Use one Quality hold statement request.")
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first
        .map(|value| value.to_str().map_err(|_| malformed()))
        .transpose()
}

fn target<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a HoldStatementTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .ok_or_else(malformed)?
        .split(':')
        .next()
        .unwrap_or_default()
        .to_owned();
    let target = state
        .targets
        .get(&(host, slug.to_owned()))
        .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
    if unique_header(headers, header::ORIGIN)? != Some(target.origin.as_str()) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        ));
    }
    Ok(target)
}

fn error_response(error: BusinessError) -> Response {
    let status = match error.kind {
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::InvalidInput => StatusCode::BAD_REQUEST,
        FailureKind::Conflict => StatusCode::CONFLICT,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    (status, Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})))
        .into_response()
}

async fn hold_statement(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let headers = request.headers().clone();
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| malformed())?;
        let input: CustomerReturnHoldInput =
            serde_json::from_slice(&bytes).map_err(|_| malformed())?;
        target
            .backend
            .statements(HoldStatementRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                input,
            })
            .await
    }
    .await;
    match result {
        Ok(statements) => (
            StatusCode::OK,
            Json(serde_json::json!({"data": statements})),
        )
            .into_response(),
        Err(error) => error_response(error),
    }
}

pub fn hold_statement_router(
    targets: Vec<HoldStatementTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || target.origin.is_empty()
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "QUALITY_HOLD_STATEMENT_CONFIG_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/quality/customer-return-hold-statement",
            post(hold_statement),
        )
        .with_state(Arc::new(AppState { targets: configured })))
}
