//! Native Directory connection save/test/activate/disable ingress. Transport
//! facts only: registered Host/Holding target, exact browser origin, bounded
//! JSON, strict Idempotency-Key and business-error mapping. Session, current
//! Work authority, fresh MFA and the owner unit stay in the backend.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::post, to_bytes,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 32 * 1024;

pub type DirectoryConnectionHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum DirectoryConnectionHttpCommand {
    Save { input: Value, idempotency_key: String },
    Test { id: String, version: i64, idempotency_key: String },
    Activate { id: String, version: i64, idempotency_key: String },
    Disable { id: String, version: i64, idempotency_key: String },
}

pub struct DirectoryConnectionHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: DirectoryConnectionHttpCommand,
}

#[derive(Clone, Debug)]
pub struct DirectoryConnectionHttpResponse {
    pub data: Value,
}

pub trait DirectoryConnectionHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: DirectoryConnectionHttpRequest<'a>,
    ) -> DirectoryConnectionHttpFuture<'a, DirectoryConnectionHttpResponse>;
}

pub struct DirectoryConnectionHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn DirectoryConnectionHttpBackend>,
}

struct StateValue {
    targets: BTreeMap<(String, String), DirectoryConnectionHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "DIRECTORY_CONNECTION_REQUEST_INVALID",
        "Review the connection request and try again.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "DIRECTORY_CONNECTION_HTTP_CONFIGURATION_INVALID",
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

struct Plan {
    command: RequestPlan,
}

enum RequestPlan {
    Save,
    Test,
    Activate,
    Disable,
}

impl RequestPlan {
    fn command_name(&self) -> &'static str {
        match self {
            Self::Save => "save",
            Self::Test => "test",
            Self::Activate => "activate",
            Self::Disable => "disable",
        }
    }
}

fn plan(path: &[&str]) -> Option<Plan> {
    let command = match path {
        ["", "api", "v1", "holdings", slug, "iam", "connections"] if valid_slug(slug) => {
            RequestPlan::Save
        }
        ["", "api", "v1", "holdings", slug, "iam", "connections", id, "test"]
            if valid_slug(slug) && !id.is_empty() =>
        {
            RequestPlan::Test
        }
        ["", "api", "v1", "holdings", slug, "iam", "connections", id, "activate"]
            if valid_slug(slug) && !id.is_empty() =>
        {
            RequestPlan::Activate
        }
        ["", "api", "v1", "holdings", slug, "iam", "connections", id, "disable"]
            if valid_slug(slug) && !id.is_empty() =>
        {
            RequestPlan::Disable
        }
        _ => return None,
    };
    Some(Plan { command })
}

async fn handle(state: Arc<StateValue>, request: Request) -> Response {
    let path = request.uri().path().to_owned();
    let parts = path.split('/').collect::<Vec<_>>();
    let Some(plan) = plan(&parts) else {
        return bounded_error(missing());
    };
    let slug = parts.get(4).copied().unwrap_or_default().to_owned();
    let id = parts.get(7).copied().unwrap_or_default().to_owned();
    if request.uri().scheme().is_some()
        || [
            "forwarded",
            "x-forwarded-host",
            "x-forwarded-proto",
            "x-forwarded-port",
            "x-original-host",
        ]
        .iter()
        .any(|h| request.headers().contains_key(*h))
    {
        return bounded_error(BusinessError::new(
            FailureKind::Forbidden,
            "UNTRUSTED_REQUEST_AUTHORITY",
            "Use the registered Holding origin.",
        ));
    }
    let host = match request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    {
        Some(host) => host.to_owned(),
        None => return bounded_error(missing()),
    };
    let Some(target) = state.targets.get(&(host.clone(), slug.clone())) else {
        return bounded_error(missing());
    };
    let origin = match unique_header(request.headers(), header::ORIGIN) {
        Ok(Some(origin)) => origin,
        _ => return bounded_error(origin_forbidden()),
    };
    if origin != target.origin || !valid_origin(origin) {
        return bounded_error(origin_forbidden());
    }
    let key = match unique_header(request.headers(), HeaderName::from_static("idempotency-key")) {
        Ok(Some(key)) if !key.is_empty() && key.len() <= 256 => key.to_owned(),
        _ => {
            return bounded_error(BusinessError::invalid(
                "IDEMPOTENCY_KEY_REQUIRED",
                "Idempotency-Key is required.",
            ));
        }
    };
    let content_type = unique_header(request.headers(), header::CONTENT_TYPE).unwrap_or(None);
    if !content_type.is_some_and(|value| value.starts_with("application/json")) {
        return bounded_error(malformed());
    }
    let headers = request.headers().clone();
    let bytes = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return bounded_error(BusinessError::invalid(
                "BODY_TOO_LARGE",
                "Use a smaller connection request.",
            ));
        }
    };
    let body: Value = match serde_json::from_slice(&bytes) {
        Ok(body) => body,
        Err(_) => return bounded_error(malformed()),
    };
    let version = body.get("expectedVersion").and_then(Value::as_i64);
    let command = match plan.command {
        RequestPlan::Save => DirectoryConnectionHttpCommand::Save {
            input: body,
            idempotency_key: key,
        },
        RequestPlan::Test => {
            let Some(version) = version else {
                return bounded_error(malformed());
            };
            DirectoryConnectionHttpCommand::Test {
                id: id.clone(),
                version,
                idempotency_key: key,
            }
        }
        RequestPlan::Activate => {
            let Some(version) = version else {
                return bounded_error(malformed());
            };
            DirectoryConnectionHttpCommand::Activate {
                id: id.clone(),
                version,
                idempotency_key: key,
            }
        }
        RequestPlan::Disable => {
            let Some(version) = version else {
                return bounded_error(malformed());
            };
            DirectoryConnectionHttpCommand::Disable {
                id: id.clone(),
                version,
                idempotency_key: key,
            }
        }
    };
    match target
        .backend
        .execute(DirectoryConnectionHttpRequest {
            host: &host,
            holding_slug: &slug,
            headers: &headers,
            command,
        })
        .await
    {
        Ok(response) => bounded(response.data),
        Err(error) => bounded_error(error),
    }
}

fn bounded_error(error: BusinessError) -> Response {
    let mut response = error_response(error);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

async fn route_save(State(state): State<Arc<StateValue>>, request: Request) -> Response {
    handle(state, request).await
}
async fn route_test(State(state): State<Arc<StateValue>>, request: Request) -> Response {
    handle(state, request).await
}
async fn route_activate(State(state): State<Arc<StateValue>>, request: Request) -> Response {
    handle(state, request).await
}
async fn route_disable(State(state): State<Arc<StateValue>>, request: Request) -> Response {
    handle(state, request).await
}

pub fn directory_connection_http_router(
    targets: Vec<DirectoryConnectionHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut registered = BTreeMap::new();
    for target in targets {
        if !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || registered
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    let state = Arc::new(StateValue { targets: registered });
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/iam/connections",
            post(route_save),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/connections/{connection_id}/test",
            post(route_test),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/connections/{connection_id}/activate",
            post(route_activate),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/connections/{connection_id}/disable",
            post(route_disable),
        )
        .with_state(state))
}

#[allow(dead_code)]
fn _command_name(plan: &Plan) -> &'static str {
    plan.command.command_name()
}
