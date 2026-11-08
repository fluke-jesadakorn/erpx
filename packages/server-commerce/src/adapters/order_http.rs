//! Owner-local HTTP ingress for the native Commerce quotation/sales-order
//! read and lifecycle transitions.
//!
//! Transport facts only: registered Host/Holding target, exact origin,
//! Idempotency-Key, bounded exact JSON input and business-error mapping. The
//! normal Account session, current Company/branch/department authority,
//! feature/package admission and serializable owner unit remain composition
//! responsibilities supplied through the backend trait.
use crate::domain::order::{
    LINES_LIMIT, OrderAction, OrderKind, SAVE_LINES_LIMIT, SaveOrderInput, SaveOrderLineInput,
    request_invalid, source_invalid,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, get, header, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;

pub type CommerceOrderHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommerceOrderHttpCommand {
    Detail {
        order_id: String,
    },
    Transition {
        order_id: String,
        idempotency_key: String,
        action: OrderAction,
        expected_version: i64,
        reason: Option<String>,
    },
    Save {
        order_id: Option<String>,
        idempotency_key: String,
        input: SaveOrderInput,
    },
}

pub struct CommerceOrderHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: CommerceOrderHttpCommand,
}

#[derive(Clone, Debug)]
pub struct CommerceOrderHttpResponse {
    /// Owner document or exact `{id,state,version,salesId}` receipt.
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

impl CommerceOrderHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({ "data": self.data, "context": self.context })
    }
}

/// Composition-owned Account session, scope and owner-unit adapter. The
/// router never mints a session, actor, Company scope, permission or feature
/// admission.
pub trait CommerceOrderHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: CommerceOrderHttpRequest<'a>,
    ) -> CommerceOrderHttpFuture<'a, CommerceOrderHttpResponse>;
}

pub struct CommerceOrderHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn CommerceOrderHttpBackend>,
}

pub struct CommerceOrderHttpState {
    targets: BTreeMap<(String, String), CommerceOrderHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a CommerceOrderHttpTarget,
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

impl CommerceOrderHttpState {
    pub fn new(targets: Vec<CommerceOrderHttpTarget>) -> Result<Self, BusinessError> {
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
                    "COMMERCE_ORDER_HTTP_CONFIGURATION_INVALID",
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

    fn order_id(value: &str) -> Result<String, BusinessError> {
        RecordId::new(value)
            .map(|id| id.as_str().to_owned())
            .map_err(|_| request_invalid())
    }

    /// Exact GET handler used by both the Router and owner tests.
    pub async fn detail(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        order_id: &str,
        query: Option<&str>,
    ) -> Response {
        match self
            .detail_inner(headers, holding_slug, order_id, query)
            .await
        {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn detail_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        order_id: &str,
        query: Option<&str>,
    ) -> Result<Value, BusinessError> {
        if query.is_some_and(|value| !value.is_empty()) {
            return Err(request_invalid());
        }
        let target = self.target(headers, holding_slug)?;
        let order_id = Self::order_id(order_id)?;
        let response = target
            .target
            .backend
            .execute(CommerceOrderHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
                command: CommerceOrderHttpCommand::Detail { order_id },
            })
            .await?;
        if !valid_document(&response.data) || !response.context.is_object() {
            return Err(source_invalid());
        }
        Ok(response.value())
    }

    /// Exact POST handler used by both the Router and owner tests.
    pub async fn transition(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        order_id: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .transition_inner(headers, holding_slug, order_id, query, body)
            .await
        {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn transition_inner(
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
        let (action, expected_version, reason) = parse_transition(body)?;
        let response = target
            .target
            .backend
            .execute(CommerceOrderHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
                command: CommerceOrderHttpCommand::Transition {
                    order_id,
                    idempotency_key: key.as_str().to_owned(),
                    action,
                    expected_version,
                    reason,
                },
            })
            .await?;
        if !valid_receipt(&response.data) || !response.context.is_object() {
            return Err(source_invalid());
        }
        Ok(response.value())
    }

    /// Exact POST handler for draft create/update, used by the Router and
    /// owner tests.
    pub async fn save(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        order_id: Option<&str>,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .save_inner(headers, holding_slug, order_id, query, body)
            .await
        {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn save_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        order_id: Option<&str>,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Value, BusinessError> {
        if query.is_some_and(|value| !value.is_empty()) {
            return Err(request_invalid());
        }
        let target = self.target(headers, holding_slug)?;
        Self::content_type(headers)?;
        let key = Self::command_key(headers)?;
        let order_id = order_id.map(Self::order_id).transpose()?;
        if body.len() > BODY_LIMIT {
            return Err(request_invalid());
        }
        let input = parse_save(body)?;
        let response = target
            .target
            .backend
            .execute(CommerceOrderHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
                command: CommerceOrderHttpCommand::Save {
                    order_id,
                    idempotency_key: key.as_str().to_owned(),
                    input,
                },
            })
            .await?;
        if !valid_save_receipt(&response.data) || !response.context.is_object() {
            return Err(source_invalid());
        }
        Ok(response.value())
    }
}

fn valid_document(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        if object.len() != 2 || !object.contains_key("order") || !object.contains_key("lines") {
            return false;
        }
        object.get("order").is_some_and(Value::is_object)
            && object
                .get("lines")
                .and_then(Value::as_array)
                .is_some_and(|rows| {
                    !rows.is_empty()
                        && rows.len() <= LINES_LIMIT
                        && rows.iter().all(Value::is_object)
                })
    })
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
        let sales_ok = match object.get("salesId") {
            Some(Value::Null) => true,
            Some(Value::String(value)) => !value.is_empty() && value.len() <= 200,
            _ => false,
        };
        id.is_some_and(|value| !value.is_empty() && value.len() <= 200)
            && state.is_some_and(|value| {
                matches!(
                    value,
                    "draft"
                        | "submitted"
                        | "confirmed"
                        | "partially_fulfilled"
                        | "fulfilled"
                        | "converted"
                        | "cancelled"
                )
            })
            && version.is_some_and(|value| value >= 1)
            && sales_ok
    })
}

fn parse_transition(body: &[u8]) -> Result<(OrderAction, i64, Option<String>), BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let Some(object) = value.as_object() else {
        return Err(request_invalid());
    };
    if !(object.len() == 2 || object.len() == 3)
        || !object.contains_key("action")
        || !object.contains_key("expectedVersion")
        || (object.len() == 3 && !object.contains_key("reason"))
    {
        return Err(request_invalid());
    }
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .and_then(OrderAction::parse)
        .ok_or_else(request_invalid)?;
    let expected_version = object
        .get("expectedVersion")
        .and_then(Value::as_i64)
        .filter(|value| *value >= 1)
        .ok_or_else(request_invalid)?;
    let reason = match object.get("reason") {
        None => None,
        Some(Value::String(value)) => Some(value.clone()),
        _ => return Err(request_invalid()),
    };
    Ok((action, expected_version, reason))
}

fn valid_save_receipt(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        if object.len() != 2 || !object.contains_key("id") || !object.contains_key("version") {
            return false;
        }
        object
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty() && value.len() <= 200)
            && object
                .get("version")
                .and_then(Value::as_i64)
                .is_some_and(|value| (1..=9_007_199_254_740_991).contains(&value))
    })
}

fn parse_save(body: &[u8]) -> Result<SaveOrderInput, BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let Some(object) = value.as_object() else {
        return Err(request_invalid());
    };
    const KEYS: [&str; 10] = [
        "kind",
        "partyId",
        "warehouseId",
        "orderDate",
        "dueDate",
        "note",
        "priceListId",
        "discountReason",
        "lines",
        "expectedVersion",
    ];
    if object.keys().any(|key| !KEYS.contains(&key.as_str())) {
        return Err(request_invalid());
    }
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .and_then(OrderKind::parse)
        .ok_or_else(request_invalid)?;
    let party_id = object
        .get("partyId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(request_invalid)?
        .to_owned();
    let warehouse_id = match object.get("warehouseId") {
        None => None,
        Some(Value::String(value)) if value.is_empty() => None,
        Some(Value::String(value)) => Some(value.clone()),
        _ => return Err(request_invalid()),
    };
    let order_date = object
        .get("orderDate")
        .and_then(Value::as_str)
        .ok_or_else(request_invalid)?
        .to_owned();
    let due_date = object
        .get("dueDate")
        .and_then(Value::as_str)
        .ok_or_else(request_invalid)?
        .to_owned();
    let note = match object.get("note") {
        None => String::new(),
        Some(Value::String(value)) => value.clone(),
        _ => return Err(request_invalid()),
    };
    let price_list_id = match object.get("priceListId") {
        None => None,
        Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
        _ => return Err(request_invalid()),
    };
    let discount_reason = match object.get("discountReason") {
        None => None,
        Some(Value::String(value)) => Some(value.clone()),
        _ => return Err(request_invalid()),
    };
    let expected_version = match object.get("expectedVersion") {
        None => None,
        Some(value) => Some(
            value
                .as_i64()
                .filter(|value| (1..=9_007_199_254_740_991).contains(value))
                .ok_or_else(request_invalid)?,
        ),
    };
    let Some(lines) = object.get("lines").and_then(Value::as_array) else {
        return Err(request_invalid());
    };
    if lines.is_empty() || lines.len() > SAVE_LINES_LIMIT {
        return Err(request_invalid());
    }
    let mut parsed = Vec::with_capacity(lines.len());
    for line in lines {
        let Some(line) = line.as_object() else {
            return Err(request_invalid());
        };
        if line.len() != 4
            || !line.contains_key("productId")
            || !line.contains_key("quantity")
            || !line.contains_key("price")
            || !line.contains_key("taxBps")
        {
            return Err(request_invalid());
        }
        let product_id = line
            .get("productId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(request_invalid)?
            .to_owned();
        let quantity = line
            .get("quantity")
            .and_then(Value::as_str)
            .ok_or_else(request_invalid)?
            .to_owned();
        let price = line
            .get("price")
            .and_then(Value::as_str)
            .ok_or_else(request_invalid)?
            .to_owned();
        let tax_bps = line
            .get("taxBps")
            .and_then(Value::as_i64)
            .filter(|value| (0..=10_000).contains(value))
            .ok_or_else(request_invalid)?;
        parsed.push(SaveOrderLineInput {
            product_id,
            quantity,
            price,
            tax_bps,
        });
    }
    Ok(SaveOrderInput {
        kind,
        party_id,
        warehouse_id,
        order_date,
        due_date,
        note,
        price_list_id,
        discount_reason,
        lines: parsed,
        expected_version,
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

pub fn commerce_order_http_router(
    targets: Vec<CommerceOrderHttpTarget>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/orders",
            post(save_create_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/orders/{order_id}",
            get(detail_handler).post(save_update_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/orders/{order_id}/transition",
            post(transition_handler),
        )
        .with_state(Arc::new(CommerceOrderHttpState::new(targets)?)))
}

async fn detail_handler(
    State(state): State<Arc<CommerceOrderHttpState>>,
    Path((slug, order_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, _) = request.into_parts();
    state
        .detail(&parts.headers, &slug, &order_id, parts.uri.query())
        .await
}

async fn save_create_handler(
    State(state): State<Arc<CommerceOrderHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .save(&parts.headers, &slug, None, parts.uri.query(), &body)
        .await
}

async fn save_update_handler(
    State(state): State<Arc<CommerceOrderHttpState>>,
    Path((slug, order_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .save(
            &parts.headers,
            &slug,
            Some(&order_id),
            parts.uri.query(),
            &body,
        )
        .await
}

async fn transition_handler(
    State(state): State<Arc<CommerceOrderHttpState>>,
    Path((slug, order_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .transition(&parts.headers, &slug, &order_id, parts.uri.query(), &body)
        .await
}
