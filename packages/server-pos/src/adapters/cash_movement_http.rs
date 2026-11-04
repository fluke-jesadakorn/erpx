//! Owner-local HTTP ingress for the native POS cash-movement command.
//!
//! Transport facts only: registered Host/Holding target, exact origin,
//! Idempotency-Key, bounded exact JSON input and business-error mapping. The
//! normal Account session, current Company/branch authority, package admission
//! and serializable owner unit remain composition responsibilities supplied
//! through the backend trait.
use crate::domain::cash_movement::{CreateCashMovement, request_invalid, valid_uuid};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;

pub type CashMovementHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct CashMovementHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub key: &'a str,
    pub shift_id: &'a str,
    pub expected_version: i64,
    pub amount_minor: i64,
    pub reason: &'a str,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct CashMovementHttpResponse {
    /// Owner command receipt, exactly `{"id":"..."}`.
    pub data: serde_json::Value,
    /// Validated current context rendered by the composition backend.
    pub context: serde_json::Value,
}

impl CashMovementHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({ "data": self.data, "context": self.context })
    }
}

/// Composition-owned Account session, authority and owner-unit adapter.
/// The router never mints a session, actor, Company scope, purchase or grant.
pub trait CashMovementHttpBackend: Send + Sync {
    fn create<'a>(
        &'a self,
        request: CashMovementHttpRequest<'a>,
    ) -> CashMovementHttpFuture<'a, CashMovementHttpResponse>;
}

pub struct CashMovementHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn CashMovementHttpBackend>,
}

pub struct CashMovementHttpState {
    targets: BTreeMap<(String, String), CashMovementHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a CashMovementHttpTarget,
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

impl CashMovementHttpState {
    pub fn new(targets: Vec<CashMovementHttpTarget>) -> Result<Self, BusinessError> {
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
                    "POS_CASH_CONFIGURATION_INVALID",
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

    /// Exact POST handler used by both the Router and owner tests.
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
    ) -> Result<serde_json::Value, BusinessError> {
        if query.is_some_and(|value| !value.is_empty()) {
            return Err(request_invalid());
        }
        let target = self.target(headers, holding_slug)?;
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
        let key = unique_header(headers, "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(command_key_required)?;
        let key = CommandKey::new(key)?;
        if body.len() > BODY_LIMIT {
            return Err(request_invalid());
        }
        let command = parse_body(body, key)?;
        let response = target
            .target
            .backend
            .create(CashMovementHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                key: command.key.as_str(),
                shift_id: command.shift_id.as_str(),
                expected_version: command.expected_version,
                amount_minor: command.amount_minor,
                reason: &command.reason,
                headers,
            })
            .await?;
        if !valid_backend_receipt(&response.data) || !response.context.is_object() {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "POS_CASH_SOURCE_INVALID",
                "The POS cash receipt could not be read back.",
            ));
        }
        Ok(response.value())
    }
}

fn valid_backend_receipt(value: &serde_json::Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 1
            && object
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(valid_uuid)
    })
}

fn parse_body(body: &[u8], key: CommandKey) -> Result<CreateCashMovement, BusinessError> {
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let Some(object) = value.as_object() else {
        return Err(request_invalid());
    };
    if object.len() != 4
        || !object.contains_key("id")
        || !object.contains_key("expectedVersion")
        || !object.contains_key("amountMinor")
        || !object.contains_key("reason")
    {
        return Err(request_invalid());
    }
    let id = object
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(request_invalid)?;
    let expected_version = object
        .get("expectedVersion")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(request_invalid)?;
    let amount_minor = object
        .get("amountMinor")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(request_invalid)?;
    let reason = object
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(request_invalid)?;
    Ok(CreateCashMovement {
        key,
        shift_id: RecordId::new(id).map_err(|_| request_invalid())?,
        expected_version,
        amount_minor,
        reason: reason.to_owned(),
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

fn bounded(value: serde_json::Value) -> Response {
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

pub fn cash_movement_router(targets: Vec<CashMovementHttpTarget>) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pos/cash/create",
            post(create_handler),
        )
        .with_state(Arc::new(CashMovementHttpState::new(targets)?)))
}

async fn create_handler(
    State(state): State<Arc<CashMovementHttpState>>,
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
