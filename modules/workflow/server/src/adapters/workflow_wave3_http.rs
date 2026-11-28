//! Native axum command ingress for the Workflow wf3a13 command-ledger slice.
//! Typed kind, bounded positional values, one Idempotency-Key and exact
//! registered Holding Host/Origin for browser writes. Company-keyed variant is
//! a private-API route for callers without a browser WorkContext.
use crate::domain::workflow_wave3::{WorkflowWave3Statement, supported};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const KEY_HEADER: header::HeaderName = header::HeaderName::from_static("idempotency-key");
const MAX_VALUES: usize = 64;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowWave3CommandBody {
    pub kind: String,
    pub values: Vec<Value>,
    #[serde(default)]
    pub now: i64,
}

pub type WorkflowWave3Future<'a> =
    Pin<Box<dyn Future<Output = Result<WorkflowWave3Statement, BusinessError>> + Send + 'a>>;

pub struct WorkflowWave3Request<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: &'a str,
    pub values: &'a [Value],
    pub now: i64,
    pub key: &'a str,
}

pub trait WorkflowWave3Backend: Send + Sync {
    fn statement<'a>(&'a self, request: WorkflowWave3Request<'a>) -> WorkflowWave3Future<'a>;
}

/// Composition-owned executor that runs the owner statement in Rust on the
/// caller's serializable unit. Used by native-write descriptors that execute
/// outside a TypeScript-owned batch.
pub type WorkflowWave3ExecuteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<serde_json::Value, BusinessError>> + Send + 'a>>;

pub trait WorkflowWave3ExecuteBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        kind: &'a str,
        values: &'a [serde_json::Value],
        now: i64,
        key: &'a str,
    ) -> WorkflowWave3ExecuteFuture<'a>;
}

#[derive(Clone)]
pub struct WorkflowWave3Target {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn WorkflowWave3Backend>,
}

struct AppState {
    targets: BTreeMap<(String, String), WorkflowWave3Target>,
    holding_targets: BTreeMap<String, Arc<dyn WorkflowWave3Backend>>,
    company_targets: BTreeMap<String, Arc<dyn WorkflowWave3Backend>>,
    execute_targets: BTreeMap<String, Arc<dyn WorkflowWave3ExecuteBackend>>,
    /// A single-target deployment can serve any dynamic Company id: the
    /// backend resolves the real scope from the signed context header, so the
    /// static key is only a routing name. Multiple targets stay fail-closed.
    default_company: Option<Arc<dyn WorkflowWave3Backend>>,
    default_execute: Option<Arc<dyn WorkflowWave3ExecuteBackend>>,
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "WORKFLOW_WAVE3_REQUEST_INVALID",
        "Use one bounded Customer Authority write request.",
    )
}
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn configuration_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "WORKFLOW_WAVE3_CONFIG_INVALID",
        "Use exact distinct registered targets.",
    )
}
fn unique_header(
    headers: &HeaderMap,
    name: header::HeaderName,
) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}
fn valid_origin(value: &str) -> bool {
    value.starts_with("https://")
        && value.len() <= 512
        && !value
            .bytes()
            .any(|byte| matches!(byte, b' ' | b'<' | b'>' | b'\r' | b'\n'))
}
fn decode(bytes: &[u8]) -> Result<WorkflowWave3CommandBody, BusinessError> {
    let body: WorkflowWave3CommandBody = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if !supported(&body.kind) || body.values.is_empty() || body.values.len() > MAX_VALUES {
        return Err(invalid());
    }
    Ok(body)
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
fn success(statement: WorkflowWave3Statement) -> Response {
    let mut response =
        (StatusCode::OK, Json(serde_json::json!({"data": statement}))).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, header::HeaderValue::from_static(value));
    }
    response
}
fn success_value(value: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(serde_json::json!({"data": value}))).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, header::HeaderValue::from_static(value));
    }
    response
}

fn browser_target<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a WorkflowWave3Target, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?
        .split(':')
        .next()
        .unwrap_or_default()
        .to_owned();
    let target = state
        .targets
        .get(&(host, slug.to_owned()))
        .ok_or_else(missing)?;
    match unique_header(headers, header::ORIGIN)? {
        Some(origin) if origin == target.origin => Ok(()),
        _ => Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the registered application origin.",
        )),
    }
    .map(|()| target)
}

async fn execute_write(
    State(state): State<Arc<AppState>>,
    Path(holding_id): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let backend = state
            .execute_targets
            .get(&holding_id)
            .or(state.default_execute.as_ref())
            .ok_or_else(missing)?;
        let key = unique_header(&headers, KEY_HEADER.clone())?
            .filter(|value| !value.is_empty() && value.len() <= 200)
            .ok_or_else(invalid)?;
        let command = decode(
            &to_bytes(request.into_body(), BODY_LIMIT)
                .await
                .map_err(|_| invalid())?,
        )?;
        backend
            .execute(&command.kind, &command.values, command.now, key)
            .await
    }
    .await;
    match result {
        Ok(value) => success_value(value),
        Err(error) => error_response(error),
    }
}

async fn browser_write(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = browser_target(&state, &headers, &slug)?;
        let key = unique_header(&headers, KEY_HEADER.clone())?
            .filter(|value| !value.is_empty() && value.len() <= 200)
            .ok_or_else(invalid)?;
        let command = decode(
            &to_bytes(request.into_body(), BODY_LIMIT)
                .await
                .map_err(|_| invalid())?,
        )?;
        target
            .backend
            .statement(WorkflowWave3Request {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind: &command.kind,
                values: &command.values,
                now: command.now,
                key,
            })
            .await
    }
    .await;
    match result {
        Ok(statement) => success(statement),
        Err(error) => error_response(error),
    }
}

async fn holding_write(
    State(state): State<Arc<AppState>>,
    Path(holding_id): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let backend = state.holding_targets.get(&holding_id).ok_or_else(missing)?;
        let key = unique_header(&headers, KEY_HEADER.clone())?
            .filter(|value| !value.is_empty() && value.len() <= 200)
            .ok_or_else(invalid)?;
        let command = decode(
            &to_bytes(request.into_body(), BODY_LIMIT)
                .await
                .map_err(|_| invalid())?,
        )?;
        backend
            .statement(WorkflowWave3Request {
                host: "",
                holding_slug: "",
                headers: &headers,
                kind: &command.kind,
                values: &command.values,
                now: command.now,
                key,
            })
            .await
    }
    .await;
    match result {
        Ok(statement) => success(statement),
        Err(error) => error_response(error),
    }
}

async fn company_write(
    State(state): State<Arc<AppState>>,
    Path(company_id): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let backend = state
            .company_targets
            .get(&company_id)
            .or(state.default_company.as_ref())
            .ok_or_else(missing)?;
        let key = unique_header(&headers, KEY_HEADER.clone())?
            .filter(|value| !value.is_empty() && value.len() <= 200)
            .ok_or_else(invalid)?;
        let command = decode(
            &to_bytes(request.into_body(), BODY_LIMIT)
                .await
                .map_err(|_| invalid())?,
        )?;
        backend
            .statement(WorkflowWave3Request {
                host: "",
                holding_slug: "",
                headers: &headers,
                kind: &command.kind,
                values: &command.values,
                now: command.now,
                key,
            })
            .await
    }
    .await;
    match result {
        Ok(statement) => success(statement),
        Err(error) => error_response(error),
    }
}

pub fn workflow_wave3_browser_router(
    targets: Vec<WorkflowWave3Target>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        let host = target.host.split(':').next().unwrap_or_default().to_owned();
        if host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&target.origin)
            || configured
                .insert((host, target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/workflow/wave3-command",
            post(browser_write),
        )
        .with_state(Arc::new(AppState {
            targets: configured,
            holding_targets: BTreeMap::new(),
            company_targets: BTreeMap::new(),
            execute_targets: BTreeMap::new(),
            default_company: None,
            default_execute: None,
        })))
}

pub fn workflow_wave3_company_router(
    pairs: Vec<(String, Arc<dyn WorkflowWave3Backend>)>,
) -> Result<Router, BusinessError> {
    let mut company_targets = BTreeMap::new();
    for (company, backend) in pairs {
        if company.is_empty() || company_targets.insert(company, backend).is_some() {
            return Err(configuration_invalid());
        }
    }
    let default_company = if company_targets.len() == 1 {
        company_targets.values().next().cloned()
    } else {
        None
    };
    Ok(Router::new()
        .route(
            "/api/v1/companies/{company_id}/workflow/wave3-command",
            post(company_write),
        )
        .with_state(Arc::new(AppState {
            targets: BTreeMap::new(),
            holding_targets: BTreeMap::new(),
            company_targets,
            execute_targets: BTreeMap::new(),
            default_company,
            default_execute: None,
        })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_rejects_invalid_targets() {
        let backend: Arc<dyn WorkflowWave3Backend> = Arc::new(FakeBackend);
        let target = WorkflowWave3Target {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        };
        assert!(workflow_wave3_browser_router(vec![target.clone()]).is_ok());
        let mut bad = target.clone();
        bad.origin = "http://acme.example.test".into();
        assert!(workflow_wave3_browser_router(vec![bad]).is_err());
    }

    struct FakeBackend;
    impl WorkflowWave3Backend for FakeBackend {
        fn statement<'a>(&'a self, _request: WorkflowWave3Request<'a>) -> WorkflowWave3Future<'a> {
            Box::pin(async {
                Ok(WorkflowWave3Statement {
                    sql: "UPDATE customer_access_bindings SET version=version+1".to_owned(),
                    values: vec![],
                })
            })
        }
    }
}

/// Holding-keyed private-API system router for workers without a Company id.
pub fn workflow_wave3_holding_router(
    pairs: Vec<(String, Arc<dyn WorkflowWave3Backend>)>,
) -> Result<Router, BusinessError> {
    let mut holding_targets = BTreeMap::new();
    for (holding, backend) in pairs {
        if holding.is_empty() || holding_targets.insert(holding, backend).is_some() {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_id}/workflow/wave3-command-by-holding",
            post(holding_write),
        )
        .with_state(Arc::new(AppState {
            targets: BTreeMap::new(),
            holding_targets,
            company_targets: BTreeMap::new(),
            execute_targets: BTreeMap::new(),
            default_company: None,
            default_execute: None,
        })))
}

/// Holding-keyed private-API router that EXECUTES owner writes in Rust.
pub fn workflow_wave3_execute_router(
    pairs: Vec<(String, Arc<dyn WorkflowWave3ExecuteBackend>)>,
) -> Result<Router, BusinessError> {
    let mut execute_targets = BTreeMap::new();
    for (holding, backend) in pairs {
        if holding.is_empty() || execute_targets.insert(holding, backend).is_some() {
            return Err(configuration_invalid());
        }
    }
    let default_execute = if execute_targets.len() == 1 {
        execute_targets.values().next().cloned()
    } else {
        None
    };
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_id}/workflow/wave3-execute",
            post(execute_write),
        )
        .with_state(Arc::new(AppState {
            targets: BTreeMap::new(),
            holding_targets: BTreeMap::new(),
            company_targets: BTreeMap::new(),
            execute_targets,
            default_company: None,
            default_execute,
        })))
}
