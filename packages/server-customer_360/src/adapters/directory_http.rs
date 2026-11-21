//! Native Customer 360 customer directory HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target, one exact directory
//! path and one bounded keyset query. Normal Account session resolution,
//! selected Company/branch/department scope and the serializable owner unit
//! remain composition responsibilities behind [`CustomerDirectoryHttpBackend`].
use crate::domain::directory::{CustomerDirectoryPage, CustomerDirectoryQuery, request_invalid};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const QUERY_LIMIT: usize = 4 * 1024;

pub type CustomerDirectoryHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct CustomerDirectoryHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub query: CustomerDirectoryQuery,
}

pub trait CustomerDirectoryHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: CustomerDirectoryHttpReadRequest<'a>,
    ) -> CustomerDirectoryHttpFuture<'a, CustomerDirectoryPage>;
}

pub struct CustomerDirectoryHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn CustomerDirectoryHttpBackend>,
}

struct CustomerDirectoryHttpState {
    targets: BTreeMap<(String, String), CustomerDirectoryHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_DIRECTORY_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}
fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
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
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
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
    state: &'a CustomerDirectoryHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a CustomerDirectoryHttpTarget, BusinessError> {
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
                return Err(request_invalid());
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
    String::from_utf8(out).map_err(|_| request_invalid())
}

fn query(request: &Request) -> Result<CustomerDirectoryQuery, BusinessError> {
    let Some(raw) = request.uri().query() else {
        return Ok(CustomerDirectoryQuery::default());
    };
    if raw.len() > QUERY_LIMIT {
        return Err(request_invalid());
    }
    let mut parsed = CustomerDirectoryQuery::default();
    let mut seen = std::collections::BTreeSet::new();
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = percent_decode(name)?;
        let value = percent_decode(value)?;
        if !seen.insert(name.clone()) {
            return Err(request_invalid());
        }
        match name.as_str() {
            "cursor" => parsed.cursor = Some(value),
            "limit" => {
                parsed.limit = Some(value.parse::<u16>().map_err(|_| request_invalid())?);
            }
            _ => return Err(request_invalid()),
        }
    }
    parsed.validate()?;
    Ok(parsed)
}

pub fn customer_directory_http_router(
    targets: Vec<CustomerDirectoryHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/customers",
            get(read_directory),
        )
        .with_state(Arc::new(CustomerDirectoryHttpState {
            targets: configured,
        })))
}

async fn read_directory(
    State(state): State<Arc<CustomerDirectoryHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let query = query(&request)?;
        let headers = request.headers().clone();
        let _ = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| request_invalid())?;
        target
            .backend
            .read(CustomerDirectoryHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                query,
            })
            .await
    }
    .await;
    match result {
        Ok(page) => bounded(serde_json::json!({"data": page})),
        Err(error) => error_response(error),
    }
}
