//! Owner-local HTTP ingress for the native Commerce purchase goods receipt.
//!
//! Transport facts only: registered Host/Holding target, exact origin,
//! Idempotency-Key, bounded exact JSON input and business-error mapping. The
//! normal Account session, current Company/branch/department authority,
//! purchase-order admission, package admission and the single serializable
//! owner unit remain composition responsibilities supplied through the backend
//! trait. The router opens no connection and never writes stock, Quality or
//! Finance.
use crate::domain::order::{js_trim, valid_date};
use crate::domain::receipt::{
    CommerceReceipt, MAX_SAFE, ReceiptAllocationCommand, ReceiptCommand, ReceiptDisposition,
    ReceiptLineCommand, readback_invalid, request_invalid,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, Path, Request, Response, Router, State, body::to_bytes, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

pub use super::order_http::CommerceOriginPolicy;
use super::order_http::{
    bounded, command_key_required, error_response, missing, origin_forbidden, unique_header,
    valid_origin,
};
/// Keeps the retained receipt command JSON bounded on ingress.
const BODY_LIMIT: usize = 256 * 1024;
/// Retained receipt command shape limits. The domain enforces its own wider
/// `ReceiptCommand::validate` bounds after this transport narrowing.
const LINES_LIMIT: usize = 20;
const ALLOCATIONS_LIMIT: usize = 200;

pub type CommerceReceiptHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Exact decoded goods-receipt request. `command.key` and `command.order_id`
/// are built from `idempotency_key` and `order_id`; the duplicated fields let
/// composition retain transport provenance without re-parsing headers.
pub struct CommerceReceiptHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub order_id: String,
    pub idempotency_key: String,
    pub command: ReceiptCommand,
}

#[derive(Clone, Debug)]
pub struct CommerceReceiptHttpResponse {
    /// Exact owner `CommerceReceipt` result document.
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

impl CommerceReceiptHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({ "data": self.data, "context": self.context })
    }
}

/// Composition-owned Account session, scope, purchase admission and owner-unit
/// adapter. The router never mints a session, actor, Company scope, permission
/// or feature admission.
pub trait CommerceReceiptHttpBackend: Send + Sync {
    fn receive<'a>(
        &'a self,
        request: CommerceReceiptHttpRequest<'a>,
    ) -> CommerceReceiptHttpFuture<'a, CommerceReceiptHttpResponse>;
}

pub struct CommerceReceiptHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn CommerceReceiptHttpBackend>,
}

pub struct CommerceReceiptHttpState {
    targets: BTreeMap<(String, String), CommerceReceiptHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a CommerceReceiptHttpTarget,
    host: &'a str,
}

impl CommerceReceiptHttpState {
    /// Existing callers default to the strict production policy. Development
    /// local-port exceptions require explicit trusted configuration.
    pub fn new(targets: Vec<CommerceReceiptHttpTarget>) -> Result<Self, BusinessError> {
        Self::new_with_origin_policy(targets, CommerceOriginPolicy::Production)
    }

    pub fn new_with_origin_policy(
        targets: Vec<CommerceReceiptHttpTarget>,
        policy: CommerceOriginPolicy,
    ) -> Result<Self, BusinessError> {
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
                || !valid_origin(&target.origin, &target.host, policy)
                || configured
                    .insert((target.host.clone(), target.holding_slug.clone()), target)
                    .is_some()
            {
                return Err(BusinessError::invalid(
                    "COMMERCE_RECEIPT_HTTP_CONFIGURATION_INVALID",
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

    fn command_key(headers: &HeaderMap) -> Result<CommandKey, BusinessError> {
        let key = unique_header(headers, "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(command_key_required)?;
        CommandKey::new(key)
    }

    fn order_id(value: &str) -> Result<RecordId, BusinessError> {
        RecordId::new(value).map_err(|_| request_invalid())
    }

    /// Exact POST handler used by both the Router and owner tests.
    pub async fn receive(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        order_id: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .receive_inner(headers, holding_slug, order_id, query, body)
            .await
        {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn receive_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        order_id: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Value, BusinessError> {
        if query.is_some_and(|value| !value.is_empty()) {
            return Err(request_invalid());
        }
        let target = self.target(headers, holding_slug)?;
        Self::content_type(headers)?;
        let key = Self::command_key(headers)?;
        let order_id = Self::order_id(order_id)?;
        if body.len() > BODY_LIMIT {
            return Err(request_invalid());
        }
        let command = parse_receipt(body, key, order_id)?;
        let order_id = command.order_id.as_str().to_owned();
        let idempotency_key = command.key.as_str().to_owned();
        let response = target
            .target
            .backend
            .receive(CommerceReceiptHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
                order_id,
                idempotency_key,
                command,
            })
            .await?;
        if !valid_receipt(&response.data) || !response.context.is_object() {
            return Err(readback_invalid());
        }
        Ok(response.value())
    }
}

fn valid_receipt(value: &Value) -> bool {
    serde_json::to_string(value)
        .ok()
        .is_some_and(|text| CommerceReceipt::from_result_json(&text).is_ok())
}

pub(crate) fn allowed(object: &Map<String, Value>, keys: &[&str]) -> bool {
    object.keys().all(|key| keys.contains(&key.as_str()))
}

pub(crate) fn optional_text(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Option<String>, BusinessError> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => {
            let value = js_trim(value);
            Ok((!value.is_empty()).then(|| value.to_owned()))
        }
        _ => Err(request_invalid()),
    }
}

pub(crate) fn optional_date(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Option<String>, BusinessError> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(value)) if value.is_empty() => Ok(None),
        Some(Value::String(value)) if valid_date(value) => Ok(Some(value.clone())),
        _ => Err(request_invalid()),
    }
}

/// Retained receipt quantity notation: an unsigned decimal string with at most
/// three fraction digits, converted to integer milli-units.
pub(crate) fn parse_milli(value: &str) -> Option<i64> {
    let (whole, fraction) = value
        .split_once('.')
        .map_or((value, None), |(whole, fraction)| (whole, Some(fraction)));
    if whole.is_empty() || !whole.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mut milli: i64 = 0;
    for byte in whole.bytes() {
        milli = milli.checked_mul(10)?.checked_add(i64::from(byte - b'0'))?;
    }
    milli = milli.checked_mul(1_000)?;
    if let Some(fraction) = fraction {
        if fraction.is_empty()
            || fraction.len() > 3
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        let mut part: i64 = 0;
        for byte in fraction.bytes() {
            part = part * 10 + i64::from(byte - b'0');
        }
        let scale = match fraction.len() {
            1 => 100,
            2 => 10,
            _ => 1,
        };
        milli = milli.checked_add(part.checked_mul(scale)?)?;
    }
    (milli <= MAX_SAFE).then_some(milli)
}

fn required_quantity(object: &Map<String, Value>, key: &str) -> Result<i64, BusinessError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .and_then(parse_milli)
        .ok_or_else(request_invalid)
}

fn parse_disposition(value: Option<&Value>) -> Result<ReceiptDisposition, BusinessError> {
    match value {
        None => Ok(ReceiptDisposition::Available),
        Some(Value::String(value)) => match value.as_str() {
            "available" => Ok(ReceiptDisposition::Available),
            "quarantine" => Ok(ReceiptDisposition::Quarantine),
            _ => Err(request_invalid()),
        },
        _ => Err(request_invalid()),
    }
}

fn parse_allocation(value: &Value) -> Result<ReceiptAllocationCommand, BusinessError> {
    let Some(object) = value.as_object() else {
        return Err(request_invalid());
    };
    if !allowed(
        object,
        &[
            "lotCode",
            "serialNumber",
            "quantity",
            "manufacturedDate",
            "expiryDate",
            "disposition",
            "quarantineReason",
            "quarantineDueDate",
        ],
    ) {
        return Err(request_invalid());
    }
    Ok(ReceiptAllocationCommand {
        lot_code: optional_text(object, "lotCode")?,
        serial_number: optional_text(object, "serialNumber")?,
        quantity_milli: required_quantity(object, "quantity")?,
        disposition: parse_disposition(object.get("disposition"))?,
        quarantine_reason: optional_text(object, "quarantineReason")?,
        quarantine_due_date: optional_date(object, "quarantineDueDate")?,
        manufactured_date: optional_date(object, "manufacturedDate")?,
        expiry_date: optional_date(object, "expiryDate")?,
    })
}

fn parse_line(value: &Value) -> Result<ReceiptLineCommand, BusinessError> {
    let Some(object) = value.as_object() else {
        return Err(request_invalid());
    };
    if !allowed(object, &["lineId", "quantity", "allocations"]) {
        return Err(request_invalid());
    }
    let line_id = object
        .get("lineId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(request_invalid)?
        .to_owned();
    let allocations = match object.get("allocations") {
        None => Vec::new(),
        Some(Value::Array(values)) if values.len() <= ALLOCATIONS_LIMIT => values
            .iter()
            .map(parse_allocation)
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(request_invalid()),
    };
    Ok(ReceiptLineCommand {
        line_id,
        quantity_milli: required_quantity(object, "quantity")?,
        allocations,
    })
}

/// Decode the retained goods-receipt command body. `completionMode` is optional
/// and may only be `goods`; delivery evidence identifiers are composition
/// attachment steps and are deliberately not accepted here.
fn parse_receipt(
    body: &[u8],
    key: CommandKey,
    order_id: RecordId,
) -> Result<ReceiptCommand, BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let Some(object) = value.as_object() else {
        return Err(request_invalid());
    };
    if !allowed(
        object,
        &["completionMode", "expectedVersion", "date", "lines"],
    ) {
        return Err(request_invalid());
    }
    if object
        .get("completionMode")
        .is_some_and(|value| value.as_str() != Some("goods"))
    {
        return Err(request_invalid());
    }
    let expected_version = object
        .get("expectedVersion")
        .and_then(Value::as_i64)
        .filter(|value| (1..=MAX_SAFE).contains(value))
        .ok_or_else(request_invalid)?;
    let date = object
        .get("date")
        .and_then(Value::as_str)
        .filter(|value| valid_date(value))
        .ok_or_else(request_invalid)?
        .to_owned();
    let Some(lines) = object.get("lines").and_then(Value::as_array) else {
        return Err(request_invalid());
    };
    if lines.is_empty() || lines.len() > LINES_LIMIT {
        return Err(request_invalid());
    }
    let lines = lines
        .iter()
        .map(parse_line)
        .collect::<Result<Vec<_>, _>>()?;
    let command = ReceiptCommand {
        key,
        order_id,
        expected_version,
        date,
        lines,
    };
    command.validate()?;
    Ok(command)
}

pub fn commerce_receipt_http_router(
    targets: Vec<CommerceReceiptHttpTarget>,
) -> Result<Router, BusinessError> {
    commerce_receipt_http_router_with_origin_policy(targets, CommerceOriginPolicy::Production)
}

pub fn commerce_receipt_http_router_with_origin_policy(
    targets: Vec<CommerceReceiptHttpTarget>,
    policy: CommerceOriginPolicy,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/purchase-orders/{order_id}/goods-receipts",
            post(receive_handler),
        )
        .with_state(Arc::new(CommerceReceiptHttpState::new_with_origin_policy(
            targets, policy,
        )?)))
}

async fn receive_handler(
    State(state): State<Arc<CommerceReceiptHttpState>>,
    Path((slug, order_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .receive(&parts.headers, &slug, &order_id, parts.uri.query(), &body)
        .await
}
