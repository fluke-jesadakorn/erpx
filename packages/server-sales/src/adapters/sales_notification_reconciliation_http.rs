//! Owner-local HTTP ingress for native Sales customer-notice reconciliation.
//!
//! Transport facts only: registered Host/Holding target, exact Origin,
//! Idempotency-Key, bounded exact JSON input and business-error mapping. The
//! normal Account session, current Company approver authority, purchase
//! settlement and serializable owner unit remain composition responsibilities
//! supplied through the backend trait.
use crate::domain::sales_notification_reconciliation::{ReconcileInput, request_invalid, source_changed};
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

pub type ReconcileHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct ReconcileHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub notification_id: String,
    pub idempotency_key: String,
    pub input: ReconcileInput,
}

#[derive(Clone, Debug)]
pub struct ReconcileHttpResponse {
    /// Exact `{id,orderId,status,version,reconciliationId}` receipt.
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

impl ReconcileHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({ "data": self.data, "context": self.context })
    }
}

/// Composition-owned Account session, scope and owner-unit adapter. The router
/// never mints a session, actor, Company scope, permission or purchase.
pub trait ReconcileHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: ReconcileHttpRequest<'a>,
    ) -> ReconcileHttpFuture<'a, ReconcileHttpResponse>;
}

pub struct ReconcileHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn ReconcileHttpBackend>,
}

pub struct ReconcileHttpState {
    targets: BTreeMap<(String, String), ReconcileHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a ReconcileHttpTarget,
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

fn integer(object: &serde_json::Map<String, Value>, key: &str, min: i64, max: i64) -> Result<i64, BusinessError> {
    let value = object
        .get(key)
        .and_then(Value::as_i64)
        .filter(|value| (*value >= min) && (*value <= max))
        .ok_or_else(request_invalid)?;
    Ok(value)
}

fn parse_input(body: &[u8]) -> Result<ReconcileInput, BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let object = value
        .as_object()
        .filter(|object| {
            object.len() == 3
                && object.contains_key("expectedVersion")
                && object.contains_key("attemptId")
                && object.contains_key("reason")
        })
        .ok_or_else(request_invalid)?;
    ReconcileInput {
        expected_version: integer(object, "expectedVersion", 1, i64::MAX)?,
        attempt_id: text(object, "attemptId")?.to_owned(),
        reason: text(object, "reason")?.to_owned(),
    }
    .validate()
}

fn valid_receipt(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 5
            && ["id", "orderId", "status", "version", "reconciliationId"]
                .iter()
                .all(|key| object.contains_key(*key))
            && object
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty() && value.len() <= 200)
            && object
                .get("orderId")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty() && value.len() <= 200)
            && object
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|value| matches!(value, "sent" | "cancelled"))
            && object
                .get("version")
                .and_then(Value::as_i64)
                .is_some_and(|value| value >= 2)
            && object
                .get("reconciliationId")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty() && value.len() <= 200)
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

impl ReconcileHttpState {
    pub fn new(targets: Vec<ReconcileHttpTarget>) -> Result<Self, BusinessError> {
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
                    "SALES_NOTIFICATION_RECONCILIATION_HTTP_CONFIGURATION_INVALID",
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

    fn notification_id(value: &str) -> Result<String, BusinessError> {
        RecordId::new(value)
            .map(|id| id.as_str().to_owned())
            .map_err(|_| request_invalid())
    }

    async fn reconcile(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        notification_id: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .reconcile_inner(headers, holding_slug, notification_id, query, body)
            .await
        {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn reconcile_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        notification_id: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Value, BusinessError> {
        if query.is_some_and(|value| !value.is_empty()) {
            return Err(request_invalid());
        }
        let target = self.target(headers, holding_slug)?;
        Self::content_type(headers)?;
        let key = Self::command_key(headers)?;
        let notification_id = Self::notification_id(notification_id)?;
        if body.len() > BODY_LIMIT {
            return Err(request_invalid());
        }
        let input = parse_input(body)?;
        let response = target
            .target
            .backend
            .execute(ReconcileHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
                notification_id,
                idempotency_key: key,
                input,
            })
            .await?;
        if !valid_receipt(&response.data) || !response.context.is_object() {
            return Err(source_changed());
        }
        Ok(response.value())
    }
}

pub fn sales_notification_reconciliation_router(
    targets: Vec<ReconcileHttpTarget>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/customer-notifications/{notification_id}/reconcile",
            post(reconcile_handler),
        )
        .with_state(Arc::new(ReconcileHttpState::new(targets)?)))
}

async fn reconcile_handler(
    State(state): State<Arc<ReconcileHttpState>>,
    Path((slug, notification_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .reconcile(
            &parts.headers,
            &slug,
            &notification_id,
            parts.uri.query(),
            &body,
        )
        .await
}
