//! Owner-local HTTP ingress for the native R3 Manufacturing posting-setup
//! route.
//!
//! Transport facts only: registered Host/Holding target, exact browser origin,
//! bounded private JSON, the retained `Idempotency-Key` guard, the 201 write
//! status and business-error mapping. Account session, Company scope, Finance
//! authority, purchased-package admission, the workflow command and the owner
//! SQL read remain composition responsibilities supplied through the backend
//! trait. The router never mints or executes any of those facts.
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::get, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const MAX_KEY: usize = 200;

pub type PostingSetupHttpFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct PostingSetupHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
}

/// The retained route returns only `{data}` (plus `Cache-Control`).
#[derive(Clone, Debug)]
pub struct PostingSetupHttpResponse {
    pub data: Value,
}

impl PostingSetupHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data})
    }
}

pub trait PostingSetupHttpBackend: Send + Sync {
    fn read<'a>(&'a self, request: PostingSetupHttpRequest<'a>) -> PostingSetupHttpFuture<'a, Value>;
    fn initialize<'a>(
        &'a self,
        request: PostingSetupHttpRequest<'a>,
        input: Value,
        key: &'a str,
    ) -> PostingSetupHttpFuture<'a, Value>;
}

pub struct PostingSetupHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn PostingSetupHttpBackend>,
}

struct PostingSetupHttpState {
    targets: BTreeMap<(String, String), PostingSetupHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_POSTING_REQUEST_INVALID",
        "Use one unambiguous Manufacturing posting-setup request.",
    )
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin for this action.",
    )
}

fn command_key_required() -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, "COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host.bytes().any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
            })
    })
}

/// Exact TS POST origin guard: an absent origin is allowed, a present one must
/// equal the registered public origin.
fn check_origin(headers: &HeaderMap, expected: &str) -> Result<(), BusinessError> {
    match unique_header(headers, "origin")? {
        Some(origin) if origin != expected => Err(origin_forbidden()),
        _ => Ok(()),
    }
}

fn require_key(headers: &HeaderMap) -> Result<String, BusinessError> {
    let key = unique_header(headers, "idempotency-key")?
        .filter(|value| !value.is_empty())
        .ok_or_else(command_key_required)?;
    if key.len() > MAX_KEY {
        return Err(request_invalid());
    }
    Ok(key.to_owned())
}

pub fn posting_setup_router(targets: Vec<PostingSetupHttpTarget>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&target.origin)
            || configured.insert((target.host.clone(), target.holding_slug.clone()), target).is_some()
        {
            return Err(BusinessError::invalid(
                "MANUFACTURING_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/production/posting-setup",
            get(read_setup).post(initialize),
        )
        .with_state(Arc::new(PostingSetupHttpState { targets: configured })))
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
    let mut response = (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

fn bounded(value: Value, status: StatusCode) -> Response {
    let mut response = (status, Json(value)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response.headers_mut().insert(name, HeaderValue::from_static(value));
    }
    response
}

fn target<'a>(
    state: &'a PostingSetupHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PostingSetupHttpTarget, BusinessError> {
    let host = unique_header(headers, "host")?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state.targets.get(&(host.to_owned(), slug.to_owned())).ok_or_else(missing)
}

async fn input(request: Request) -> Result<Value, BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT).await.map_err(|_| request_invalid())?;
    serde_json::from_slice(&bytes).map_err(|_| request_invalid())
}

async fn read_setup(
    State(state): State<Arc<PostingSetupHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        if request.uri().query().is_some() {
            return Err(request_invalid());
        }
        let target = target(&state, request.headers(), &slug)?;
        target
            .backend
            .read(PostingSetupHttpRequest { host: &target.host, holding_slug: &target.holding_slug })
            .await
    }
    .await;
    match result {
        Ok(data) => bounded(PostingSetupHttpResponse { data }.value(), StatusCode::OK),
        Err(error) => error_response(error),
    }
}

async fn initialize(
    State(state): State<Arc<PostingSetupHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let (host, origin) = {
            let target = target(&state, request.headers(), &slug)?;
            (target.host.clone(), target.origin.clone())
        };
        check_origin(request.headers(), &origin)?;
        let key = require_key(request.headers())?;
        let body = input(request).await?;
        let backend = state.targets.get(&(host.clone(), slug.clone())).ok_or_else(missing)?;
        backend
            .backend
            .initialize(
                PostingSetupHttpRequest { host: &host, holding_slug: &backend.holding_slug },
                body,
                &key,
            )
            .await
    }
    .await;
    match result {
        Ok(data) => bounded(PostingSetupHttpResponse { data }.value(), StatusCode::CREATED),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_is_exactly_data() {
        assert_eq!(
            PostingSetupHttpResponse { data: serde_json::json!({"companyId": "c1"}) }.value(),
            serde_json::json!({"data": {"companyId": "c1"}})
        );
    }

    #[test]
    fn origin_guard_matches_the_retained_rules() {
        let headers = HeaderMap::new();
        assert!(check_origin(&headers, "https://acme.jesadakorn.com").is_ok());
        let mut headers = HeaderMap::new();
        headers.insert("origin", HeaderValue::from_static("https://acme.jesadakorn.com"));
        assert!(check_origin(&headers, "https://acme.jesadakorn.com").is_ok());
        assert_eq!(
            check_origin(&headers, "https://evil.example").unwrap_err().code,
            "ORIGIN_FORBIDDEN"
        );
    }

    #[test]
    fn command_key_is_required() {
        assert_eq!(require_key(&HeaderMap::new()).unwrap_err().code, "COMMAND_KEY_REQUIRED");
        let mut headers = HeaderMap::new();
        headers.insert("Idempotency-Key", HeaderValue::from_static("key-1"));
        assert_eq!(require_key(&headers).unwrap(), "key-1");
    }

    #[test]
    fn invalid_origin_is_rejected() {
        assert!(!valid_origin("http://acme.jesadakorn.com"));
        assert!(!valid_origin("https://acme.jesadakorn.com/evil"));
        assert!(valid_origin("https://acme.jesadakorn.com"));
    }
}
