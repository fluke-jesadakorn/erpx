//! Native Position GPO mapping/delivery HTTP ingress.
//!
//! Transport facts only: registered Host/Holding targets, exact browser origin,
//! bounded JSON commands, strict Idempotency-Key and business-error mapping.
//! Account session, current Work context, IT authority, fresh MFA, the
//! serializable owner unit and the Directory connector stay behind
//! [`PositionGpoHttpBackend`]; the router never mints authority.
use crate::domain::position_gpo::{
    ConfigRetryInput, JobRetryInput, PositionGpoInput, PublishInput, UnmapInput, UnmapPublishInput,
    request_invalid,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, routing::post, to_bytes,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type PositionGpoHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// One reviewed owner command. Read commands carry no Idempotency-Key.
#[derive(Clone, Debug)]
pub enum PositionGpoHttpCommand {
    Overview,
    Preview {
        input: PositionGpoInput,
    },
    Publish {
        input: PublishInput,
        idempotency_key: String,
    },
    UnmapPreview {
        input: UnmapInput,
    },
    Unmap {
        input: UnmapPublishInput,
        idempotency_key: String,
    },
    Retry {
        input: JobRetryInput,
        idempotency_key: String,
    },
    RetryConfig {
        input: ConfigRetryInput,
        idempotency_key: String,
    },
}

pub struct PositionGpoHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: PositionGpoHttpCommand,
}

#[derive(Clone, Debug)]
pub struct PositionGpoHttpResponse {
    /// Owner data. The backend adds the validated current context.
    pub data: Value,
}

/// Composition-owned Account session, Work scope, IT authority and owner unit.
pub trait PositionGpoHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: PositionGpoHttpRequest<'a>,
    ) -> PositionGpoHttpFuture<'a, PositionGpoHttpResponse>;
}

pub struct PositionGpoHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn PositionGpoHttpBackend>,
}

struct PositionGpoHttpState {
    targets: BTreeMap<(String, String), PositionGpoHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    request_invalid()
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "POSITION_GPO_HTTP_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
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

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
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
    state: &'a PositionGpoHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PositionGpoHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn ensure_no_query(request: &Request) -> Result<(), BusinessError> {
    match request.uri().query() {
        None | Some("") => Ok(()),
        Some(_) => Err(malformed()),
    }
}

async fn strict_body(request: Request) -> Result<Value, BusinessError> {
    let content_type =
        unique_header(request.headers(), header::CONTENT_TYPE)?.ok_or_else(malformed)?;
    if content_type.split(';').next().map(str::trim) != Some("application/json") {
        return Err(malformed());
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| malformed())?;
    if bytes.is_empty() {
        return Err(malformed());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
    if !value.is_object() {
        return Err(malformed());
    }
    Ok(value)
}

fn strict_key(headers: &HeaderMap) -> Result<String, BusinessError> {
    let value =
        unique_header(headers, HeaderName::from_static("idempotency-key"))?.ok_or_else(|| {
            BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "Idempotency-Key is required for this Position GPO command.",
            )
        })?;
    CommandKey::new(value.to_owned())?;
    Ok(value.to_owned())
}

/// The browser route allows a missing Origin (same-origin navigation) but never
/// a foreign one, matching the previous Node route.
fn origin_allowed(request: &Request, target: &PositionGpoHttpTarget) -> bool {
    match unique_header(request.headers(), header::ORIGIN) {
        Ok(None) => true,
        Ok(Some(origin)) => origin == target.origin,
        Err(_) => false,
    }
}

pub fn position_gpo_http_router(
    targets: Vec<PositionGpoHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/technology/position-gpo",
            get(overview),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/technology/position-gpo/preview",
            post(preview),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/technology/position-gpo/publish",
            post(publish),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/technology/position-gpo/unmap-preview",
            post(unmap_preview),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/technology/position-gpo/unmap",
            post(unmap),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/technology/position-gpo/retry",
            post(retry),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/technology/position-gpo/retry-config",
            post(retry_config),
        )
        .with_state(Arc::new(PositionGpoHttpState {
            targets: configured,
        })))
}

async fn overview(
    State(state): State<Arc<PositionGpoHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let target = match target(&state, request.headers(), &slug) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    if let Err(error) = ensure_no_query(&request) {
        return error_response(error);
    }
    let headers = request.headers().clone();
    match target
        .backend
        .execute(PositionGpoHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: &headers,
            command: PositionGpoHttpCommand::Overview,
        })
        .await
    {
        Ok(reply) => bounded(serde_json::json!({ "data": reply.data })),
        Err(error) => error_response(error),
    }
}

async fn preview(
    State(state): State<Arc<PositionGpoHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        if !origin_allowed(&request, target) {
            return Err(origin_forbidden());
        }
        let _ = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input = PositionGpoInput::parse(&body)?;
        target
            .backend
            .execute(PositionGpoHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: PositionGpoHttpCommand::Preview { input },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(serde_json::json!({"data": reply.data})),
        Err(error) => error_response(error),
    }
}

async fn publish(
    State(state): State<Arc<PositionGpoHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        if !origin_allowed(&request, target) {
            return Err(origin_forbidden());
        }
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input = PublishInput::parse(&body)?;
        target
            .backend
            .execute(PositionGpoHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: PositionGpoHttpCommand::Publish {
                    input,
                    idempotency_key: key,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(serde_json::json!({"data": reply.data})),
        Err(error) => error_response(error),
    }
}

async fn unmap_preview(
    State(state): State<Arc<PositionGpoHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        if !origin_allowed(&request, target) {
            return Err(origin_forbidden());
        }
        let _ = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input = UnmapInput::parse(&body)?;
        target
            .backend
            .execute(PositionGpoHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: PositionGpoHttpCommand::UnmapPreview { input },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(serde_json::json!({"data": reply.data})),
        Err(error) => error_response(error),
    }
}

async fn unmap(
    State(state): State<Arc<PositionGpoHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        if !origin_allowed(&request, target) {
            return Err(origin_forbidden());
        }
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input = UnmapPublishInput::parse(&body)?;
        target
            .backend
            .execute(PositionGpoHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: PositionGpoHttpCommand::Unmap {
                    input,
                    idempotency_key: key,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(serde_json::json!({"data": reply.data})),
        Err(error) => error_response(error),
    }
}

async fn retry(
    State(state): State<Arc<PositionGpoHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        if !origin_allowed(&request, target) {
            return Err(origin_forbidden());
        }
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input = JobRetryInput::parse(&body)?;
        target
            .backend
            .execute(PositionGpoHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: PositionGpoHttpCommand::Retry {
                    input,
                    idempotency_key: key,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(serde_json::json!({"data": reply.data})),
        Err(error) => error_response(error),
    }
}

async fn retry_config(
    State(state): State<Arc<PositionGpoHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        if !origin_allowed(&request, target) {
            return Err(origin_forbidden());
        }
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let input = ConfigRetryInput::parse(&body)?;
        target
            .backend
            .execute(PositionGpoHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: PositionGpoHttpCommand::RetryConfig {
                    input,
                    idempotency_key: key,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(serde_json::json!({"data": reply.data})),
        Err(error) => error_response(error),
    }
}
