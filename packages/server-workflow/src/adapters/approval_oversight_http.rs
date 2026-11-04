//! Owner-local Workflow approval oversight HTTP ingress.
//!
//! Transport facts only: exact registered Host/Holding targets, one queue read,
//! one record evidence read, bounded JSON/query/body rules and BusinessError
//! status mapping. Account session resolution, selected Company authority and
//! the serializable owner unit stay behind [`ApprovalOversightHttpBackend`].
use crate::domain::approval_oversight::{ApprovalEvidenceQuery, request_invalid};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 0;

pub type ApprovalOversightHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum ApprovalOversightHttpCommand {
    Queue,
    Evidence { query: ApprovalEvidenceQuery },
}

pub struct ApprovalOversightHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: ApprovalOversightHttpCommand,
}

#[derive(Clone, Debug)]
pub struct ApprovalOversightHttpResponse {
    pub data: Value,
    pub context: Value,
}

impl ApprovalOversightHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, Company scope and owner-unit adapter. The
/// router never mints a session, actor, Company, permission or role.
pub trait ApprovalOversightHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: ApprovalOversightHttpRequest<'a>,
    ) -> ApprovalOversightHttpFuture<'a, ApprovalOversightHttpResponse>;
}

pub struct ApprovalOversightHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn ApprovalOversightHttpBackend>,
}

struct ApprovalOversightHttpState {
    targets: BTreeMap<(String, String), ApprovalOversightHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    request_invalid()
}
fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "This browser origin is not registered for the Holding.",
    )
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "APPROVAL_EVIDENCE_CONFIGURATION_INVALID",
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
    state: &'a ApprovalOversightHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a ApprovalOversightHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn ensure_origin(
    target: &ApprovalOversightHttpTarget,
    headers: &HeaderMap,
) -> Result<(), BusinessError> {
    if let Some(origin) = unique_header(headers, header::ORIGIN)?
        && origin != target.origin
    {
        return Err(origin_forbidden());
    }
    Ok(())
}
fn ensure_no_query(request: &Request) -> Result<(), BusinessError> {
    match request.uri().query() {
        None | Some("") => Ok(()),
        Some(_) => Err(malformed()),
    }
}
async fn ensure_empty_body(request: Request) -> Result<(), BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| malformed())?;
    if bytes.is_empty() {
        Ok(())
    } else {
        Err(malformed())
    }
}

pub fn approval_oversight_http_router(
    targets: Vec<ApprovalOversightHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/workflow/approvals",
            get(read_queue),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/workflow/approvals/{kind}/{record_id}/evidence",
            get(read_evidence),
        )
        .with_state(Arc::new(ApprovalOversightHttpState {
            targets: configured,
        })))
}

async fn read_queue(
    State(state): State<Arc<ApprovalOversightHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_origin(target, request.headers())?;
        ensure_no_query(&request)?;
        let headers = request.headers().clone();
        ensure_empty_body(request).await?;
        target
            .backend
            .execute(ApprovalOversightHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: ApprovalOversightHttpCommand::Queue,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn read_evidence(
    State(state): State<Arc<ApprovalOversightHttpState>>,
    Path((slug, kind, record_id)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_origin(target, request.headers())?;
        ensure_no_query(&request)?;
        let query = ApprovalEvidenceQuery::new(&kind, &record_id)?;
        let headers = request.headers().clone();
        ensure_empty_body(request).await?;
        target
            .backend
            .execute(ApprovalOversightHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: ApprovalOversightHttpCommand::Evidence { query },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}
