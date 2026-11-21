//! Native user-management HTTP ingress.
//!
//! Transport facts only: registered Host/Holding targets, bounded JSON bodies,
//! strict Idempotency-Key on apply and business-error mapping. Account session
//! resolution, the selected Company scope, current IAM authority and the
//! serializable owner unit remain composition responsibilities behind
//! [`UserManagementHttpBackend`].
use crate::domain::user_management::{preview_changed, request_invalid};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode,
    body::to_bytes,
    header,
    routing::{get, post},
    serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 256 * 1024;

pub type UserManagementHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// One reviewed user-management transport command.
#[derive(Clone, Debug)]
pub enum UserManagementHttpCommand {
    Read,
    Preview {
        change: Value,
    },
    Apply {
        change: Value,
        token: String,
        idempotency_key: String,
    },
    ApplyReview {
        payload: Value,
        idempotency_key: String,
    },
    DecideReview {
        review_id: String,
        payload: Value,
        idempotency_key: String,
    },
    Reconcile,
}

pub struct UserManagementHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: UserManagementHttpCommand,
}

#[derive(Clone, Debug)]
pub struct UserManagementHttpResponse {
    /// Owner data. The backend adds the validated current context.
    pub data: Value,
    pub context: Value,
}

impl UserManagementHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope, authority and owner unit adapter.
/// The router never mints a session, actor, Company or permission.
pub trait UserManagementHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: UserManagementHttpRequest<'a>,
    ) -> UserManagementHttpFuture<'a, UserManagementHttpResponse>;
}

pub struct UserManagementHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn UserManagementHttpBackend>,
}

struct UserManagementHttpState {
    targets: BTreeMap<(String, String), UserManagementHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    request_invalid()
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "USER_MANAGEMENT_HTTP_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
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
    state: &'a UserManagementHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a UserManagementHttpTarget, BusinessError> {
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
                "Idempotency-Key is required for this identity command.",
            )
        })?;
    CommandKey::new(value.to_owned())?;
    Ok(value.to_owned())
}

/// Holding ingress for the reviewed user-management commands.
pub fn user_management_http_router(
    targets: Vec<UserManagementHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/iam/user-management",
            get(read),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/user-management/preview",
            post(preview),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/user-management/save",
            post(apply),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/user-management/reviews/apply",
            post(apply_review),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/user-management/reviews/{review_id}/decide",
            post(decide_review),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/user-management/reconcile",
            post(reconcile),
        )
        .with_state(Arc::new(UserManagementHttpState {
            targets: configured,
        })))
}

async fn read(
    State(state): State<Arc<UserManagementHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let headers = request.headers().clone();
        target
            .backend
            .execute(UserManagementHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: UserManagementHttpCommand::Read,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn preview(
    State(state): State<Arc<UserManagementHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let change = body
            .as_object()
            .and_then(|object| {
                if object.len() == 1 {
                    object.get("change")
                } else {
                    None
                }
            })
            .cloned()
            .ok_or_else(malformed)?;
        target
            .backend
            .execute(UserManagementHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: UserManagementHttpCommand::Preview { change },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn apply(
    State(state): State<Arc<UserManagementHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let change = body
            .as_object()
            .and_then(|object| object.get("change"))
            .cloned()
            .ok_or_else(malformed)?;
        let token = body
            .as_object()
            .and_then(|object| object.get("previewToken"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(preview_changed)?;
        target
            .backend
            .execute(UserManagementHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: UserManagementHttpCommand::Apply {
                    change,
                    token,
                    idempotency_key: key,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn apply_review(
    State(state): State<Arc<UserManagementHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let payload = strict_body(request).await?;
        target
            .backend
            .execute(UserManagementHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: UserManagementHttpCommand::ApplyReview {
                    payload,
                    idempotency_key: key,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn decide_review(
    State(state): State<Arc<UserManagementHttpState>>,
    Path((slug, review_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let payload = strict_body(request).await?;
        target
            .backend
            .execute(UserManagementHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: UserManagementHttpCommand::DecideReview {
                    review_id,
                    payload,
                    idempotency_key: key,
                },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn reconcile(
    State(state): State<Arc<UserManagementHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let headers = request.headers().clone();
        target
            .backend
            .execute(UserManagementHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: UserManagementHttpCommand::Reconcile,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}
