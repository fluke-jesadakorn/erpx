//! Native AI provider settings HTTP ingress.
//!
//! Transport facts only: one registered Host/Holding target, the retained path
//! table (`/`, `/{id}`, `/selection`, `/discover`, `/{id}/test`,
//! `/{id}/activate`, `/{id}/disable`, `/{id}/delete`, `/{id}/models`), the
//! origin and idempotency contract. Current authority, credential sealing,
//! provider transport, receipts, audit and outbox remain behind the backend
//! trait. Credential/provider adapters fail closed until A1.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const KEY_LIMIT: usize = 180;

pub type AiProviderHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct AiProviderHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub path: Vec<String>,
}

pub struct AiProviderHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub path: Vec<String>,
    pub key: Option<&'a str>,
    pub body: serde_json::Value,
}

pub trait AiProviderHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: AiProviderHttpReadRequest<'a>,
    ) -> AiProviderHttpFuture<'a, serde_json::Value>;
    fn command<'a>(
        &'a self,
        request: AiProviderHttpCommandRequest<'a>,
    ) -> AiProviderHttpFuture<'a, serde_json::Value>;
}

pub struct AiProviderHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn AiProviderHttpBackend>,
}

struct AiProviderHttpState {
    targets: BTreeMap<(String, String), AiProviderHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid("AI_REQUEST_INVALID", "Use one bounded AI provider request.")
}
fn key_required() -> BusinessError {
    BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
}
fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "AI_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}
fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
            })
    })
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
fn bounded(body: serde_json::Value, created: bool) -> Response {
    let mut response = (
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(body),
    )
        .into_response();
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
fn respond(result: Result<serde_json::Value, BusinessError>, created: bool) -> Response {
    match result {
        Ok(data) => bounded(serde_json::json!({"data": data}), created),
        Err(error) => error_response(error),
    }
}
fn target<'a>(
    state: &'a AiProviderHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a AiProviderHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn require_origin(
    target: &AiProviderHttpTarget,
    headers: &HeaderMap,
    mutation: bool,
) -> Result<(), BusinessError> {
    let origin = unique_header(headers, header::ORIGIN)?;
    if mutation && origin != Some(target.origin.as_str())
        || origin.is_some_and(|value| value != target.origin)
    {
        return Err(origin_forbidden());
    }
    Ok(())
}
fn path_segments(path: Option<String>) -> Result<Vec<String>, BusinessError> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if segments.len() > 2
        || segments
            .iter()
            .any(|segment| segment.len() > 180 || segment.chars().any(char::is_control))
    {
        return Err(missing());
    }
    Ok(segments)
}

async fn read(
    State(state): State<Arc<AiProviderHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    read_at(state, slug, Vec::new(), request).await
}

async fn read_path(
    State(state): State<Arc<AiProviderHttpState>>,
    Path((slug, path)): Path<(String, String)>,
    request: Request,
) -> Response {
    let segments = match path_segments(Some(path)) {
        Ok(segments) => segments,
        Err(error) => return error_response(error),
    };
    read_at(state, slug, segments, request).await
}

async fn read_at(
    state: Arc<AiProviderHttpState>,
    slug: String,
    path: Vec<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result: Result<serde_json::Value, BusinessError> = async move {
        let target = target(&state, &headers, &slug)?;
        require_origin(target, &headers, false)?;
        target
            .backend
            .read(AiProviderHttpReadRequest {
                host: unique_header(&headers, header::HOST)?.ok_or_else(missing)?,
                holding_slug: &slug,
                headers: &headers,
                path,
            })
            .await
    }
    .await;
    respond(result, false)
}

async fn command(
    State(state): State<Arc<AiProviderHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    command_at(state, slug, Vec::new(), request).await
}

async fn command_path(
    State(state): State<Arc<AiProviderHttpState>>,
    Path((slug, path)): Path<(String, String)>,
    request: Request,
) -> Response {
    let segments = match path_segments(Some(path)) {
        Ok(segments) => segments,
        Err(error) => return error_response(error),
    };
    command_at(state, slug, segments, request).await
}

async fn command_at(
    state: Arc<AiProviderHttpState>,
    slug: String,
    path: Vec<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(malformed()),
    };
    let created = path.is_empty();
    let result: Result<serde_json::Value, BusinessError> = async move {
        let target = target(&state, &headers, &slug)?;
        require_origin(target, &headers, true)?;
        let discover = path.len() == 1 && path[0] == "discover";
        let key = unique_header(&headers, HeaderName::from_static("idempotency-key"))?;
        if !discover {
            let key = key.ok_or_else(key_required)?;
            if key.len() < 8 || key.len() > KEY_LIMIT || key.chars().any(char::is_control) {
                return Err(key_required());
            }
        }
        if unique_header(&headers, header::CONTENT_TYPE)?
            .is_none_or(|value| !value.starts_with("application/json"))
        {
            return Err(malformed());
        }
        let value: serde_json::Value = serde_json::from_slice(&body).map_err(|_| malformed())?;
        if !value.is_object() {
            return Err(malformed());
        }
        target
            .backend
            .command(AiProviderHttpCommandRequest {
                host: unique_header(&headers, header::HOST)?.ok_or_else(missing)?,
                holding_slug: &slug,
                headers: &headers,
                path,
                key,
                body: value,
            })
            .await
    }
    .await;
    respond(result, created)
}

pub fn ai_provider_http_router(
    targets: Vec<AiProviderHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut map = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || map
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/ai-providers",
            get(read).post(command),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/ai-providers/{*path}",
            get(read_path).post(command_path),
        )
        .with_state(Arc::new(AiProviderHttpState { targets: map })))
}
