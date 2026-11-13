//! Native Directory automation HTTP ingress.
//!
//! Transport facts only: registered Host/Holding targets, bounded JSON bodies,
//! strict Idempotency-Key on commands and business-error mapping. Account
//! session resolution, current IAM authority, fresh MFA and the owner unit
//! remain composition responsibilities behind [`DirectoryOperationHttpBackend`].
use crate::domain::directory_operation::{
    request_invalid, DirectoryChangeInput, MANAGE_PERMISSION,
};
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

const BODY_LIMIT: usize = 512 * 1024;

pub type DirectoryOperationHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum DirectoryOperationHttpCommand {
    Read,
    Refresh {
        connection_id: String,
        expected_version: i64,
        idempotency_key: String,
    },
    Configure {
        input: Value,
        idempotency_key: String,
    },
    Preview {
        change: Value,
    },
    Submit {
        change: Value,
        token: String,
        direct: bool,
        idempotency_key: String,
    },
    ApplyReview {
        payload: Value,
        decision: String,
        idempotency_key: String,
    },
    Retry {
        id: String,
        expected_version: i64,
        idempotency_key: String,
    },
    Cancel {
        id: String,
        expected_version: i64,
        idempotency_key: String,
    },
}

pub struct DirectoryOperationHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: DirectoryOperationHttpCommand,
}

#[derive(Clone, Debug)]
pub struct DirectoryOperationHttpResponse {
    pub data: Value,
}

impl DirectoryOperationHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data})
    }
}

/// Composition-owned Account session, scope, authority and owner unit adapter.
pub trait DirectoryOperationHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: DirectoryOperationHttpRequest<'a>,
    ) -> DirectoryOperationHttpFuture<'a, DirectoryOperationHttpResponse>;
}

pub struct DirectoryOperationHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn DirectoryOperationHttpBackend>,
}

struct DirectoryOperationHttpState {
    targets: BTreeMap<(String, String), DirectoryOperationHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    request_invalid()
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "DIRECTORY_OPERATION_HTTP_CONFIGURATION_INVALID",
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
    state: &'a DirectoryOperationHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a DirectoryOperationHttpTarget, BusinessError> {
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

pub fn directory_operation_http_router(
    targets: Vec<DirectoryOperationHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/iam/directory-operations",
            get(read),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/directory-operations/refresh",
            post(refresh),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/directory-operations/configure",
            post(configure),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/directory-operations/preview",
            post(preview),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/directory-operations/submit",
            post(submit),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/directory-operations/apply",
            post(apply),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/directory-operations/reviews/apply",
            post(apply_review),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/directory-operations/retry",
            post(retry),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/iam/directory-operations/cancel",
            post(cancel),
        )
        .with_state(Arc::new(DirectoryOperationHttpState {
            targets: configured,
        })))
}

async fn execute(
    state: Arc<DirectoryOperationHttpState>,
    slug: String,
    request: Request,
    build: impl FnOnce(Value) -> Result<DirectoryOperationHttpCommand, BusinessError>,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        let command = build(body)?;
        target
            .backend
            .execute(DirectoryOperationHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn read(
    State(state): State<Arc<DirectoryOperationHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let headers = request.headers().clone();
        target
            .backend
            .execute(DirectoryOperationHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: DirectoryOperationHttpCommand::Read,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

fn string(value: &Value, key: &str) -> Result<String, BusinessError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(malformed)
}
fn integer(value: &Value, key: &str) -> Result<i64, BusinessError> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .filter(|value| *value >= 1)
        .ok_or_else(malformed)
}

async fn refresh(
    State(state): State<Arc<DirectoryOperationHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let key = match strict_key(request.headers()) {
        Ok(key) => key,
        Err(error) => return error_response(error),
    };
    execute(state, slug, request, move |body| {
        Ok(DirectoryOperationHttpCommand::Refresh {
            connection_id: string(&body, "connectionId")?,
            expected_version: integer(&body, "expectedVersion")?,
            idempotency_key: key,
        })
    })
    .await
}

async fn configure(
    State(state): State<Arc<DirectoryOperationHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let key = match strict_key(request.headers()) {
        Ok(key) => key,
        Err(error) => return error_response(error),
    };
    execute(state, slug, request, move |body| {
        Ok(DirectoryOperationHttpCommand::Configure {
            input: body,
            idempotency_key: key,
        })
    })
    .await
}

async fn preview(
    State(state): State<Arc<DirectoryOperationHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    execute(state, slug, request, |body| {
        Ok(DirectoryOperationHttpCommand::Preview { change: body })
    })
    .await
}

async fn submit(
    State(state): State<Arc<DirectoryOperationHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let key = match strict_key(request.headers()) {
        Ok(key) => key,
        Err(error) => return error_response(error),
    };
    execute(state, slug, request, move |body| {
        let token = string(&body, "previewToken")?;
        let mut change = body;
        if let Some(object) = change.as_object_mut() {
            object.remove("previewToken");
        }
        Ok(DirectoryOperationHttpCommand::Submit {
            change,
            token,
            direct: false,
            idempotency_key: key,
        })
    })
    .await
}

async fn apply(
    State(state): State<Arc<DirectoryOperationHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let key = match strict_key(request.headers()) {
        Ok(key) => key,
        Err(error) => return error_response(error),
    };
    execute(state, slug, request, move |body| {
        let token = string(&body, "previewToken")?;
        let mut change = body;
        if let Some(object) = change.as_object_mut() {
            object.remove("previewToken");
        }
        Ok(DirectoryOperationHttpCommand::Submit {
            change,
            token,
            direct: true,
            idempotency_key: key,
        })
    })
    .await
}

async fn apply_review(
    State(state): State<Arc<DirectoryOperationHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let key = match strict_key(request.headers()) {
        Ok(key) => key,
        Err(error) => return error_response(error),
    };
    execute(state, slug, request, move |body| {
        let decision = body
            .get("decision")
            .and_then(Value::as_str)
            .unwrap_or("approve")
            .to_owned();
        let mut payload = body;
        if let Some(object) = payload.as_object_mut() {
            object.remove("decision");
        }
        Ok(DirectoryOperationHttpCommand::ApplyReview {
            payload,
            decision,
            idempotency_key: key,
        })
    })
    .await
}

async fn retry(
    State(state): State<Arc<DirectoryOperationHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let key = match strict_key(request.headers()) {
        Ok(key) => key,
        Err(error) => return error_response(error),
    };
    execute(state, slug, request, move |body| {
        Ok(DirectoryOperationHttpCommand::Retry {
            id: string(&body, "id")?,
            expected_version: integer(&body, "expectedVersion")?,
            idempotency_key: key,
        })
    })
    .await
}

async fn cancel(
    State(state): State<Arc<DirectoryOperationHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let key = match strict_key(request.headers()) {
        Ok(key) => key,
        Err(error) => return error_response(error),
    };
    execute(state, slug, request, move |body| {
        Ok(DirectoryOperationHttpCommand::Cancel {
            id: string(&body, "id")?,
            expected_version: integer(&body, "expectedVersion")?,
            idempotency_key: key,
        })
    })
    .await
}

/// Parse the reviewed change at the owner boundary so a transport typo never
/// reaches the application transaction.
pub fn parse_change(value: Value) -> Result<DirectoryChangeInput, BusinessError> {
    serde_json::from_value(value).map_err(|_| request_invalid())
}

pub const _MANAGE_PERMISSION: &str = MANAGE_PERMISSION;
