//! ONE owner-local axum command router for native Files writes.
//!
//! Transport facts only: typed kind allow-list, exact registered Host/Holding,
//! exact browser Origin, one Idempotency-Key, bounded JSON and error mapping.
//! The backend executes the write in a caller-owned serializable unit.
use crate::application::files_wave1b::{CommandResult, FilesCommandInput};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

pub type FilesCommandFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CommandResult, BusinessError>> + Send + 'a>>;
pub type FilesCommandPlanFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<crate::adapters::files_wave1b_postgres::PlannedStatement>, BusinessError>> + Send + 'a>>;

pub struct FilesCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub input: &'a FilesCommandInput,
    pub key: &'a CommandKey,
}

pub trait FilesCommandBackend: Send + Sync {
    fn execute<'a>(&'a self, request: FilesCommandRequest<'a>) -> FilesCommandFuture<'a>;
    /// Caller-unit plan: owner SQL for the command plus its ledger, executed by
    /// the caller inside its open transaction.
    fn plan<'a>(&'a self, _request: FilesCommandRequest<'a>) -> FilesCommandPlanFuture<'a> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "FILES_COMMAND_PLAN_UNAVAILABLE",
                "The native Files command plan is unavailable.",
            ))
        })
    }
}

#[derive(Clone)]
pub struct FilesCommandTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn FilesCommandBackend>,
}

struct AppState {
    targets: BTreeMap<(String, String), FilesCommandTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid("FILES_COMMAND_INVALID", "Use one bounded Files command.")
}
fn forbidden() -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, "ORIGIN_FORBIDDEN", "Use the registered application origin.")
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid("FILES_COMMAND_CONFIGURATION_INVALID", "Use exact distinct registered Holding targets.")
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first.map(|value| value.to_str().map_err(|_| malformed())).transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty() && !rest.bytes().any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    })
}
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty() && slug.len() <= 128 && slug.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
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
    (status(error.kind), Json(serde_json::json!({"error":{"code":error.code,"message":error.message}}))).into_response()
}
fn bounded(body: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response.headers_mut().insert(name, HeaderValue::from_static(value));
    }
    response
}
fn target<'a>(state: &'a AppState, headers: &HeaderMap, slug: &str) -> Result<&'a FilesCommandTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?.filter(|value| !value.is_empty()).ok_or_else(missing)?;
    state.targets.get(&(host.to_owned(), slug.to_owned())).ok_or_else(missing)
}
fn mutation_key(headers: &HeaderMap) -> Result<CommandKey, BusinessError> {
    unique_header(headers, KEY_HEADER)?
        .ok_or_else(|| BusinessError::invalid("COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key for this action."))
        .and_then(CommandKey::new)
}

/// Build the exact registered-Holding router. Every command proves Host,
/// Holding, browser Origin and one Idempotency-Key before the backend runs.
pub fn files_command_router(targets: Vec<FilesCommandTarget>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty() || !valid_slug(&target.holding_slug) || !valid_origin(&target.origin)
            || configured.insert((target.host.clone(), target.holding_slug.clone()), target).is_some() {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route("/api/v1/holdings/{holding_slug}/files/commands", post(command))
        .route("/api/v1/holdings/{holding_slug}/files/command-plan", post(plan_command))
        .with_state(Arc::new(AppState { targets: configured })))
}

async fn command(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let headers = request.headers().clone();
        let origin = unique_header(&headers, header::ORIGIN)?.ok_or_else(forbidden)?;
        if origin != target.origin {
            return Err(forbidden());
        }
        let key = mutation_key(&headers)?;
        if request.uri().query().is_some() {
            return Err(malformed());
        }
        let bytes = to_bytes(request.into_body(), BODY_LIMIT).await.map_err(|_| malformed())?;
        let input: FilesCommandInput = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
        target.backend.execute(FilesCommandRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: &headers,
            input: &input,
            key: &key,
        }).await
    }
    .await;
    match result {
        Ok(result) => bounded(serde_json::json!({"data": result})),
        Err(error) => error_response(error),
    }
}

async fn plan_command(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let headers = request.headers().clone();
        let origin = unique_header(&headers, header::ORIGIN)?.ok_or_else(forbidden)?;
        if origin != target.origin {
            return Err(forbidden());
        }
        let key = mutation_key(&headers)?;
        if request.uri().query().is_some() {
            return Err(malformed());
        }
        let bytes = to_bytes(request.into_body(), BODY_LIMIT).await.map_err(|_| malformed())?;
        let input: FilesCommandInput = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
        target
            .backend
            .plan(FilesCommandRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                input: &input,
                key: &key,
            })
            .await
    }
    .await;
    match result {
        Ok(statements) => bounded(serde_json::json!({"data": {"statements": statements}})),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Noop;
    impl FilesCommandBackend for Noop {
        fn execute<'a>(&'a self, _: FilesCommandRequest<'a>) -> FilesCommandFuture<'a> {
            Box::pin(async { Ok(CommandResult::default()) })
        }
    }

    #[test]
    fn configuration_requires_distinct_exact_holdings() {
        let backend = Arc::new(Noop);
        let target = || FilesCommandTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(files_command_router(vec![target()]).is_ok());
        assert!(files_command_router(vec![target(), target()]).is_err());
        assert!(files_command_router(vec![FilesCommandTarget { origin: "http://acme.example.test".into(), ..target() }]).is_err());
    }
}
