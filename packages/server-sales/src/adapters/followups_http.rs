//! Owner-local HTTP ingress for native Sales customer follow-up commands.
//!
//! Transport facts only: registered Host/Holding target, exact Origin,
//! Idempotency-Key, bounded exact JSON input and business-error mapping. Current
//! Account/Holding authority, purchase capability and the serializable owner
//! unit remain composition responsibilities supplied through the backend trait.
use crate::domain::followups::{
    ChangeFollowup, CreateFollowup, FollowupAction, request_invalid, source_changed,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 16 * 1024;
const COMMAND_KEY_MAX: usize = 200;

pub type FollowupHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum FollowupHttpCommand {
    Create { input: CreateFollowup },
    Change { id: String, input: ChangeFollowup },
}

pub struct FollowupHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub idempotency_key: String,
    pub command: FollowupHttpCommand,
}

#[derive(Clone, Debug)]
pub struct FollowupHttpResponse {
    /// Exact `{id,version,status}` receipt.
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

impl FollowupHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({ "data": self.data, "context": self.context })
    }
}

pub trait FollowupHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: FollowupHttpRequest<'a>,
    ) -> FollowupHttpFuture<'a, FollowupHttpResponse>;
}

pub struct FollowupHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn FollowupHttpBackend>,
}

struct FollowupHttpState {
    targets: BTreeMap<(String, String), FollowupHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a FollowupHttpTarget,
    host: &'a str,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn command_key_required() -> BusinessError {
    BusinessError::invalid(
        "COMMAND_KEY_REQUIRED",
        "Use a valid Idempotency-Key for this action.",
    )
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
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

fn text<'a>(object: &'a serde_json::Map<String, Value>, key: &str) -> Result<&'a str, BusinessError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(request_invalid)
}

fn integer(
    object: &serde_json::Map<String, Value>,
    key: &str,
    min: i64,
    max: i64,
) -> Result<i64, BusinessError> {
    object
        .get(key)
        .and_then(Value::as_i64)
        .filter(|value| (*value >= min) && (*value <= max))
        .ok_or_else(request_invalid)
}

fn parse_create(body: &[u8]) -> Result<CreateFollowup, BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let object = value
        .as_object()
        .filter(|object| {
            object.len() == 5
                && ["orderId", "expectedVersion", "ownerId", "dueDate", "nextAction"]
                    .iter()
                    .all(|key| object.contains_key(*key))
        })
        .ok_or_else(request_invalid)?;
    CreateFollowup {
        order_id: text(object, "orderId")?.to_owned(),
        expected_version: integer(object, "expectedVersion", 1, i64::MAX)?,
        owner_id: text(object, "ownerId")?.to_owned(),
        due_date: text(object, "dueDate")?.to_owned(),
        next_action: text(object, "nextAction")?.to_owned(),
    }
    .validate()
}

fn parse_change(body: &[u8]) -> Result<ChangeFollowup, BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let object = value.as_object().ok_or_else(request_invalid)?;
    let action = FollowupAction::parse(text(object, "action")?).ok_or_else(request_invalid)?;
    let expected_keys = if action == FollowupAction::Reassign {
        5
    } else {
        3
    };
    if object.len() != expected_keys
        || !object.contains_key("expectedVersion")
        || !object.contains_key("reason")
    {
        return Err(request_invalid());
    }
    ChangeFollowup {
        action,
        expected_version: integer(object, "expectedVersion", 1, i64::MAX)?,
        owner_id: object
            .get("ownerId")
            .map(|value| value.as_str().map(str::to_owned).ok_or_else(request_invalid))
            .transpose()?,
        due_date: object
            .get("dueDate")
            .map(|value| value.as_str().map(str::to_owned).ok_or_else(request_invalid))
            .transpose()?,
        reason: text(object, "reason")?.to_owned(),
    }
    .validate()
}

fn valid_receipt(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 3
            && ["id", "version", "status"]
                .iter()
                .all(|key| object.contains_key(*key))
            && object
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty() && value.len() <= 200)
            && object
                .get("version")
                .and_then(Value::as_i64)
                .is_some_and(|value| value >= 1)
            && object
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|value| matches!(value, "open" | "completed" | "cancelled"))
    })
}

fn status_code(kind: FailureKind) -> StatusCode {
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
        status_code(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(value: Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
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

impl FollowupHttpState {
    pub fn new(targets: Vec<FollowupHttpTarget>) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || target.holding_slug.is_empty()
                || target.holding_slug.len() > 128
                || !target.holding_slug.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
                || !valid_origin(&target.origin)
                || configured
                    .insert((target.host.clone(), target.holding_slug.clone()), target)
                    .is_some()
            {
                return Err(BusinessError::invalid(
                    "CUSTOMER_FOLLOWUP_HTTP_CONFIGURATION_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
        }
        Ok(Self {
            targets: configured,
        })
    }

    fn target<'a>(
        &'a self,
        headers: &'a HeaderMap,
        holding_slug: &str,
    ) -> Result<ParsedTarget<'a>, BusinessError> {
        let host = unique_header(headers, "host")?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let target = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        let origin = unique_header(headers, "origin")?;
        if origin.is_some_and(|value| value != target.origin) {
            return Err(origin_forbidden());
        }
        Ok(ParsedTarget { target, host })
    }

    fn content_type(headers: &HeaderMap) -> Result<(), BusinessError> {
        let content_type = unique_header(headers, "content-type")?
            .ok_or_else(request_invalid)?
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if content_type != "application/json" {
            return Err(request_invalid());
        }
        Ok(())
    }

    fn command_key(headers: &HeaderMap) -> Result<String, BusinessError> {
        let raw = unique_header(headers, "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(command_key_required)?;
        let key = CommandKey::new(raw)?;
        if key.as_str().len() > COMMAND_KEY_MAX {
            return Err(request_invalid());
        }
        Ok(key.as_str().to_owned())
    }

    pub async fn create(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self.create_inner(headers, holding_slug, query, body).await {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn create_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Value, BusinessError> {
        if query.is_some_and(|value| !value.is_empty()) {
            return Err(request_invalid());
        }
        let target = self.target(headers, holding_slug)?;
        Self::content_type(headers)?;
        let key = Self::command_key(headers)?;
        if body.len() > BODY_LIMIT {
            return Err(request_invalid());
        }
        let input = parse_create(body)?;
        let response = target
            .target
            .backend
            .execute(FollowupHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
                idempotency_key: key,
                command: FollowupHttpCommand::Create { input },
            })
            .await?;
        if !valid_receipt(&response.data) || !response.context.is_object() {
            return Err(source_changed());
        }
        Ok(response.value())
    }

    pub async fn change(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        id: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .change_inner(headers, holding_slug, id, query, body)
            .await
        {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn change_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        id: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Value, BusinessError> {
        if query.is_some_and(|value| !value.is_empty()) {
            return Err(request_invalid());
        }
        let target = self.target(headers, holding_slug)?;
        Self::content_type(headers)?;
        let key = Self::command_key(headers)?;
        let id = RecordId::new(id)
            .map(|value| value.as_str().to_owned())
            .map_err(|_| request_invalid())?;
        if body.len() > BODY_LIMIT {
            return Err(request_invalid());
        }
        let input = parse_change(body)?;
        let response = target
            .target
            .backend
            .execute(FollowupHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
                idempotency_key: key,
                command: FollowupHttpCommand::Change { id, input },
            })
            .await?;
        if !valid_receipt(&response.data) || !response.context.is_object() {
            return Err(source_changed());
        }
        Ok(response.value())
    }
}

pub fn customer_followups_router(targets: Vec<FollowupHttpTarget>) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/customer-followups",
            post(create_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/customer-followups/{followup_id}",
            post(change_handler),
        )
        .with_state(Arc::new(FollowupHttpState::new(targets)?)))
}

async fn create_handler(
    State(state): State<Arc<FollowupHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .create(&parts.headers, &slug, parts.uri.query(), &body)
        .await
}

async fn change_handler(
    State(state): State<Arc<FollowupHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .change(&parts.headers, &slug, &id, parts.uri.query(), &body)
        .await
}
