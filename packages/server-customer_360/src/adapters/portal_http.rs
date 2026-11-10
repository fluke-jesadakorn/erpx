//! Native customer-portal HTTP ingress.
//!
//! Transport facts only: one registered Host/Holding target, one exact portal
//! path, one bounded read and one idempotent acknowledgement. Verified portal
//! session proof, the Commercial purchase scope, the Files shared-file read and
//! the owned transaction remain composition responsibilities behind
//! [`CustomerPortalHttpBackend`].
use crate::domain::portal::{
    CustomerPortalProjection, PortalCommandInput, PortalCommandOutcome, valid_key,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type CustomerPortalHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct CustomerPortalHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

pub struct CustomerPortalHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a str,
    pub input: PortalCommandInput,
}

pub trait CustomerPortalHttpBackend: Send + Sync {
    fn projection<'a>(
        &'a self,
        request: CustomerPortalHttpReadRequest<'a>,
    ) -> CustomerPortalHttpFuture<'a, CustomerPortalProjection>;

    fn command<'a>(
        &'a self,
        request: CustomerPortalHttpCommandRequest<'a>,
    ) -> CustomerPortalHttpFuture<'a, PortalCommandOutcome>;
}

pub struct CustomerPortalHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn CustomerPortalHttpBackend>,
}

struct CustomerPortalHttpState {
    targets: BTreeMap<(String, String), CustomerPortalHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_PORTAL_REQUEST_INVALID",
        "Use one bounded customer portal request.",
    )
}
fn key_required() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "COMMAND_KEY_REQUIRED",
        "Use a valid Idempotency-Key for this action.",
    )
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
        "CUSTOMER_PORTAL_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
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
fn respond<T: serde::Serialize>(result: Result<T, BusinessError>) -> Response {
    match result {
        Ok(data) => bounded(serde_json::json!({"data": data})),
        Err(error) => error_response(error),
    }
}
fn target<'a>(
    state: &'a CustomerPortalHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a CustomerPortalHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn require_origin(
    target: &CustomerPortalHttpTarget,
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
    State(state): State<Arc<CustomerPortalHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    if request.uri().query().is_some_and(|query| !query.is_empty()) {
        return error_response(malformed());
    }
    let headers = request.headers().clone();
    let result = async move {
        let target = target(&state, &headers, &slug)?;
        require_origin(target, &headers, false)?;
        target
            .backend
            .projection(CustomerPortalHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
            })
            .await
    }
    .await;
    respond(result)
}

async fn command(
    State(state): State<Arc<CustomerPortalHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(malformed()),
    };
    let result = async move {
        let target = target(&state, &headers, &slug)?;
        require_origin(target, &headers, true)?;
        let key = unique_header(&headers, HeaderName::from_static("idempotency-key"))?
            .ok_or_else(key_required)?;
        if !valid_key(key) {
            return Err(key_required());
        }
        if unique_header(&headers, header::CONTENT_TYPE)?
            .is_none_or(|value| !value.starts_with("application/json"))
        {
            return Err(malformed());
        }
        let input: PortalCommandInput = serde_json::from_slice(&body).map_err(|_| malformed())?;
        input.validate()?;
        target
            .backend
            .command(CustomerPortalHttpCommandRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                key,
                input,
            })
            .await
    }
    .await;
    respond(result)
}

pub fn customer_portal_http_router(
    targets: Vec<CustomerPortalHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/customer-portal",
            get(read).post(command),
        )
        .with_state(Arc::new(CustomerPortalHttpState {
            targets: configured,
        })))
}
