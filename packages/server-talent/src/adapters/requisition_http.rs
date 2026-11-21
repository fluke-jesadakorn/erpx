//! Native Talent requisition HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target, one bounded
//! requisition path, one strict JSON command and business-error mapping. Normal
//! Account session resolution, selected Company/branch/department scope,
//! purchase admission and the serializable owner unit remain composition
//! responsibilities behind [`TalentRequisitionHttpBackend`].
use crate::domain::requisition::{request_invalid, requisition_not_found};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 16 * 1024;

pub type TalentRequisitionHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum TalentRequisitionHttpCommand {
    Workspace,
    Detail {
        requisition_id: String,
    },
    Create {
        body: Value,
        idempotency_key: String,
    },
    Change {
        requisition_id: String,
        body: Value,
        idempotency_key: String,
    },
}

pub struct TalentRequisitionHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: TalentRequisitionHttpCommand,
}

#[derive(Clone, Debug)]
pub struct TalentRequisitionHttpResponse {
    /// Owner data. The backend adds the validated current context.
    pub data: Value,
    pub context: Value,
}

impl TalentRequisitionHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope and owner-unit adapter. The router
/// never mints a session, actor, Company, permission or purchase.
pub trait TalentRequisitionHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: TalentRequisitionHttpRequest<'a>,
    ) -> TalentRequisitionHttpFuture<'a, TalentRequisitionHttpResponse>;
}

pub struct TalentRequisitionHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn TalentRequisitionHttpBackend>,
}

struct TalentRequisitionHttpState {
    targets: BTreeMap<(String, String), TalentRequisitionHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    request_invalid()
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "TALENT_REQUISITION_CONFIGURATION_INVALID",
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
fn valid_id(value: &str) -> bool {
    RecordId::new(value.to_owned()).is_ok()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
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
    state: &'a TalentRequisitionHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a TalentRequisitionHttpTarget, BusinessError> {
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
                "Idempotency-Key is required for this Talent requisition command.",
            )
        })?;
    CommandKey::new(value.to_owned())?;
    Ok(value.to_owned())
}

pub fn talent_requisition_http_router(
    targets: Vec<TalentRequisitionHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/talent/requisitions",
            get(read_workspace).post(create_requisition),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/talent/requisitions/{requisition_id}",
            get(read_detail).post(change_requisition),
        )
        .with_state(Arc::new(TalentRequisitionHttpState {
            targets: configured,
        })))
}

async fn read_workspace(
    State(state): State<Arc<TalentRequisitionHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let headers = request.headers().clone();
        ensure_empty_body(request).await?;
        target
            .backend
            .execute(TalentRequisitionHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: TalentRequisitionHttpCommand::Workspace,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn read_detail(
    State(state): State<Arc<TalentRequisitionHttpState>>,
    Path((slug, requisition_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        if !valid_id(&requisition_id) {
            return Err(requisition_not_found());
        }
        let headers = request.headers().clone();
        ensure_empty_body(request).await?;
        target
            .backend
            .execute(TalentRequisitionHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: TalentRequisitionHttpCommand::Detail { requisition_id },
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn create_requisition(
    State(state): State<Arc<TalentRequisitionHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        target
            .backend
            .execute(TalentRequisitionHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: TalentRequisitionHttpCommand::Create {
                    body,
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

async fn change_requisition(
    State(state): State<Arc<TalentRequisitionHttpState>>,
    Path((slug, requisition_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        ensure_no_query(&request)?;
        if !valid_id(&requisition_id) {
            return Err(requisition_not_found());
        }
        let key = strict_key(request.headers())?;
        let headers = request.headers().clone();
        let body = strict_body(request).await?;
        target
            .backend
            .execute(TalentRequisitionHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                command: TalentRequisitionHttpCommand::Change {
                    requisition_id,
                    body,
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
