//! Owner-local HTTP ingress for native POS register-session, bill, void and
//! refund commands. Transport facts only; composition supplies the Account
//! session, current Company/branch, purchase admission and owner ports.
use crate::domain::commands::{PosCommand, PosOperation};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type PosCommandHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct PosCommandHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub key: &'a str,
    pub command: PosCommand,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct PosCommandHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}
impl PosCommandHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({ "data": self.data, "context": self.context })
    }
}

pub trait PosCommandHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: PosCommandHttpRequest<'a>,
    ) -> PosCommandHttpFuture<'a, PosCommandHttpResponse>;
}

pub struct PosCommandHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn PosCommandHttpBackend>,
}

pub struct PosCommandHttpState {
    targets: BTreeMap<(String, String), PosCommandHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn invalid() -> BusinessError {
    BusinessError::invalid("POS_COMMAND_INVALID", "Review the POS command fields.")
}
fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}
fn key_required() -> BusinessError {
    BusinessError::invalid(
        "COMMAND_KEY_REQUIRED",
        "Use a valid Idempotency-Key for this action.",
    )
}
fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
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

impl PosCommandHttpState {
    pub fn new(targets: Vec<PosCommandHttpTarget>) -> Result<Self, BusinessError> {
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
                    "POS_COMMAND_CONFIGURATION_INVALID",
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
    ) -> Result<(&'a PosCommandHttpTarget, &'a str), BusinessError> {
        let host = unique_header(headers, "host")?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let target = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        if unique_header(headers, "origin")?.is_some_and(|value| value != target.origin) {
            return Err(origin_forbidden());
        }
        Ok((target, host))
    }

    pub async fn execute(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        operation: &str,
        body: &[u8],
    ) -> Response {
        match self
            .execute_inner(headers, holding_slug, operation, body)
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
        operation: &str,
        body: &[u8],
    ) -> Result<serde_json::Value, BusinessError> {
        let Some(operation_kind) = PosOperation::parse(operation) else {
            return Err(missing());
        };
        let (target, host) = self.target(headers, holding_slug)?;
        let content_type = unique_header(headers, "content-type")?
            .ok_or_else(invalid)?
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if content_type != "application/json" {
            return Err(invalid());
        }
        let key = unique_header(headers, "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(key_required)?;
        let key = CommandKey::new(key)?;
        if body.len() > BODY_LIMIT {
            return Err(invalid());
        }
        let value: serde_json::Value = serde_json::from_slice(body).map_err(|_| invalid())?;
        let command = PosCommand::parse(operation_kind, key, &value)?;
        let key = command.key.as_str().to_owned();
        let response = target
            .backend
            .execute(PosCommandHttpRequest {
                host,
                holding_slug: &target.holding_slug,
                key: &key,
                command,
                headers,
            })
            .await?;
        if !response.data.is_object() || !response.context.is_object() {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "POS_SOURCE_INVALID",
                "The POS command acknowledgement could not be read back.",
            ));
        }
        let Some(id) = response.data.get("id").and_then(serde_json::Value::as_str) else {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "POS_SOURCE_INVALID",
                "The POS command acknowledgement could not be read back.",
            ));
        };
        let generated = matches!(
            operation_kind,
            PosOperation::ShiftOpen | PosOperation::SaleCreate | PosOperation::RefundRequest
        );
        if id.is_empty()
            || id.len() > 200
            || (generated && !crate::domain::commands::valid_uuid(id))
        {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "POS_SOURCE_INVALID",
                "The POS command acknowledgement could not be read back.",
            ));
        }
        Ok(response.value())
    }
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

pub fn pos_command_router(targets: Vec<PosCommandHttpTarget>) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pos/commands/{operation}",
            post(handler),
        )
        .with_state(Arc::new(PosCommandHttpState::new(targets)?)))
}

async fn handler(
    State(state): State<Arc<PosCommandHttpState>>,
    Path((slug, operation)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(invalid()),
    };
    state
        .execute(&parts.headers, &slug, &operation, &body)
        .await
}
