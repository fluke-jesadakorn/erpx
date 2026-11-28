//! Owner-local HTTP ingress for the native R3 RFQ workspace/detail routes.
//!
//! Transport facts only: registered Host/Holding target, exact browser origin,
//! bounded private JSON, the retained `Idempotency-Key` guard and business-error
//! mapping. Account session resolution, Company scope, purchased-package
//! admission, the workflow command write and the owner SQL read remain
//! composition responsibilities supplied through the backend trait. The router
//! never mints a session, actor, Company or purchase and never executes SQL.
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::get, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const MAX_ID: usize = 200;

pub type RfqHttpFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct RfqHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
}

/// The retained route returns only `{data}` (plus `Cache-Control`); no
/// `context` object is added so the envelope stays byte-compatible.
#[derive(Clone, Debug)]
pub struct RfqHttpResponse {
    pub data: Value,
}

impl RfqHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data})
    }
}

/// Composition-owned Account session, scope, purchase, workflow writer and
/// owner-unit adapter. The router never mints any of those facts.
pub trait RfqHttpBackend: Send + Sync {
    fn workspace<'a>(&'a self, request: RfqHttpRequest<'a>) -> RfqHttpFuture<'a, Value>;
    fn create<'a>(
        &'a self,
        request: RfqHttpRequest<'a>,
        input: Value,
        key: &'a str,
    ) -> RfqHttpFuture<'a, Value>;
    fn detail<'a>(&'a self, request: RfqHttpRequest<'a>, id: &'a str) -> RfqHttpFuture<'a, Value>;
    fn act<'a>(
        &'a self,
        request: RfqHttpRequest<'a>,
        id: &'a str,
        input: Value,
        key: &'a str,
    ) -> RfqHttpFuture<'a, Value>;
}

pub struct RfqHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn RfqHttpBackend>,
}

struct RfqHttpState {
    targets: BTreeMap<(String, String), RfqHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid("PROCUREMENT_RFQ_REQUEST_INVALID", "Use one unambiguous Procurement quotation request.")
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, "ORIGIN_FORBIDDEN", "Use the application origin.")
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
    if key.len() > MAX_ID {
        return Err(request_invalid());
    }
    Ok(key.to_owned())
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID
}

pub fn rfq_router(targets: Vec<RfqHttpTarget>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&target.origin)
            || configured.insert((target.host.clone(), target.holding_slug.clone()), target).is_some()
        {
            return Err(BusinessError::invalid(
                "PROCUREMENT_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/procurement-rfqs",
            get(workspace).post(create),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/procurement-rfqs/{id}",
            get(detail).post(act),
        )
        .with_state(Arc::new(RfqHttpState { targets: configured })))
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

fn bounded(value: Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response.headers_mut().insert(name, HeaderValue::from_static(value));
    }
    response
}

fn target<'a>(state: &'a RfqHttpState, headers: &HeaderMap, slug: &str) -> Result<&'a RfqHttpTarget, BusinessError> {
    let host = unique_header(headers, "host")?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state.targets.get(&(host.to_owned(), slug.to_owned())).ok_or_else(missing)
}

async fn input(request: Request) -> Result<Value, BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT).await.map_err(|_| request_invalid())?;
    serde_json::from_slice(&bytes).map_err(|_| request_invalid())
}

async fn workspace(
    State(state): State<Arc<RfqHttpState>>,
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
            .workspace(RfqHttpRequest { host: &target.host, holding_slug: &target.holding_slug })
            .await
    }
    .await;
    match result {
        Ok(data) => bounded(RfqHttpResponse { data }.value()),
        Err(error) => error_response(error),
    }
}

async fn create(
    State(state): State<Arc<RfqHttpState>>,
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
            .create(
                RfqHttpRequest { host: &host, holding_slug: &backend.holding_slug },
                body,
                &key,
            )
            .await
    }
    .await;
    match result {
        Ok(data) => bounded(RfqHttpResponse { data }.value()),
        Err(error) => error_response(error),
    }
}

async fn detail(
    State(state): State<Arc<RfqHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        if request.uri().query().is_some() || !valid_id(&id) {
            return Err(request_invalid());
        }
        let target = target(&state, request.headers(), &slug)?;
        target
            .backend
            .detail(
                RfqHttpRequest { host: &target.host, holding_slug: &target.holding_slug },
                &id,
            )
            .await
    }
    .await;
    match result {
        Ok(data) => bounded(RfqHttpResponse { data }.value()),
        Err(error) => error_response(error),
    }
}

async fn act(
    State(state): State<Arc<RfqHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        if !valid_id(&id) {
            return Err(request_invalid());
        }
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
            .act(
                RfqHttpRequest { host: &host, holding_slug: &backend.holding_slug },
                &id,
                body,
                &key,
            )
            .await
    }
    .await;
    match result {
        Ok(data) => bounded(RfqHttpResponse { data }.value()),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_is_exactly_data() {
        assert_eq!(
            RfqHttpResponse { data: serde_json::json!({"rfqs": []}) }.value(),
            serde_json::json!({"data": {"rfqs": []}})
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
    fn duplicate_headers_are_rejected() {
        let mut headers = HeaderMap::new();
        headers.append("origin", HeaderValue::from_static("https://a.example"));
        headers.append("origin", HeaderValue::from_static("https://b.example"));
        assert_eq!(check_origin(&headers, "https://a.example").unwrap_err().code, "PROCUREMENT_RFQ_REQUEST_INVALID");
    }

    #[test]
    fn invalid_origin_is_rejected() {
        assert!(!valid_origin("http://acme.jesadakorn.com"));
        assert!(!valid_origin("https://acme.jesadakorn.com/evil"));
        assert!(valid_origin("https://acme.jesadakorn.com"));
    }
}
