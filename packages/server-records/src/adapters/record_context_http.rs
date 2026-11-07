//! Native Records context HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target, one bounded record
//! path and owner error mapping. Normal Account session resolution, selected
//! Company/branch/department scope, current IAM authority, cross-owner record
//! admission and the serializable owner unit remain composition responsibilities
//! behind [`RecordsContextHttpBackend`].
use crate::domain::record_context::{request_invalid, validate_request};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

pub type RecordsContextHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum RecordsContextHttpCommand {
    Context {
        module_id: String,
        record_id: String,
    },
}

pub struct RecordsContextHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: RecordsContextHttpCommand,
}

#[derive(Clone, Debug)]
pub struct RecordsContextHttpResponse {
    /// Owner context data. The backend adds the validated current context.
    pub data: Value,
    pub context: Value,
}

impl RecordsContextHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope and owner-unit adapter. The router
/// never mints a session, actor, Company, module or permission.
pub trait RecordsContextHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: RecordsContextHttpRequest<'a>,
    ) -> RecordsContextHttpFuture<'a, RecordsContextHttpResponse>;
}

pub struct RecordsContextHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn RecordsContextHttpBackend>,
}

struct RecordsContextHttpState {
    targets: BTreeMap<(String, String), RecordsContextHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    request_invalid()
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "RECORDS_CONTEXT_CONFIGURATION_INVALID",
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
        !rest.is_empty()
            && !rest
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
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
    state: &'a RecordsContextHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a RecordsContextHttpTarget, BusinessError> {
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
async fn ensure_empty_body(request: Request) -> Result<(), BusinessError> {
    let bytes = to_bytes(request.into_body(), 0)
        .await
        .map_err(|_| malformed())?;
    if bytes.is_empty() {
        Ok(())
    } else {
        Err(malformed())
    }
}

pub fn records_context_http_router(
    targets: Vec<RecordsContextHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}",
            get(read_context),
        )
        .with_state(Arc::new(RecordsContextHttpState {
            targets: configured,
        })))
}

async fn read_context(
    State(state): State<Arc<RecordsContextHttpState>>,
    Path((slug, module_id, record_id)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        validate_request(&module_id, &record_id).map_err(|_| missing())?;
        let headers = request.headers().clone();
        ensure_empty_body(request).await?;
        target
            .backend
            .execute(RecordsContextHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: RecordsContextHttpCommand::Context {
                    module_id,
                    record_id,
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
