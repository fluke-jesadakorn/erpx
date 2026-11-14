//! Native Staff Work HTTP ingress for the Workplace `staff-tasks` route.
//!
//! Transport facts only: exact registered Host/Holding target, Origin,
//! Idempotency-Key, bounded JSON body and business-error mapping. Account
//! session resolution, selected Company/branch/department, People/Projects/
//! Collaboration/Files/Workflow ports and the serializable owner unit remain
//! composition responsibilities behind [`StaffWorkHttpBackend`].
use crate::domain::staff_tasks::{
    DelegateTaskCommand, RevokeTaskDelegationCommand, SaveTaskCommand, TransitionTaskCommand,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode,
    body::to_bytes,
    header,
    routing::{get, post},
    serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const QUERY_LIMIT: usize = 2 * 1024;
const BODY_LIMIT: usize = 64 * 1024;

pub type StaffWorkHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum StaffWorkHttpReadKind {
    Workspace { project_id: Option<String> },
    Detail(String),
}

pub struct StaffWorkHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: StaffWorkHttpReadKind,
}

#[derive(Clone, Debug)]
pub enum StaffWorkHttpCommand {
    Save {
        id: Option<String>,
        revision: bool,
        command: SaveTaskCommand,
    },
    Transition {
        id: String,
        command: TransitionTaskCommand,
    },
    Delegate {
        id: String,
        command: DelegateTaskCommand,
    },
    Revoke {
        id: String,
        delegation_id: String,
        command: RevokeTaskDelegationCommand,
    },
}

pub struct StaffWorkHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: CommandKey,
    pub command: StaffWorkHttpCommand,
}

#[derive(Clone, Debug)]
pub struct StaffWorkHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}
impl StaffWorkHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

pub trait StaffWorkHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: StaffWorkHttpReadRequest<'a>,
    ) -> StaffWorkHttpFuture<'a, StaffWorkHttpResponse>;
    fn command<'a>(
        &'a self,
        request: StaffWorkHttpCommandRequest<'a>,
    ) -> StaffWorkHttpFuture<'a, StaffWorkHttpResponse>;
}

pub struct StaffWorkHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn StaffWorkHttpBackend>,
}

struct StaffWorkHttpState {
    targets: BTreeMap<(String, String), StaffWorkHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid("STAFF_TASK_REQUEST_INVALID", "Use one Staff Work request.")
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
    state: &'a StaffWorkHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a StaffWorkHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn query_value(value: &str) -> Result<String, BusinessError> {
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
        } else if bytes[index] == b'+' {
            out.push(b' ');
            index += 1;
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
fn query_project(request: &Request) -> Result<Option<String>, BusinessError> {
    let Some(query) = request.uri().query() else {
        return Ok(None);
    };
    if query.len() > QUERY_LIMIT {
        return Err(malformed());
    }
    let mut project = None;
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name == "projectId" {
            if project.is_some() || value.is_empty() {
                return Err(malformed());
            }
            project = Some(RecordId::new(query_value(value)?)?.as_str().to_owned());
        } else if name == "id" || name == "path" {
            return Err(malformed());
        }
    }
    Ok(project)
}

pub fn staff_work_router(targets: Vec<StaffWorkHttpTarget>) -> Result<Router, BusinessError> {
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
                "STAFF_TASK_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/staff-tasks",
            get(read_workspace).post(command_save),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/staff-tasks/{id}",
            get(read_detail).post(command_save_existing),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/staff-tasks/{id}/revise",
            post(command_revise),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/staff-tasks/{id}/transition",
            post(command_transition),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/staff-tasks/{id}/delegate",
            post(command_delegate),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/staff-tasks/{id}/delegate/revoke/{delegation_id}",
            post(command_revoke),
        )
        .with_state(Arc::new(StaffWorkHttpState {
            targets: configured,
        })))
}
fn command_error(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}
fn command_origin(target: &StaffWorkHttpTarget, headers: &HeaderMap) -> Result<(), BusinessError> {
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
            "STAFF_TASK_REQUEST_INVALID",
            "Use a JSON Staff Work command body.",
        ));
    }
    Ok(())
}
async fn command_body(request: Request) -> Result<serde_json::Value, BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| command_error("BODY_TOO_LARGE", "Use a smaller Staff Work request."))?;
    if bytes.is_empty() {
        return Err(command_error(
            "STAFF_TASK_REQUEST_INVALID",
            "Use a JSON Staff Work command body.",
        ));
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
        command_error(
            "STAFF_TASK_REQUEST_INVALID",
            "Use a valid JSON Staff Work command body.",
        )
    })?;
    if !value.is_object() {
        return Err(command_error(
            "STAFF_TASK_REQUEST_INVALID",
            "Use a JSON Staff Work command object.",
        ));
    }
    Ok(value)
}

fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, BusinessError> {
    serde_json::from_value(value)
        .map_err(|_| command_error("STAFF_TASK_INPUT_INVALID", "Review the Staff Work request."))
}
fn decode_command(
    value: serde_json::Value,
    id: Option<String>,
    action: Option<&str>,
    delegation_id: Option<String>,
) -> Result<StaffWorkHttpCommand, BusinessError> {
    match action {
        None => Ok(StaffWorkHttpCommand::Save {
            id,
            revision: false,
            command: decode(value)?,
        }),
        Some("revise") => Ok(StaffWorkHttpCommand::Save {
            id,
            revision: true,
            command: decode(value)?,
        }),
        Some("transition") => Ok(StaffWorkHttpCommand::Transition {
            id: id.ok_or_else(malformed)?,
            command: decode(value)?,
        }),
        Some("delegate") => Ok(StaffWorkHttpCommand::Delegate {
            id: id.ok_or_else(malformed)?,
            command: decode(value)?,
        }),
        Some("revoke") => {
            let delegation_id = delegation_id.ok_or_else(malformed)?;
            let id = id.ok_or_else(malformed)?;
            let mut value = value;
            let object = value.as_object_mut().ok_or_else(malformed)?;
            object.insert(
                "delegationId".into(),
                serde_json::Value::String(delegation_id.clone()),
            );
            Ok(StaffWorkHttpCommand::Revoke {
                id,
                delegation_id,
                command: decode(value)?,
            })
        }
        _ => Err(malformed()),
    }
}

async fn run_command(
    state: Arc<StaffWorkHttpState>,
    slug: String,
    request: Request,
    id: Option<String>,
    action: Option<&'static str>,
    delegation_id: Option<String>,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        if request.uri().query().is_some_and(|query| !query.is_empty()) {
            return Err(malformed());
        }
        command_origin(target, request.headers())?;
        let key = command_key(request.headers())?;
        command_content_type(request.headers())?;
        let headers = request.headers().clone();
        let body = command_body(request).await?;
        let command = decode_command(body, id, action, delegation_id)?;
        target
            .backend
            .command(StaffWorkHttpCommandRequest {
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

async fn command_revise(
    State(state): State<Arc<StaffWorkHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    run_command(state, slug, request, Some(id), Some("revise"), None).await
}
async fn command_transition(
    State(state): State<Arc<StaffWorkHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    run_command(state, slug, request, Some(id), Some("transition"), None).await
}
async fn command_delegate(
    State(state): State<Arc<StaffWorkHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    run_command(state, slug, request, Some(id), Some("delegate"), None).await
}
async fn command_revoke(
    State(state): State<Arc<StaffWorkHttpState>>,
    Path((slug, id, delegation_id)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    run_command(
        state,
        slug,
        request,
        Some(id),
        Some("revoke"),
        Some(delegation_id),
    )
    .await
}

async fn command_save(
    State(state): State<Arc<StaffWorkHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        if request.uri().query().is_some_and(|query| !query.is_empty()) {
            return Err(malformed());
        }
        command_origin(target, request.headers())?;
        let key = command_key(request.headers())?;
        command_content_type(request.headers())?;
        let headers = request.headers().clone();
        let body = command_body(request).await?;
        let command = decode_command(body, None, None, None)?;
        target
            .backend
            .command(StaffWorkHttpCommandRequest {
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

async fn command_save_existing(
    State(state): State<Arc<StaffWorkHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        if request.uri().query().is_some_and(|query| !query.is_empty()) {
            return Err(malformed());
        }
        command_origin(target, request.headers())?;
        let key = command_key(request.headers())?;
        command_content_type(request.headers())?;
        let headers = request.headers().clone();
        let body = command_body(request).await?;
        let command = decode_command(body, Some(id), None, None)?;
        target
            .backend
            .command(StaffWorkHttpCommandRequest {
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

async fn read_workspace(
    State(state): State<Arc<StaffWorkHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let project_id = query_project(&request)?;
        let headers = request.headers().clone();
        let _ = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| malformed())?;
        target
            .backend
            .read(StaffWorkHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind: StaffWorkHttpReadKind::Workspace { project_id },
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
    State(state): State<Arc<StaffWorkHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        if request.uri().query().is_some_and(|query| !query.is_empty()) {
            return Err(malformed());
        }
        let id = RecordId::new(id)?.as_str().to_owned();
        let headers = request.headers().clone();
        let _ = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| malformed())?;
        target
            .backend
            .read(StaffWorkHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind: StaffWorkHttpReadKind::Detail(id),
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}
