//! Owner-local HTTP ingress for the native Sales price-list lifecycle.
//!
//! Transport facts only: registered Host/Holding target, exact Origin,
//! Idempotency-Key, bounded exact JSON input and business-error mapping. The
//! normal Account session, current Company authority, purchase/module
//! admission and serializable owner unit remain composition responsibilities
//! supplied through the backend trait.
use crate::application::price_lists::COMMAND_KEY_MAX;
use crate::domain::price_lists::{
    PriceListDecision, PriceListDecisionAction, PriceListInput, PriceListItemInput,
    PriceListStatus, PriceListUpdate, request_invalid, source_invalid,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 256 * 1024;

pub type PriceListHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum PriceListHttpCommand {
    Create {
        idempotency_key: String,
        input: PriceListInput,
    },
    Update {
        price_list_id: String,
        idempotency_key: String,
        input: PriceListUpdate,
    },
    Decide {
        price_list_id: String,
        idempotency_key: String,
        input: PriceListDecision,
    },
}

pub struct PriceListHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: PriceListHttpCommand,
}

#[derive(Clone, Debug)]
pub struct PriceListHttpResponse {
    /// Exact `{id,status,version}` receipt.
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

impl PriceListHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({ "data": self.data, "context": self.context })
    }
}

/// Composition-owned Account session, scope and owner-unit adapter. The router
/// never mints a session, actor, Company scope, permission or feature
/// admission.
pub trait PriceListHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: PriceListHttpRequest<'a>,
    ) -> PriceListHttpFuture<'a, PriceListHttpResponse>;
}

pub struct PriceListHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn PriceListHttpBackend>,
}

pub struct PriceListHttpState {
    targets: BTreeMap<(String, String), PriceListHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a PriceListHttpTarget,
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

impl PriceListHttpState {
    pub fn new(targets: Vec<PriceListHttpTarget>) -> Result<Self, BusinessError> {
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
                    "SALES_PRICING_HTTP_CONFIGURATION_INVALID",
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

    fn price_list_id(value: &str) -> Result<String, BusinessError> {
        RecordId::new(value)
            .map(|id| id.as_str().to_owned())
            .map_err(|_| request_invalid())
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
            .execute(PriceListHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
                command: PriceListHttpCommand::Create {
                    idempotency_key: key,
                    input,
                },
            })
            .await?;
        if !valid_receipt(&response.data) || !response.context.is_object() {
            return Err(source_invalid());
        }
        Ok(response.value())
    }

    pub async fn update(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        price_list_id: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .update_inner(headers, holding_slug, price_list_id, query, body)
            .await
        {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn update_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        price_list_id: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Value, BusinessError> {
        if query.is_some_and(|value| !value.is_empty()) {
            return Err(request_invalid());
        }
        let target = self.target(headers, holding_slug)?;
        Self::content_type(headers)?;
        let key = Self::command_key(headers)?;
        let price_list_id = Self::price_list_id(price_list_id)?;
        if body.len() > BODY_LIMIT {
            return Err(request_invalid());
        }
        let input = parse_update(body)?;
        let response = target
            .target
            .backend
            .execute(PriceListHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
                command: PriceListHttpCommand::Update {
                    price_list_id,
                    idempotency_key: key,
                    input,
                },
            })
            .await?;
        if !valid_receipt(&response.data) || !response.context.is_object() {
            return Err(source_invalid());
        }
        Ok(response.value())
    }

    pub async fn decide(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        price_list_id: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .decide_inner(headers, holding_slug, price_list_id, query, body)
            .await
        {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn decide_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        price_list_id: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Value, BusinessError> {
        if query.is_some_and(|value| !value.is_empty()) {
            return Err(request_invalid());
        }
        let target = self.target(headers, holding_slug)?;
        Self::content_type(headers)?;
        let key = Self::command_key(headers)?;
        let price_list_id = Self::price_list_id(price_list_id)?;
        if body.len() > BODY_LIMIT {
            return Err(request_invalid());
        }
        let input = parse_decision(body)?;
        let response = target
            .target
            .backend
            .execute(PriceListHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
                command: PriceListHttpCommand::Decide {
                    price_list_id,
                    idempotency_key: key,
                    input,
                },
            })
            .await?;
        if !valid_receipt(&response.data) || !response.context.is_object() {
            return Err(source_invalid());
        }
        Ok(response.value())
    }
}

fn object<'a>(
    value: &'a Value,
    keys: &[&str],
) -> Result<&'a serde_json::Map<String, Value>, BusinessError> {
    let Some(object) = value.as_object() else {
        return Err(request_invalid());
    };
    if object.len() != keys.len() || !keys.iter().all(|key| object.contains_key(*key)) {
        return Err(request_invalid());
    }
    Ok(object)
}

fn text<'a>(
    object: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Result<&'a str, BusinessError> {
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
        .filter(|value| (min..=max).contains(value))
        .ok_or_else(request_invalid)
}

fn price_minor(value: &Value) -> Result<i64, BusinessError> {
    let value = value.as_str().ok_or_else(request_invalid)?;
    if value.is_empty() || value.len() > 32 {
        return Err(request_invalid());
    }
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.is_some_and(|part| {
            part.is_empty() || part.len() > 2 || !part.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(request_invalid());
    }
    let whole: i64 = whole.parse().map_err(|_| request_invalid())?;
    let fraction = match fraction {
        None => 0,
        Some(part) if part.len() == 1 => part.parse::<i64>().map_err(|_| request_invalid())? * 10,
        Some(part) => part.parse::<i64>().map_err(|_| request_invalid())?,
    };
    whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction))
        .ok_or_else(request_invalid)
}

fn items(
    object: &serde_json::Map<String, Value>,
) -> Result<Vec<PriceListItemInput>, BusinessError> {
    let rows = object
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(request_invalid)?;
    if rows.is_empty() || rows.len() > crate::domain::price_lists::ITEMS_MAX {
        return Err(request_invalid());
    }
    rows.iter()
        .map(|row| {
            let row = object_ref(row, &["productId", "price", "taxBps"])?;
            let product_id = text(row, "productId")?.to_owned();
            if product_id.is_empty() || product_id.len() > 200 {
                return Err(request_invalid());
            }
            Ok(PriceListItemInput {
                product_id,
                price_minor: price_minor(row.get("price").unwrap_or(&Value::Null))?,
                tax_bps: integer(row, "taxBps", 0, 10_000)?,
            })
        })
        .collect()
}

fn object_ref<'a>(
    value: &'a Value,
    keys: &[&str],
) -> Result<&'a serde_json::Map<String, Value>, BusinessError> {
    object(value, keys)
}

fn input(object: &serde_json::Map<String, Value>) -> Result<PriceListInput, BusinessError> {
    let customer_id = match object.get("customerId") {
        Some(Value::Null) | None => None,
        Some(Value::String(value)) if !value.is_empty() && value.len() <= 200 => {
            Some(value.clone())
        }
        _ => return Err(request_invalid()),
    };
    Ok(PriceListInput {
        name: text(object, "name")?.to_owned(),
        currency: text(object, "currency")?.to_owned(),
        customer_id,
        valid_from: text(object, "validFrom")?.to_owned(),
        valid_until: text(object, "validUntil")?.to_owned(),
        max_auto_discount_bps: integer(object, "maxAutoDiscountBps", 0, 10_000)?,
        items: items(object)?,
    })
}

fn parse_create(body: &[u8]) -> Result<PriceListInput, BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let object = object(
        &value,
        &[
            "name",
            "currency",
            "customerId",
            "validFrom",
            "validUntil",
            "maxAutoDiscountBps",
            "items",
        ],
    )?;
    input(object)
}

fn parse_update(body: &[u8]) -> Result<PriceListUpdate, BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let object = object(
        &value,
        &[
            "name",
            "currency",
            "customerId",
            "validFrom",
            "validUntil",
            "maxAutoDiscountBps",
            "items",
            "expectedVersion",
        ],
    )?;
    Ok(PriceListUpdate {
        input: input(object)?,
        expected_version: integer(object, "expectedVersion", 1, i64::MAX)?,
    })
}

fn parse_decision(body: &[u8]) -> Result<PriceListDecision, BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let object = object(&value, &["expectedVersion", "action", "reason"])?;
    Ok(PriceListDecision {
        expected_version: integer(object, "expectedVersion", 1, i64::MAX)?,
        action: PriceListDecisionAction::parse(text(object, "action")?)?,
        reason: text(object, "reason")?.to_owned(),
    })
}

fn valid_receipt(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        if object.len() != 3
            || !object.contains_key("id")
            || !object.contains_key("status")
            || !object.contains_key("version")
        {
            return false;
        }
        let id = object.get("id").and_then(Value::as_str);
        let status = object.get("status").and_then(Value::as_str);
        let version = object.get("version").and_then(Value::as_i64);
        id.is_some_and(|value| !value.is_empty() && value.len() <= 200)
            && status.is_some_and(|value| PriceListStatus::parse(value).is_some())
            && version.is_some_and(|value| value >= 1)
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

pub fn sales_price_lists_router(
    targets: Vec<PriceListHttpTarget>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/sales/pricing",
            post(create_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/sales/pricing/{price_list_id}/edit",
            post(update_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/sales/pricing/{price_list_id}",
            post(decide_handler),
        )
        .with_state(Arc::new(PriceListHttpState::new(targets)?)))
}

async fn create_handler(
    State(state): State<Arc<PriceListHttpState>>,
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

async fn update_handler(
    State(state): State<Arc<PriceListHttpState>>,
    Path((slug, price_list_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .update(
            &parts.headers,
            &slug,
            &price_list_id,
            parts.uri.query(),
            &body,
        )
        .await
}

async fn decide_handler(
    State(state): State<Arc<PriceListHttpState>>,
    Path((slug, price_list_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .decide(
            &parts.headers,
            &slug,
            &price_list_id,
            parts.uri.query(),
            &body,
        )
        .await
}
