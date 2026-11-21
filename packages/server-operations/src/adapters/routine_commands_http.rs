//! Native routine template/schedule HTTP ingress for the Workplace
//! `routine-templates` and `routine-schedules` routes.
//!
//! Transport facts only: exact registered Host/Holding target, Origin,
//! Idempotency-Key, bounded JSON body and business-error mapping. Account
//! session resolution, selected Company/branch/department, People/Projects/
//! Workflow ports, purchase boundaries and the serializable owner unit remain
//! composition responsibilities behind [`RoutineHttpBackend`].
use crate::domain::routine_commands::{
    RunRoutineTemplateCommand, SaveRoutineScheduleCommand, SaveRoutineTemplateCommand,
    ScheduleStateCommand,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode,
    body::to_bytes,
    header,
    routing::post,
    serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type RoutineHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum RoutineHttpCommand {
    SaveTemplate {
        id: Option<String>,
        clone_id: Option<String>,
        command: SaveRoutineTemplateCommand,
    },
    SaveSchedule {
        template_id: String,
        id: Option<String>,
        command: SaveRoutineScheduleCommand,
    },
    ChangeScheduleState {
        id: String,
        resume: bool,
        command: ScheduleStateCommand,
    },
    EndSchedule {
        id: String,
        command: ScheduleStateCommand,
    },
    RunTemplate {
        template_id: String,
        due_at: String,
    },
}

pub struct RoutineHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: CommandKey,
    pub command: RoutineHttpCommand,
}

#[derive(Clone, Debug)]
pub struct RoutineHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}
impl RoutineHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

pub trait RoutineHttpBackend: Send + Sync {
    fn command<'a>(
        &'a self,
        request: RoutineHttpCommandRequest<'a>,
    ) -> RoutineHttpFuture<'a, RoutineHttpResponse>;
}

pub struct RoutineHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn RoutineHttpBackend>,
}

struct RoutineHttpState {
    targets: BTreeMap<(String, String), RoutineHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "ROUTINE_REQUEST_INVALID",
        "Use one routine request.",
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
    state: &'a RoutineHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a RoutineHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

pub fn routine_commands_router(targets: Vec<RoutineHttpTarget>) -> Result<Router, BusinessError> {
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
                "ROUTINE_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/routine-templates",
            post(command_template_create),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/routine-templates/{id}",
            post(command_template_update),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/routine-templates/{id}/clone",
            post(command_template_clone),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/routine-templates/{template_id}/run",
            post(command_template_run),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/routine-templates/{template_id}/schedules",
            post(command_schedule_create),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/routine-templates/{template_id}/schedules/{id}",
            post(command_schedule_update),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/routine-schedules/{id}/pause",
            post(command_schedule_pause),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/routine-schedules/{id}/resume",
            post(command_schedule_resume),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/routine-schedules/{id}/end",
            post(command_schedule_end),
        )
        .with_state(Arc::new(RoutineHttpState {
            targets: configured,
        })))
}
fn command_error(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}
fn command_origin(target: &RoutineHttpTarget, headers: &HeaderMap) -> Result<(), BusinessError> {
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
        .ok_or_else(|| {
            command_error(
                "COMMAND_KEY_REQUIRED",
                "Idempotency-Key is required.",
            )
        })?;
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
            "ROUTINE_REQUEST_INVALID",
            "Use a JSON routine command body.",
        ));
    }
    Ok(())
}
async fn command_body(request: Request) -> Result<serde_json::Value, BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| command_error("BODY_TOO_LARGE", "Use a smaller routine request."))?;
    if bytes.is_empty() {
        return Err(command_error(
            "ROUTINE_REQUEST_INVALID",
            "Use a JSON routine command body.",
        ));
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
        command_error(
            "ROUTINE_REQUEST_INVALID",
            "Use a valid JSON routine command body.",
        )
    })?;
    if !value.is_object() {
        return Err(command_error(
            "ROUTINE_REQUEST_INVALID",
            "Use a JSON routine command object.",
        ));
    }
    Ok(value)
}

fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, BusinessError> {
    serde_json::from_value(value)
        .map_err(|_| command_error("ROUTINE_INPUT_INVALID", "Review the routine request."))
}

/// Route shape to command decode. `action` is the final path segment for the
/// schedule state routes; `clone` distinguishes a template clone.
fn decode_command(
    body: serde_json::Value,
    template_id: Option<String>,
    id: Option<String>,
    action: Option<&'static str>,
) -> Result<RoutineHttpCommand, BusinessError> {
    match action {
        None => match (template_id, id) {
            (None, None) => Ok(RoutineHttpCommand::SaveTemplate {
                id: None,
                clone_id: None,
                command: decode(body)?,
            }),
            (None, Some(id)) => Ok(RoutineHttpCommand::SaveTemplate {
                id: Some(id),
                clone_id: None,
                command: decode(body)?,
            }),
            (Some(template_id), None) => Ok(RoutineHttpCommand::SaveSchedule {
                template_id,
                id: None,
                command: decode(body)?,
            }),
            (Some(template_id), Some(id)) => Ok(RoutineHttpCommand::SaveSchedule {
                template_id,
                id: Some(id),
                command: decode(body)?,
            }),
        },
        Some("clone") => Ok(RoutineHttpCommand::SaveTemplate {
            id: None,
            clone_id: Some(id.ok_or_else(malformed)?),
            command: decode(body)?,
        }),
        Some("pause") => Ok(RoutineHttpCommand::ChangeScheduleState {
            id: id.ok_or_else(malformed)?,
            resume: false,
            command: decode(body)?,
        }),
        Some("resume") => Ok(RoutineHttpCommand::ChangeScheduleState {
            id: id.ok_or_else(malformed)?,
            resume: true,
            command: decode(body)?,
        }),
        Some("end") => Ok(RoutineHttpCommand::EndSchedule {
            id: id.ok_or_else(malformed)?,
            command: decode(body)?,
        }),
        Some("run") => {
            let command: RunRoutineTemplateCommand = decode(body)?;
            Ok(RoutineHttpCommand::RunTemplate {
                template_id: template_id.ok_or_else(malformed)?,
                due_at: command.due_at,
            })
        }
        _ => Err(malformed()),
    }
}

async fn run(
    state: Arc<RoutineHttpState>,
    slug: String,
    request: Request,
    template_id: Option<String>,
    id: Option<String>,
    action: Option<&'static str>,
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
        let command = decode_command(body, template_id, id, action)?;
        target
            .backend
            .command(RoutineHttpCommandRequest {
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

async fn command_template_create(
    State(state): State<Arc<RoutineHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    run(state, slug, request, None, None, None).await
}
async fn command_template_update(
    State(state): State<Arc<RoutineHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let id = RecordId::new(id).map(|id| id.as_str().to_owned());
    match id {
        Ok(id) => run(state, slug, request, None, Some(id), None).await,
        Err(error) => error_response(error),
    }
}
async fn command_template_clone(
    State(state): State<Arc<RoutineHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let id = RecordId::new(id).map(|id| id.as_str().to_owned());
    match id {
        Ok(id) => run(state, slug, request, None, Some(id), Some("clone")).await,
        Err(error) => error_response(error),
    }
}
async fn command_template_run(
    State(state): State<Arc<RoutineHttpState>>,
    Path((slug, template_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let template_id = RecordId::new(template_id).map(|id| id.as_str().to_owned());
    match template_id {
        Ok(template_id) => run(state, slug, request, Some(template_id), None, Some("run")).await,
        Err(error) => error_response(error),
    }
}
async fn command_schedule_create(
    State(state): State<Arc<RoutineHttpState>>,
    Path((slug, template_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let template_id = RecordId::new(template_id).map(|id| id.as_str().to_owned());
    match template_id {
        Ok(template_id) => run(state, slug, request, Some(template_id), None, None).await,
        Err(error) => error_response(error),
    }
}
async fn command_schedule_update(
    State(state): State<Arc<RoutineHttpState>>,
    Path((slug, template_id, id)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let template_id = RecordId::new(template_id).map(|id| id.as_str().to_owned());
    let id = RecordId::new(id).map(|id| id.as_str().to_owned());
    match (template_id, id) {
        (Ok(template_id), Ok(id)) => {
            run(state, slug, request, Some(template_id), Some(id), None).await
        }
        (Err(error), _) | (_, Err(error)) => error_response(error),
    }
}
async fn command_schedule_pause(
    State(state): State<Arc<RoutineHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let id = RecordId::new(id).map(|id| id.as_str().to_owned());
    match id {
        Ok(id) => run(state, slug, request, None, Some(id), Some("pause")).await,
        Err(error) => error_response(error),
    }
}
async fn command_schedule_resume(
    State(state): State<Arc<RoutineHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let id = RecordId::new(id).map(|id| id.as_str().to_owned());
    match id {
        Ok(id) => run(state, slug, request, None, Some(id), Some("resume")).await,
        Err(error) => error_response(error),
    }
}
async fn command_schedule_end(
    State(state): State<Arc<RoutineHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let id = RecordId::new(id).map(|id| id.as_str().to_owned());
    match id {
        Ok(id) => run(state, slug, request, None, Some(id), Some("end")).await,
        Err(error) => error_response(error),
    }
}
