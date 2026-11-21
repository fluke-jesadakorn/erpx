//! Native Agent governance HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target, one bounded GET/POST
//! and the idempotency/origin contract. Current Account authority, Agent
//! policy, snapshots, receipts, audit and outbox remain composition
//! responsibilities behind the backend trait. This router does not serve the
//! platform-owned Agent evidence audit read.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const KEY_LIMIT: usize = 180;

pub type AgentGovernanceHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct AgentGovernanceHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

pub struct AgentGovernanceHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a str,
    pub body: serde_json::Value,
}

pub trait AgentGovernanceHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: AgentGovernanceHttpReadRequest<'a>,
    ) -> AgentGovernanceHttpFuture<'a, serde_json::Value>;
    fn command<'a>(
        &'a self,
        request: AgentGovernanceHttpCommandRequest<'a>,
    ) -> AgentGovernanceHttpFuture<'a, serde_json::Value>;
}

pub struct AgentGovernanceHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn AgentGovernanceHttpBackend>,
}

struct AgentGovernanceHttpState {
    targets: BTreeMap<(String, String), AgentGovernanceHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "AGENT_REQUEST_INVALID",
        "Use one bounded Agent governance request.",
    )
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
        "AGENT_CONFIGURATION_INVALID",
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
fn respond(result: Result<serde_json::Value, BusinessError>) -> Response {
    match result {
        Ok(data) => bounded(serde_json::json!({"data": data})),
        Err(error) => error_response(error),
    }
}
fn target<'a>(
    state: &'a AgentGovernanceHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a AgentGovernanceHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn require_origin(
    target: &AgentGovernanceHttpTarget,
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

async fn read(
    State(state): State<Arc<AgentGovernanceHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result: Result<serde_json::Value, BusinessError> = async move {
        let target = target(&state, &headers, &slug)?;
        require_origin(target, &headers, false)?;
        target
            .backend
            .read(AgentGovernanceHttpReadRequest {
                host: unique_header(&headers, header::HOST)?.ok_or_else(missing)?,
                holding_slug: &slug,
                headers: &headers,
            })
            .await
    }
    .await;
    respond(result)
}

async fn command(
    State(state): State<Arc<AgentGovernanceHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(malformed()),
    };
    let result: Result<serde_json::Value, BusinessError> = async move {
        let target = target(&state, &headers, &slug)?;
        require_origin(target, &headers, true)?;
        let key = unique_header(&headers, HeaderName::from_static("idempotency-key"))?
            .ok_or_else(key_required)?;
        if key.len() < 8 || key.len() > KEY_LIMIT || key.chars().any(char::is_control) {
            return Err(key_required());
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
            .command(AgentGovernanceHttpCommandRequest {
                host: unique_header(&headers, header::HOST)?.ok_or_else(missing)?,
                holding_slug: &slug,
                headers: &headers,
                key,
                body: value,
            })
            .await
    }
    .await;
    respond(result)
}

pub fn agent_governance_http_router(
    targets: Vec<AgentGovernanceHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/agents",
            get(read).post(command),
        )
        .with_state(Arc::new(AgentGovernanceHttpState { targets: map })))
}
