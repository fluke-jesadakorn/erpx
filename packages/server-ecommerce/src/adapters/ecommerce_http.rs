//! Owner-local HTTP ingress for the native Ecommerce catalog, channel draft
//! and channel/listing review commands.
//!
//! Transport facts only: registered Host/Holding target, exact origin,
//! Idempotency-Key, bounded exact JSON input and business-error mapping. The
//! normal Account session, current Company/branch authority, package admission
//! and serializable owner unit remain composition responsibilities supplied
//! through the backend trait.
use crate::domain::channel::CreateChannel;
use crate::domain::command::request_invalid;
use crate::domain::review::{ReviewAction, ReviewCommand, ReviewKind};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, get, header, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;
const CATALOG_LIMIT: usize = 500;

pub type EcommerceHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct EcommerceCatalogHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

pub struct EcommerceChannelHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub key: &'a str,
    pub name: &'a str,
    pub channel_type: &'a str,
    pub configuration: &'a Value,
    pub headers: &'a HeaderMap,
}

pub struct EcommerceReviewHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub key: &'a str,
    pub kind: ReviewKind,
    pub id: &'a str,
    pub action: ReviewAction,
    pub expected_version: i64,
    pub reason: &'a str,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct EcommerceHttpResponse {
    /// Owner catalog page or exact `{id,status,version}` command receipt.
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

impl EcommerceHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({ "data": self.data, "context": self.context })
    }
}

/// Composition-owned Account session, authority and owner-unit adapter.
/// The router never mints a session, actor, Company scope, package purchase or
/// grant.
pub trait EcommerceHttpBackend: Send + Sync {
    fn catalog<'a>(
        &'a self,
        request: EcommerceCatalogHttpRequest<'a>,
    ) -> EcommerceHttpFuture<'a, EcommerceHttpResponse>;

    fn create_channel<'a>(
        &'a self,
        request: EcommerceChannelHttpRequest<'a>,
    ) -> EcommerceHttpFuture<'a, EcommerceHttpResponse>;

    fn review<'a>(
        &'a self,
        request: EcommerceReviewHttpRequest<'a>,
    ) -> EcommerceHttpFuture<'a, EcommerceHttpResponse>;
}

pub struct EcommerceHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn EcommerceHttpBackend>,
}

pub struct EcommerceHttpState {
    targets: BTreeMap<(String, String), EcommerceHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a EcommerceHttpTarget,
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
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
            })
    })
}

impl EcommerceHttpState {
    pub fn new(targets: Vec<EcommerceHttpTarget>) -> Result<Self, BusinessError> {
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
                    "ECOMMERCE_HTTP_CONFIGURATION_INVALID",
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

    /// Exact GET handler used by both the Router and owner tests.
    pub async fn catalog(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
    ) -> Response {
        match self.catalog_inner(headers, holding_slug, query).await {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn catalog_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
    ) -> Result<Value, BusinessError> {
        if query.is_some_and(|value| !value.is_empty()) {
            return Err(request_invalid());
        }
        let target = self.target(headers, holding_slug)?;
        let response = target
            .target
            .backend
            .catalog(EcommerceCatalogHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
            })
            .await?;
        if !valid_catalog(&response.data) || !response.context.is_object() {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "ECOMMERCE_SOURCE_INVALID",
                "The Ecommerce catalog could not be read back.",
            ));
        }
        Ok(response.value())
    }

    /// Exact POST handler for a channel draft used by the Router and tests.
    pub async fn create_channel(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .create_channel_inner(headers, holding_slug, query, body)
            .await
        {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn create_channel_inner(
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
        let (name, channel_type, configuration) = parse_channel(body)?;
        let response = target
            .target
            .backend
            .create_channel(EcommerceChannelHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                key: key.as_str(),
                name: &name,
                channel_type: &channel_type,
                configuration: &configuration,
                headers,
            })
            .await?;
        if !valid_command(&response.data) || !response.context.is_object() {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "ECOMMERCE_SOURCE_INVALID",
                "The Ecommerce channel receipt could not be read back.",
            ));
        }
        Ok(response.value())
    }

    /// Exact POST handler for a channel/listing review used by tests.
    pub async fn review(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self.review_inner(headers, holding_slug, query, body).await {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn review_inner(
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
        let review = parse_review(body, key)?;
        let response = target
            .target
            .backend
            .review(EcommerceReviewHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                key: review.key.as_str(),
                kind: review.kind,
                id: review.id.as_str(),
                action: review.action,
                expected_version: review.expected_version,
                reason: &review.reason,
                headers,
            })
            .await?;
        if !valid_command(&response.data) || !response.context.is_object() {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "ECOMMERCE_SOURCE_INVALID",
                "The Ecommerce review receipt could not be read back.",
            ));
        }
        Ok(response.value())
    }
}

fn valid_catalog(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        if object.len() != 2 || !object.contains_key("channels") || !object.contains_key("listings")
        {
            return false;
        }
        object
            .get("channels")
            .and_then(Value::as_array)
            .is_some_and(|rows| rows.len() <= CATALOG_LIMIT && rows.iter().all(Value::is_object))
            && object
                .get("listings")
                .and_then(Value::as_array)
                .is_some_and(|rows| {
                    rows.len() <= CATALOG_LIMIT && rows.iter().all(Value::is_object)
                })
    })
}

fn valid_command(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 3
            && object
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| !id.is_empty() && id.len() <= 200)
            && object
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|status| !status.is_empty() && status.len() <= 40)
            && object
                .get("version")
                .and_then(Value::as_i64)
                .is_some_and(|version| version >= 1)
    })
}

fn parse_channel(body: &[u8]) -> Result<(String, String, Value), BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let Some(object) = value.as_object() else {
        return Err(request_invalid());
    };
    if object.len() != 3
        || !object.contains_key("name")
        || !object.contains_key("channelType")
        || !object.contains_key("configuration")
    {
        return Err(request_invalid());
    }
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(request_invalid)?;
    let channel_type = object
        .get("channelType")
        .and_then(Value::as_str)
        .ok_or_else(request_invalid)?;
    let configuration = object
        .get("configuration")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or_else(request_invalid)?;
    let key = CommandKey::new("ecommerce-http-parse")?;
    let command = CreateChannel {
        key,
        name: name.to_owned(),
        channel_type: channel_type.to_owned(),
        configuration,
    };
    let validated = command.validate()?;
    Ok((
        validated.name,
        validated.channel_type,
        command.configuration,
    ))
}

fn parse_review(body: &[u8], key: CommandKey) -> Result<ReviewCommand, BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let Some(object) = value.as_object() else {
        return Err(request_invalid());
    };
    if object.len() != 5
        || !object.contains_key("kind")
        || !object.contains_key("id")
        || !object.contains_key("action")
        || !object.contains_key("expectedVersion")
        || !object.contains_key("reason")
    {
        return Err(request_invalid());
    }
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .and_then(ReviewKind::parse)
        .ok_or_else(request_invalid)?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(request_invalid)?;
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .and_then(ReviewAction::parse)
        .ok_or_else(request_invalid)?;
    let expected_version = object
        .get("expectedVersion")
        .and_then(Value::as_i64)
        .ok_or_else(request_invalid)?;
    let reason = object
        .get("reason")
        .and_then(Value::as_str)
        .ok_or_else(request_invalid)?;
    let command = ReviewCommand {
        key,
        kind,
        id: RecordId::new(id).map_err(|_| request_invalid())?,
        action,
        expected_version,
        reason: reason.to_owned(),
    };
    command.validate()?;
    Ok(command)
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

pub fn ecommerce_router(targets: Vec<EcommerceHttpTarget>) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/ecommerce/catalog",
            get(catalog_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/ecommerce/channels",
            post(create_channel_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/ecommerce/reviews",
            post(review_handler),
        )
        .with_state(Arc::new(EcommerceHttpState::new(targets)?)))
}

async fn catalog_handler(
    State(state): State<Arc<EcommerceHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, _) = request.into_parts();
    state
        .catalog(&parts.headers, &slug, parts.uri.query())
        .await
}

async fn create_channel_handler(
    State(state): State<Arc<EcommerceHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .create_channel(&parts.headers, &slug, parts.uri.query(), &body)
        .await
}

async fn review_handler(
    State(state): State<Arc<EcommerceHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .review(&parts.headers, &slug, parts.uri.query(), &body)
        .await
}
