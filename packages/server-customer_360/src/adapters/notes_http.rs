//! Native Customer 360 contact-note HTTP ingress.
//!
//! Transport facts only: registered Host/Holding target, one exact customer or
//! order source, bounded cursor-free read and one idempotent command. Current
//! Account authority, source visibility, same-person policy and the serializable
//! owner unit remain composition responsibilities behind the backend trait.
use crate::domain::notes::{
    ChangeCustomerNoteInput, CreateCustomerNoteInput, CustomerNoteCommandResult, CustomerNotesView,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const QUERY_LIMIT: usize = 4 * 1024;
const BODY_LIMIT: usize = 32 * 1024;
const KEY_LIMIT: usize = 180;

pub type CustomerNotesHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct CustomerNotesHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub customer_id: Option<&'a str>,
    pub order_id: Option<&'a str>,
}

pub struct CustomerNotesHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a str,
    pub body: serde_json::Value,
}

pub trait CustomerNotesHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: CustomerNotesHttpReadRequest<'a>,
    ) -> CustomerNotesHttpFuture<'a, CustomerNotesView>;
    fn command<'a>(
        &'a self,
        request: CustomerNotesHttpCommandRequest<'a>,
    ) -> CustomerNotesHttpFuture<'a, CustomerNoteCommandResult>;
}

pub struct CustomerNotesHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn CustomerNotesHttpBackend>,
}

struct CustomerNotesHttpState {
    targets: BTreeMap<(String, String), CustomerNotesHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "CUSTOMER_NOTE_REQUEST_INVALID",
        "Use one bounded contact-note request.",
    )
}
fn key_required() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "COMMAND_KEY_REQUIRED",
        "Idempotency-Key is required.",
    )
}
fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_NOTE_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}
fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 200 && !value.chars().any(char::is_control)
}
fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first
        .map(|value| value.to_str().map_err(|_| malformed()))
        .transpose()
}
fn status(kind: FailureKind) -> StatusCode {
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
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}
fn respond<T: serde::Serialize>(result: Result<T, BusinessError>) -> Response {
    match result {
        Ok(data) => Json(serde_json::json!({"data": data})).into_response(),
        Err(error) => error_response(error),
    }
}
fn target<'a>(
    state: &'a CustomerNotesHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a CustomerNotesHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?.ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn require_origin(
    target: &CustomerNotesHttpTarget,
    headers: &HeaderMap,
    mutation: bool,
) -> Result<(), BusinessError> {
    let origin = unique_header(headers, header::ORIGIN)?;
    if mutation && origin != Some(target.origin.as_str())
        || origin.is_some_and(|value| value != target.origin)
    {
        return Err(origin_forbidden());
    }
    Ok(())
}

async fn read(
    State(state): State<Arc<CustomerNotesHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let query = request.uri().query().unwrap_or("").to_owned();
    let result: Result<CustomerNotesView, BusinessError> = async move {
        let target = target(&state, &headers, &slug)?;
        require_origin(target, &headers, false)?;
        if query.len() > QUERY_LIMIT {
            return Err(malformed());
        }
        let mut customer_id = None;
        let mut order_id = None;
        for pair in query.split('&').filter(|value| !value.is_empty()) {
            let (name, value) = pair.split_once('=').ok_or_else(malformed)?;
            let value = percent_decode(value)?;
            match name {
                "customerId" if customer_id.is_none() => customer_id = Some(value),
                "orderId" if order_id.is_none() => order_id = Some(value),
                _ => return Err(malformed()),
            }
        }
        if customer_id.is_some() == order_id.is_some() {
            return Err(BusinessError::new(
                FailureKind::InvalidInput,
                "SOURCE_REQUIRED",
                "Choose exactly one order or customer source.",
            ));
        }
        if customer_id
            .as_deref()
            .or(order_id.as_deref())
            .is_some_and(|id| !valid_id(id))
        {
            return Err(malformed());
        }
        target
            .backend
            .read(CustomerNotesHttpReadRequest {
                host: unique_header(&headers, header::HOST)?.ok_or_else(missing)?,
                holding_slug: &slug,
                headers: &headers,
                customer_id: customer_id.as_deref(),
                order_id: order_id.as_deref(),
            })
            .await
    }
    .await;
    respond(result)
}

async fn command(
    State(state): State<Arc<CustomerNotesHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(malformed()),
    };
    let result: Result<CustomerNoteCommandResult, BusinessError> = async move {
        let target = target(&state, &headers, &slug)?;
        require_origin(target, &headers, true)?;
        let key = unique_header(&headers, HeaderName::from_static("idempotency-key"))?
            .ok_or_else(key_required)?;
        if key.len() < 8 || key.len() > KEY_LIMIT || key.chars().any(char::is_control) {
            return Err(key_required());
        }
        if unique_header(&headers, header::CONTENT_TYPE)?
            .is_none_or(|value| !value.starts_with("application/json"))
        {
            return Err(malformed());
        }
        let value: serde_json::Value =
            serde_json::from_slice(&body).map_err(|_| malformed())?;
        // Parse both shapes here so an unknown or extra field never reaches the
        // owner unit. The owner revalidates every value against current facts.
        if value.get("id").is_some() {
            let input: ChangeCustomerNoteInput =
                serde_json::from_value(value.clone()).map_err(|_| malformed())?;
            input.validate()?;
        } else {
            let input: CreateCustomerNoteInput =
                serde_json::from_value(value.clone()).map_err(|_| malformed())?;
            input.validate(i64::MAX)?;
        }
        target
            .backend
            .command(CustomerNotesHttpCommandRequest {
                host: unique_header(&headers, header::HOST)?.ok_or_else(missing)?,
                holding_slug: &slug,
                headers: &headers,
                key,
                body: value,
            })
            .await
    }
    .await;
    respond(result)
}

fn percent_decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(malformed());
            }
            let high = hex(bytes[index + 1]).ok_or_else(malformed)?;
            let low = hex(bytes[index + 2]).ok_or_else(malformed)?;
            out.push(high * 16 + low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| malformed())
}
fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

pub fn customer_notes_http_router(
    targets: Vec<CustomerNotesHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut map = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !target.origin.starts_with("https://")
            || map
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    target,
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/customer-notes",
            get(read).post(command),
        )
        .with_state(Arc::new(CustomerNotesHttpState { targets: map })))
}
