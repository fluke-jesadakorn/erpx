//! Native Customer 360 timeline HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target, one exact customer
//! path, one bounded cursor/limit query and business-error mapping. Normal
//! Account session resolution, selected Company/branch/department scope,
//! Support history purchase admission and the serializable owner unit remain
//! composition responsibilities behind [`CustomerTimelineHttpBackend`].
use crate::domain::timeline::{CustomerTimelineQuery, request_invalid};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const QUERY_LIMIT: usize = 4 * 1024;

pub type CustomerTimelineHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct CustomerTimelineHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub customer_id: &'a str,
    pub headers: &'a HeaderMap,
    pub query: CustomerTimelineQuery,
}

#[derive(Clone, Debug)]
pub struct CustomerTimelineHttpResponse {
    /// Owner read data. The backend adds the validated current context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl CustomerTimelineHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope and owner-unit adapter. The router
/// never mints a session, actor, Company, permission or Support purchase.
pub trait CustomerTimelineHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: CustomerTimelineHttpReadRequest<'a>,
    ) -> CustomerTimelineHttpFuture<'a, CustomerTimelineHttpResponse>;
}

pub struct CustomerTimelineHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn CustomerTimelineHttpBackend>,
}

struct CustomerTimelineHttpState {
    targets: BTreeMap<(String, String), CustomerTimelineHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    request_invalid()
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_TIMELINE_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}
fn customer_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_TIMELINE_CUSTOMER_INVALID",
        "Choose a customer available in this workspace.",
    )
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
fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty()
            && !rest
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    })
}
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}
fn valid_customer(value: &str) -> bool {
    RecordId::new(value.to_owned()).is_ok()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
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
fn bounded(body: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
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
fn target<'a>(
    state: &'a CustomerTimelineHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a CustomerTimelineHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
fn percent_decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied().and_then(hex);
            let low = bytes.get(index + 2).copied().and_then(hex);
            let (Some(high), Some(low)) = (high, low) else {
                return Err(malformed());
            };
            out.push(high * 16 + low);
            index += 3;
        } else if bytes[index] == b'+' {
            out.push(b' ');
            index += 1;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| malformed())
}

fn query(request: &Request) -> Result<CustomerTimelineQuery, BusinessError> {
    let Some(raw) = request.uri().query() else {
        return Ok(CustomerTimelineQuery::default());
    };
    if raw.len() > QUERY_LIMIT {
        return Err(malformed());
    }
    let mut parsed = CustomerTimelineQuery::default();
    let mut seen = std::collections::BTreeSet::new();
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = percent_decode(name)?;
        let value = percent_decode(value)?;
        if !seen.insert(name.clone()) {
            return Err(malformed());
        }
        match name.as_str() {
            "cursor" => parsed.cursor = Some(value),
            "limit" => {
                parsed.limit = Some(value.parse::<u16>().map_err(|_| malformed())?);
            }
            _ => return Err(malformed()),
        }
    }
    parsed.validate()?;
    Ok(parsed)
}

pub fn customer_timeline_http_router(
    targets: Vec<CustomerTimelineHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/customers/{customer_id}/timeline",
            get(read_timeline),
        )
        .with_state(Arc::new(CustomerTimelineHttpState {
            targets: configured,
        })))
}

async fn read_timeline(
    State(state): State<Arc<CustomerTimelineHttpState>>,
    Path((slug, customer_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        if !valid_customer(&customer_id) {
            return Err(customer_invalid());
        }
        let query = query(&request)?;
        let headers = request.headers().clone();
        let _ = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| malformed())?;
        target
            .backend
            .read(CustomerTimelineHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                customer_id: &customer_id,
                headers: &headers,
                query,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}
