//! Owner-local HTTP ingress for native Sales confirmed sales-order cancellation.
//!
//! Transport facts only: registered Host/Holding target, exact Origin,
//! Idempotency-Key, one bounded JSON body and business-error mapping. Normal
//! Account session resolution, current Company/branch/department scope,
//! purchase admission and the serializable owner unit remain composition
//! responsibilities supplied through the backend trait.
use crate::domain::sales_order_cancellation::{request_invalid, validate_receipt_shape};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 1_024;

pub type SalesOrderCancellationHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct SalesOrderCancellationHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub order_id: &'a str,
    pub idempotency_key: &'a str,
    pub expected_version: i64,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct SalesOrderCancellationHttpResponse {
    /// Owner `{id,state:"cancelled",version,salesId:null}` receipt.
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

impl SalesOrderCancellationHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope and owner-unit adapter. The router
/// never mints a session, actor, Company scope or purchase.
pub trait SalesOrderCancellationHttpBackend: Send + Sync {
    fn cancel<'a>(
        &'a self,
        request: SalesOrderCancellationHttpRequest<'a>,
    ) -> SalesOrderCancellationHttpFuture<'a, SalesOrderCancellationHttpResponse>;
}

pub struct SalesOrderCancellationHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn SalesOrderCancellationHttpBackend>,
}

struct SalesOrderCancellationHttpState {
    targets: BTreeMap<(String, String), SalesOrderCancellationHttpTarget>,
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

fn valid_origin(origin: &str, host: &str) -> bool {
    origin
        .strip_prefix("https://")
        .is_some_and(|rest| rest == host)
        && !host.is_empty()
}

pub fn sales_order_cancellation_router(
    targets: Vec<SalesOrderCancellationHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || target.holding_slug.len() > 128
            || !target.holding_slug.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
            })
            || !valid_origin(&target.origin, &target.host)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "SALES_CANCELLATION_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/sales/orders/{order_id}/cancel",
            post(cancel_order),
        )
        .with_state(Arc::new(SalesOrderCancellationHttpState {
            targets: configured,
        })))
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

impl SalesOrderCancellationHttpState {
    fn target<'a>(
        &'a self,
        headers: &'a HeaderMap,
        holding_slug: &str,
    ) -> Result<&'a SalesOrderCancellationHttpTarget, BusinessError> {
        let host = unique_header(headers, "host")?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let target = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        let origin = unique_header(headers, "origin")?
            .filter(|value| !value.is_empty())
            .ok_or_else(origin_forbidden)?;
        if !valid_origin(origin, &target.host) || origin != target.origin {
            return Err(origin_forbidden());
        }
        Ok(target)
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

    fn command_key(headers: &HeaderMap) -> Result<CommandKey, BusinessError> {
        let key = unique_header(headers, "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(command_key_required)?;
        CommandKey::new(key)
    }

    fn order_id(value: &str) -> Result<String, BusinessError> {
        RecordId::new(value)
            .map(|id| id.as_str().to_owned())
            .map_err(|_| request_invalid())
    }
}

fn parse_expected_version(body: &[u8]) -> Result<i64, BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let Some(object) = value.as_object() else {
        return Err(request_invalid());
    };
    if object.len() != 1 || !object.contains_key("expectedVersion") {
        return Err(request_invalid());
    }
    object
        .get("expectedVersion")
        .and_then(Value::as_i64)
        .filter(|value| (1..=9_007_199_254_740_991).contains(value))
        .ok_or_else(request_invalid)
}

fn valid_receipt(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        if object.len() != 4
            || !object.contains_key("id")
            || !object.contains_key("state")
            || !object.contains_key("version")
            || !object.contains_key("salesId")
        {
            return false;
        }
        let id = object.get("id").and_then(Value::as_str);
        let state = object.get("state").and_then(Value::as_str);
        let version = object.get("version").and_then(Value::as_i64);
        let sales_ok = matches!(object.get("salesId"), Some(Value::Null));
        id.zip(state)
            .zip(version)
            .is_some_and(|((id, state), version)| {
                validate_receipt_shape(id, state, version, None) && sales_ok
            })
    })
}

async fn cancel_order(
    State(state): State<Arc<SalesOrderCancellationHttpState>>,
    Path((holding_slug, order_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let (parts, body) = request.into_parts();
        let headers = parts.headers;
        if parts.uri.query().is_some_and(|value| !value.is_empty()) {
            return Err(request_invalid());
        }
        let target = state.target(&headers, &holding_slug)?;
        SalesOrderCancellationHttpState::content_type(&headers)?;
        let key = SalesOrderCancellationHttpState::command_key(&headers)?;
        let order_id = SalesOrderCancellationHttpState::order_id(&order_id)?;
        let body = to_bytes(body, BODY_LIMIT)
            .await
            .map_err(|_| request_invalid())?;
        if body.is_empty() || body.len() > BODY_LIMIT {
            return Err(request_invalid());
        }
        let expected_version = parse_expected_version(&body)?;
        let reply = target
            .backend
            .cancel(SalesOrderCancellationHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                order_id: &order_id,
                idempotency_key: key.as_str(),
                expected_version,
                headers: &headers,
            })
            .await?;
        if !valid_receipt(&reply.data) || !reply.context.is_object() {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "SALES_CANCELLATION_SOURCE_INVALID",
                "The current sales order source is invalid.",
            ));
        }
        Ok(reply.value())
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}
