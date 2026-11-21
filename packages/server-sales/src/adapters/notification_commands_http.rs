//! Owner-local HTTP ingress for native Sales customer-notification commands.
use crate::domain::notifications::{
    QueueNotification, RetryNotification, SettingsAdapter, UpdateSettings, request_invalid,
    source_changed,
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

pub type NotificationHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum NotificationHttpCommand {
    Settings { input: UpdateSettings },
    Queue { fulfillment_id: String, input: QueueNotification },
    Retry { notification_id: String, input: RetryNotification },
}

pub struct NotificationHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub idempotency_key: String,
    pub command: NotificationHttpCommand,
}

#[derive(Clone, Debug)]
pub struct NotificationHttpResponse {
    pub data: Value,
    pub context: Value,
}

impl NotificationHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({ "data": self.data, "context": self.context })
    }
}

pub trait NotificationHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: NotificationHttpRequest<'a>,
    ) -> NotificationHttpFuture<'a, NotificationHttpResponse>;
}

pub struct NotificationHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn NotificationHttpBackend>,
}

struct NotificationHttpState {
    targets: BTreeMap<(String, String), NotificationHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a NotificationHttpTarget,
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

fn parse_settings(body: &[u8]) -> Result<UpdateSettings, BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let object = value
        .as_object()
        .filter(|object| {
            object.len() == 2
                && object.contains_key("expectedVersion")
                && object.contains_key("adapter")
        })
        .ok_or_else(request_invalid)?;
    UpdateSettings {
        expected_version: integer(object, "expectedVersion", 0, i64::MAX)?,
        adapter: SettingsAdapter::parse(text(object, "adapter")?).ok_or_else(request_invalid)?,
    }
    .validate()
}

fn parse_queue(body: &[u8]) -> Result<QueueNotification, BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let object = value
        .as_object()
        .filter(|object| {
            object.len() == 2
                && object.contains_key("expectedVersion")
                && object.contains_key("note")
        })
        .ok_or_else(request_invalid)?;
    QueueNotification {
        expected_version: integer(object, "expectedVersion", 1, i64::MAX)?,
        note: text(object, "note")?.to_owned(),
    }
    .validate()
}

fn parse_retry(body: &[u8]) -> Result<RetryNotification, BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let object = value
        .as_object()
        .filter(|object| object.len() == 1 && object.contains_key("expectedVersion"))
        .ok_or_else(request_invalid)?;
    RetryNotification {
        expected_version: integer(object, "expectedVersion", 1, i64::MAX)?,
    }
    .validate()
}

fn valid_settings_receipt(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 3
            && object.contains_key("companyId")
            && object.contains_key("adapter")
            && object.contains_key("version")
            && object.get("companyId").and_then(Value::as_str).is_some_and(|v| !v.is_empty())
            && object
                .get("adapter")
                .and_then(Value::as_str)
                .and_then(SettingsAdapter::parse)
                .is_some()
            && object.get("version").and_then(Value::as_i64).is_some_and(|v| v >= 1)
    })
}

fn valid_notice_receipt(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 4
            && object.contains_key("id")
            && object.contains_key("orderId")
            && object.contains_key("status")
            && object.contains_key("version")
            && object.get("id").and_then(Value::as_str).is_some_and(|v| !v.is_empty())
            && object.get("orderId").and_then(Value::as_str).is_some_and(|v| !v.is_empty())
            && object.get("status").and_then(Value::as_str) == Some("pending")
            && object.get("version").and_then(Value::as_i64).is_some_and(|v| v >= 1)
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

impl NotificationHttpState {
    pub fn new(targets: Vec<NotificationHttpTarget>) -> Result<Self, BusinessError> {
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
                    "CUSTOMER_NOTIFICATION_HTTP_CONFIGURATION_INVALID",
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

    async fn execute(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
        command: NotificationHttpCommand,
    ) -> Response {
        match self
            .execute_inner(headers, holding_slug, query, body, command)
            .await
        {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn execute_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
        command: NotificationHttpCommand,
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
        let settings = matches!(command, NotificationHttpCommand::Settings { .. });
        let response = target
            .target
            .backend
            .execute(NotificationHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
                idempotency_key: key,
                command,
            })
            .await?;
        let valid = if settings {
            valid_settings_receipt(&response.data)
        } else {
            valid_notice_receipt(&response.data)
        };
        if !valid || !response.context.is_object() {
            return Err(source_changed());
        }
        Ok(response.value())
    }
}

pub fn sales_notification_commands_router(
    targets: Vec<NotificationHttpTarget>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/customer-notifications/settings",
            post(settings_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/fulfillments/{fulfillment_id}/notify",
            post(queue_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/customer-notifications/{notification_id}/retry",
            post(retry_handler),
        )
        .with_state(Arc::new(NotificationHttpState::new(targets)?)))
}

async fn settings_handler(
    State(state): State<Arc<NotificationHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    let parsed = match parse_settings(&body) {
        Ok(input) => input,
        Err(error) => return error_response(error),
    };
    state
        .execute(
            &parts.headers,
            &slug,
            parts.uri.query(),
            &body,
            NotificationHttpCommand::Settings { input: parsed },
        )
        .await
}

async fn queue_handler(
    State(state): State<Arc<NotificationHttpState>>,
    Path((slug, fulfillment_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    let input = match parse_queue(&body) {
        Ok(input) => input,
        Err(error) => return error_response(error),
    };
    let Ok(fulfillment_id) = RecordId::new(fulfillment_id) else {
        return error_response(request_invalid());
    };
    state
        .execute(
            &parts.headers,
            &slug,
            parts.uri.query(),
            &body,
            NotificationHttpCommand::Queue {
                fulfillment_id: fulfillment_id.as_str().to_owned(),
                input,
            },
        )
        .await
}

async fn retry_handler(
    State(state): State<Arc<NotificationHttpState>>,
    Path((slug, notification_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    let input = match parse_retry(&body) {
        Ok(input) => input,
        Err(error) => return error_response(error),
    };
    let Ok(notification_id) = RecordId::new(notification_id) else {
        return error_response(request_invalid());
    };
    state
        .execute(
            &parts.headers,
            &slug,
            parts.uri.query(),
            &body,
            NotificationHttpCommand::Retry {
                notification_id: notification_id.as_str().to_owned(),
                input,
            },
        )
        .await
}
