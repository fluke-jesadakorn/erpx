//! Owner-local HTTP ingress for the native Ecommerce listing draft/create/change
//! commands.
//!
//! Transport facts only: a registered Host/Holding target, exact browser origin,
//! Idempotency-Key, the bounded retained listing wire body and business-error
//! mapping. The normal Account session, current Company/branch authority, package
//! admission, Commerce/Finance producer ports and the serializable owner unit
//! remain composition responsibilities behind [`EcommerceListingHttpBackend`].
//! This bounded module keeps the frozen channel/review backend trait unchanged;
//! composition mounts both routers against the same registered Holding targets.
use super::ecommerce_http::{
    EcommerceHttpFuture, EcommerceHttpResponse, bounded, error_response, unique_header,
    valid_command, valid_origin,
};
use crate::domain::command::request_invalid;
use crate::domain::listing::{ChangeListing, CreateListing};
use erpx_server_runtime::public::http_support::{
    HeaderMap, Path, Request, Response, Router, State, body::to_bytes, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;

pub struct EcommerceListingCreateHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub key: &'a str,
    pub channel_id: &'a str,
    pub product_id: &'a str,
    pub title: &'a str,
    pub currency: &'a str,
    pub price: &'a str,
    pub headers: &'a HeaderMap,
}

pub struct EcommerceListingChangeHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub key: &'a str,
    pub id: &'a str,
    pub expected_version: i64,
    pub title: &'a str,
    pub headers: &'a HeaderMap,
}

/// Composition-owned Account session, authority, Commerce/Finance source and
/// owner-unit adapter. The router never mints a session, actor, Company,
/// permission, product, currency or Money snapshot.
pub trait EcommerceListingHttpBackend: Send + Sync {
    fn create_listing<'a>(
        &'a self,
        request: EcommerceListingCreateHttpRequest<'a>,
    ) -> EcommerceHttpFuture<'a, EcommerceHttpResponse>;

    fn change_listing<'a>(
        &'a self,
        request: EcommerceListingChangeHttpRequest<'a>,
    ) -> EcommerceHttpFuture<'a, EcommerceHttpResponse>;
}

pub struct EcommerceListingHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn EcommerceListingHttpBackend>,
}

pub struct EcommerceListingHttpState {
    targets: BTreeMap<(String, String), EcommerceListingHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a EcommerceListingHttpTarget,
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

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "ECOMMERCE_HTTP_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn receipt_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_SOURCE_INVALID",
        "The Ecommerce listing receipt could not be read back.",
    )
}

impl EcommerceListingHttpState {
    pub fn new(targets: Vec<EcommerceListingHttpTarget>) -> Result<Self, BusinessError> {
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
                return Err(configuration_invalid());
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

    /// Exact POST handler for a listing draft used by the Router and tests.
    pub async fn create_listing(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .create_listing_inner(headers, holding_slug, query, body)
            .await
        {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn create_listing_inner(
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
        let listing = parse_create(body, key)?;
        let validated = listing.validate()?;
        let response = target
            .target
            .backend
            .create_listing(EcommerceListingCreateHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                key: listing.key.as_str(),
                channel_id: &validated.channel_id,
                product_id: &validated.product_id,
                title: &validated.title,
                currency: &validated.currency,
                price: &listing.price,
                headers,
            })
            .await?;
        if !valid_command(&response.data) || !response.context.is_object() {
            return Err(receipt_invalid());
        }
        Ok(response.value())
    }

    /// Exact POST handler for a draft listing retitle used by the Router and tests.
    pub async fn change_listing(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        listing_id: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .change_listing_inner(headers, holding_slug, listing_id, query, body)
            .await
        {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn change_listing_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        listing_id: &str,
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
        let listing_id = RecordId::new(listing_id).map_err(|_| request_invalid())?;
        let change = parse_change(body, key, listing_id)?;
        let validated = change.validate()?;
        let response = target
            .target
            .backend
            .change_listing(EcommerceListingChangeHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                key: change.key.as_str(),
                id: &validated.id,
                expected_version: validated.expected_version,
                title: &validated.title,
                headers,
            })
            .await?;
        if !valid_command(&response.data) || !response.context.is_object() {
            return Err(receipt_invalid());
        }
        Ok(response.value())
    }
}

fn parse_create(body: &[u8], key: CommandKey) -> Result<CreateListing, BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let Some(object) = value.as_object() else {
        return Err(request_invalid());
    };
    if object.len() != 5
        || !object.contains_key("channelId")
        || !object.contains_key("productId")
        || !object.contains_key("title")
        || !object.contains_key("currency")
        || !object.contains_key("price")
    {
        return Err(request_invalid());
    }
    let text = |name: &str| -> Result<&str, BusinessError> {
        object
            .get(name)
            .and_then(Value::as_str)
            .ok_or_else(request_invalid)
    };
    Ok(CreateListing {
        key,
        channel_id: RecordId::new(text("channelId")?).map_err(|_| request_invalid())?,
        product_id: RecordId::new(text("productId")?).map_err(|_| request_invalid())?,
        title: text("title")?.to_owned(),
        currency: text("currency")?.to_owned(),
        price: text("price")?.to_owned(),
    })
}

fn parse_change(
    body: &[u8],
    key: CommandKey,
    id: RecordId,
) -> Result<ChangeListing, BusinessError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| request_invalid())?;
    let Some(object) = value.as_object() else {
        return Err(request_invalid());
    };
    if object.len() != 2
        || !object.contains_key("expectedVersion")
        || !object.contains_key("title")
    {
        return Err(request_invalid());
    }
    let expected_version = object
        .get("expectedVersion")
        .and_then(Value::as_i64)
        .ok_or_else(request_invalid)?;
    let title = object
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(request_invalid)?;
    Ok(ChangeListing {
        key,
        id,
        expected_version,
        title: title.to_owned(),
    })
}

pub fn ecommerce_listing_router(
    targets: Vec<EcommerceListingHttpTarget>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/ecommerce/listings",
            post(create_listing_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/ecommerce/listings/{listing_id}/changes",
            post(change_listing_handler),
        )
        .with_state(Arc::new(EcommerceListingHttpState::new(targets)?)))
}

async fn create_listing_handler(
    State(state): State<Arc<EcommerceListingHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .create_listing(&parts.headers, &slug, parts.uri.query(), &body)
        .await
}

async fn change_listing_handler(
    State(state): State<Arc<EcommerceListingHttpState>>,
    Path((slug, listing_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, BODY_LIMIT + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .change_listing(
            &parts.headers,
            &slug,
            &listing_id,
            parts.uri.query(),
            &body,
        )
        .await
}
