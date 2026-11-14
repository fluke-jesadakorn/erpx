//! Native Operations HTTP ingress for the queue reads and Operations summary.
//!
//! Transport facts only: registered Host/Holding target, optional record id,
//! bounded response and business-error mapping. Normal Account session
//! resolution, selected Company/branch/department scope, Records source
//! visibility, People labels and the serializable owner unit remain
//! composition responsibilities behind [`OperationsHttpBackend`].
use crate::domain::operations_commands::{
    ChangeExceptionCommand, DraftPolicyCommand, OpenExceptionCommand, ReviewPolicyCommand,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const QUERY_LIMIT: usize = 2 * 1024;
const BODY_LIMIT: usize = 64 * 1024;

pub type OperationsHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum OperationsHttpReadKind {
    Workspace,
    Detail(String),
}

pub struct OperationsHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: OperationsHttpReadKind,
}

#[derive(Clone, Debug)]
pub enum OperationsHttpCommand {
    OpenException(OpenExceptionCommand),
    ChangeException(ChangeExceptionCommand),
    DraftPolicy(DraftPolicyCommand),
    ReviewPolicy(ReviewPolicyCommand),
}

pub struct OperationsHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: CommandKey,
    pub command: OperationsHttpCommand,
}

#[derive(Clone, Debug)]
pub struct OperationsHttpResponse {
    /// Owner read data. The backend adds the validated current context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}
impl OperationsHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope and owner-unit adapter. The router
/// never mints a session, actor, Company, permission or source visibility.
pub trait OperationsHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: OperationsHttpReadRequest<'a>,
    ) -> OperationsHttpFuture<'a, OperationsHttpResponse>;
    fn command<'a>(
        &'a self,
        request: OperationsHttpCommandRequest<'a>,
    ) -> OperationsHttpFuture<'a, OperationsHttpResponse>;
}

pub struct OperationsHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn OperationsHttpBackend>,
}

struct OperationsHttpState {
    targets: BTreeMap<(String, String), OperationsHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_REQUEST_INVALID",
        "Use one Operations queue read.",
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
                (1..=5).contains(&port.len()) && port.bytes().all(|byte| byte.is_ascii_digit())
            })
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
fn target<'a>(
    state: &'a OperationsHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a OperationsHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn query_id(request: &Request) -> Result<Option<String>, BusinessError> {
    let Some(query) = request.uri().query() else {
        return Ok(None);
    };
    if query.len() > QUERY_LIMIT {
        return Err(malformed());
    }
    let mut id = None;
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name == "id" {
            if id.is_some() {
                return Err(malformed());
            }
            let decoded = percent_decode(value)?;
            id = Some(RecordId::new(decoded)?.as_str().to_owned());
        }
    }
    Ok(id)
}
fn percent_decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied().and_then(hex);
            let low = bytes.get(index + 2).copied().and_then(hex);
            let (Some(high), Some(low)) = (high, low) else {
                return Err(malformed());
            };
            out.push(high * 16 + low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| malformed())
}
fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

pub fn operations_router(targets: Vec<OperationsHttpTarget>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "OPERATIONS_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/operations",
            get(read_operations).post(command_operations),
        )
        .with_state(Arc::new(OperationsHttpState {
            targets: configured,
        })))
}

fn command_error(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}
fn command_origin(target: &OperationsHttpTarget, headers: &HeaderMap) -> Result<(), BusinessError> {
    if unique_header(headers, header::ORIGIN)? != Some(target.origin.as_str()) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        ));
    }
    Ok(())
}
fn command_key(headers: &HeaderMap) -> Result<CommandKey, BusinessError> {
    let key = unique_header(headers, HeaderName::from_static("idempotency-key"))?
        .ok_or_else(|| command_error("COMMAND_KEY_REQUIRED", "Idempotency-Key is required."))?;
    CommandKey::new(key)
}
fn command_content_type(headers: &HeaderMap) -> Result<(), BusinessError> {
    if !unique_header(headers, header::CONTENT_TYPE)?.is_some_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
    }) {
        return Err(command_error(
            "OPERATIONS_REQUEST_INVALID",
            "Use a JSON Operations command body.",
        ));
    }
    Ok(())
}
async fn command_body(request: Request) -> Result<serde_json::Value, BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| command_error("BODY_TOO_LARGE", "Use a smaller Operations request."))?;
    if bytes.is_empty() {
        return Err(command_error(
            "OPERATIONS_REQUEST_INVALID",
            "Use a JSON Operations command body.",
        ));
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
        command_error(
            "OPERATIONS_REQUEST_INVALID",
            "Use a valid JSON Operations command body.",
        )
    })?;
    if !value.is_object() {
        return Err(command_error(
            "OPERATIONS_REQUEST_INVALID",
            "Use a JSON Operations command object.",
        ));
    }
    Ok(value)
}
fn decode_command(mut value: serde_json::Value) -> Result<OperationsHttpCommand, BusinessError> {
    let object = value.as_object_mut().ok_or_else(|| {
        command_error(
            "OPERATIONS_REQUEST_INVALID",
            "Use a JSON Operations command object.",
        )
    })?;
    let entity = object
        .remove("entity")
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| {
            command_error(
                "OPERATIONS_ENTITY_REQUIRED",
                "Choose a policy or exception command.",
            )
        })?;
    let id = object
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let value = serde_json::Value::Object(object.clone());
    match (entity.as_str(), id) {
        ("exception", None) => serde_json::from_value::<OpenExceptionCommand>(value)
            .map(OperationsHttpCommand::OpenException)
            .map_err(|_| {
                command_error(
                    "OPERATIONS_INPUT_INVALID",
                    "Review the exception opening request.",
                )
            }),
        ("exception", Some(_)) => serde_json::from_value::<ChangeExceptionCommand>(value)
            .map(OperationsHttpCommand::ChangeException)
            .map_err(|_| {
                command_error(
                    "OPERATIONS_INPUT_INVALID",
                    "Review the exception decision request.",
                )
            }),
        ("policy", None) => serde_json::from_value::<DraftPolicyCommand>(value)
            .map(OperationsHttpCommand::DraftPolicy)
            .map_err(|_| {
                command_error(
                    "OPERATIONS_INPUT_INVALID",
                    "Review the queue policy draft request.",
                )
            }),
        ("policy", Some(_)) => serde_json::from_value::<ReviewPolicyCommand>(value)
            .map(OperationsHttpCommand::ReviewPolicy)
            .map_err(|_| {
                command_error(
                    "OPERATIONS_INPUT_INVALID",
                    "Review the queue policy decision request.",
                )
            }),
        _ => Err(command_error(
            "OPERATIONS_ENTITY_REQUIRED",
            "Choose a policy or exception command.",
        )),
    }
}

async fn command_operations(
    State(state): State<Arc<OperationsHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        if request.uri().query().is_some_and(|query| !query.is_empty()) {
            return Err(command_error(
                "OPERATIONS_REQUEST_INVALID",
                "Use one Operations command path.",
            ));
        }
        command_origin(target, request.headers())?;
        let key = command_key(request.headers())?;
        command_content_type(request.headers())?;
        let headers = request.headers().clone();
        let body = command_body(request).await?;
        let command = decode_command(body)?;
        target
            .backend
            .command(OperationsHttpCommandRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                key,
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

async fn read_operations(
    State(state): State<Arc<OperationsHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let kind = match query_id(&request)? {
            Some(id) => OperationsHttpReadKind::Detail(id),
            None => OperationsHttpReadKind::Workspace,
        };
        let headers = request.headers().clone();
        let _ = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| malformed())?;
        target
            .backend
            .read(OperationsHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}
