//! Owner-local HTTP ingress for Executive external connections.
//!
//! Transport facts only: exact registered Holding Host, bounded body and
//! response headers. The composition backend resolves the normal Account
//! session, selected work authority and origin before calling this module. No
//! route mints a session, assignment, scope or permission.
use crate::public::http_support::{
    HeaderMap, HeaderValue, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, get, header, json, post,
};
use axum::response::IntoResponse;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 4096;

pub type ExecutiveConnectionsHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct ExecutiveConnectionsHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

pub struct ExecutiveConnectionsHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a str,
    pub body: &'a serde_json::Value,
}

/// Composition-owned Account session, selected Work authority and owner store.
pub trait ExecutiveConnectionsHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: ExecutiveConnectionsHttpReadRequest<'a>,
    ) -> ExecutiveConnectionsHttpFuture<'a, serde_json::Value>;
    fn command<'a>(
        &'a self,
        request: ExecutiveConnectionsHttpCommandRequest<'a>,
    ) -> ExecutiveConnectionsHttpFuture<'a, serde_json::Value>;
}

pub struct ExecutiveConnectionsHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub backend: Arc<dyn ExecutiveConnectionsHttpBackend>,
}

struct Service {
    targets: BTreeMap<(String, String), ExecutiveConnectionsHttpTarget>,
}

struct Stateful {
    service: Arc<Service>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn relative_path(value: &str) -> bool {
    value.is_empty()
        || value
            .bytes()
            .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
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

fn status_for(error: &BusinessError) -> StatusCode {
    if error.code == "RATE_LIMITED" {
        StatusCode::TOO_MANY_REQUESTS
    } else {
        status(error.kind)
    }
}

fn error_response(error: BusinessError) -> Response {
    let mut response = (
        status_for(&error),
        Json(json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response();
    headers(&mut response);
    if error.code == "RATE_LIMITED" {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
    }
    response
}

fn headers(response: &mut Response) {
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
}

fn bounded(data: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(json!({"data": data}))).into_response();
    headers(&mut response);
    response
}

impl Service {
    fn new(targets: Vec<ExecutiveConnectionsHttpTarget>) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || target
                    .host
                    .bytes()
                    .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
                || target.holding_slug.is_empty()
                || relative_path(&target.holding_slug)
                || configured
                    .insert((target.host.clone(), target.holding_slug.clone()), target)
                    .is_some()
            {
                return Err(BusinessError::invalid(
                    "EXECUTIVE_CONNECTIONS_CONFIGURATION_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
        }
        Ok(Self {
            targets: configured,
        })
    }

    fn target(
        &self,
        headers: &HeaderMap,
        slug: &str,
    ) -> Result<&ExecutiveConnectionsHttpTarget, BusinessError> {
        let mut values = headers.get_all(header::HOST).iter();
        let first = values.next();
        if values.next().is_some() {
            return Err(BusinessError::invalid(
                "EXECUTIVE_CONNECTIONS_INVALID",
                "Use one unambiguous Holding Host.",
            ));
        }
        let host = first
            .map(|value| value.to_str())
            .transpose()
            .map_err(|_| {
                BusinessError::invalid("EXECUTIVE_CONNECTIONS_INVALID", "Use a valid Holding Host.")
            })?
            .ok_or_else(missing)?;
        self.targets
            .get(&(host.to_owned(), slug.to_owned()))
            .ok_or_else(missing)
    }
}

pub fn executive_connections_router(
    targets: Vec<ExecutiveConnectionsHttpTarget>,
) -> Result<Router, BusinessError> {
    let service = Arc::new(Service::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/executive/connections",
            get(read_connections),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/executive/connections",
            post(command_connections),
        )
        .with_state(Arc::new(Stateful { service })))
}

async fn read_connections(
    State(state): State<Arc<Stateful>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (host, backend) = match state.service.target(request.headers(), &slug) {
        Ok(target) => (target.host.clone(), target.backend.clone()),
        Err(error) => return error_response(error),
    };
    match backend
        .read(ExecutiveConnectionsHttpReadRequest {
            host: &host,
            holding_slug: &slug,
            headers: request.headers(),
        })
        .await
    {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

/// Exactly the retained UUID Idempotency-Key, parsed before the body.
fn command_key(request: &Request) -> Result<String, BusinessError> {
    let mut values = request.headers().get_all("idempotency-key").iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(BusinessError::invalid(
            "VALIDATION_FAILED",
            "Use one Idempotency-Key.",
        ));
    }
    let value = first
        .map(|value| value.to_str())
        .transpose()
        .map_err(|_| BusinessError::invalid("VALIDATION_FAILED", "Invalid uuid"))?
        .ok_or_else(|| BusinessError::invalid("VALIDATION_FAILED", "Invalid uuid"))?;
    if value.len() != 36
        || !value.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
    {
        return Err(BusinessError::invalid("VALIDATION_FAILED", "Invalid uuid"));
    }
    Ok(value.to_ascii_lowercase())
}

async fn command_connections(
    State(state): State<Arc<Stateful>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = state.service.target(request.headers(), &slug)?.host.clone();
        let key = command_key(&request)?;
        let (parts, body) = request.into_parts();
        let bytes = to_bytes(body, BODY_LIMIT)
            .await
            .map_err(|_| BusinessError::invalid("INVALID_JSON", "Use valid JSON."))?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| BusinessError::invalid("INVALID_JSON", "Use valid JSON."))?;
        let target = state
            .service
            .targets
            .get(&(host.clone(), slug.clone()))
            .ok_or_else(missing)?;
        target
            .backend
            .command(ExecutiveConnectionsHttpCommandRequest {
                host: &host,
                holding_slug: &slug,
                headers: &parts.headers,
                key: &key,
                body: &value,
            })
            .await
    };
    match result.await {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

/// One shared MCP request budget, authenticated by the bearer secret itself.
pub trait ExecutiveMcpRateLimitBackend: Send + Sync {
    fn consume<'a>(
        &'a self,
        authorization: Option<&'a str>,
    ) -> ExecutiveConnectionsHttpFuture<'a, i64>;
}

struct RateLimitState {
    backend: Arc<dyn ExecutiveMcpRateLimitBackend>,
}

pub fn executive_mcp_rate_limit_router(backend: Arc<dyn ExecutiveMcpRateLimitBackend>) -> Router {
    Router::new()
        .route("/api/v1/executive/mcp/rate-limit", post(command_rate_limit))
        .with_state(Arc::new(RateLimitState { backend }))
}

async fn command_rate_limit(
    State(state): State<Arc<RateLimitState>>,
    request: Request,
) -> Response {
    let authorization = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    match state.backend.consume(authorization).await {
        Ok(requests) => bounded(json!({"requests": requests})),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::executive_connections::token_secret;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Probe {
        reads: Mutex<Vec<String>>,
        commands: Mutex<Vec<String>>,
        fail: bool,
    }

    impl ExecutiveConnectionsHttpBackend for Probe {
        fn read<'a>(
            &'a self,
            request: ExecutiveConnectionsHttpReadRequest<'a>,
        ) -> ExecutiveConnectionsHttpFuture<'a, serde_json::Value> {
            Box::pin(async move {
                self.reads
                    .lock()
                    .unwrap()
                    .push(format!("{}:{}", request.host, request.holding_slug));
                if self.fail {
                    return Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "EXECUTIVE_UNAVAILABLE",
                        "Synthetic unavailable.",
                    ));
                }
                Ok(json!({"canCreate":true,"scopes":[],"connections":[]}))
            })
        }
        fn command<'a>(
            &'a self,
            request: ExecutiveConnectionsHttpCommandRequest<'a>,
        ) -> ExecutiveConnectionsHttpFuture<'a, serde_json::Value> {
            Box::pin(async move {
                self.commands.lock().unwrap().push(format!(
                    "{}:{}:{}:{}",
                    request.host, request.holding_slug, request.key, request.body
                ));
                Ok(json!({"id":request.key}))
            })
        }
    }

    #[test]
    fn targets_require_distinct_exact_hosts_and_slugs() {
        let backend = Arc::new(Probe::default());
        let target = || ExecutiveConnectionsHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            backend: backend.clone(),
        };
        assert!(executive_connections_router(vec![target()]).is_ok());
        assert!(executive_connections_router(vec![target(), target()]).is_err());
        assert!(
            executive_connections_router(vec![ExecutiveConnectionsHttpTarget {
                host: "https://acme.example.test".into(),
                ..target()
            }])
            .is_err()
        );
        assert!(
            executive_connections_router(vec![ExecutiveConnectionsHttpTarget {
                holding_slug: "a/b".into(),
                ..target()
            }])
            .is_err()
        );
    }

    #[test]
    fn bearer_secret_parser_never_accepts_a_session_cookie() {
        assert!(token_secret("Bearer session-cookie").is_none());
        assert!(token_secret("").is_none());
    }

    #[test]
    fn rate_limit_status_is_429_and_other_conflicts_stay_409() {
        assert_eq!(
            status_for(&crate::domain::executive_connections::rate_limited()),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            status_for(&crate::domain::executive_connections::connection_limit()),
            StatusCode::CONFLICT
        );
    }
}
