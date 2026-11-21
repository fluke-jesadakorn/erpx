//! Owner-local HTTP ingress for native POS day-close commands.
//!
//! Transport facts only: an exact registered Host/Holding target, exact browser
//! Origin for mutations, one Idempotency-Key, a typed command kind and a bounded
//! JSON body. Normal Account session resolution, current IAM/Company/branch
//! authority, package admission, the serializable owner transaction and the
//! cross-owner report/Finance witnesses remain composition responsibilities
//! supplied through [`DayCloseHttpBackend`].
use crate::application::day_close::DayCloseCommand;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "POS_DAY_COMMAND_INVALID",
        "Review the POS day-close fields.",
    )
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
fn key_required() -> BusinessError {
    BusinessError::invalid(
        "COMMAND_KEY_REQUIRED",
        "Use a valid Idempotency-Key for this action.",
    )
}

pub type DayCloseHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct DayCloseHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub key: &'a CommandKey,
    pub command: DayCloseCommand,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct DayCloseHttpResponse {
    /// Owner acknowledgement, exactly `{"id":"...","state":"..."}`.
    pub data: serde_json::Value,
    /// Validated current context rendered by the composition backend.
    pub context: serde_json::Value,
}

impl DayCloseHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({ "data": self.data, "context": self.context })
    }
}

/// Composition-owned Account/IAM/package/owner-unit adapter. The router never
/// mints a session, actor, Company scope, purchase or report fact.
pub trait DayCloseHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: DayCloseHttpRequest<'a>,
    ) -> DayCloseHttpFuture<'a, DayCloseHttpResponse>;
}

pub struct DayCloseHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn DayCloseHttpBackend>,
}

struct DayCloseHttpState {
    targets: BTreeMap<(String, String), DayCloseHttpTarget>,
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
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

fn parse_kind(operation: &str) -> Option<&'static str> {
    match operation {
        "prepare" => Some("day.prepare"),
        "confirm" => Some("day.confirm"),
        _ => None,
    }
}

fn parse_command(operation: &'static str, body: &[u8]) -> Result<DayCloseCommand, BusinessError> {
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|_| malformed())?;
    let object = value.as_object().ok_or_else(malformed)?;
    match operation {
        "day.prepare" => {
            if object.len() != 2 {
                return Err(malformed());
            }
            let branch_id = object
                .get("branchId")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(malformed)?;
            let business_date = object
                .get("businessDate")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(malformed)?;
            if branch_id.is_empty() || branch_id.len() > 200 || business_date.len() != 10 {
                return Err(malformed());
            }
            Ok(DayCloseCommand::Prepare {
                branch_id: branch_id.to_owned(),
                business_date: business_date.to_owned(),
            })
        }
        "day.confirm" => {
            if object.len() != 2 {
                return Err(malformed());
            }
            let id = object
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(malformed)?;
            let expected_version = object
                .get("expectedVersion")
                .and_then(serde_json::Value::as_i64)
                .ok_or_else(malformed)?;
            if id.is_empty() || id.len() > 200 || expected_version < 1 {
                return Err(malformed());
            }
            Ok(DayCloseCommand::Confirm {
                id: id.to_owned(),
                expected_version,
            })
        }
        _ => Err(missing()),
    }
}

impl DayCloseHttpState {
    pub fn new(targets: Vec<DayCloseHttpTarget>) -> Result<Self, BusinessError> {
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
                    "POS_DAY_CONFIGURATION_INVALID",
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
    ) -> Result<&'a DayCloseHttpTarget, BusinessError> {
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
        Ok(target)
    }

    async fn execute(
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
        let operation = parse_kind(operation).ok_or_else(missing)?;
        let target = self.target(headers, holding_slug)?;
        let content_type = unique_header(headers, "content-type")?
            .ok_or_else(malformed)?
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if content_type != "application/json" || body.len() > BODY_LIMIT {
            return Err(malformed());
        }
        let key = unique_header(headers, "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(key_required)?;
        let key = CommandKey::new(key)?;
        let command = parse_command(operation, body)?;
        let response = target
            .backend
            .execute(DayCloseHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                key: &key,
                command,
                headers,
            })
            .await?;
        if !response.data.is_object() || !response.context.is_object() {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "POS_DAY_SOURCE_INVALID",
                "The POS day-close acknowledgement could not be read back.",
            ));
        }
        let valid_id = response
            .data
            .get("id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|id| !id.is_empty() && id.len() <= 200);
        let valid_state = matches!(
            response
                .data
                .get("state")
                .and_then(serde_json::Value::as_str),
            Some("prepared") | Some("confirmed")
        );
        if !valid_id || !valid_state {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "POS_DAY_SOURCE_INVALID",
                "The POS day-close acknowledgement could not be read back.",
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

pub fn pos_day_close_router(targets: Vec<DayCloseHttpTarget>) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pos/day/{operation}",
            post(handler),
        )
        .with_state(Arc::new(DayCloseHttpState::new(targets)?)))
}

async fn handler(
    State(state): State<Arc<DayCloseHttpState>>,
    Path((slug, operation)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(malformed()),
    };
    state
        .execute(&parts.headers, &slug, &operation, &body)
        .await
}
